//! Triggered modifier clause variants, by reader identity, with the status and gaps of each field,
//! and the triggered-named fields that no clause reader is bound to.
use super::{applies, by_depth, flattened};
use pdx_native::{
    Answer, BlockFamily, Field, FieldMembers, GrammarProperty, ModifierBlock,
    TriggeredModifierBlock,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Tally {
    variants: BTreeMap<String, Variant>,
    unbound: BTreeSet<String>,
    /// The fields below a root field.
    nested: BTreeSet<String>,
}

#[derive(Default)]
struct Variant {
    fields: BTreeMap<String, &'static str>,
    failure_shapes: BTreeMap<String, usize>,
}

fn modifier_established(block: &ModifierBlock) -> bool {
    matches!(block.fixed_keys, GrammarProperty::Known(_))
        && matches!(block.entries, GrammarProperty::Known(_))
}

/// Complete when the clause's keys, its other keys and each embedded modifier block are known.
fn established(block: &TriggeredModifierBlock) -> bool {
    let GrammarProperty::Known(keys) = &block.fixed_keys else {
        return false;
    };
    let GrammarProperty::Known(other_keys) = &block.other_keys else {
        return false;
    };
    let nested = keys.iter().all(|key| match &key.members {
        FieldMembers::ModifierBlock(modifier) => modifier_established(modifier),
        FieldMembers::Unresolved => false,
        _ => true,
    });

    nested && modifier_established(&other_keys.block)
}

impl Tally {
    pub fn add(
        &mut self,
        registry: &str,
        answer: &Answer<Vec<Field>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (path, field) in flattened(&answer.value) {
            let name = format!("{registry}#{}", path.join("."));

            if field.reader.family != BlockFamily::TriggeredModifier {
                if field.name.contains("triggered") {
                    self.unbound.insert(name);
                }
                continue;
            }

            let id = serde_json::to_value(&field.reader.id)?
                .as_str()
                .ok_or("triggered modifier field has no identity")?
                .to_owned();
            let gaps: Vec<_> = answer
                .gaps
                .iter()
                .filter(|gap| applies(&gap.subject, &path))
                .collect();
            let status = match &field.members {
                FieldMembers::TriggeredModifier(block) if established(block) && gaps.is_empty() => {
                    "complete"
                }
                FieldMembers::TriggeredModifier(_) => "partial",
                _ => "failed",
            };
            let variant = self.variants.entry(id).or_default();
            if path.len() > 1 {
                self.nested.insert(name.clone());
            }
            variant.fields.insert(name, status);

            for gap in gaps {
                *variant
                    .failure_shapes
                    .entry(format!("{:?}: {}", gap.kind, gap.detail))
                    .or_default() += 1;
            }
        }

        Ok(())
    }

    pub fn report(self) -> Value {
        let mut counts = BTreeMap::<&str, usize>::new();
        let depths = by_depth(
            self.variants
                .values()
                .flat_map(|variant| variant.fields.iter())
                .map(|(name, status)| (name, *status)),
            &self.nested,
        );
        let variants: BTreeMap<_, _> = self
            .variants
            .into_iter()
            .map(|(id, variant)| {
                for status in variant.fields.values() {
                    *counts.entry(status).or_default() += 1;
                }
                let report = json!({
                    "fields": variant.fields,
                    "failure_shapes": variant.failure_shapes,
                });
                (id, report)
            })
            .collect();

        json!({
            "fields_by_status": counts,
            "fields_by_depth": depths,
            "variants": variants,
            "unbound_triggered_named_fields": self.unbound,
        })
    }
}
