//! Exact executable binding for shared scoped numeric bodies and vtable address points.
use std::collections::{BTreeMap, BTreeSet};

use super::{declarations::Text, references::Image};
use crate::AnalysisError;
use crate::engine::analysis::{
    decode::decode_arm64,
    scoped_numeric::{Input, SubtypeInput, SubtypeKind},
};

pub(in crate::binding) fn read(
    image: &Image<'_>,
    bound_slots: &BTreeSet<u64>,
    value_token_offset: u64,
) -> Result<Input, AnalysisError> {
    let text = Text::read(image.bytes, image.symbols)?;
    let names =
        super::references::names(image.symbols, image.pointers, image.imports, image.strings);
    let mut bodies = BTreeMap::new();
    for name in [
        "CVariableValue::Read(CReader&, EScopeType)",
        "CVariableValue::Assign(CToken const&, EScopeType, CString const&)",
        "CVariableValue::ReadTriggerModifierOrScriptValue(CString&, EScopeType)",
        "CVariableValue::ReadVariable(CString&)",
        "CVariableValue::GetVariableValue(CEventScope&) const",
        "CVariableValue::GetModifierValue(CEventScope&) const",
        "CVariableValue::AssignSimple(CToken const&)",
        "CIntVariableValue::AssignSimple(CToken const&)",
        "CFixedPointVariableValue::AssignSimple(CToken const&)",
        "CIntVariableValue::GetValue(CEventScope&) const",
        "CIntVariableValue::GetValueInternal(CEventScope&) const",
        "CFixedPointVariableValue::GetValue(CEventScope&) const",
        "CFixedPointVariableValue::GetValueInternal(CEventScope&) const",
    ] {
        let addresses: Vec<_> = image
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.address)
            .collect();
        if addresses.len() != 1 {
            bodies.insert(name.into(), Vec::new());
            continue;
        }
        let (address, bytes) = text.function(addresses[0])?;
        bodies.insert(
            name.into(),
            decode_arm64(bytes, address).map_err(|_| AnalysisError::InvalidRange)?,
        );
    }

    let data = super::language::constant_data(image.bytes, image.pointers, bound_slots)?;
    let mut points = BTreeMap::new();
    for (class, simple, kind, token_reader) in [
        (
            "CVariableValue",
            "CVariableValue::AssignSimple(CToken const&)",
            SubtypeKind::Base,
            None,
        ),
        (
            "CIntVariableValue",
            "CIntVariableValue::AssignSimple(CToken const&)",
            SubtypeKind::Integer,
            Some("CToken::ReadValue(int&) const"),
        ),
        (
            "CFixedPointVariableValue",
            "CFixedPointVariableValue::AssignSimple(CToken const&)",
            SubtypeKind::FixedPoint,
            Some("CToken::ReadValue(CFixedPoint&) const"),
        ),
    ] {
        if let Some(group) = super::families::vtable_group(image.symbols, &data, class)
            && let Some(point) = group.address_points.get(&0)
        {
            points.insert(
                *point,
                SubtypeInput {
                    assign_simple: simple.into(),
                    kind,
                    token_reader: token_reader.map(str::to_owned),
                },
            );
        }
    }
    Ok(Input {
        bodies,
        names,
        points,
        pointers: image.pointers.clone(),
        value_token_offset,
        reader_name: "CVariableValue::Read(CReader&, EScopeType)".into(),
        assign_name: "CVariableValue::Assign(CToken const&, EScopeType, CString const&)".into(),
        prefix_name: "CVariableValue::ReadTriggerModifierOrScriptValue(CString&, EScopeType)"
            .into(),
        variable_name: "CVariableValue::ReadVariable(CString&)".into(),
        helper_names: [
            "CVariableValue::GetVariableValue(CEventScope&) const".into(),
            "CVariableValue::GetModifierValue(CEventScope&) const".into(),
        ],
        integer_value_path: (
            "CIntVariableValue::GetValue(CEventScope&) const".into(),
            "CIntVariableValue::GetValueInternal(CEventScope&) const".into(),
        ),
        fixed_point_value_path: (
            "CFixedPointVariableValue::GetValue(CEventScope&) const".into(),
            "CFixedPointVariableValue::GetValueInternal(CEventScope&) const".into(),
        ),
    })
}
