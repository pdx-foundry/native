use pdx_native::{Field, Native, ReaderKind};
use std::collections::BTreeMap;

const DEFAULT_REGISTRIES: &[&str] = &[
    "common/traditions",
    "common/tradition_categories",
    "common/council_agendas",
];

fn kind_name(kind: ReaderKind) -> &'static str {
    match kind {
        ReaderKind::Boolean => "boolean",
        ReaderKind::Integer => "integer",
        ReaderKind::FixedPoint => "fixed-point",
        ReaderKind::String => "string",
        ReaderKind::Reference => "reference",
        ReaderKind::Block => "block",
        ReaderKind::Unknown => "unknown",
        _ => "new-kind",
    }
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
            let conditional = if field.conditional {
                ", conditional"
            } else {
                ""
            };
            println!("    {} [{identity}{conditional}]", field.name);
        }
    }
    let unknown = fields
        .iter()
        .filter(|field| field.reader.kind == ReaderKind::Unknown)
        .count();
    let missing_id = fields
        .iter()
        .filter(|field| field.reader.id.is_none())
        .count();
    let counts = [
        ReaderKind::Boolean,
        ReaderKind::Integer,
        ReaderKind::FixedPoint,
        ReaderKind::String,
        ReaderKind::Reference,
        ReaderKind::Block,
        ReaderKind::Unknown,
    ]
    .map(|kind| {
        let count = fields
            .iter()
            .filter(|field| field.reader.kind == kind)
            .count();
        format!("{}={count}", kind_name(kind))
    });
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
