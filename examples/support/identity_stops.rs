//! Fields without a shared reader, grouped by where their read alternatives stopped.
use pdx_native::internals::inspect::{Image, Note};
use pdx_native::internals::registry_field_stops::{ReaderJoin, RegistryFieldResult, RootField};
use pdx_native::{Field, FieldMembers};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Shared persistent read of an embedded object; its identity comes from the destination.
const PERSISTENT_READ: &str = "CReader::Read(CPersistent&)";

#[derive(Default)]
pub struct Tally {
    /// Fields by shape, each as `registry#path`.
    shapes: BTreeMap<String, Vec<String>>,
}

impl Tally {
    /// Add each root and nested field of `registry` that has no reader identity, by the shape of
    /// its alternatives in `result`.
    pub fn add(
        &mut self,
        image: &Image,
        registry: &str,
        fields: &[Field],
        result: &RegistryFieldResult,
    ) {
        self.add_level(image, registry, "", fields, result);
    }

    fn add_level(
        &mut self,
        image: &Image,
        registry: &str,
        parent: &str,
        fields: &[Field],
        result: &RegistryFieldResult,
    ) {
        for field in fields {
            let path = format!("{parent}{}", field.name);
            let loader_keys = result.loader.iter().flat_map(|loader| &loader.keys);
            let root = result
                .fields
                .iter()
                .chain(loader_keys)
                .find(|root| root.name == field.name);
            if let (FieldMembers::Fields(children), Some(root)) = (&field.members, root)
                && let Some(nested) = nested_result(result, root)
            {
                self.add_level(image, registry, &format!("{path}."), children, nested);
            }
            if field.reader.id.is_some() {
                continue;
            }

            let shape = root.map_or_else(|| "no root field".to_owned(), |root| shape(image, root));
            self.shapes
                .entry(shape)
                .or_default()
                .push(format!("{registry}#{path}"));
        }
    }

    /// Each shape with its field count, the registries that hold it and its fields.
    pub fn report(&self) -> Value {
        let mut shapes: Vec<_> = self.shapes.iter().collect();
        shapes.sort_by_key(|(shape, fields)| (std::cmp::Reverse(fields.len()), *shape));
        let fields: usize = self.shapes.values().map(Vec::len).sum();
        let registries: BTreeSet<_> = self
            .shapes
            .values()
            .flatten()
            .map(|field| registry_of(field))
            .collect();

        json!({
            "fields": fields,
            "registries": registries.len(),
            "shapes": shapes
                .into_iter()
                .map(|(shape, fields)| {
                    let registries: BTreeSet<_> = fields.iter().map(|field| registry_of(field)).collect();
                    json!({
                        "shape": shape,
                        "fields": fields.len(),
                        "registries": registries.len(),
                        "members": fields,
                    })
                })
                .collect::<Vec<_>>(),
        })
    }
}

fn registry_of(field: &str) -> &str {
    field
        .split_once('#')
        .map_or(field, |(registry, _)| registry)
}

/// The result that holds the members of `root`: its collected or embedded object, or the owner
/// itself for a key of a loader root, whose members are the owner's own fields.
fn nested_result<'r>(
    result: &'r RegistryFieldResult,
    root: &RootField,
) -> Option<&'r RegistryFieldResult> {
    let collected = result
        .collections
        .iter()
        .find(|collection| collection.token == root.token)
        .map(|collection| collection.fields.as_ref());
    let embedded = result
        .embedded
        .iter()
        .find(|embedded| embedded.token == root.token)
        .map(|embedded| embedded.fields.as_ref());
    let loader_key = result
        .loader
        .as_ref()
        .is_some_and(|loader| loader.keys.contains(root));

    collected.or(embedded).or(loader_key.then_some(result))
}

/// Every distinct alternative of `root`, in order. A stopped one names its reason and, when it
/// stopped at a call, the callee; a joined one names why the joins share no identity.
fn shape(image: &Image, root: &RootField) -> String {
    let callees: BTreeSet<_> = root
        .readers
        .iter()
        .filter_map(|join| match join {
            ReaderJoin::Joined { callee, .. } | ReaderJoin::Stored { callee, .. } => Some(callee),
            ReaderJoin::Missing(_) => None,
        })
        .collect();
    let mut alternatives: BTreeSet<String> = root
        .readers
        .iter()
        .filter_map(|join| match join {
            ReaderJoin::Missing(unresolved) => Some(match unresolved.stop {
                Some(stop) => format!(
                    "{}: {}",
                    unresolved.reason,
                    stopped_at(image, stop.instruction)
                ),
                None => unresolved.reason.to_owned(),
            }),
            _ => None,
        })
        .collect();
    if callees.len() > 1 {
        alternatives.insert("joins different readers".into());
    } else if callees
        .first()
        .is_some_and(|callee| *callee == PERSISTENT_READ)
    {
        alternatives.insert("persistent read with no constructor-agreed reader".into());
    }
    if alternatives.is_empty() {
        alternatives.insert("joined".into());
    }

    alternatives.into_iter().collect::<Vec<_>>().join(" || ")
}

/// The callee of a direct call at `instruction`, or the instruction's mnemonic.
fn stopped_at(image: &Image, instruction: u64) -> String {
    let Some(row) = image
        .disassemble(instruction, 4)
        .ok()
        .and_then(|listing| listing.rows.into_iter().next())
    else {
        return "outside the text section".into();
    };
    let callee = row.notes.iter().find_map(|note| match note {
        Note::Target { place, .. }
            if matches!(row.operation.as_str(), "b" | "bl") && !place.contains('+') =>
        {
            Some(place.clone())
        }
        _ => None,
    });

    match callee {
        Some(callee) => format!("{} {callee}", row.operation),
        None => row.operation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_counts_each_field_and_each_registry_once() {
        let mut tally = Tally::default();
        for (shape, field) in [
            ("reader-routing: bl A", "common/a#x"),
            ("reader-routing: bl A", "common/a#y"),
            ("reader-routing: bl A", "common/b#x"),
            ("instruction: ret", "common/a#z"),
        ] {
            tally
                .shapes
                .entry(shape.into())
                .or_default()
                .push(field.into());
        }

        let report = tally.report();
        assert_eq!(report["fields"], 4);
        assert_eq!(report["registries"], 2);
        assert_eq!(report["shapes"][0]["shape"], "reader-routing: bl A");
        assert_eq!(report["shapes"][0]["fields"], 3);
        assert_eq!(report["shapes"][0]["registries"], 2);
        assert_eq!(report["shapes"][1]["registries"], 1);
    }
}
