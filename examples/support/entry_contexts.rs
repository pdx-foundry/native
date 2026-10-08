//! Root trigger, effect and weight blocks by the entry contexts of their answer, with the gaps
//! that entry contexts add. The counts are the ones that `docs/native/registry-fields.md` records.
use pdx_native::internals::registry_field_stops::EntryContexts;
use pdx_native::{Answer, EntryContext, EntryScope, Field, GapSubject};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Tally {
    blocks: usize,
    registries: usize,
    counts: BTreeMap<&'static str, usize>,
    statuses_by_family: BTreeMap<String, BTreeMap<&'static str, usize>>,
    gaps: BTreeMap<String, usize>,
    unnamed_evaluations: BTreeMap<String, usize>,
}

impl Tally {
    pub fn add(&mut self, registry: &str, answer: &Answer<Vec<Field>>, entries: &EntryContexts) {
        let blocks: Vec<&Field> = answer
            .value
            .iter()
            .filter(|field| entries.block_offsets.contains_key(&field.name))
            .collect();
        if !blocks.is_empty() {
            self.registries += 1;
        }
        if entries.unnamed_evaluations > 0 {
            self.unnamed_evaluations
                .insert(registry.to_owned(), entries.unnamed_evaluations);
        }

        for field in blocks {
            let subject = Some(GapSubject::Field {
                name: field.name.clone(),
            });
            let gaps: Vec<_> = entries
                .gaps
                .iter()
                .filter(|gap| gap.subject == subject)
                .collect();
            let contexts = &field.entry_contexts;

            self.blocks += 1;
            let status = match (contexts.is_empty(), gaps.is_empty()) {
                (true, _) => "without_contexts",
                (false, true) => "contexts_without_gap",
                (false, false) => "contexts_with_gap",
            };
            self.count(status, true);
            *self
                .statuses_by_family
                .entry(format!("{:?}", field.reader.family))
                .or_default()
                .entry(status)
                .or_default() += 1;
            let known_this = contexts
                .iter()
                .filter(|context| context.this != EntryScope::Unresolved)
                .count();
            self.count("several_known_this", known_this > 1);
            self.count("typed_from", contexts.iter().any(|c| has_scope(&c.from)));
            self.count("typed_prev", contexts.iter().any(|c| has_scope(&c.prev)));
            self.count(
                "with_unresolved_slot",
                contexts.iter().any(has_unresolved_slot),
            );
            for gap in gaps {
                *self
                    .gaps
                    .entry(format!("{:?}: {}", gap.kind, gap.detail))
                    .or_default() += 1;
            }
        }
    }

    fn count(&mut self, name: &'static str, applies: bool) {
        *self.counts.entry(name).or_default() += usize::from(applies);
    }

    pub fn report(self) -> Value {
        json!({
            "blocks": self.blocks,
            "registries_with_blocks": self.registries,
            "counts": self.counts,
            "statuses_by_family": self.statuses_by_family,
            "gaps": self.gaps,
            "unnamed_evaluations": self.unnamed_evaluations,
        })
    }
}

fn has_scope(chain: &[EntryScope]) -> bool {
    chain
        .iter()
        .any(|scope| matches!(scope, EntryScope::Scope(_)))
}

fn has_unresolved_slot(context: &EntryContext) -> bool {
    [&context.this, &context.root]
        .into_iter()
        .chain(&context.from)
        .chain(&context.prev)
        .any(|scope| *scope == EntryScope::Unresolved)
}
