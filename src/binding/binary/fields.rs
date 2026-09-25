use crate::AnalysisError;
use crate::engine::analysis::{
    discovery::{CandidateRecord, Symbol},
    fields::{DataSection, FieldInput, Function},
};
use object::{Object, ObjectSection, SectionKind};
use std::collections::BTreeMap;

/// Read the selected root, the shared engine token constructor and the read-only data that holds
/// jump tables from the same verified buffer.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
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
        let starts: std::collections::BTreeSet<_> = symbols
            .iter()
            .filter(|s| s.name == name)
            .map(|s| s.address)
            .collect();
        if starts.len() != 1 {
            gaps.push(format!("missing or ambiguous function {name}"));
            continue;
        }
        let start = *starts.first().unwrap();
        let Some(end) = symbols
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
        functions.push(Function {
            name: name.into(),
            address: start,
            code: super::code_range(bytes, start, length)?,
        });
    }
    Ok(FieldInput {
        selection,
        symbols: symbols.to_vec(),
        functions,
        strings: strings.clone(),
        read_only_data: read_only_data(bytes)?,
        gaps,
    })
}

fn read_only_data(bytes: &[u8]) -> Result<Vec<DataSection>, AnalysisError> {
    let slice = super::selected_slice(bytes).map_err(|_| AnalysisError::InvalidRange)?;
    let file = object::File::parse(slice).map_err(|_| AnalysisError::InvalidRange)?;
    file.sections()
        .filter(|section| section.kind() == SectionKind::ReadOnlyData)
        .map(|section| {
            let bytes = section.data().map_err(|_| AnalysisError::InvalidRange)?;
            Ok(DataSection {
                address: section.address(),
                bytes: bytes.to_vec(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::analysis_support::{IMAGE_JUMP_TABLE, macho_with_fixups};

    #[test]
    fn the_field_input_holds_the_jump_tables_in_read_only_data() {
        let sections = read_only_data(&macho_with_fixups(6)).unwrap();

        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].address, 0x1_0000_3000);
        assert_eq!(sections[0].bytes, IMAGE_JUMP_TABLE);
    }
}
