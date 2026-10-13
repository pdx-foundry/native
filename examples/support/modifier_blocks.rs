//! Constructor-bound modifier variants and the uses left outside that join.
use super::{applies, by_depth, flattened};
use pdx_native::internals::registry_field_stops::{
    ReaderJoin, RegistryFieldResult, Value as Argument,
};
use pdx_native::{
    Answer, BlockFamily, Field, FieldMembers, GrammarProperty, ModifierBlock, ReaderKind,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Tally {
    variants: BTreeMap<String, Variant>,
    failed_persistent_fields: BTreeSet<String>,
    /// The fields below a root field.
    nested: BTreeSet<String>,
}

struct Variant {
    block: Option<ModifierBlock>,
    fields: Vec<String>,
    points: BTreeSet<u64>,
    failure_shapes: BTreeMap<String, usize>,
}

pub fn children(members: &FieldMembers) -> Option<&[Field]> {
    match members {
        FieldMembers::Fields(fields) => Some(fields),
        FieldMembers::ModifierBlock(block) => match &block.fixed_keys {
            GrammarProperty::Known(fields) | GrammarProperty::Partial(fields) => Some(fields),
            _ => None,
        },
        FieldMembers::TriggeredModifier(clause) => match &clause.fixed_keys {
            GrammarProperty::Known(fields) | GrammarProperty::Partial(fields) => Some(fields),
            _ => None,
        },
        _ => None,
    }
}

/// The result of the owner that reads the field at `path`: the registry's own result for a root
/// field, or the result of the object that the enclosing field collects or embeds.
fn owner_result<'a>(
    result: &'a RegistryFieldResult,
    path: &[String],
) -> Option<&'a RegistryFieldResult> {
    let Some((enclosing, rest)) = path.split_first().filter(|(_, rest)| !rest.is_empty()) else {
        return Some(result);
    };
    let token = result
        .fields
        .iter()
        .find(|root| root.name == *enclosing)?
        .token;
    let nested = result
        .collections
        .iter()
        .find(|collection| collection.token == token)
        .map(|collection| collection.fields.as_ref())
        .or_else(|| {
            result
                .embedded
                .iter()
                .find(|embedded| embedded.token == token)
                .map(|embedded| embedded.fields.as_ref())
        })?;

    owner_result(nested, rest)
}

impl Tally {
    pub fn add(
        &mut self,
        registry: &str,
        answer: &Answer<Vec<Field>>,
        result: &RegistryFieldResult,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (path, field) in flattened(&answer.value) {
            let name = format!("{registry}#{}", path.join("."));
            let Some(result) = owner_result(result, &path) else {
                continue;
            };
            let persistent_failed = result
                .fields
                .iter()
                .filter(|root| root.name == field.name)
                .flat_map(|root| &root.readers)
                .any(|join| {
                    matches!(join, ReaderJoin::Joined { callee, arguments, .. }
                    if callee == "CReader::Read(CPersistent&)" && match arguments.get("x1") {
                        Some(Argument::Owner(offset)) => !result.persistent.contains_key(offset),
                        _ => true,
                    })
                });
            if path.len() == 1 && field.reader.kind == ReaderKind::Block && persistent_failed {
                self.failed_persistent_fields.insert(name.clone());
            }
            if field.reader.family != BlockFamily::Modifier {
                continue;
            }
            let id = serde_json::to_value(&field.reader.id)?
                .as_str()
                .ok_or("modifier field has no identity")?
                .to_owned();
            let block = match &field.members {
                FieldMembers::ModifierBlock(block) => Some(block.clone()),
                _ => None,
            };
            let variant = self.variants.entry(id).or_insert_with(|| Variant {
                block: block.clone(),
                fields: vec![],
                points: BTreeSet::new(),
                failure_shapes: BTreeMap::new(),
            });
            if variant.block != block {
                return Err(format!("modifier grammar differs for {name}").into());
            }
            if path.len() > 1 {
                self.nested.insert(name.clone());
            }
            variant.fields.push(name);
            // A persistent reader's x1 is the destination whose constructor established the point.
            for join in result
                .fields
                .iter()
                .filter(|root| root.name == field.name)
                .flat_map(|root| &root.readers)
            {
                if let ReaderJoin::Joined { arguments, .. } = join
                    && let Some(Argument::Owner(offset)) = arguments.get("x1")
                    && let Some(point) = result.persistent_points.get(offset)
                {
                    variant.points.insert(*point);
                }
            }
            for gap in answer
                .gaps
                .iter()
                .filter(|gap| applies(&gap.subject, &path))
            {
                *variant
                    .failure_shapes
                    .entry(format!("{:?}: {}", gap.kind, gap.detail))
                    .or_default() += 1;
            }
        }
        Ok(())
    }

    pub fn report(self) -> Value {
        let keys = |variant: &Variant| -> BTreeSet<String> {
            match variant.block.as_ref().map(|block| &block.fixed_keys) {
                Some(GrammarProperty::Known(keys) | GrammarProperty::Partial(keys)) => {
                    keys.iter().map(|field| field.name.clone()).collect()
                }
                _ => BTreeSet::new(),
            }
        };
        let smallest = self
            .variants
            .values()
            .filter(|variant| variant.block.is_some())
            .map(keys)
            .min_by_key(BTreeSet::len)
            .unwrap_or_default();
        let status = |variant: &Variant| match &variant.block {
            None => "failed",
            Some(block)
                if matches!(block.fixed_keys, GrammarProperty::Known(_))
                    && matches!(block.entries, GrammarProperty::Known(_)) =>
            {
                "complete"
            }
            _ => "partial",
        };
        let depths = by_depth(
            self.variants
                .values()
                .flat_map(|variant| variant.fields.iter().map(|name| (name, status(variant)))),
            &self.nested,
        );
        let mut counts = BTreeMap::<&str, usize>::new();
        let variants: BTreeMap<_, _> = self.variants.into_iter().map(|(id, variant)| {
            let status = status(&variant);
            *counts.entry(status).or_default() += variant.fields.len();
            let names = keys(&variant);
            let added: Vec<_> = names.difference(&smallest).collect();
            (id, json!({"fields":variant.fields,"vtable_points":variant.points.len(),
                "status":status,"keys":names,"added_keys":added,"failure_shapes":variant.failure_shapes}))
        }).collect();
        json!({"fields_by_status":counts,"fields_by_depth":depths,"variants":variants,
            "failed_persistent_fields":self.failed_persistent_fields,
            "failed_persistent_count":self.failed_persistent_fields.len()})
    }
}
