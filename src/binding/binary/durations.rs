//! Locate the flag-store bodies that consume a shared-factor duration, by their exact signatures.
use std::collections::BTreeMap;

use super::declarations::{Text, unique};
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::durations::{self, Countdown, Input};
use crate::engine::analysis::stop::Unresolved;

const SET_FLAG: &str = "CPdxIntegerFlags::SetFlag(CPdxIntegerFlags::CIntFlag<unsigned short>, CDate const&, int, CPdxIntegerFlags::ESetFlagMode)";
const UPDATE_FLAGS: &str = "CPdxIntegerFlags::UpdateFlags()";

pub(in crate::binding) fn input(
    text: &Text<'_>,
    symbols: &[Symbol],
    names: BTreeMap<u64, String>,
    execute_slot: u64,
) -> Input {
    let countdown = countdown(text, symbols, &names);

    Input {
        execute_slot,
        names,
        countdown,
    }
}

/// A missing or ambiguous body leaves consumption unresolved instead of failing the grammar.
fn countdown(
    text: &Text<'_>,
    symbols: &[Symbol],
    names: &BTreeMap<u64, String>,
) -> Result<Countdown, Unresolved> {
    let set_flag = body(text, symbols, SET_FLAG).ok_or(Unresolved::new("duration-flag-setter"))?;
    let update_flags =
        body(text, symbols, UPDATE_FLAGS).ok_or(Unresolved::new("duration-flag-update"))?;

    durations::countdown(&set_flag, &update_flags, names)
}

fn body(text: &Text<'_>, symbols: &[Symbol], name: &str) -> Option<Vec<Instruction>> {
    let start = unique(symbols, name).ok()?;
    let (start, code) = text.function(start).ok()?;

    decode_arm64(code, start).ok()
}

#[cfg(test)]
pub(crate) type Bodies = (Vec<Instruction>, Vec<Instruction>, Vec<Instruction>);

#[cfg(test)]
pub(in crate::binding) fn bodies(bytes: &[u8], symbols: &[Symbol]) -> Option<Bodies> {
    let text = Text::read(bytes, symbols).ok()?;

    Some((
        body(&text, symbols, SET_FLAG)?,
        body(&text, symbols, UPDATE_FLAGS)?,
        body(
            &text,
            symbols,
            "CSetTimedFlagEffect::ExecuteActual(CEventScope&) const",
        )?,
    ))
}
