//! The inline database search that precedes the base modifier reader. The anchors cover
//! key provenance, string equality, the value-token conversion, and the two fallback routes.
//! Allocation, insertion and cleanup after those routes do not qualify conversion limits.
use crate::engine::analysis::{
    decode::Instruction,
    references::shapes::{Shape, canonical},
    stop::Unresolved,
};
use std::collections::BTreeMap;

pub(crate) struct Input {
    pub member: Vec<Instruction>,
    pub conversion: Vec<Instruction>,
    pub names: BTreeMap<u64, String>,
    pub key_text_offset: u64,
    pub value_token_offset: u64,
    pub token_text_offset: u64,
}

pub(crate) fn analyze(input: &Input) -> Result<(), Unresolved> {
    let failed = || Unresolved::new("modifier-reference-flow");
    let lines = canonical(&input.member, &input.names);
    if lines.len() < 105
        || !input.member[..8].iter().all(frame_setup)
        || !input.member[49..56].iter().all(frame_restore)
        || lines[56].text != "b CALL"
        || lines[56].value.as_deref() != Some("base_member")
    {
        return Err(failed());
    }
    let anchors: Vec<_> = lines[8..49]
        .iter()
        .chain(&lines[57..105])
        .cloned()
        .collect();
    let binding = Shape::parse(include_str!("reference_lookup.txt"))
        .matches(&anchors)
        .ok_or_else(failed)?;
    let offset = |name: &str| {
        binding
            .get(name)
            .and_then(|value| u64::from_str_radix(value.trim_start_matches("0x"), 16).ok())
    };
    if offset("key_text") != Some(input.key_text_offset)
        || offset("value_token") != Some(input.value_token_offset)
    {
        return Err(failed());
    }
    let conversion = canonical(&input.conversion, &input.names);
    let shape = format!(
        "ldr x0,[x0,#{:#x}]\nb CALL = fixed_conversion",
        input.token_text_offset
    );
    Shape::parse(&shape)
        .matches(&conversion)
        .ok_or_else(failed)?;
    Ok(())
}

fn frame_setup(row: &Instruction) -> bool {
    (row.operation == "sub" && row.operands.starts_with("sp,sp,#"))
        || (row.operation == "add" && row.operands.starts_with("x29,sp,#"))
        || (row.operation == "stp" && row.operands.contains(",[sp,"))
}

fn frame_restore(row: &Instruction) -> bool {
    (row.operation == "add" && row.operands.starts_with("sp,sp,#"))
        || (row.operation == "ldp"
            && row.operands.contains(",[sp,")
            && row
                .operands
                .split(',')
                .take(2)
                .all(|reg| !matches!(reg, "x0" | "x1" | "x2")))
}

#[cfg(test)]
#[path = "reference_tests.rs"]
mod tests;
