//! Keep the reviewed files' indentation, row layout and object key order.
use super::Result;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Deserialize)]
#[serde(untagged)]
enum Template {
    Object(#[serde(deserialize_with = "object_entries")] Vec<(String, Template)>),
    Array(Vec<Template>),
    Scalar(Value),
}

fn object_entries<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Vec<(String, Template)>, D::Error> {
    struct Entries;
    impl<'de> Visitor<'de> for Entries {
        type Value = Vec<(String, Template)>;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an object in source key order")
        }
        fn visit_map<M: MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = map.next_entry()? {
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_map(Entries)
}

type KeyOrders = BTreeMap<Vec<String>, Vec<String>>;

impl Template {
    fn collect_orders(&self, orders: &mut KeyOrders) {
        match self {
            Self::Object(entries) => {
                let order: Vec<_> = entries.iter().map(|(key, _)| key.clone()).collect();
                let mut keys = order.clone();
                keys.sort();
                orders.insert(keys, order);
                for (_, child) in entries {
                    child.collect_orders(orders);
                }
            }
            Self::Array(entries) => {
                for child in entries {
                    child.collect_orders(orders);
                }
            }
            Self::Scalar(value) => {
                debug_assert!(!value.is_object() && !value.is_array());
            }
        }
    }
}

pub fn render(name: &str, value: &Value, template: &[u8]) -> Result<Vec<u8>> {
    let template: Template = serde_json::from_slice(template)?;
    let mut orders = KeyOrders::new();
    template.collect_orders(&mut orders);
    // New field reader objects use the same order even when no tracked reader had this shape.
    for order in [
        vec!["id", "kind", "family", "numeric"],
        vec!["id", "kind", "family", "numeric", "scoped_operand"],
    ] {
        let order: Vec<String> = order.into_iter().map(String::from).collect();
        let mut keys = order.clone();
        keys.sort();
        orders.insert(keys, order);
    }
    let layout = Layout { name, orders };
    let mut text = String::new();
    layout.write(value, 0, "", false, &mut text);
    text.push('\n');
    Ok(text.into_bytes())
}

struct Layout<'a> {
    name: &'a str,
    orders: KeyOrders,
}

impl Layout<'_> {
    fn indent(&self) -> usize {
        if self.name.starts_with("fields-")
            || matches!(
                self.name,
                "registries.json" | "references.json" | "localization-declarations.json"
            )
        {
            1
        } else {
            2
        }
    }

    fn inline(&self, depth: usize, section: &str) -> bool {
        match self.name {
            "on-actions.json" | "game-rules.json" | "localization-declarations.json" => depth >= 2,
            "modifier-blocks.json" => depth >= 4,
            "modifier-nodes.json" => depth >= 2,
            "weight-blocks.json" => depth >= 5 || (section == "gaps" && depth >= 3),
            "triggered-modifiers.json" => depth >= 4 || (section == "gaps" && depth >= 3),
            "dynamic-names.json" => section == "gaps" || depth >= 3,
            "defines.json" => section == "samples" && depth >= 2,
            name if name.starts_with("derived-names-") => depth >= 2,
            _ => false,
        }
    }

    fn write(&self, value: &Value, depth: usize, section: &str, inline: bool, out: &mut String) {
        let inline = inline
            || self.inline(depth, section)
            || (self.name == "modifier-families.json"
                && value.get("kind").is_some()
                && value.get("name").is_some());
        let padded = matches!(self.name, "defines.json" | "dynamic-names.json")
            && inline
            && value.is_object();
        let (open, close, entries): (char, char, Vec<(Option<&str>, &Value)>) = match value {
            Value::Object(object) => {
                let keys: Vec<_> = object.keys().cloned().collect();
                let order = self.orders.get(&keys).unwrap_or(&keys);
                (
                    '{',
                    '}',
                    order
                        .iter()
                        .map(|key| {
                            (
                                Some(object.get_key_value(key).unwrap().0.as_str()),
                                &object[key],
                            )
                        })
                        .collect(),
                )
            }
            Value::Array(array) => ('[', ']', array.iter().map(|value| (None, value)).collect()),
            scalar => {
                out.push_str(&serde_json::to_string(scalar).expect("JSON scalar serializes"));
                return;
            }
        };
        out.push(open);
        if entries.is_empty() {
            out.push(close);
            return;
        }
        if padded {
            out.push(' ');
        }
        for (index, (key, child)) in entries.iter().enumerate() {
            if index > 0 {
                out.push(',');
                if inline {
                    out.push(' ');
                }
            }
            if !inline {
                out.push('\n');
                out.push_str(&" ".repeat((depth + 1) * self.indent()));
            }
            if let Some(key) = key {
                out.push_str(&serde_json::to_string(key).expect("JSON key serializes"));
                out.push_str(": ");
            }
            let section = if depth == 0 {
                key.unwrap_or("")
            } else {
                section
            };
            self.write(child, depth + 1, section, inline, out);
        }
        if padded {
            out.push(' ');
        }
        if !inline {
            out.push('\n');
            out.push_str(&" ".repeat(depth * self.indent()));
        }
        out.push(close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tracked_layout_round_trips_byte_for_byte() {
        for name in super::super::FILES {
            let bytes = std::fs::read(super::super::expected_directory().join(name)).unwrap();
            let value = serde_json::from_slice(&bytes).unwrap();
            let rendered = render(name, &value, &bytes).unwrap();
            let first_difference = rendered
                .iter()
                .zip(&bytes)
                .position(|(left, right)| left != right);
            assert!(
                rendered == bytes,
                "{name}: first layout difference at {first_difference:?}"
            );
        }
    }
}
