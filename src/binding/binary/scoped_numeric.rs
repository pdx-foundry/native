//! Exact executable binding for shared scoped numeric bodies and vtable address points.
use std::collections::BTreeMap;

use super::{declarations::Text, numeric::READ_INT_VALUE, references::Image};
use crate::AnalysisError;
use crate::engine::analysis::scoped_numeric::{Input, SubtypeInput, SubtypeKind};

pub(in crate::binding) fn read(
    image: &Image<'_>,
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
        if let Some(body) = text.unique_body(image.symbols, name)? {
            bodies.insert(name.into(), body);
        }
    }

    let data = super::language::constant_data(image.bytes, image.pointers, image.bound_slots)?;
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
            Some(READ_INT_VALUE),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::analysis_support::macho_with_fixups;

    #[test]
    fn an_image_without_the_scoped_readers_gives_a_body_gap_and_not_a_shape_gap() {
        let bytes = macho_with_fixups(6);
        let facts = super::super::discovery::read(&bytes).unwrap();
        let image = Image {
            bytes: &bytes,
            symbols: &facts.symbols,
            strings: &facts.strings,
            pointers: &facts.pointers,
            imports: &facts.imports,
            bound_slots: &facts.bound_slots,
            index: &super::super::symbols::SymbolIndex::new(&facts.symbols),
        };

        let input = read(&image, 0x278).unwrap();
        let analysis = crate::engine::analysis::scoped_numeric::analyze(&input);

        assert!(input.bodies.is_empty());
        assert_eq!(analysis.shared.selection.unwrap_err().reason, "scoped-body");
    }
}
