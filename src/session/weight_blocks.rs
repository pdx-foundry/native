//! Attach shared weight grammar only where the field constructor proved its address point.
use crate::engine::analysis::{
    fields::{ReaderJoin, RegistryFieldResult, Value},
    numeric::NumericFacts,
    readers,
    references::ReferenceFacts,
    scoped_numeric::Facts as ScopedFacts,
    weight_blocks::{Grammar, Operand, OtherKeys, WeightBlockFacts},
};
use crate::{
    BlockFamily, Field, FieldMembers, FieldReadOutcome, Gap, GapKind, GapSubject, GrammarProperty,
    ReadScope, Reader, ReaderKind, ValueShape, WeightBlock, WeightOperation, WeightOtherKeys,
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
        let arguments: BTreeSet<_> = scope_arguments(grammar)
            .map(|scope| scope.and_then(|scope| resolve(scope, words)))
            .collect();
        if arguments.is_empty() {
            return GrammarProperty::Known(Vec::new());
        }
        super::read_scope::normalize_scopes(
            arguments.into_iter().collect(),
            self.facts.scope_names,
            self.subject(None),
            gaps,
        )
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
            scope => super::read_scope::normalize_scopes(
                vec![scope.and_then(|scope| resolve(scope, words))],
                self.facts.scope_names,
                subject,
                gaps,
            ),
        }
    }

    fn normalize(
        &self,
        grammar: &Grammar,
        words: &dyn Fn(u64) -> Option<u64>,
        gaps: &mut Vec<Gap>,
    ) -> WeightBlock {
        for (key, stop) in &grammar.stops {
            let kind = if key.is_some() {
                GapKind::UnresolvedReader
            } else {
                GapKind::UnresolvedPath
            };
            push(
                gaps,
                kind,
                self.subject(key.as_deref()),
                format!("The weight block analysis stopped at {}.", stop.reason),
            );
        }

        let keys = self.fixed_keys(grammar, words, gaps);
        let key_stops = grammar.stops.iter().any(|(key, _)| key.is_some());
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
            key.read_scope = match grammar.key_scopes.get(&key.name) {
                None if key.shape.value == ValueShape::Scalar => GrammarProperty::Known(Vec::new()),
                scope => self.scope(grammar, scope, words, subject.clone(), gaps),
            };
            if let Some(nested) = grammar.nested.get(&key.name) {
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
                    Ok(child) => {
                        let mut path = self.path.clone();
                        path.push(key.name.clone());
                        let block = Block {
                            facts: self.facts,
                            path,
                        };
                        let entry_words = |offset: u64| nested.words.get(&offset).copied();
                        FieldMembers::WeightBlock(Box::new(block.normalize(
                            child,
                            &entry_words,
                            gaps,
                        )))
                    }
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
            if key.reader.kind == ReaderKind::Unknown {
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
