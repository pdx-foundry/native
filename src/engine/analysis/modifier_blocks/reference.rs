//! The inline database search that precedes the base modifier reader. The shape covers the
//! frame, key provenance, string equality, the value-token conversion, and the two fallback
//! routes. Allocation, insertion and cleanup after those routes do not qualify conversion limits.
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
    let binding = Shape::parse(include_str!("reference_lookup.txt"))
        .matches_prefix(&lines)
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

#[cfg(test)]
#[path = "reference_tests.rs"]
mod tests;
