//! Apply one cached conversion fact to each occurrence of a shared reader.
use super::push_once;
use crate::engine::analysis::numeric::{
    BINARY_INPUT_BOUNDARY, FLOAT_BOUND_REPRESENTATION_GAP, NumericFacts, NumericReader,
};
use crate::{
    CommandForm, CommandGrammar, Field, FieldMembers, FieldReadOutcome, Gap, GapKind, GapSubject,
    GrammarProperty, NumericConversion, Reader, ReaderId, ReaderKind,
};

/// What the numeric facts of one or more readers add to an answer.
#[derive(Default)]
pub(super) struct ReaderLimits {
    /// The most fundamental obstacle that makes the answer partial.
    incomplete: Option<Obstacle>,
    unrepresentable_bounds: bool,
    /// The platform scanner's own conversion, outside the method.
    scanner: bool,
    /// The fixed-point raw path of a binary lexer, outside the method.
    binary_input: bool,
}

/// What keeps a numeric conversion incomplete, from the least to the most fundamental. A
/// conversion path that is not established leaves its storage and range unknown too.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Obstacle {
    Range,
    Storage,
    ConversionPath,
}

impl Obstacle {
    const ALL: [Self; 3] = [Self::Range, Self::Storage, Self::ConversionPath];

    fn detail(self) -> &'static str {
        match self {
            Self::Range => UNKNOWN_RANGE,
            Self::Storage => UNKNOWN_STORAGE,
            Self::ConversionPath => UNKNOWN_CONVERSION_PATH,
        }
    }

    /// The obstacle that an incomplete-conversion gap names.
    fn of(gap: &Gap) -> Option<Self> {
        Self::ALL.into_iter().find(|obstacle| {
            gap.kind == GapKind::NumericConversion && gap.detail == obstacle.detail()
        })
    }
}

impl ReaderLimits {
    fn include(&mut self, other: Self) {
        self.incomplete = self.incomplete.max(other.incomplete);
        self.unrepresentable_bounds |= other.unrepresentable_bounds;
        self.scanner |= other.scanner;
        self.binary_input |= other.binary_input;
    }
}

/// Classify one numeric fact. A typed reason, or a storage property or range that is not
/// `Known`, makes it incomplete; a boundary reason never does. The float bound reason is a range
/// fact with its own gap, so it is not a conversion-path obstacle.
pub(super) fn limits(fact: &NumericReader) -> ReaderLimits {
    let path_gap = fact
        .gaps
        .iter()
        .any(|gap| gap.reason != FLOAT_BOUND_REPRESENTATION_GAP);
    let conversion_obstacle = match &fact.conversion {
        GrammarProperty::Known(Some(conversion)) | GrammarProperty::Partial(Some(conversion)) => {
            obstacle(conversion)
        }
        GrammarProperty::Known(None) => None,
        _ => Some(Obstacle::ConversionPath),
    };

    ReaderLimits {
        incomplete: if path_gap {
            Some(Obstacle::ConversionPath)
        } else {
            conversion_obstacle
        },
        unrepresentable_bounds: fact
            .gaps
            .iter()
            .any(|gap| gap.reason == FLOAT_BOUND_REPRESENTATION_GAP),
        scanner: fact
            .boundary
            .iter()
            .any(|gap| gap.reason != BINARY_INPUT_BOUNDARY),
        binary_input: fact
            .boundary
            .iter()
            .any(|gap| gap.reason == BINARY_INPUT_BOUNDARY),
    }
}

/// The first storage property or the range that is not `Known`. Literal syntax stays partial by
/// design, so it is not part of the storage that must be known.
fn obstacle(conversion: &NumericConversion) -> Option<Obstacle> {
    let storage_known = matches!(conversion.representation, GrammarProperty::Known(_))
        && matches!(conversion.width_bits, GrammarProperty::Known(_))
        && matches!(conversion.signedness, GrammarProperty::Known(_))
        && matches!(conversion.scale, GrammarProperty::Known(_));

    if !storage_known {
        Some(Obstacle::Storage)
    } else if !matches!(conversion.accepted_range, GrammarProperty::Known(_)) {
        Some(Obstacle::Range)
    } else {
        None
    }
}

/// Attach conversion facts and retain the obstructions that need public gaps.
fn attach_reader_facts(reader: &mut Reader, facts: &NumericFacts) -> ReaderLimits {
    reader.scoped_operand =
        if matches!(reader.kind, ReaderKind::Unknown | ReaderKind::ScopedNumeric) {
            GrammarProperty::Unresolved
        } else {
            GrammarProperty::Known(None)
        };
    if let Some(fact) = facts.readers.iter().find_map(|(callee, fact)| {
        (reader.id.as_ref() == Some(&ReaderId::from_callee(callee))).then_some(fact)
    }) {
        reader.numeric = fact.conversion.clone();
        return limits(fact);
    }
    reader.numeric = match reader.kind {
        ReaderKind::Boolean
        | ReaderKind::String
        | ReaderKind::Reference
        | ReaderKind::Target
        | ReaderKind::Keyword
        | ReaderKind::Skipped
        | ReaderKind::Block => GrammarProperty::Known(None),
        _ => GrammarProperty::Unresolved,
    };
    let numeric = matches!(
        reader.kind,
        ReaderKind::Integer | ReaderKind::FixedPoint | ReaderKind::Float
    );
    ReaderLimits {
        incomplete: numeric.then_some(Obstacle::ConversionPath),
        ..ReaderLimits::default()
    }
}

pub(super) fn fields(
    values: &mut [Field],
    facts: &NumericFacts,
    prefix: &[String],
    gaps: &mut Vec<Gap>,
) {
    for field in values {
        let mut path = prefix.to_vec();
        path.push(field.name.clone());
        let field_limits = attach_reader_facts(&mut field.reader, facts);
        let mut limits = if alternatives_carry_the_readers(field) {
            ReaderLimits::default()
        } else {
            field_limits
        };
        for alternative in &mut field.read {
            if let FieldReadOutcome::Read { reader: value, .. } = &mut alternative.outcome {
                limits.include(attach_reader_facts(value, facts));
            }
        }
        push_gaps(gaps, GapSubject::path(&path), &limits);
        match &mut field.members {
            FieldMembers::Fields(children) => fields(children, facts, &path, gaps),
            FieldMembers::ModifierBlock(block) => modifier_block(block, facts, &path, gaps),
            _ => {}
        }
    }
}

/// A field whose alternatives read with different readers has no reader identity of its own.
/// When every alternative is rejected or reads with an identified reader, those readers' facts are
/// the field's facts, and the field reader adds no limit.
fn alternatives_carry_the_readers(field: &Field) -> bool {
    let identified = |alternative: &crate::FieldReadAlternative| match &alternative.outcome {
        FieldReadOutcome::Read { reader, .. } => reader.id.is_some(),
        FieldReadOutcome::Rejected => true,
        FieldReadOutcome::Unresolved => false,
    };
    let reads = field
        .read
        .iter()
        .any(|alternative| matches!(alternative.outcome, FieldReadOutcome::Read { .. }));

    field.reader.id.is_none() && reads && field.read.iter().all(identified)
}

/// Attach numeric facts to the keys and numeric entries of the modifier block at `path`.
pub(super) fn modifier_block(
    block: &mut crate::ModifierBlock,
    facts: &NumericFacts,
    path: &[String],
    gaps: &mut Vec<Gap>,
) {
    if let GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) = &mut block.fixed_keys {
        fields(keys, facts, path, gaps);
    }

    let (GrammarProperty::Known(entries) | GrammarProperty::Partial(entries)) = &mut block.entries
    else {
        return;
    };

    for entry in entries {
        if let crate::ModifierEntry::Numeric { value } = entry {
            let limits = attach_reader_facts(value, facts);
            push_gaps(gaps, GapSubject::path(path), &limits);
        }
    }
}

/// Attach the numeric facts of one reader that no field carries, with its gap at `subject`.
pub(super) fn reader(
    reader: &mut Reader,
    facts: &NumericFacts,
    subject: GapSubject,
    gaps: &mut Vec<Gap>,
) {
    let limits = attach_reader_facts(reader, facts);
    push_gaps(gaps, subject, &limits);
}

pub(super) fn grammar(
    value: &mut CommandGrammar,
    name: &str,
    facts: &NumericFacts,
    gaps: &mut Vec<Gap>,
) {
    let mut limits = attach_reader_facts(&mut value.reader, facts);
    if let GrammarProperty::Known(forms) | GrammarProperty::Partial(forms) = &mut value.forms {
        for form in forms {
            if let CommandForm::Value(value) = form {
                limits.include(attach_reader_facts(&mut value.reader, facts));
            }
        }
    }
    push_gaps(gaps, GapSubject::answer_item(name), &limits);
    if let GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) = &mut value.fixed_keys {
        fields(keys, facts, &[], gaps);
    }
    if let GrammarProperty::Known(rules) | GrammarProperty::Partial(rules) = &mut value.ordering {
        for rule in rules {
            if let crate::ChildOrderOutcome::Read(selected) = &mut rule.outcome {
                let limits = attach_reader_facts(selected, facts);
                push_gaps(gaps, GapSubject::field(&rule.child), &limits);
            }
        }
    }
    let child_was_known = matches!(value.numeric_keys, GrammarProperty::Known(_));
    if let GrammarProperty::Known(Some(child)) | GrammarProperty::Partial(Some(child)) =
        &mut value.numeric_keys
    {
        let mut child_gaps = Vec::new();
        grammar(child, name, facts, &mut child_gaps);
        if child_gaps
            .iter()
            .any(|gap| gap.kind != GapKind::OutsideMethod)
            && child_was_known
            && let GrammarProperty::Known(child) = std::mem::take(&mut value.numeric_keys)
        {
            value.numeric_keys = GrammarProperty::Partial(child);
        }
        for mut child_gap in child_gaps {
            child_gap.subject = Some(GapSubject::answer_item(name));
            push_most_fundamental(gaps, child_gap);
        }
    }
}

/// Add `gap` once. An incomplete-conversion gap keeps one gap per subject: the most fundamental
/// obstacle replaces a lesser one, and a lesser one is dropped.
fn push_most_fundamental(gaps: &mut Vec<Gap>, gap: Gap) {
    let Some(obstacle) = Obstacle::of(&gap) else {
        return push_once(gaps, gap);
    };
    let existing = gaps
        .iter()
        .position(|other| other.subject == gap.subject && Obstacle::of(other).is_some());
    match existing {
        None => gaps.push(gap),
        Some(index) if Obstacle::of(&gaps[index]) < Some(obstacle) => gaps[index] = gap,
        Some(_) => {}
    }
}

/// Add the public gaps of `limits` at `subject`, each once.
pub(super) fn push_gaps(gaps: &mut Vec<Gap>, subject: GapSubject, limits: &ReaderLimits) {
    let details = [
        limits
            .incomplete
            .map(|obstacle| (GapKind::NumericConversion, obstacle.detail())),
        limits
            .unrepresentable_bounds
            .then_some((GapKind::NumericConversion, UNREPRESENTABLE_BOUNDS)),
        limits.scanner.then_some((GapKind::OutsideMethod, SCANNER)),
        limits
            .binary_input
            .then_some((GapKind::OutsideMethod, BINARY_INPUT)),
    ];
    for (kind, detail) in details.into_iter().flatten() {
        let gap = Gap {
            kind,
            subject: Some(subject.clone()),
            detail: detail.into(),
        };
        push_once(gaps, gap);
    }
}

const UNKNOWN_RANGE: &str =
    "Numeric conversion is incomplete: the accepted range is not established.";

const UNKNOWN_STORAGE: &str = "Numeric conversion is incomplete: a storage property (representation, width, signedness or scale) is not established.";

const UNKNOWN_CONVERSION_PATH: &str =
    "Numeric conversion is incomplete: the engine conversion path is not established.";

const UNREPRESENTABLE_BOUNDS: &str =
    "Exact binary32 range endpoints cannot be represented by NumericBound.";

const SCANNER: &str = "The platform scanner's own conversion of the token text (overflow, the characters it consumes, locale and library behavior) is outside this method.";

const BINARY_INPUT: &str =
    "The binary-lexer input path of save games and network data is outside this method.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::numeric::NumericReader;
    use crate::engine::analysis::stop::Unresolved;
    use crate::{
        CommandValue, FieldCondition, FieldDomain, FieldReadAlternative, FieldReference,
        FieldShape, NumericConversion, RepeatBehavior, ValueShape,
    };

    fn numeric_field(reader: Reader) -> Field {
        let shape = FieldShape {
            value: ValueShape::Scalar,
            repeat: RepeatBehavior::Replace,
        };
        Field {
            name: "amount".into(),
            reader: reader.clone(),
            shape,
            read: vec![FieldReadAlternative {
                condition: FieldCondition::Always,
                outcome: FieldReadOutcome::Read { reader, shape },
            }],
            members: FieldMembers::None,
            domain: FieldDomain::Unknown,

            uses: Vec::new(),
            entry_contexts: Vec::new(),
            read_scope: crate::GrammarProperty::Unresolved,
            accepted_categories: crate::AcceptedCategories::NotApplicable,
            reference: FieldReference::NotEstablished,
        }
    }

    #[test]
    fn fields_alternatives_and_command_values_reuse_the_same_fact_without_changing_ids() {
        for (callee, representation, bound_gap) in [
            (
                "CReader::Read(int&)",
                crate::NumericRepresentation::Integer,
                false,
            ),
            (
                "CReader::Read(float&)",
                crate::NumericRepresentation::BinaryFloat,
                true,
            ),
            (
                "CReader::Read(float&)",
                crate::NumericRepresentation::BinaryFloat,
                false,
            ),
        ] {
            let id = Some(ReaderId::from_callee(callee));
            let shared = Reader {
                id: id.clone(),
                kind: ReaderKind::Integer,
                family: crate::BlockFamily::NotApplicable,
                numeric: GrammarProperty::Unresolved,
                scoped_operand: crate::GrammarProperty::Unresolved,
            };
            let conversion = GrammarProperty::Partial(Some(NumericConversion {
                representation: GrammarProperty::Known(representation),
                width_bits: GrammarProperty::Known(32),
                ..NumericConversion::default()
            }));
            let facts = NumericFacts {
                lexer: Ok(()),
                token_readers: Default::default(),
                modifier_entry: Err(Unresolved::new("modifier-numeric-not-analyzed")),
                readers: [(
                    callee.into(),
                    NumericReader {
                        boundary: Vec::new(),
                        conversion: conversion.clone(),
                        gaps: if bound_gap {
                            vec![Unresolved::new(FLOAT_BOUND_REPRESENTATION_GAP)]
                        } else {
                            vec![Unresolved::new("numeric-external-library-conversion")]
                        },
                    },
                )]
                .into(),
            };
            let mut registry = vec![numeric_field(shared.clone())];
            let mut gaps = Vec::new();
            fields(&mut registry, &facts, &[], &mut gaps);
            let mut command = CommandGrammar {
                reader: shared.clone(),
                forms: GrammarProperty::Known(vec![CommandForm::Value(CommandValue {
                    reader: shared,
                    reference: FieldReference::NotEstablished,
                })]),
                fixed_keys: GrammarProperty::Known(vec![numeric_field(registry[0].reader.clone())]),
                targets: GrammarProperty::Unresolved,
                child_families: GrammarProperty::Unresolved,
                child_scopes: GrammarProperty::Unresolved,
                numeric_keys: GrammarProperty::Unresolved,
                ordering: GrammarProperty::Known(vec![crate::ChildOrderRule {
                    child: "amount".into(),
                    conditions: vec![crate::ChildOrderCondition::First(true)],
                    outcome: crate::ChildOrderOutcome::Read(registry[0].reader.clone()),
                }]),
                durations: GrammarProperty::Unresolved,
            };
            let mut nested = command.clone();
            nested.numeric_keys = GrammarProperty::Known(Some(Box::new(command.clone())));
            grammar(&mut nested, "nested", &facts, &mut Vec::new());
            assert!(matches!(
                nested.numeric_keys,
                GrammarProperty::Partial(Some(_))
            ));
            grammar(&mut command, "command", &facts, &mut gaps);
            let FieldReadOutcome::Read {
                reader: alternative,
                ..
            } = &registry[0].read[0].outcome
            else {
                panic!()
            };
            let GrammarProperty::Known(forms) = command.forms else {
                panic!()
            };
            let CommandForm::Value(value) = &forms[0] else {
                panic!()
            };
            let GrammarProperty::Known(ordering) = &command.ordering else {
                panic!()
            };
            let crate::ChildOrderOutcome::Read(ordered) = &ordering[0].outcome else {
                panic!()
            };
            for reader in [
                &registry[0].reader,
                alternative,
                &command.reader,
                &value.reader,
                ordered,
            ] {
                assert_eq!(reader.id, id);
                assert_eq!(reader.kind, ReaderKind::Integer);
                assert_eq!(reader.numeric, conversion);
            }
            assert!(
                gaps.iter()
                    .all(|gap| gap.kind == GapKind::NumericConversion)
            );
            assert_eq!(
                gaps.iter().any(|gap| gap.detail.contains("NumericBound")),
                bound_gap
            );
        }
    }
    const SCANNER_REASONS: [&str; 3] = [
        "numeric-overflow",
        "numeric-trailing-text",
        "numeric-external-library-conversion",
    ];

    /// A conversion whose storage and range are known, as the matched `int` reader's.
    fn established() -> NumericConversion {
        NumericConversion {
            representation: GrammarProperty::Known(crate::NumericRepresentation::Integer),
            width_bits: GrammarProperty::Known(32),
            signedness: GrammarProperty::Known(crate::NumericSignedness::Signed),
            scale: GrammarProperty::Known(Some(1)),
            literal_syntax: GrammarProperty::Partial(vec![
                crate::NumericLiteralSyntax::DecimalInteger,
            ]),
            accepted_range: GrammarProperty::Known(Box::new(crate::NumericRange {
                minimum: GrammarProperty::Known(crate::NumericBound::Signed(-2147483648)),
                maximum: GrammarProperty::Known(crate::NumericBound::Signed(2147483647)),
            })),
        }
    }

    fn integer_reader() -> Reader {
        Reader {
            id: Some(ReaderId::from_callee("CReader::Read(int&)")),
            kind: ReaderKind::Integer,
            family: crate::BlockFamily::NotApplicable,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: GrammarProperty::Unresolved,
        }
    }

    fn integer_facts(
        conversion: GrammarProperty<Option<NumericConversion>>,
        gaps: &[&'static str],
        boundary: &[&'static str],
    ) -> NumericFacts {
        NumericFacts {
            readers: [(
                "CReader::Read(int&)".into(),
                NumericReader {
                    conversion,
                    gaps: gaps.iter().copied().map(Unresolved::new).collect(),
                    boundary: boundary.iter().copied().map(Unresolved::new).collect(),
                },
            )]
            .into(),
            ..NumericFacts::default()
        }
    }

    #[test]
    fn only_typed_limits_make_a_numeric_answer_partial_and_the_gap_names_the_obstacle() {
        let mut narrow = established();
        narrow.signedness = GrammarProperty::Unresolved;
        let mut mismatched_scale = established();
        mismatched_scale.scale = GrammarProperty::Unresolved;
        let mut unknown_range = established();
        unknown_range.accepted_range = GrammarProperty::Unresolved;
        let mut fixed_point_boundary = SCANNER_REASONS.to_vec();
        fixed_point_boundary.push(BINARY_INPUT_BOUNDARY);

        for (case, conversion, typed, boundary, expected) in [
            (
                "boundary only",
                established(),
                vec![],
                SCANNER_REASONS.to_vec(),
                None,
            ),
            (
                "unknown range",
                unknown_range,
                vec![],
                SCANNER_REASONS.to_vec(),
                Some(UNKNOWN_RANGE),
            ),
            (
                "narrow signedness",
                narrow,
                vec![],
                SCANNER_REASONS.to_vec(),
                Some(UNKNOWN_STORAGE),
            ),
            (
                "mismatched scale",
                mismatched_scale,
                vec![],
                SCANNER_REASONS.to_vec(),
                Some(UNKNOWN_STORAGE),
            ),
            (
                "binary input",
                established(),
                vec![],
                fixed_point_boundary,
                None,
            ),
            (
                "raw path not joined",
                established(),
                vec!["numeric-raw-value-mode"],
                SCANNER_REASONS.to_vec(),
                Some(UNKNOWN_CONVERSION_PATH),
            ),
        ] {
            let facts = integer_facts(
                GrammarProperty::Partial(Some(conversion)),
                &typed,
                &boundary,
            );
            let mut values = [numeric_field(integer_reader())];
            let mut gaps = Vec::new();
            fields(&mut values, &facts, &[], &mut gaps);

            let has = |kind, detail: &str| {
                gaps.iter()
                    .any(|gap| gap.kind == kind && gap.detail == detail)
            };
            assert!(has(GapKind::OutsideMethod, SCANNER), "{case}");
            assert_eq!(
                has(GapKind::OutsideMethod, BINARY_INPUT),
                boundary.contains(&BINARY_INPUT_BOUNDARY),
                "{case}"
            );
            let numeric: Vec<_> = gaps
                .iter()
                .filter(|gap| gap.kind == GapKind::NumericConversion)
                .map(|gap| gap.detail.as_str())
                .collect();
            assert_eq!(numeric, Vec::from_iter(expected), "{case}");
        }

        let unmatched = integer_facts(GrammarProperty::Unresolved, &["numeric-token-shape"], &[]);
        let mut values = [numeric_field(integer_reader())];
        let mut gaps = Vec::new();
        fields(&mut values, &unmatched, &[], &mut gaps);
        assert_eq!(values[0].reader.numeric, GrammarProperty::Unresolved);
        assert_eq!(gaps[0].detail, UNKNOWN_CONVERSION_PATH);
    }

    /// Facts for `CReader::Read(int&)` with an unknown range, and for `CReader::Read(float&)`
    /// with a typed conversion-path reason on otherwise established storage.
    fn range_and_path_facts() -> NumericFacts {
        let mut unknown_range = established();
        unknown_range.accepted_range = GrammarProperty::Unresolved;
        let fact = |conversion: NumericConversion, gaps: Vec<Unresolved>| NumericReader {
            conversion: GrammarProperty::Partial(Some(conversion)),
            gaps,
            boundary: Vec::new(),
        };
        NumericFacts {
            readers: [
                (
                    "CReader::Read(int&)".into(),
                    fact(unknown_range, Vec::new()),
                ),
                (
                    "CReader::Read(float&)".into(),
                    fact(
                        established(),
                        vec![Unresolved::new("numeric-raw-value-mode")],
                    ),
                ),
            ]
            .into(),
            ..NumericFacts::default()
        }
    }

    fn numeric_details(gaps: &[Gap]) -> Vec<&str> {
        gaps.iter()
            .filter(|gap| gap.kind == GapKind::NumericConversion)
            .map(|gap| gap.detail.as_str())
            .collect()
    }

    #[test]
    fn a_field_reports_the_conversion_path_of_any_reader_over_a_range() {
        for (field_callee, alternative_callee) in [
            ("CReader::Read(int&)", "CReader::Read(float&)"),
            ("CReader::Read(float&)", "CReader::Read(int&)"),
        ] {
            let mut field = numeric_field(integer_reader());
            field.reader.id = Some(ReaderId::from_callee(field_callee));
            let FieldReadOutcome::Read { reader, .. } = &mut field.read[0].outcome else {
                panic!()
            };
            reader.id = Some(ReaderId::from_callee(alternative_callee));
            let mut gaps = Vec::new();

            fields(
                std::slice::from_mut(&mut field),
                &range_and_path_facts(),
                &[],
                &mut gaps,
            );

            assert_eq!(
                numeric_details(&gaps),
                [UNKNOWN_CONVERSION_PATH],
                "{field_callee}"
            );
        }
    }

    #[test]
    fn a_field_without_a_reader_identity_takes_the_limits_of_its_alternatives() {
        let alternative = |callee: &str| {
            let mut reader = integer_reader();
            reader.id = Some(ReaderId::from_callee(callee));
            FieldReadAlternative {
                condition: FieldCondition::Always,
                outcome: FieldReadOutcome::Read {
                    reader,
                    shape: FieldShape {
                        value: ValueShape::Scalar,
                        repeat: RepeatBehavior::Replace,
                    },
                },
            }
        };
        let mut facts = range_and_path_facts();
        let mut unknown_range = established();
        unknown_range.accepted_range = GrammarProperty::Unresolved;
        facts.readers.insert(
            "CReader::Read(short&)".into(),
            NumericReader {
                conversion: GrammarProperty::Partial(Some(unknown_range)),
                gaps: Vec::new(),
                boundary: Vec::new(),
            },
        );
        let unresolved = FieldReadAlternative {
            condition: FieldCondition::Always,
            outcome: FieldReadOutcome::Unresolved,
        };

        for (case, read, expected) in [
            (
                "identified readers",
                vec![
                    alternative("CReader::Read(int&)"),
                    alternative("CReader::Read(short&)"),
                ],
                UNKNOWN_RANGE,
            ),
            (
                "an unresolved alternative",
                vec![alternative("CReader::Read(int&)"), unresolved.clone()],
                UNKNOWN_CONVERSION_PATH,
            ),
        ] {
            let mut field = numeric_field(integer_reader());
            field.reader.id = None;
            field.read = read;
            let mut gaps = Vec::new();

            fields(std::slice::from_mut(&mut field), &facts, &[], &mut gaps);

            assert_eq!(numeric_details(&gaps), [expected], "{case}");
        }
    }

    #[test]
    fn every_reader_occurrence_reports_the_boundary_without_making_its_answer_partial() {
        let facts = integer_facts(
            GrammarProperty::Partial(Some(established())),
            &[],
            &SCANNER_REASONS,
        );
        let scanner = |subject: GapSubject| Gap {
            kind: GapKind::OutsideMethod,
            subject: Some(subject),
            detail: SCANNER.into(),
        };

        let mut block = crate::ModifierBlock {
            fixed_keys: GrammarProperty::Known(Vec::new()),
            entries: GrammarProperty::Known(vec![crate::ModifierEntry::Numeric {
                value: integer_reader(),
            }]),
        };
        let mut gaps = Vec::new();
        modifier_block(&mut block, &facts, &["modifier".into()], &mut gaps);
        assert_eq!(gaps, [scanner(GapSubject::field("modifier"))]);

        let mut gaps = Vec::new();
        reader(
            &mut integer_reader(),
            &facts,
            GapSubject::field("ai_weight"),
            &mut gaps,
        );
        assert_eq!(gaps, [scanner(GapSubject::field("ai_weight"))]);

        let child = CommandGrammar {
            reader: integer_reader(),
            forms: GrammarProperty::Known(vec![CommandForm::Value(CommandValue {
                reader: integer_reader(),
                reference: FieldReference::NotEstablished,
            })]),
            fixed_keys: GrammarProperty::Known(Vec::new()),
            targets: GrammarProperty::Known(Vec::new()),
            child_families: GrammarProperty::Known(Vec::new()),
            child_scopes: GrammarProperty::Unresolved,
            numeric_keys: GrammarProperty::Known(None),
            ordering: GrammarProperty::Known(vec![crate::ChildOrderRule {
                child: "ordered".into(),
                conditions: vec![crate::ChildOrderCondition::First(true)],
                outcome: crate::ChildOrderOutcome::Read(integer_reader()),
            }]),
            durations: GrammarProperty::Unresolved,
        };
        let mut parent = child.clone();
        parent.numeric_keys = GrammarProperty::Known(Some(Box::new(child)));
        let mut gaps = Vec::new();
        grammar(&mut parent, "command", &facts, &mut gaps);

        assert!(matches!(
            parent.numeric_keys,
            GrammarProperty::Known(Some(_))
        ));
        assert_eq!(
            gaps,
            [
                scanner(GapSubject::answer_item("command")),
                scanner(GapSubject::field("ordered")),
            ]
        );
        assert_eq!(
            crate::Completeness::from_gaps(&gaps),
            crate::Completeness::Complete
        );
    }

    #[test]
    fn unknown_reader_keeps_scoped_operand_unresolved() {
        let mut reader = Reader {
            id: None,
            kind: ReaderKind::Unknown,
            family: crate::BlockFamily::Unknown,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: GrammarProperty::Unresolved,
        };
        attach_reader_facts(&mut reader, &NumericFacts::default());
        assert_eq!(reader.scoped_operand, GrammarProperty::Unresolved);
    }

    #[test]
    fn a_keyword_reader_and_its_alternatives_have_no_numeric_conversion() {
        let reader = Reader {
            id: None,
            kind: ReaderKind::Keyword,
            family: crate::BlockFamily::NotApplicable,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: GrammarProperty::Unresolved,
        };
        let mut values = [numeric_field(reader)];
        let mut gaps = Vec::new();

        fields(&mut values, &NumericFacts::default(), &[], &mut gaps);

        assert_eq!(values[0].reader.numeric, GrammarProperty::Known(None));
        assert!(matches!(
            &values[0].read[0].outcome,
            FieldReadOutcome::Read { reader, .. } if reader.numeric == GrammarProperty::Known(None)
        ));
        assert!(gaps.is_empty(), "{gaps:?}");
    }

    #[test]
    fn a_modifier_block_does_not_inherit_the_numeric_entry_conversion() {
        let mut block = Reader {
            id: Some(ReaderId::from_callee("modifier-block")),
            kind: ReaderKind::Block,
            family: crate::BlockFamily::Modifier,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: crate::GrammarProperty::Unresolved,
        };
        let callee = "CReader::Read(CFixedPoint&)";
        let facts = NumericFacts {
            lexer: Ok(()),
            token_readers: Default::default(),
            readers: [(
                callee.into(),
                NumericReader {
                    boundary: Vec::new(),
                    conversion: GrammarProperty::Partial(Some(NumericConversion::default())),
                    gaps: Vec::new(),
                },
            )]
            .into(),
            modifier_entry: Ok(crate::engine::analysis::numeric::ModifierNumericEntry {
                shared_callee: callee.into(),
                reader_id: ReaderId::from_callee(callee),
                storage_width_bits: 64,
            }),
        };
        assert!(attach_reader_facts(&mut block, &facts).incomplete.is_none());
        assert_eq!(block.numeric, GrammarProperty::Known(None));
    }

    #[test]
    fn dynamic_numeric_child_gaps_use_the_parent_subject_without_hiding_parent_fields() {
        let number = Reader {
            id: Some(ReaderId::from_callee("number")),
            kind: ReaderKind::Integer,
            family: crate::BlockFamily::NotApplicable,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: crate::GrammarProperty::Unresolved,
        };
        let block = Reader {
            id: None,
            kind: ReaderKind::Block,
            family: crate::BlockFamily::Effect,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: crate::GrammarProperty::Unresolved,
        };
        let parent = CommandGrammar {
            reader: block,
            forms: GrammarProperty::Known(vec![CommandForm::Block]),
            targets: GrammarProperty::Known(vec![]),
            child_scopes: GrammarProperty::Unresolved,
            child_families: GrammarProperty::Known(vec![]),
            fixed_keys: GrammarProperty::Known(vec![numeric_field(number.clone())]),
            numeric_keys: GrammarProperty::Known(None),
            ordering: GrammarProperty::Known(vec![]),
            durations: GrammarProperty::Unresolved,
        };
        let mut nested_field = numeric_field(number.clone());
        nested_field.name = "nested".into();
        nested_field.members = FieldMembers::Fields(vec![numeric_field(number.clone())]);
        let mut child = parent.clone();
        child.fixed_keys =
            GrammarProperty::Known(vec![numeric_field(number.clone()), nested_field]);
        child.ordering = GrammarProperty::Known(vec![crate::ChildOrderRule {
            child: "ordered".into(),
            conditions: vec![crate::ChildOrderCondition::First(true)],
            outcome: crate::ChildOrderOutcome::Read(number),
        }]);
        for numeric_keys in [
            GrammarProperty::Known(Some(Box::new(child.clone()))),
            GrammarProperty::Partial(Some(Box::new(child))),
        ] {
            let mut value = parent.clone();
            value.numeric_keys = numeric_keys;
            let mut gaps = Vec::new();
            grammar(&mut value, "parent", &NumericFacts::default(), &mut gaps);
            assert_eq!(gaps.len(), 2);
            assert!(
                gaps.iter()
                    .all(|gap| gap.kind == GapKind::NumericConversion)
            );
            assert_eq!(gaps[0].subject, Some(GapSubject::field("amount")));
            assert_eq!(gaps[1].subject, Some(GapSubject::answer_item("parent")));
            assert!(matches!(
                value.numeric_keys,
                GrammarProperty::Partial(Some(_))
            ));
        }
    }
    #[test]
    fn numeric_child_gaps_keep_the_most_fundamental_obstacle_at_the_parent() {
        let reader = |callee: &str| {
            let mut reader = integer_reader();
            reader.id = Some(ReaderId::from_callee(callee));
            reader
        };
        let field = |name: &str, callee: &str| {
            let mut field = numeric_field(reader(callee));
            field.name = name.into();
            field
        };
        let block = Reader {
            id: None,
            kind: ReaderKind::Block,
            family: crate::BlockFamily::Effect,
            numeric: GrammarProperty::Unresolved,
            scoped_operand: GrammarProperty::Unresolved,
        };
        for keys in [
            [
                ("range", "CReader::Read(int&)"),
                ("path", "CReader::Read(float&)"),
            ],
            [
                ("path", "CReader::Read(float&)"),
                ("range", "CReader::Read(int&)"),
            ],
        ] {
            let child = CommandGrammar {
                reader: block.clone(),
                forms: GrammarProperty::Known(vec![CommandForm::Block]),
                targets: GrammarProperty::Known(vec![]),
                child_scopes: GrammarProperty::Unresolved,
                child_families: GrammarProperty::Known(vec![]),
                fixed_keys: GrammarProperty::Known(
                    keys.iter()
                        .map(|(name, callee)| field(name, callee))
                        .collect(),
                ),
                numeric_keys: GrammarProperty::Known(None),
                ordering: GrammarProperty::Known(vec![]),
                durations: GrammarProperty::Unresolved,
            };
            let mut parent = child.clone();
            parent.fixed_keys = GrammarProperty::Known(vec![]);
            parent.numeric_keys = GrammarProperty::Known(Some(Box::new(child)));
            let mut gaps = Vec::new();

            grammar(&mut parent, "parent", &range_and_path_facts(), &mut gaps);

            assert_eq!(
                numeric_details(&gaps),
                [UNKNOWN_CONVERSION_PATH],
                "{keys:?}"
            );
        }
    }

    #[test]
    fn modifier_children_and_entries_use_the_shared_normalization_pass() {
        use crate::engine::analysis::fields::ReaderJoin;
        use crate::{ModifierBlock, ModifierEntry};
        let reader = |callee: &str| {
            super::super::fields::reader(&[ReaderJoin::Joined {
                callee: callee.into(),
                arguments: Default::default(),
                tail: false,
            }])
        };
        let conversion = GrammarProperty::Partial(Some(NumericConversion::default()));
        let facts = NumericFacts {
            readers: ["CReader::Read(int&)", "CReader::Read(CFixedPoint&)"]
                .into_iter()
                .map(|callee| {
                    (
                        callee.into(),
                        NumericReader {
                            boundary: Vec::new(),
                            conversion: conversion.clone(),
                            gaps: vec![Unresolved::new("numeric-overflow")],
                        },
                    )
                })
                .collect(),
            ..NumericFacts::default()
        };
        let integer = numeric_field(reader("CReader::Read(int&)"));
        let mut children: Vec<_> = ["icon", "custom_tooltip", "description"]
            .into_iter()
            .map(|name| {
                let mut field = numeric_field(reader("CReader::Read(CString&, bool)"));
                field.name = name.into();
                field
            })
            .collect();
        children.push(integer.clone());
        let mut parent = numeric_field(reader("CReader::Read(CPersistent&)"));
        parent.name = "modifier".into();
        parent.members = FieldMembers::ModifierBlock(ModifierBlock {
            fixed_keys: GrammarProperty::Partial(children),
            entries: GrammarProperty::Known(vec![ModifierEntry::Numeric {
                value: reader("CReader::Read(CFixedPoint&)"),
            }]),
        });
        let mut gaps = vec![];
        fields(std::slice::from_mut(&mut parent), &facts, &[], &mut gaps);
        let mut outside = vec![integer];
        fields(&mut outside, &facts, &[], &mut vec![]);
        assert_eq!(parent.reader.numeric, GrammarProperty::Known(None));
        let FieldMembers::ModifierBlock(block) = &parent.members else {
            panic!()
        };
        let GrammarProperty::Partial(children) = &block.fixed_keys else {
            panic!()
        };
        for child in &children[..3] {
            assert_eq!(child.reader.numeric, GrammarProperty::Known(None));
            assert_eq!(child.reader.scoped_operand, GrammarProperty::Known(None));
            assert_eq!(child.reference, FieldReference::NotEstablished);
        }
        assert_eq!(children[3], outside[0]);
        assert!(gaps.iter().any(|gap| gap.kind == GapKind::NumericConversion
            && gap.subject == Some(GapSubject::field("modifier"))));
        assert_eq!(
            crate::Completeness::from_gaps(&gaps),
            crate::Completeness::Partial
        );
        let GrammarProperty::Known(entries) = &block.entries else {
            panic!()
        };
        let ModifierEntry::Numeric { value } = &entries[0] else {
            panic!()
        };
        assert_eq!(value.numeric, conversion);
        assert_eq!(value.scoped_operand, GrammarProperty::Known(None));
    }
}
