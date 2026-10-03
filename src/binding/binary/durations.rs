//! Locate the execute body used by the authored duration control.
#[cfg(test)]
pub(in crate::binding) fn execute_body(
    bytes: &[u8],
    symbols: &[crate::engine::analysis::discovery::Symbol],
) -> Option<Vec<crate::engine::analysis::decode::Instruction>> {
    use super::declarations::{Text, unique};
    let text = Text::read(bytes, symbols).ok()?;
    let start = unique(
        symbols,
        "CSetTimedFlagEffect::ExecuteActual(CEventScope&) const",
    )
    .ok()?;
    let (start, code) = text.function(start).ok()?;
    crate::engine::analysis::decode::decode_arm64(code, start).ok()
}
