//! Recursive coverage of the internal member ledgers, before public field normalization.
use super::{GrammarResult, fields};
use crate::engine::analysis::{readers, stop::Unresolved};
use crate::{BlockFamily, ReaderKind};
use std::collections::{BTreeMap, BTreeSet};

/// One outcome for a token interval on a member path.
#[derive(Clone, Debug)]
pub enum Disposition {
    Rejected,
    Field {
        name: String,
    },
    Family,
    Numeric,
    Delegated(usize),
    Dynamic,
    Gap {
        name: Option<String>,
        stop: Unresolved,
    },
}

/// An inclusive interval and its disposition on this path.
#[derive(Clone, Debug)]
pub struct LedgerEntry {
    pub domain: [i64; 2],
    pub disposition: Disposition,
}

/// The local obligations of one root or delegated member reader.
#[derive(Clone, Debug)]
pub struct ReaderNode {
    pub domain: [i64; 2],
    pub ledger: Vec<LedgerEntry>,
    pub stops: Vec<Unresolved>,
    pub table_gaps: usize,
}

impl ReaderNode {
    pub(super) fn new(domain: [i64; 2]) -> Self {
        Self {
            domain,
            ledger: vec![],
            stops: vec![],
            table_gaps: 0,
        }
    }

    pub(super) fn gap(
        domain: [i64; 2],
        tokens: &BTreeMap<i64, fields::Token>,
        stop: Unresolved,
    ) -> Disposition {
        let name = tokens
            .get(&domain[0])
            .filter(|token| !token.ambiguous && domain[0] == domain[1])
            .map(|token| token.name.clone());
        Disposition::Gap { name, stop }
    }

    pub(super) fn record(
        &mut self,
        path: &fields::TokenPath,
        tokens: &BTreeMap<i64, fields::Token>,
    ) {
        use fields::{PathOutcome, ReaderJoin};
        let disposition = match &path.outcome {
            PathOutcome::Rejected => Disposition::Rejected,
            PathOutcome::Gap(stop) | PathOutcome::Reader(ReaderJoin::Missing(stop)) => {
                Self::gap(path.domain, tokens, stop.clone())
            }
            PathOutcome::Reader(_) => {
                match tokens
                    .get(&path.domain[0])
                    .filter(|token| !token.ambiguous && path.domain[0] == path.domain[1])
                {
                    Some(token) => Disposition::Field {
                        name: token.name.clone(),
                    },
                    None => Disposition::Dynamic,
                }
            }
        };
        self.ledger.push(LedgerEntry {
            domain: path.domain,
            disposition,
        });
    }
}

/// Coverage failures, each relative to the current command's named-key tree.
#[derive(Default, Debug)]
pub struct Coverage {
    pub gaps: Vec<(Vec<String>, Unresolved)>,
}
impl Coverage {
    pub fn covered(&self) -> bool {
        self.gaps.is_empty()
    }
}

impl GrammarResult {
    /// Whether the outer reader proves that the inherited member tree is unreachable.
    pub fn value_only(&self) -> bool {
        self.forms
            .as_ref()
            .is_some_and(|forms| forms.complete && !forms.block)
    }

    /// The single recursive rule for the internal member tree.
    pub fn coverage(&self) -> Coverage {
        let mut result = Coverage::default();
        visit(self, 0, &[], &mut BTreeSet::new(), &mut result);
        result
            .gaps
            .sort_by(|a, b| a.0.cmp(&b.0).then(a.1.reason.cmp(b.1.reason)));
        result.gaps.dedup();
        result
    }
}

fn visit(
    grammar: &GrammarResult,
    index: usize,
    path: &[String],
    active: &mut BTreeSet<usize>,
    coverage: &mut Coverage,
) {
    let Some(node) = grammar.nodes.get(index) else {
        coverage
            .gaps
            .push((path.to_vec(), Unresolved::new("member-ledger-missing")));
        return;
    };
    if !active.insert(index) {
        coverage
            .gaps
            .push((path.to_vec(), Unresolved::new("grammar-delegation-limit")));
        return;
    }
    if !tiles(node) {
        coverage
            .gaps
            .push((path.to_vec(), Unresolved::new("member-ledger-partition")));
    }
    if node.table_gaps != 0 {
        coverage
            .gaps
            .push((path.to_vec(), Unresolved::new("member-table-gap")));
    }
    coverage
        .gaps
        .extend(node.stops.iter().cloned().map(|stop| (path.to_vec(), stop)));
    let mut classifications = BTreeMap::new();
    for entry in &node.ledger {
        match &entry.disposition {
            Disposition::Rejected | Disposition::Family => {}
            Disposition::Gap { name, stop } => {
                let mut key = path.to_vec();
                key.extend(name.iter().cloned());
                coverage.gaps.push((key, stop.clone()));
            }
            Disposition::Dynamic => coverage
                .gaps
                .push((path.to_vec(), Unresolved::new("member-ledger-gap"))),
            Disposition::Delegated(child) => visit(grammar, *child, path, active, coverage),
            Disposition::Numeric => match &grammar.numeric {
                Some(child) => {
                    // Numeric children have no named path component. Their gaps retain the
                    // parent answer subject used by numeric-child normalization.
                    for (_, stop) in child.coverage().gaps {
                        coverage.gaps.push((vec![], stop));
                    }
                }
                None => coverage
                    .gaps
                    .push((path.to_vec(), Unresolved::new("numeric-child-missing"))),
            },
            Disposition::Field { name } => {
                let classification = classifications.entry(name).or_insert_with(|| {
                    let joins = grammar
                        .fields
                        .fields
                        .iter()
                        .find(|field| field.name == *name)
                        .map_or(&[][..], |field| field.readers.as_slice());
                    readers::classify(joins)
                });
                let kind = classification.kind;
                let family = classification.family;
                let mut key = path.to_vec();
                key.push(name.clone());
                if kind == ReaderKind::Unknown {
                    coverage
                        .gaps
                        .push((key, Unresolved::new("unknown-key-reader")));
                } else if kind == ReaderKind::Block && family == BlockFamily::Unknown {
                    match grammar.nested.get(name) {
                        Some(child) => visit(child, 0, &key, &mut BTreeSet::new(), coverage),
                        None => coverage.gaps.push((
                            key,
                            grammar
                                .nested_stops
                                .get(name)
                                .cloned()
                                .unwrap_or_else(|| Unresolved::new("nested-member-missing")),
                        )),
                    }
                }
            }
        }
    }
    active.remove(&index);
}

/// Conditional paths may overlap; their union must cover the node's whole input range.
fn tiles(node: &ReaderNode) -> bool {
    let mut intervals: Vec<_> = node.ledger.iter().map(|entry| entry.domain).collect();
    intervals.sort();
    let mut cursor = node.domain[0];
    for [low, high] in intervals {
        if low > cursor || high < low || low < node.domain[0] || high > node.domain[1] {
            return false;
        }
        cursor = cursor.max(high + 1);
    }
    cursor == node.domain[1] + 1
}
