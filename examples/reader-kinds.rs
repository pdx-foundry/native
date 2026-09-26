use pdx_native::{Field, Native, ReaderKind};
use std::collections::BTreeMap;

const DEFAULT_REGISTRIES: &[&str] = &[
    "common/traditions",
    "common/tradition_categories",
    "common/council_agendas",
];

/// Each known reader kind with its display label, in the order of the summary line.
const KIND_LABELS: [(ReaderKind, &str); 7] = [
    (ReaderKind::Boolean, "boolean"),
    (ReaderKind::Integer, "integer"),
    (ReaderKind::FixedPoint, "fixed-point"),
    (ReaderKind::String, "string"),
    (ReaderKind::Reference, "reference"),
    (ReaderKind::Block, "block"),
    (ReaderKind::Unknown, "unknown"),
];

fn kind_name(kind: ReaderKind) -> &'static str {
    KIND_LABELS
        .iter()
        .find(|(known, _)| *known == kind)
        .map_or("new-kind", |(_, label)| *label)
}

fn print_registry(registry: &str, fields: &[Field]) {
    println!("{registry}");
    let mut by_kind = BTreeMap::<ReaderKind, Vec<&Field>>::new();
    for field in fields {
        by_kind.entry(field.reader.kind).or_default().push(field);
    }
    for (kind, fields) in &by_kind {
        println!("  {} ({})", kind_name(*kind), fields.len());
        for field in fields {
            let identity = field
                .reader
                .id
                .as_ref()
                .map_or("missing-id", |_| "shared-id");
            let conditional = if field
                .read
                .iter()
                .any(|alternative| alternative.condition != pdx_native::FieldCondition::Always)
            {
                ", conditional"
            } else {
                ""
            };
            println!(
                "    {} [{identity}{conditional}, family={:?}]",
                field.name, field.reader.family
            );
        }
    }
    let count_of = |kind: ReaderKind| by_kind.get(&kind).map_or(0, Vec::len);
    let unknown = count_of(ReaderKind::Unknown);
    let missing_id = fields
        .iter()
        .filter(|field| field.reader.id.is_none())
        .count();
    let counts = KIND_LABELS.map(|(kind, label)| format!("{label}={}", count_of(kind)));

    println!(
        "  total={} {}; unknown-readers={unknown}; missing-id={missing_id}",
        fields.len(),
        counts.join(" ")
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let installation = arguments
        .next()
        .ok_or("usage: reader-kinds <installation-or-executable> [registry ...]")?;
    let requested: Vec<String> = arguments
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect();
    let registries: Vec<&str> = if requested.is_empty() {
        DEFAULT_REGISTRIES.to_vec()
    } else {
        requested.iter().map(String::as_str).collect()
    };
    let native = Native::open(installation)?;
    for registry in registries {
        let answer = native.registry_fields(registry)?;
        print_registry(registry, &answer.value);
    }
    Ok(())
}
