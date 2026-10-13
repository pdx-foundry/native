//! Constructor-bound weight variants, by reader identity, with the status and gaps of each field.
use super::{applies, by_depth, flattened};
use pdx_native::{
    Answer, BlockFamily, Field, FieldMembers, GrammarProperty, WeightBlock, WeightOtherKeys,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Tally {
    variants: BTreeMap<String, Variant>,
    /// The fields below a root field.
    nested: BTreeSet<String>,
}

#[derive(Default)]
struct Variant {
    fields: BTreeMap<String, &'static str>,
    failure_shapes: BTreeMap<String, usize>,
}

/// Complete when every property of the block and of each nested entry is known.
fn established(block: &WeightBlock) -> bool {
    let GrammarProperty::Known(keys) = &block.fixed_keys else {
        return false;
    };
    let nested = keys.iter().all(|key| match &key.members {
        FieldMembers::WeightBlock(entry) => established(entry),
        FieldMembers::Unresolved => false,
        _ => true,
    });
    nested
        && matches!(block.scalar, GrammarProperty::Known(_))
        && matches!(block.operations, GrammarProperty::Known(_))
        && !matches!(block.other_keys, WeightOtherKeys::Unresolved)
}

impl Tally {
    pub fn add(
        &mut self,
        registry: &str,
        answer: &Answer<Vec<Field>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (path, field) in flattened(&answer.value)
            .into_iter()
            .filter(|(_, field)| field.reader.family == BlockFamily::Weight)
        {
            let id = serde_json::to_value(&field.reader.id)?
                .as_str()
                .ok_or("weight field has no identity")?
                .to_owned();
            let gaps: Vec<_> = answer
                .gaps
                .iter()
                .filter(|gap| applies(&gap.subject, &path))
                .collect();
            let status = match &field.members {
                FieldMembers::WeightBlock(block) if established(block) && gaps.is_empty() => {
                    "complete"
                }
                FieldMembers::WeightBlock(_) => "partial",
                _ => "failed",
            };
            let variant = self.variants.entry(id).or_default();
            let name = format!("{registry}#{}", path.join("."));
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
        json!({"fields_by_status": counts, "fields_by_depth": depths, "variants": variants})
    }
}
