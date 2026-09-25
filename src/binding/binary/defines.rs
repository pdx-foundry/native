//! Read define helper functions and their typed reader targets from one executable.
use std::collections::BTreeMap;

use crate::engine::analysis::{
    decode::decode_arm64,
    defines::{DefineInput, ReadSite, ReaderSource, ReaderSpec},
    discovery::Symbol,
};
use crate::{AnalysisError, DefineValueType};

use super::declarations::{Text, read_only_data};

pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
) -> Result<DefineInput, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let readers: BTreeMap<_, _> = symbols
        .iter()
        .filter_map(|symbol| Some((symbol.address, reader_spec(&symbol.name)?)))
        .collect();
    let mut sites = Vec::new();
    for symbol in symbols.iter().filter(|symbol| is_helper(&symbol.name)) {
        let rows = text
            .function(symbol.address)
            .ok()
            .and_then(|(address, code)| decode_arm64(code, address).ok())
            .ok_or("define helper could not be decoded");
        sites.push(ReadSite {
            symbol: symbol.name.clone(),
            address: symbol.address,
            rows,
        });
    }
    if sites.is_empty() || readers.is_empty() {
        return Err(AnalysisError::InvalidRange);
    }
    sites.sort_by_key(|site| site.address);
    sites.dedup_by_key(|site| site.address);
    Ok(DefineInput {
        sites,
        readers,
        strings: strings.clone(),
        data: read_only_data(bytes)?,
    })
}

fn is_helper(name: &str) -> bool {
    (name.starts_with("NDefines::CDefineRegistryHelper_")
        || name.starts_with("NUncheckedDefines::CDefineRegistryHelper_"))
        && name.ends_with("::ReadDefine(CDefinesContainer const&)")
}

fn reader_spec(name: &str) -> Option<ReaderSpec> {
    let source = if name.contains("CDefinesContainer::GetValue<")
        || name.starts_with("ReadDefinesValue(CDefinesContainer const&,")
    {
        ReaderSource::Container
    } else if name.contains("CDefinesTable::GetArrayValue<") {
        ReaderSource::Table
    } else {
        return None;
    };
    let value_type = if name.contains("CGameDate&") {
        Some(DefineValueType::Date)
    } else if name.contains("CColor&") {
        Some(DefineValueType::Color)
    } else if name.contains("CVector3FixedPoint&")
        || name.contains("Eigen::Matrix<float,")
        || name.contains("CPdxHybridArray<")
    {
        Some(DefineValueType::Vector)
    } else if name.contains("CPdxArray<") {
        Some(DefineValueType::List)
    } else if name.contains("GetValue<CString>") {
        Some(DefineValueType::String)
    } else if name.contains("GetValue<CFixedPoint>") || name.contains("GetValue<fpml::fixed_point<")
    {
        Some(DefineValueType::FixedPoint)
    } else if name.contains("GetValue<float>") {
        Some(DefineValueType::Float)
    } else if name.contains("GetValue<bool>") {
        Some(DefineValueType::Boolean)
    } else if name.contains("GetValue<int>")
        || name.contains("GetValue<short>")
        || name.contains("GetValue<unsigned long long>")
    {
        Some(DefineValueType::Integer)
    } else {
        None
    };
    Some(ReaderSpec { source, value_type })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_signatures_keep_scalar_array_vector_and_custom_types_distinct() {
        for (symbol, kind) in [
            (
                "void CDefinesContainer::GetValue<float>(char const*, char const*, float&) const",
                DefineValueType::Float,
            ),
            (
                "void CDefinesContainer::GetValue<CFixedPoint>(char const*, char const*, CFixedPoint&) const",
                DefineValueType::FixedPoint,
            ),
            (
                "void CDefinesContainer::GetValue<CPdxArray<CString, int>>(char const*, char const*, CPdxArray<CString, int>&) const",
                DefineValueType::List,
            ),
            (
                "bool CDefinesTable::GetArrayValue<CPdxHybridArray<float, 3u, int>>(char const*, CPdxHybridArray<float, 3u, int>&) const",
                DefineValueType::Vector,
            ),
            (
                "ReadDefinesValue(CDefinesContainer const&, char const*, char const*, CColor&)",
                DefineValueType::Color,
            ),
            (
                "ReadDefinesValue(CDefinesContainer const&, char const*, char const*, CGameDate&)",
                DefineValueType::Date,
            ),
        ] {
            assert_eq!(reader_spec(symbol).unwrap().value_type, Some(kind));
        }
    }
}
