//! Exact whole-body proofs for the shared scoped numeric reader and its concrete subtypes.
use std::collections::BTreeMap;

use super::decode::Instruction;
use super::references::shapes::{Bindings, Shape, canonical_local};
use super::stop::Unresolved;

#[derive(Clone)]
pub(crate) struct SubtypeInput {
    pub assign_simple: String,
    pub token_reader: Option<String>,
    pub kind: SubtypeKind,
}

#[derive(Clone)]
pub(crate) enum SubtypeKind {
    Base,
    Integer,
    FixedPoint,
}

#[derive(Clone)]
pub(crate) struct Input {
    pub bodies: BTreeMap<String, Vec<Instruction>>,
    pub names: BTreeMap<u64, String>,
    pub points: BTreeMap<u64, SubtypeInput>,
    pub pointers: BTreeMap<u64, u64>,
    pub value_token_offset: u64,
    pub integer_value_path: (String, String),
    pub fixed_point_value_path: (String, String),
    pub reader_name: String,
    pub assign_name: String,
    pub prefix_name: String,
    pub variable_name: String,
    pub helper_names: [String; 2],
}

/// Storage offsets established jointly by shared forms and both numeric GetValue bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Layout {
    pub literal: u64,
    pub location: u64,
    pub trigger: u64,
    pub script_value: u64,
    pub modifier: u64,
    pub modifier_unset: u64,
    pub variable: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct Shared {
    pub forms: Result<Forms, Unresolved>,

    pub selection: Result<Layout, Unresolved>,
}

#[derive(Debug, Clone)]
pub(crate) struct Forms {
    pub prefixes: [String; 3],
    pub separator: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subtype {
    Base,
    Numeric { token_reader: String, literal: u64 },
}

#[derive(Debug, Clone)]
pub(crate) struct Facts {
    pub shared: Shared,
    pub subtypes: BTreeMap<u64, Result<Subtype, Unresolved>>,
}

pub(crate) fn analyze(input: &Input) -> Facts {
    let read = matched(
        input,
        &input.reader_name,
        include_str!("scoped_numeric/shapes/read.txt"),
    )
    .and_then(|bindings| offset(&bindings, "value_token"));
    let assign = matched(
        input,
        &input.assign_name,
        include_str!("scoped_numeric/shapes/assign.txt"),
    )
    .and_then(|bindings| {
        Ok((
            offset(&bindings, "simple_slot")?,
            offset(&bindings, "location")?,
        ))
    });
    let prefixes = matched(
        input,
        &input.prefix_name,
        include_str!("scoped_numeric/shapes/prefixes.txt"),
    );
    let variable = matched(
        input,
        &input.variable_name,
        include_str!("scoped_numeric/shapes/variable.txt"),
    )
    .and_then(|bindings| offset(&bindings, "variable"));

    let read_joined = read == Ok(input.value_token_offset);
    let forms = if read_joined && assign.is_ok() && prefixes.is_ok() && variable.is_ok() {
        Ok(Forms {
            separator: ":".into(),
            prefixes: ["trigger".into(), "modifier".into(), "value".into()],
        })
    } else {
        Err(Unresolved::new("scoped-operand-forms"))
    };
    let selection = selection(
        input,
        assign.as_ref().map(|(_, location)| *location),
        &prefixes,
        variable,
    );
    let subtypes = input
        .points
        .iter()
        .map(|(&point, config)| {
            (
                point,
                subtype(input, point, config, assign.as_ref().map(|(slot, _)| *slot)),
            )
        })
        .collect();
    Facts {
        shared: Shared { forms, selection },
        subtypes,
    }
}

fn matched(input: &Input, name: &str, shape: &str) -> Result<Bindings, Unresolved> {
    let body = input
        .bodies
        .get(name)
        .ok_or(Unresolved::new("scoped-body"))?;
    let lines =
        canonical_local(body, &input.names).ok_or(Unresolved::new("scoped-local-address"))?;
    let bindings = Shape::parse(shape)
        .matches(&lines)
        .ok_or(Unresolved::new("scoped-body-shape"))?;
    if let Some(source) = bindings.get("diagnostic_source")
        && !(source.len() > 2 && source.starts_with('"') && source.ends_with('"'))
    {
        return Err(Unresolved::new("scoped-diagnostic-source"));
    }
    Ok(bindings)
}

fn offset(bindings: &Bindings, name: &str) -> Result<u64, Unresolved> {
    bindings
        .get(name)
        .and_then(|value| value.strip_prefix("0x"))
        .and_then(|value| u64::from_str_radix(value, 16).ok())
        .ok_or(Unresolved::new("scoped-layout-offset"))
}

fn subtype(
    input: &Input,
    point: u64,
    config: &SubtypeInput,
    slot: Result<u64, &Unresolved>,
) -> Result<Subtype, Unresolved> {
    let slot = slot.map_err(Clone::clone)?;
    let target = input
        .pointers
        .get(&(point + slot))
        .ok_or(Unresolved::new("scoped-simple-slot"))?;
    if input.names.get(target).map(String::as_str) != Some(config.assign_simple.as_str()) {
        return Err(Unresolved::new("scoped-simple-target"));
    }
    let shape = match config.kind {
        SubtypeKind::Base => include_str!("scoped_numeric/shapes/base_simple.txt"),
        SubtypeKind::Integer => include_str!("scoped_numeric/shapes/int_simple.txt"),
        SubtypeKind::FixedPoint => include_str!("scoped_numeric/shapes/fixed_simple.txt"),
    };
    let bindings = matched(input, &config.assign_simple, shape)?;
    match &config.token_reader {
        None => Ok(Subtype::Base),
        Some(token_reader) => Ok(Subtype::Numeric {
            token_reader: token_reader.clone(),
            literal: offset(&bindings, "literal")?,
        }),
    }
}

fn selection(
    input: &Input,
    location: Result<u64, &Unresolved>,
    prefixes: &Result<Bindings, Unresolved>,
    variable: Result<u64, Unresolved>,
) -> Result<Layout, Unresolved> {
    let location = location.map_err(Clone::clone)?;
    let prefixes = prefixes.as_ref().map_err(Clone::clone)?;
    let trigger = offset(prefixes, "trigger")?;
    let script_value = offset(prefixes, "script_value")?;
    let modifier = offset(prefixes, "modifier")?;
    let modifier_unset = offset(prefixes, "modifier_unset")?;
    let variable = variable?;
    let variable_helper = matched(
        input,
        &input.helper_names[0],
        include_str!("scoped_numeric/shapes/get_variable.txt"),
    )?;
    let modifier_helper = matched(
        input,
        &input.helper_names[1],
        include_str!("scoped_numeric/shapes/get_modifier.txt"),
    )?;
    if offset(&variable_helper, "variable")? != variable
        || offset(&modifier_helper, "modifier")? != modifier
    {
        return Err(Unresolved::new("scoped-helper-store-disagreement"));
    }
    let mut agreement = None;
    for ((get_name, internal_name), get_shape, internal_shape) in [
        (
            &input.integer_value_path,
            include_str!("scoped_numeric/shapes/int_get.txt") as &str,
            include_str!("scoped_numeric/shapes/int_internal.txt") as &str,
        ),
        (
            &input.fixed_point_value_path,
            include_str!("scoped_numeric/shapes/fixed_get.txt"),
            include_str!("scoped_numeric/shapes/fixed_internal.txt"),
        ),
    ] {
        let get = matched(input, get_name, get_shape)?;
        let internal = matched(input, internal_name, internal_shape)?;
        if offset(&get, "short_tag")? != location + 0x17
            || offset(&get, "long_size")? != location + 8
        {
            return Err(Unresolved::new("scoped-location-disagreement"));
        }
        let candidate = Layout {
            literal: offset(&get, "literal")?,
            location,
            trigger: offset(&internal, "trigger")?,
            script_value: offset(&internal, "script_value")?,
            modifier: offset(&internal, "modifier")?,
            modifier_unset: offset(&internal, "modifier_unset")?,
            variable,
        };
        if candidate.modifier_unset != modifier_unset
            || candidate.trigger != trigger
            || candidate.script_value != script_value
            || candidate.modifier != modifier
        {
            return Err(Unresolved::new("scoped-layout-disagreement"));
        }
        if let Some(previous) = &agreement
            && previous != &candidate
        {
            return Err(Unresolved::new("scoped-subtype-disagreement"));
        }
        agreement = Some(candidate);
    }
    agreement.ok_or(Unresolved::new("scoped-selection"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::{assembler::arm64, decode::decode_arm64};

    fn public_summary(reader: &crate::Reader) -> serde_json::Value {
        let storage = match &reader.numeric {
            crate::GrammarProperty::Known(Some(conversion))
            | crate::GrammarProperty::Partial(Some(conversion)) => {
                Some((&conversion.width_bits, &conversion.scale))
            }
            _ => None,
        };
        serde_json::json!({
            "kind": reader.kind,
            "width": storage.map(|(width, _)| width),
            "scale": storage.map(|(_, scale)| scale),
            "operand": if matches!(reader.scoped_operand, crate::GrammarProperty::Known(Some(_))) { "proved" } else { "unresolved" },
        })
    }

    fn expected() -> serde_json::Value {
        serde_json::from_str(include_str!(
            "../../../tests/expected/scoped-numeric-m452/static.json"
        ))
        .unwrap()
    }

    fn named_field<'a>(fields: &'a [crate::Field], name: &str) -> &'a crate::Field {
        fields.iter().find(|field| field.name == name).unwrap()
    }

    fn shape_input(body: Vec<Instruction>, names: BTreeMap<u64, String>) -> Input {
        Input {
            bodies: [("body".into(), body)].into(),
            names,
            points: BTreeMap::new(),
            pointers: BTreeMap::new(),
            value_token_offset: 0,
            integer_value_path: (String::new(), String::new()),
            fixed_point_value_path: (String::new(), String::new()),
            reader_name: String::new(),
            assign_name: String::new(),
            prefix_name: String::new(),
            variable_name: String::new(),
            helper_names: [String::new(), String::new()],
        }
    }

    #[test]
    fn local_jump_bases_follow_relocation_but_require_the_same_instruction() {
        let shape = "adr xr0,@+1\nret";
        for address in [0x1000, 0x9000] {
            let bytes = arm64!(at address;
                adr x9, extern (address + 4) as usize;
                ret
            );
            let mut input = shape_input(decode_arm64(&bytes, address).unwrap(), BTreeMap::new());
            assert!(matched(&input, "body", shape).is_ok());
            input.bodies.get_mut("body").unwrap()[0].operands = format!("x9,#{address:#x}");
            assert!(matched(&input, "body", shape).is_err());
            input.bodies.get_mut("body").unwrap()[0].operands = format!("x9,#{:#x}", address + 8);
            assert!(matched(&input, "body", shape).is_err());
        }
    }

    #[test]
    fn diagnostic_source_is_captured_but_must_be_named_and_consistent() {
        let bytes = arm64!(at 0x1000;
            adrp x1, extern 0x8000;
            add x1, x1, #0x10;
            adrp x1, extern 0x8000;
            add x1, x1, #0x20;
            ret
        );
        let body = decode_arm64(&bytes, 0x1000).unwrap();
        let shape = "adrp x1,PAGE\nadd x1,x1,G = {diagnostic_source}\nadrp x1,PAGE\nadd x1,x1,G = {diagnostic_source}\nret";
        for source in ["\"/old-build/source.cpp\"", "\"/new-build/source.cpp\""] {
            let mut input = shape_input(
                body.clone(),
                [(0x8010, source.into()), (0x8020, source.into())].into(),
            );
            assert!(matched(&input, "body", shape).is_ok());
            input.names.insert(0x8020, "\"/other/source.cpp\"".into());
            assert!(matched(&input, "body", shape).is_err());
            input.names.clear();
            assert!(matched(&input, "body", shape).is_err());
            input.names = [(0x8010, "\"\"".into()), (0x8020, "\"\"".into())].into();
            assert!(matched(&input, "body", shape).is_err());
        }
    }

    #[test]
    fn authored_simple_reader_requires_token_call_and_vtable_join() {
        let body = arm64!(at 0x1000;
            mov x8, x1;
            add x1, x0, #0x200;
            mov x0, x8;
            b extern 0x9000
        );
        let rows = decode_arm64(&body, 0x1000).unwrap();
        let name = "CIntVariableValue::AssignSimple(CToken const&)";
        let mut input = Input {
            bodies: [(name.into(), rows)].into(),
            names: [
                (0x9000, "CToken::ReadValue(int&) const".into()),
                (0x1000, name.into()),
            ]
            .into(),
            points: [(
                0x8000,
                SubtypeInput {
                    assign_simple: name.into(),
                    kind: SubtypeKind::Integer,
                    token_reader: Some("CToken::ReadValue(int&) const".into()),
                },
            )]
            .into(),
            pointers: [(0x8010, 0x1000)].into(),
            value_token_offset: 0x278,
            integer_value_path: (String::new(), String::new()),
            fixed_point_value_path: (String::new(), String::new()),
            reader_name: String::new(),
            assign_name: String::new(),
            prefix_name: String::new(),
            variable_name: String::new(),
            helper_names: [String::new(), String::new()],
        };
        let config = input.points[&0x8000].clone();
        assert_eq!(
            subtype(&input, 0x8000, &config, Ok(0x10)),
            Ok(Subtype::Numeric {
                token_reader: "CToken::ReadValue(int&) const".into(),
                literal: 0x200,
            })
        );
        input.names.remove(&0x9000);
        assert!(subtype(&input, 0x8000, &config, Ok(0x10)).is_err());
        input
            .names
            .insert(0x9000, "CToken::ReadValue(int&) const".into());
        input.pointers.remove(&0x8010);
        assert!(subtype(&input, 0x8000, &config, Ok(0x10)).is_err());
        input.pointers.insert(0x8010, 0x1000);
        input.bodies.get_mut(name).unwrap()[1].operation = "sub".into();
        assert!(subtype(&input, 0x8000, &config, Ok(0x10)).is_err());
    }
    #[test]
    fn reader_recordings_require_scoped_operand_facts() {
        assert!(
            serde_json::from_str::<crate::Reader>(
                r#"{"numeric":"Unresolved","family":"Unknown","id":null,"kind":"ScopedNumeric"}"#
            )
            .is_err()
        );
    }
    #[test]
    #[ignore = "requires the exact supported executable through STELLARIS_PATH"]
    fn m452_scoped_numeric_shared_proofs() {
        let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
        let input = native
            .bound()
            .analysis
            .as_ref()
            .unwrap()
            .scoped_numeric_input_for_test()
            .unwrap();
        let facts = native
            .scoped_numeric_facts(crate::Operation::RegistryFields)
            .unwrap();
        assert!(facts.shared.forms.is_ok(), "{:?}", facts.shared.forms);
        assert!(
            facts.shared.selection.is_ok(),
            "{:?}",
            facts.shared.selection
        );
        assert_eq!(facts.subtypes.len(), 3);
        assert!(
            facts.subtypes.values().all(Result::is_ok),
            "{:?}",
            facts.subtypes
        );

        let mut missing_read = input.clone();
        missing_read.bodies.remove(&input.reader_name);
        assert!(analyze(&missing_read).shared.forms.is_err());

        let mut wrong_token = input.clone();
        let row = wrong_token
            .bodies
            .get_mut(&input.reader_name)
            .unwrap()
            .iter_mut()
            .find(|row| row.operation == "add" && row.operands.ends_with("#0x278"))
            .unwrap();
        row.operands = row.operands.replace("#0x278", "#0x280");
        assert!(analyze(&wrong_token).shared.forms.is_err());

        let mut changed_assign = input.clone();
        changed_assign
            .bodies
            .get_mut(&input.assign_name)
            .unwrap()
            .iter_mut()
            .find(|row| row.operation == "blr")
            .unwrap()
            .operation = "br".into();
        assert!(analyze(&changed_assign).shared.forms.is_err());

        let mut missing_prefix = input.clone();
        missing_prefix
            .names
            .retain(|_, value| value != "\"trigger\"");
        assert!(analyze(&missing_prefix).shared.forms.is_err());

        let mut missing_dispatch = input.clone();
        missing_dispatch.bodies.remove(&input.prefix_name);
        assert!(analyze(&missing_dispatch).shared.forms.is_err());

        let mut missing_selection = input.clone();
        missing_selection.bodies.remove(&input.integer_value_path.1);
        assert!(analyze(&missing_selection).shared.selection.is_err());

        let mut missing_variable = input.clone();
        missing_variable.bodies.remove(&input.variable_name);
        assert!(analyze(&missing_variable).shared.forms.is_err());

        let mut missing_helper = input.clone();
        missing_helper.bodies.remove(&input.helper_names[0]);
        assert!(analyze(&missing_helper).shared.selection.is_err());

        let mut changed_value = input.clone();
        changed_value
            .bodies
            .get_mut(&input.integer_value_path.0)
            .unwrap()
            .iter_mut()
            .find(|row| row.operation == "ldr" && row.operands.ends_with("#0x200]"))
            .unwrap()
            .operation = "str".into();
        assert!(analyze(&changed_value).shared.selection.is_err());

        let mut missing_slot = input.clone();
        let point = *missing_slot.points.keys().next().unwrap();
        missing_slot.pointers.remove(&(point + 0x10));
        assert!(analyze(&missing_slot).subtypes[&point].is_err());
    }

    #[test]
    #[ignore = "requires the exact supported executable through STELLARIS_PATH"]
    fn m452_scoped_numeric_static_parity() {
        let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
        let agenda = native.registry_fields("common/council_agendas").unwrap();
        let cost = agenda
            .value
            .iter()
            .find(|field| field.name == "agenda_cost")
            .unwrap();
        assert_eq!(cost.reader.kind, crate::ReaderKind::ScopedNumeric);
        let conversion = match &cost.reader.numeric {
            crate::GrammarProperty::Partial(Some(value)) => value,
            other => panic!("agenda_cost conversion: {other:?}; gaps: {:?}", agenda.gaps),
        };
        assert_eq!(conversion.width_bits, crate::GrammarProperty::Known(32));
        assert_eq!(conversion.scale, crate::GrammarProperty::Known(Some(1)));
        assert!(matches!(
            cost.reader.scoped_operand,
            crate::GrammarProperty::Known(Some(_))
        ));
        let megastructures = native.registry_fields("common/megastructures").unwrap();
        let purges = native
            .registry_fields("common/species_rights/purge_types")
            .unwrap();
        let trust = native
            .command_grammar(crate::DeclarationKind::Effect, "add_trust")
            .unwrap();
        let crate::GrammarProperty::Known(trust_keys) = &trust.value.fixed_keys else {
            panic!("add_trust keys");
        };
        let actual = serde_json::json!({
            "agenda_cost": public_summary(&cost.reader),
            "cycle_length_in_days": public_summary(&named_field(&megastructures.value, "cycle_length_in_days").reader),
            "overclock_cooldown": public_summary(&named_field(&megastructures.value, "overclock_cooldown").reader),
            "pop_decline_rate": public_summary(&named_field(&purges.value, "pop_decline_rate").reader),
            "add_trust.amount": public_summary(&named_field(trust_keys, "amount").reader),
        });
        assert_eq!(actual, expected()["registry_and_effect"]);
    }

    #[test]
    #[ignore = "requires the exact supported executable through STELLARIS_PATH"]
    fn m452_scoped_numeric_command_parity() {
        let native = crate::Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
        let answer = native
            .command_grammar(crate::DeclarationKind::Effect, "set_timed_country_flag")
            .unwrap();
        let (crate::GrammarProperty::Known(keys) | crate::GrammarProperty::Partial(keys)) =
            &answer.value.fixed_keys
        else {
            panic!("no timed keys");
        };
        let mut actual = serde_json::Map::new();
        for key in ["days", "months", "years"] {
            let field = keys.iter().find(|field| field.name == key).unwrap();
            assert_eq!(field.reader.kind, crate::ReaderKind::ScopedNumeric, "{key}");
            let crate::GrammarProperty::Partial(Some(conversion)) = &field.reader.numeric else {
                panic!("{key}: {:?}", field.reader.numeric);
            };
            assert_eq!(
                conversion.scale,
                crate::GrammarProperty::Known(Some(1)),
                "{key}"
            );
            actual.insert(key.into(), public_summary(&field.reader));
        }
        assert_eq!(serde_json::Value::Object(actual), expected()["timed_flag"]);
    }
}

#[cfg(test)]
mod constructor_parity {
    use crate::{DeclarationKind, GrammarProperty, Native};

    #[test]
    #[ignore = "requires the exact supported executable through STELLARIS_PATH"]
    fn m452_constructor_initial_storage_parity() {
        let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap()).unwrap();
        let field = |kind: DeclarationKind, command: &str, key: &str| {
            let answer = native.command_grammar(kind, command).unwrap();
            let (GrammarProperty::Known(fields) | GrammarProperty::Partial(fields)) =
                answer.value.fixed_keys
            else {
                panic!("{command}: {:?}", answer.gaps);
            };
            let field = fields.into_iter().find(|field| field.name == key).unwrap();
            assert_eq!(field.reader.kind, crate::ReaderKind::ScopedNumeric);
            (field, answer.gaps)
        };
        // Owner stores after the last call that may reach the owner establish these operands.
        for (command, key, width, scale) in [
            ("country_event", "random", 32, 1),
            ("set_saved_date", "expires", 32, 1),
            ("set_timed_relation_flag", "days", 32, 1),
            ("set_timed_relation_flag", "months", 32, 1),
            ("set_timed_relation_flag", "years", 32, 1),
            ("steal_specimens", "count", 32, 1),
            ("ordered_active_first_contact", "order_by", 64, 100000),
            ("add_modifier", "time_multiplier", 64, 100000),
            ("give_culling_rewards", "mult", 64, 100000),
            ("give_culling_rewards", "multiplier", 64, 100000),
            ("steal_planet_output", "percentage", 64, 100000),
            ("create_ambient_object", "scripted_scale", 64, 100000),
        ] {
            let (field, _) = field(DeclarationKind::Effect, command, key);
            let GrammarProperty::Partial(Some(numeric)) = &field.reader.numeric else {
                panic!("{command}.{key}: {:?}", field.reader.numeric);
            };
            assert_eq!(
                numeric.width_bits,
                GrammarProperty::Known(width),
                "{command}.{key}"
            );
            assert_eq!(
                numeric.scale,
                GrammarProperty::Known(Some(scale)),
                "{command}.{key}"
            );
            assert!(
                matches!(field.reader.scoped_operand, GrammarProperty::Known(Some(_))),
                "{command}.{key}"
            );
        }
        // Each destination after a later member's calls has the storage of a proved sibling.
        let int32 = field(DeclarationKind::Effect, "country_event", "random").0;
        let fixed64 = field(DeclarationKind::Effect, "add_modifier", "time_multiplier").0;
        let integer_destinations = EVENTS
            .iter()
            .map(|event| (DeclarationKind::Effect, *event, "days"))
            .chain([
                (
                    DeclarationKind::Effect,
                    "set_saved_date",
                    "days_from_present",
                ),
                (DeclarationKind::Effect, "closest_system", "min_steps"),
                (DeclarationKind::Effect, "closest_system", "max_steps"),
                (DeclarationKind::Trigger, "closest_system", "min_steps"),
                (DeclarationKind::Trigger, "closest_system", "max_steps"),
                (
                    DeclarationKind::Trigger,
                    "num_neighbor_systems",
                    "min_distance",
                ),
                (
                    DeclarationKind::Trigger,
                    "num_neighbor_systems",
                    "max_distance",
                ),
            ]);
        let fixed_point_destinations = [
            (DeclarationKind::Effect, "add_modifier", "mult"),
            (DeclarationKind::Effect, "add_modifier", "multiplier"),
            (DeclarationKind::Effect, "add_stage_modifier", "mult"),
            (DeclarationKind::Effect, "add_stage_modifier", "multiplier"),
            (DeclarationKind::Effect, "create_pop_group", "size"),
            (
                DeclarationKind::Effect,
                "effect_on_blob",
                "owned_planets_percentage",
            ),
            (
                DeclarationKind::Effect,
                "spawn_megastructure",
                "orbit_distance",
            ),
            (
                DeclarationKind::Effect,
                "release_vivarium_fauna_count",
                "count",
            ),
        ];
        let destinations: Vec<_> = integer_destinations
            .map(|destination| (destination, &int32))
            .chain(fixed_point_destinations.map(|destination| (destination, &fixed64)))
            .collect();
        assert_eq!(destinations.len(), 35);
        for ((kind, command, key), sibling) in destinations {
            let (field, _) = field(kind, command, key);
            assert_eq!(
                field.reader.numeric, sibling.reader.numeric,
                "{kind:?} {command}.{key}"
            );
            assert_eq!(
                field.reader.scoped_operand, sibling.reader.scoped_operand,
                "{kind:?} {command}.{key}"
            );
        }
        let purges = native
            .registry_fields("common/species_rights/purge_types")
            .unwrap();
        let field = purges
            .value
            .iter()
            .find(|field| field.name == "pop_decline_rate")
            .unwrap();
        let GrammarProperty::Partial(Some(numeric)) = &field.reader.numeric else {
            panic!("pop_decline_rate: {:?}", field.reader.numeric);
        };
        assert_eq!(numeric.width_bits, GrammarProperty::Known(64));
        assert_eq!(numeric.scale, GrammarProperty::Known(Some(100000)));

        let omitted_counts = TIMED_FLAGS
            .iter()
            .chain(&EVENTS)
            .map(|command| (DeclarationKind::Effect, *command, 0))
            .chain([
                (DeclarationKind::Effect, "add_modifier", -1),
                (DeclarationKind::Effect, "add_stage_modifier", -1),
                (DeclarationKind::Trigger, "has_passed_resolution", 0),
                (DeclarationKind::Effect, "set_timed_relation_flag", 0),
                (DeclarationKind::Effect, "add_timed_trait", 0),
            ]);
        for (kind, command, omitted) in omitted_counts {
            let answer = native.command_grammar(kind, command).unwrap();
            let (GrammarProperty::Known(groups) | GrammarProperty::Partial(groups)) =
                answer.value.durations
            else {
                panic!("{command}: {:?}", answer.gaps);
            };
            assert_eq!(groups.len(), 1, "{command}");
            assert_eq!(
                groups[0].omitted_count,
                GrammarProperty::Known(omitted),
                "{command}"
            );
        }
        // The modifier operands' literal widths keep their storage apart from the count.
        for command in ["add_modifier", "add_stage_modifier"] {
            let answer = native
                .command_grammar(DeclarationKind::Effect, command)
                .unwrap();
            assert!(
                answer
                    .gaps
                    .iter()
                    .all(|gap| !gap.detail.contains("duration-scoped-literal")),
                "{command}: {:?}",
                answer.gaps
            );
        }
    }

    /// The 20 effects that fire an event, each with a scoped `days` operand.
    const EVENTS: [&str; 20] = [
        "agreement_event",
        "astral_rift_event",
        "bypass_event",
        "carrier_event",
        "colony_event",
        "cosmic_storm_event",
        "cosmic_storm_influence_field_event",
        "country_event",
        "espionage_operation_event",
        "first_contact_event",
        "fleet_event",
        "leader_event",
        "observer_event",
        "planet_event",
        "pop_faction_event",
        "pop_group_event",
        "ship_event",
        "situation_event",
        "starbase_event",
        "system_event",
    ];

    /// The ordinary timed flag effects.
    const TIMED_FLAGS: [&str; 27] = [
        "set_timed_agreement_flag",
        "set_timed_ambient_object_flag",
        "set_timed_archaeology_flag",
        "set_timed_army_flag",
        "set_timed_carrier_flag",
        "set_timed_country_flag",
        "set_timed_deposit_flag",
        "set_timed_espionage_asset_flag",
        "set_timed_espionage_operation_flag",
        "set_timed_federation_flag",
        "set_timed_first_contact_flag",
        "set_timed_fleet_flag",
        "set_timed_global_flag",
        "set_timed_leader_flag",
        "set_timed_megastructure_flag",
        "set_timed_planet_flag",
        "set_timed_pop_faction_flag",
        "set_timed_pop_flag",
        "set_timed_pop_group_flag",
        "set_timed_sector_flag",
        "set_timed_ship_flag",
        "set_timed_situation_flag",
        "set_timed_species_flag",
        "set_timed_spynetwork_flag",
        "set_timed_star_flag",
        "set_timed_starbase_flag",
        "set_timed_war_flag",
    ];
}
