//! Locate the flag functions that the dynamic-name method recognizes, by their exact signatures.
use crate::AnalysisError;
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::dynamic_names::FlagFunctions;

use super::declarations::unique;

pub(in crate::binding) fn flag_functions(
    symbols: &[Symbol],
) -> Result<FlagFunctions, AnalysisError> {
    Ok(FlagFunctions {
        name_reader: unique(
            symbols,
            "ReadAsDynamicFlag(CString const&, CString&, CEventTarget&, EScopeType, CString const&)",
        )?,
        interner: unique(symbols, "CPdxIntegerFlags::CreateFlagIndex(CString const&)")?,
        setter: unique(
            symbols,
            "CPdxIntegerFlags::SetFlag(CPdxIntegerFlags::CIntFlag<unsigned short>, CDate const&, int, CPdxIntegerFlags::ESetFlagMode)",
        )?,
        remover: unique(
            symbols,
            "CPdxIntegerFlags::ClearFlag(CPdxIntegerFlags::CIntFlag<unsigned short>)",
        )?,
    })
}
