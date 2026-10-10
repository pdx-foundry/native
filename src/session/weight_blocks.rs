//! Attach shared weight grammar only where the field constructor proved its address point.
use crate::engine::analysis::{
    fields::{ReaderJoin, RegistryFieldResult, RootField, Value},
    numeric::NumericFacts,
    readers,
    references::ReferenceFacts,
    scoped_numeric::Facts as ScopedFacts,
    weight_blocks::{Grammar, Nested, Operand, OtherKeys, WeightBlockFacts},
};
use crate::{
    BlockFamily, Field, FieldCondition, FieldDomain, FieldMembers, FieldReadOutcome,
    FieldReference, Gap, GapKind, GapSubject, GrammarProperty, ReadScope, Reader, ReaderKind,
    ReferenceLookup, ReferenceTarget, ValueShape, WeightBlock, WeightOperation, WeightOtherKeys,
};
use std::collections::BTreeSet;

/// The executable facts that a weight block joins.
pub(super) struct WeightFacts<'a> {
    pub weights: &'a WeightBlockFacts,
    pub references: &'a ReferenceFacts,
    pub numeric: &'a NumericFacts,
    pub scoped: &'a ScopedFacts,
    pub scope_names: Option<&'a [String]>,
}

pub(super) fn attach(
    values: &mut [Field],
    result: &RegistryFieldResult,
    facts: &WeightFacts<'_>,
    gaps: &mut Vec<Gap>,
) {
    for field in values
        .iter_mut()
        .filter(|field| field.reader.family == BlockFamily::Weight)
    {
        let Some(root) = result.fields.iter().find(|root| root.name == field.name) else {
            continue;
        };
        let subject = GapSubject::field(&field.name);
        let destinations: BTreeSet<_> = root
            .readers
            .iter()
            .filter_map(readers::destination)
            .collect();
        let points: BTreeSet<_> = destinations
            .iter()
            .filter_map(|offset| result.persistent_points.get(offset))
            .collect();
        let (Some(&destination), Some(&&point), 1, 1) = (
            destinations.first(),
            points.first(),
            destinations.len(),
            points.len(),
        ) else {
            push(
                gaps,
                GapKind::UnresolvedReader,
                subject,
                "The weight block has no unique constructor-proven address point.".into(),
            );
            continue;
        };

        let grammar = match facts.weights.points.get(&point) {
            Some(Ok(grammar)) => grammar,
            failure => {
                let reason = match failure {
                    Some(Err(stop)) => stop.reason,
                    _ => "weight-block-point",
                };
                push(
                    gaps,
                    GapKind::UnresolvedReader,
                    subject,
                    format!("The weight block analysis stopped at {reason}."),
                );
                continue;
            }
        };

        let stored = |offset: u64| result.stored_words.get(&(destination, offset)).copied();
        let block = Block {
            facts,
            path: vec![field.name.clone()],
        };
        field.read_scope = block.read_scope(grammar, &stored, gaps);
        field.members =
            FieldMembers::WeightBlock(Box::new(block.normalize(grammar, &stored, gaps)));
    }
}

/// One weight block at a field path, with the facts its members join.
struct Block<'a> {
    facts: &'a WeightFacts<'a>,
    path: Vec<String>,
}

impl Block<'_> {
    fn subject(&self, key: Option<&str>) -> GapSubject {
        let mut path = self.path.clone();
        path.extend(key.map(str::to_owned));
        if path.len() == 1 {
            GapSubject::field(&path[0])
        } else {
            GapSubject::key_path(path)
        }
    }

    /// The scopes that the block's keys and conditions are read in. `words` gives the known
    /// words of the block's own object, by offset.
    fn read_scope(
        &self,
        grammar: &Grammar,
        words: &dyn Fn(u64) -> Option<u64>,
        gaps: &mut Vec<Gap>,
    ) -> GrammarProperty<Vec<ReadScope>> {
        let arguments: BTreeSet<_> = scope_arguments(grammar).collect();
        if arguments.is_empty() {
            return GrammarProperty::Known(Vec::new());
        }
        self.normalize_scopes(
            arguments.into_iter().collect(),
            words,
            self.subject(None),
            gaps,
        )
    }

    /// The read scopes that the scope `arguments` give, with a stored scope replaced by the word
    /// that `words` gives. The scaled and complex entry readers pass a literal scope mask 0 and
    /// check the scope only later, so a literal zero is outside the method (decision D5a). A
    /// stored zero has no such check and stays an unresolved path.
    fn normalize_scopes(
        &self,
        arguments: Vec<Option<&Value>>,
        words: &dyn Fn(u64) -> Option<u64>,
        subject: GapSubject,
        gaps: &mut Vec<Gap>,
    ) -> GrammarProperty<Vec<ReadScope>> {
        let (zero, other): (Vec<_>, Vec<_>) = arguments
            .into_iter()
            .partition(|argument| *argument == Some(&Value::Constant(0)));
        let other: Vec<_> = other
            .into_iter()
            .map(|scope| scope.and_then(|scope| resolve(scope, words)))
            .collect();
        if zero.is_empty() {
            return super::read_scope::normalize_scopes(
                other,
                self.facts.scope_names,
                subject,
                gaps,
            );
        }

        push(
            gaps,
            GapKind::OutsideMethod,
            subject.clone(),
            ZERO_MASK.into(),
        );
        if other.is_empty() {
            return GrammarProperty::Unresolved;
        }
        match super::read_scope::normalize_scopes(other, self.facts.scope_names, subject, gaps) {
            GrammarProperty::Known(scopes) | GrammarProperty::Partial(scopes) => {
                GrammarProperty::Partial(scopes)
            }
            GrammarProperty::Unresolved => GrammarProperty::Unresolved,
        }
    }

    /// The scope that a key or condition of the block is read in. When every key and condition
    /// takes one stored or received scope, that scope is the block's own read scope and is
    /// `Enclosing`, so equal readers give equal grammars whatever scope their owner stores.
    fn scope(
        &self,
        grammar: &Grammar,
        scope: Option<&Value>,
        words: &dyn Fn(u64) -> Option<u64>,
        subject: GapSubject,
        gaps: &mut Vec<Gap>,
    ) -> GrammarProperty<Vec<ReadScope>> {
        let arguments: BTreeSet<_> = scope_arguments(grammar).collect();
        let own = (arguments.len() == 1)
            .then(|| *arguments.first().unwrap())
            .flatten()
            .filter(|own| matches!(own, Value::Load(..) | Value::EnclosingScope));
        match scope {
            Some(scope) if Some(scope) == own => GrammarProperty::Known(vec![ReadScope::Enclosing]),
            scope => self.normalize_scopes(vec![scope], words, subject, gaps),
        }
    }

    fn normalize(
        &self,
        grammar: &Grammar,
        words: &dyn Fn(u64) -> Option<u64>,
        gaps: &mut Vec<Gap>,
    ) -> WeightBlock {
        let mut key_stops = false;
        for (key, stop) in &grammar.stops {
            let source = key
                .as_ref()
                .and_then(|key| grammar.dependent_readers.get(key));
            let (kind, detail) = match (key, source) {
                (Some(_), Some(source)) if reads_trigger(grammar, source) => (
                    GapKind::OutsideMethod,
                    format!(
                        "The value is read by the trigger that `{source}` names: a built-in \
                         trigger's form and grammar are its `command_grammar` answer, and a \
                         scripted trigger's are its parameter forms."
                    ),
                ),
                (Some(_), Some(source)) => (
                    GapKind::UnresolvedReader,
                    format!("The value is read by the object that `{source}` stores."),
                ),
                (Some(_), None) => (
                    GapKind::UnresolvedReader,
                    format!("The weight block analysis stopped at {}.", stop.reason),
                ),
                (None, _) => (
                    GapKind::UnresolvedPath,
                    format!("The weight block analysis stopped at {}.", stop.reason),
                ),
            };
            key_stops |= key.is_some() && kind != GapKind::OutsideMethod;
            push(gaps, kind, self.subject(key.as_deref()), detail);
        }

        let keys = self.fixed_keys(grammar, words, gaps);
        let operations: Vec<_> = grammar
            .operations
            .iter()
            .map(|operation| WeightOperation {
                key: operation.key.clone(),
                operand: operation
                    .operand
                    .as_ref()
                    .map(|operand| self.operand(operand, &operation.key, words, gaps)),
            })
            .collect();

        WeightBlock {
            scalar: self.scalar(grammar, gaps),
            fixed_keys: if key_stops || grammar.undetermined_keys {
                GrammarProperty::Partial(keys)
            } else {
                GrammarProperty::Known(keys)
            },
            operations: if grammar.undetermined_keys {
                GrammarProperty::Partial(operations)
            } else {
                GrammarProperty::Known(operations)
            },
            operation_repeat: grammar.operation_repeat,
            other_keys: match &grammar.other_keys {
                OtherKeys::Rejected => WeightOtherKeys::Rejected,
                OtherKeys::Triggers(scope) => WeightOtherKeys::Triggers(self.scope(
                    grammar,
                    scope.as_ref(),
                    words,
                    self.subject(None),
                    gaps,
                )),
                OtherKeys::Unresolved(_) => WeightOtherKeys::Unresolved,
            },
        }
    }

    fn scalar(&self, grammar: &Grammar, gaps: &mut Vec<Gap>) -> GrammarProperty<Option<Reader>> {
        match grammar.read_entry.as_ref().map(|entry| &entry.scalar) {
            Ok(Some(join)) => {
                let mut reader = super::fields::reader(std::slice::from_ref(join));
                super::numeric::reader(&mut reader, self.facts.numeric, self.subject(None), gaps);
                GrammarProperty::Known(Some(reader))
            }
            Ok(None) => GrammarProperty::Known(None),
            Err(stop) => {
                push(
                    gaps,
                    GapKind::UnresolvedReader,
                    self.subject(None),
                    format!("The weight block's bare value stopped at {}.", stop.reason),
                );
                GrammarProperty::Unresolved
            }
        }
    }

    fn fixed_keys(
        &self,
        grammar: &Grammar,
        words: &dyn Fn(u64) -> Option<u64>,
        gaps: &mut Vec<Gap>,
    ) -> Vec<Field> {
        let mut keys = super::fields::grammar_fields(
            &grammar.fields,
            &grammar.paths,
            self.facts.references,
            None,
        );
        for key in &mut keys {
            let subject = self.subject(Some(&key.name));
            let stopped = grammar
                .stops
                .iter()
                .any(|(stop, _)| stop.as_deref() == Some(key.name.as_str()));
            let nested = grammar.nested.get(&key.name);
            let entry = nested.map(|nested| (self.child(&key.name), nested));
            let entry_words =
                |offset: u64| nested.and_then(|nested| nested.words.get(&offset).copied());
            key.read_scope = match (grammar.key_scopes.get(&key.name), &entry) {
                // The stop's gap says why the key's scope is unknown.
                _ if stopped => GrammarProperty::Unresolved,
                // An entry read with no scope argument reads in the scopes of its own keys.
                (
                    None,
                    Some((
                        block,
                        Nested {
                            grammar: Ok(child), ..
                        },
                    )),
                ) => block.read_scope(child, &entry_words, gaps),
                (None, None) if key.shape.value == ValueShape::Scalar => {
                    GrammarProperty::Known(Vec::new())
                }
                (scope, _) => self.scope(grammar, scope, words, subject.clone(), gaps),
            };
            if let Some((block, nested)) = entry {
                let id =
                    super::fields::concrete_reader_id(&nested.reader.read, &nested.reader.member);
                let concrete = |reader: &mut Reader| {
                    reader.id = Some(id.clone());
                    reader.family = BlockFamily::Weight;
                };
                concrete(&mut key.reader);
                for alternative in &mut key.read {
                    if let FieldReadOutcome::Read { reader, .. } = &mut alternative.outcome {
                        concrete(reader);
                    }
                }
                key.members = match &nested.grammar {
                    Ok(child) => FieldMembers::WeightBlock(Box::new(block.normalize(
                        child,
                        &entry_words,
                        gaps,
                    ))),
                    Err(stop) => {
                        push(
                            gaps,
                            GapKind::UnresolvedReader,
                            subject.clone(),
                            format!("The weight entry analysis stopped at {}.", stop.reason),
                        );
                        FieldMembers::Unresolved
                    }
                };
            }

            let root = grammar.fields.iter().find(|field| field.name == key.name);
            agree_on_kind(key);
            if root.is_some_and(compares_inline) {
                clear_identity(key);
            }
            if key.reader.kind == ReaderKind::Keyword {
                match grammar.keyword_domains.get(&key.name) {
                    Some(names) => key.domain = FieldDomain::Listed(names.clone()),
                    None => push(
                        gaps,
                        GapKind::ReaderSemantics,
                        subject.clone(),
                        "The keyword domain is not established.".into(),
                    ),
                }
            }
            if root.is_some_and(looks_up_trigger) {
                key.reference = self.trigger_lookup(subject.clone(), gaps);
            }
            if key.reader.kind == ReaderKind::Unknown && !stopped {
                push(
                    gaps,
                    GapKind::UnresolvedReader,
                    subject,
                    "The weight key's read alternatives do not share one reader.".into(),
                );
            }
        }
        super::numeric::fields(&mut keys, self.facts.numeric, &self.path, gaps);
        keys
    }

    /// The trigger lookup of a key's value, from the reference method's trigger lookup fact. The
    /// map is keyed by the lexer's token number, so the key match stays unresolved.
    fn trigger_lookup(&self, subject: GapSubject, gaps: &mut Vec<Gap>) -> FieldReference {
        let lookup = self.facts.references.trigger_lookup.as_ref().ok();
        let (kind, detail) = match lookup {
            Some(_) => (GapKind::OutsideMethod, LEXER_MATCH),
            None => (
                GapKind::ReaderSemantics,
                "No lookup shape matched the trigger lookup.",
            ),
        };
        push(gaps, kind, subject, detail.into());

        FieldReference::Lookups(vec![ReferenceLookup {
            target: ReferenceTarget::Triggers,
            ..super::fields::reference_lookup(FieldCondition::Always, None, lookup)
        }])
    }

    /// The block of the nested entry at `key`.
    fn child(&self, key: &str) -> Block<'_> {
        let mut path = self.path.clone();
        path.push(key.to_owned());

        Block {
            facts: self.facts,
            path,
        }
    }

    fn operand(
        &self,
        operand: &Operand,
        key: &str,
        words: &dyn Fn(u64) -> Option<u64>,
        gaps: &mut Vec<Gap>,
    ) -> Reader {
        let join = ReaderJoin::Joined {
            callee: operand.callee.clone(),
            arguments: Default::default(),
            tail: false,
        };
        let mut reader = super::fields::reader(std::slice::from_ref(&join));
        let subject = self.subject(Some(key));
        super::numeric::reader(&mut reader, self.facts.numeric, subject.clone(), gaps);
        let point = operand.point.or_else(|| {
            operand
                .destination
                .and_then(|offset| words(u64::try_from(offset).ok()?))
        });
        super::scoped_numeric::attach(
            &mut reader,
            point,
            self.facts.scoped,
            self.facts.numeric,
            &subject,
            gaps,
        );
        reader
    }
}

/// Why a trigger lookup's key match is outside the method.
const LEXER_MATCH: &str =
    "The name is matched by its lexer token; how the lexer matches a name is outside the method.";

/// Why the read scope of a block that a weight entry reads with scope mask 0 is outside the
/// method (decision D5a, a recorded exception in `docs/design/simplification.md`).
const ZERO_MASK: &str = "Read with scope mask 0; the engine checks the scope later, when it \
                         validates the block, which the method does not report.";

/// Give the key the value kind that every read alternative shares, when its readers differ but
/// agree on the kind, such as a fixed-point value read either way. The reader identity stays
/// unknown.
fn agree_on_kind(key: &mut Field) {
    if key.reader.kind != ReaderKind::Unknown {
        return;
    }

    let kinds: BTreeSet<_> = key
        .read
        .iter()
        .map(|alternative| match &alternative.outcome {
            FieldReadOutcome::Read { reader, .. } => Some(reader.kind),
            _ => None,
        })
        .collect();
    if let Ok([Some(kind)]) = <[_; 1]>::try_from(kinds.into_iter().collect::<Vec<_>>())
        && kind != ReaderKind::Unknown
    {
        key.reader.kind = kind;
    }
}

/// Whether the member reader itself compares the key's value with fixed names, so no shared
/// reader reads it.
fn compares_inline(field: &RootField) -> bool {
    field.readers.iter().any(|join| {
        matches!(join, ReaderJoin::Stored { callee, kind: ReaderKind::Keyword, .. } if readers::is_member(callee))
    })
}

/// Remove the reader identity of a key that no shared reader reads.
fn clear_identity(key: &mut Field) {
    key.reader.id = None;
    for alternative in &mut key.read {
        if let FieldReadOutcome::Read { reader, .. } = &mut alternative.outcome {
            reader.id = None;
        }
    }
}

fn looks_up_trigger(field: &RootField) -> bool {
    field.readers.iter().any(|join| {
        matches!(join, ReaderJoin::Stored { callee, .. } if callee == readers::TRIGGER_LOOKUP)
    })
}

/// Whether `source` is a key whose value names the trigger that the engine looks up and stores,
/// so a value that the stored object reads is read by that trigger (decision D3).
fn reads_trigger(grammar: &Grammar, source: &str) -> bool {
    grammar
        .fields
        .iter()
        .any(|field| field.name == source && looks_up_trigger(field))
}

/// The scope arguments of the block's keys and conditions; `None` is a trigger condition whose
/// scope argument is not established.
fn scope_arguments(grammar: &Grammar) -> impl Iterator<Item = Option<&Value>> {
    let conditions = match &grammar.other_keys {
        OtherKeys::Triggers(scope) => Some(scope.as_ref()),
        _ => None,
    };
    grammar.key_scopes.values().map(Some).chain(conditions)
}

/// The scope argument with a stored scope replaced by the word that the object holds.
fn resolve(scope: &Value, words: &dyn Fn(u64) -> Option<u64>) -> Option<Value> {
    match scope {
        Value::Load(base, 8) => match **base {
            Value::Owner(offset) => {
                let word = words(u64::try_from(offset).ok()?)?;
                Some(Value::Constant(i64::from(word as u32)))
            }
            _ => None,
        },
        Value::EnclosingScope | Value::Constant(_) => Some(scope.clone()),
        _ => None,
    }
}

fn push(gaps: &mut Vec<Gap>, kind: GapKind, subject: GapSubject, detail: String) {
    let gap = Gap {
        kind,
        subject: Some(subject),
        detail,
    };
    if !gaps.contains(&gap) {
        gaps.push(gap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FieldReadAlternative, FieldShape, ReaderId, RepeatBehavior};

    fn reader(kind: ReaderKind, callee: &str) -> Reader {
        Reader {
            numeric: GrammarProperty::Unresolved,
            scoped_operand: GrammarProperty::Unresolved,
            id: Some(ReaderId::from_callee(callee)),
            kind,
            family: BlockFamily::NotApplicable,
        }
    }

    /// A scalar key read by each of `readers` on its own path.
    fn key(readers: Vec<Reader>) -> Field {
        let shape = FieldShape {
            value: ValueShape::Scalar,
            repeat: RepeatBehavior::Replace,
        };
        Field {
            name: "factor".into(),
            reader: Reader {
                id: None,
                kind: ReaderKind::Unknown,
                ..readers[0].clone()
            },
            shape,
            read: readers
                .into_iter()
                .map(|reader| FieldReadAlternative {
                    condition: FieldCondition::Unresolved,
                    outcome: FieldReadOutcome::Read { reader, shape },
                })
                .collect(),
            members: FieldMembers::None,
            domain: FieldDomain::Unknown,
            uses: Vec::new(),
            entry_contexts: Vec::new(),
            read_scope: GrammarProperty::Known(Vec::new()),
            accepted_categories: crate::AcceptedCategories::NotApplicable,
            reference: FieldReference::NotEstablished,
        }
    }

    #[test]
    fn readers_that_agree_on_the_kind_give_the_key_that_kind_without_an_identity() {
        let mut factor = key(vec![
            reader(ReaderKind::FixedPoint, "CReader::Read(CFixedPoint&)"),
            reader(ReaderKind::FixedPoint, "CToken::GetFloat() const"),
        ]);

        agree_on_kind(&mut factor);

        assert_eq!(factor.reader.kind, ReaderKind::FixedPoint);
        assert_eq!(factor.reader.id, None);
    }

    #[test]
    fn readers_of_different_kinds_leave_the_key_unknown() {
        let mut mixed = key(vec![
            reader(ReaderKind::FixedPoint, "CReader::Read(CFixedPoint&)"),
            reader(ReaderKind::Integer, "CToken::GetInt() const"),
        ]);

        agree_on_kind(&mut mixed);

        assert_eq!(mixed.reader.kind, ReaderKind::Unknown);
    }

    #[test]
    fn an_unresolved_alternative_leaves_the_key_unknown() {
        let mut partial = key(vec![reader(
            ReaderKind::FixedPoint,
            "CReader::Read(CFixedPoint&)",
        )]);
        partial.read.push(FieldReadAlternative {
            condition: FieldCondition::Unresolved,
            outcome: FieldReadOutcome::Unresolved,
        });

        agree_on_kind(&mut partial);

        assert_eq!(partial.reader.kind, ReaderKind::Unknown);
    }

    #[test]
    fn a_keyword_compared_inside_the_member_reader_has_no_reader_identity() {
        let stored = |callee: &str| RootField {
            name: "calc".into(),
            token: 41,
            constructor: 0,
            paths: Vec::new(),
            readers: vec![ReaderJoin::Stored {
                callee: callee.into(),
                kind: ReaderKind::Keyword,
                destination: 0x298,
                repeat: crate::RepeatBehavior::Replace,
            }],
        };

        assert!(compares_inline(&stored(
            "Modifier::ReadMember(CReader&, int)"
        )));
        assert!(!compares_inline(&stored(
            "EScriptMaths TokenToEnum<EScriptMaths>(int const&)"
        )));
    }
}
