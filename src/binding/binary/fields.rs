use crate::AnalysisError;
use evidence::{
    discovery::{CandidateRecord, StaticInput},
    fields::{FieldInput, Function},
};

/// Read the selected root and the shared engine token constructor from the same verified buffer.
pub(in crate::binding) fn read(
    bytes: &[u8],
    discovery: StaticInput,
    selection: CandidateRecord,
) -> Result<FieldInput, AnalysisError> {
    let mut functions = Vec::new();
    let mut gaps = Vec::new();
    let root = format!("{}::ReadMember(CReader&, int)", selection.owner_candidate);
    for name in [
        root.as_str(),
        "CPersistent::ReadMember(CReader&, int)",
        "GetTokenArray()",
    ] {
        let starts: std::collections::BTreeSet<_> = discovery
            .symbols
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.address)
            .collect();
        if starts.len() != 1 {
            gaps.push(format!("missing or ambiguous function {name}"));
            continue;
        }
        let start = *starts.first().unwrap();
        let Some(end) = discovery
            .symbols
            .iter()
            .filter(|s| s.address > start)
            .map(|s| s.address)
            .min()
        else {
            gaps.push(format!("unbounded function {name}"));
            continue;
        };
        let length = end - start;
        if length == 0 || length > 1024 * 1024 || !length.is_multiple_of(4) {
            gaps.push(format!("unsupported function extent {name}"));
            continue;
        }
        let mut code = Vec::new();
        for offset in (0..length).step_by(4096) {
            code.extend(super::code_range(
                bytes,
                start + offset,
                (length - offset).min(4096),
            )?);
        }
        functions.push(Function {
            name: name.into(),
            address: start,
            code,
        });
    }
    Ok(FieldInput {
        selection,
        symbols: discovery.symbols,
        functions,
        strings: discovery.strings,
        gaps,
    })
}
