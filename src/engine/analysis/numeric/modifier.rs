//! The numeric-entry boundary only: the reader's stack result is passed unchanged to insertion,
//! whose two capacity paths load that same value and store eight bytes into the new entry.
//! Matching the surrounding bodies qualifies the data flow, not the modifier block grammar,
//! category applicability, duplicate handling, or the entry's later use.
use super::number;
use crate::ReaderId;
use crate::engine::analysis::{
    decode::Instruction,
    references::shapes::{Shape, canonical},
    stop::Unresolved,
};
use std::collections::BTreeMap;

/// The shared conversion at the numeric-entry store boundary, not at a modifier block reader.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModifierNumericEntry {
    /// Callee key into `NumericFacts::readers`; no second conversion result is created.
    pub shared_callee: String,
    /// The same identity reported on fields and scalar command readers.
    pub reader_id: ReaderId,
    /// Width stored in the entry on both insertion capacity paths.
    pub storage_width_bits: u8,
}

pub(crate) struct ModifierInput {
    pub member: Vec<Instruction>,
    pub insert: Vec<Instruction>,
    pub names: BTreeMap<u64, String>,
    pub shared_callee: String,
}

pub(super) fn analyze(
    input: &ModifierInput,
    readers: &BTreeMap<String, super::NumericReader>,
) -> Result<ModifierNumericEntry, Unresolved> {
    let member = canonical(&input.member, &input.names);
    let binding = Shape::parse(include_str!("shapes/modifier_member.txt"))
        .matches(&member)
        .ok_or(Unresolved::new("modifier-numeric-member-flow"))?;
    let insert = canonical(&input.insert, &input.names);
    let insertion = Shape::parse(include_str!("shapes/modifier_insert.txt"))
        .matches(&insert)
        .ok_or(Unresolved::new("modifier-numeric-entry-store"))?;
    let count_offset = number(&insertion, "count_offset");
    let adjacent_count =
        number(&insertion, "capacity_offset").and_then(|offset| offset.checked_add(4));
    let append_index =
        number(&binding, "entry_array_offset").and_then(|offset| offset.checked_add(count_offset?));
    if count_offset != adjacent_count || append_index != number(&binding, "entry_count_offset") {
        return Err(Unresolved::new("modifier-numeric-append-index"));
    }
    if !readers.contains_key(&input.shared_callee) {
        return Err(Unresolved::new("modifier-numeric-shared-reader"));
    }
    Ok(ModifierNumericEntry {
        shared_callee: input.shared_callee.clone(),
        reader_id: ReaderId::from_callee(&input.shared_callee),
        storage_width_bits: 64,
    })
}
