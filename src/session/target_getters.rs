//! Developer-only comparison with command target getter slots. These masks never enter answers.
pub use crate::engine::analysis::grammar::targets::{Bit, Check, Getter};
use crate::engine::analysis::{declarations, decode::decode_arm64, grammar::targets};
use crate::{DeclarationKind, Error, Native, Operation};
use std::collections::{BTreeMap, BTreeSet};

/// Independent getter-slot masks and the per-bit method census.
#[derive(serde::Serialize)]
pub struct Census {
    /// Names ordered by executable scope bit.
    pub scope_names: Option<Vec<String>>,
    /// Commands whose slot returns a constant mask.
    pub commands: BTreeMap<String, u64>,
    /// Typed target getters, named only for developer output.
    pub getters: BTreeMap<String, targets::Getter>,
    /// Scope accessors, including explicit no-null-object bindings.
    pub accessors: BTreeMap<String, targets::Getter>,
}

/// Evaluate the hidden target getter slot and the method's per-bit getter table.
pub fn run(native: &Native, kind: DeclarationKind) -> Result<Census, Error> {
    native.method_result(Operation::CommandGrammar, || {
        let (input, inventory) = native
            .declaration_analysis(Operation::CommandGrammar)?
            .grammar_input(kind)
            .map_err(|error| super::questions::error(Operation::CommandGrammar, error))?;
        let names: BTreeSet<_> = inventory
            .sites
            .iter()
            .filter_map(|(_, site)| match site {
                declarations::Site::Declared { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        let mut commands = BTreeMap::new();
        for name in names {
            let Some(factory) = super::grammar::registered_factory(inventory, &name)
                .ok()
                .flatten()
            else {
                continue;
            };
            let Ok(reader) = declarations::command_reader(&input.declarations, factory) else {
                continue;
            };
            let Some(body) = input
                .declarations
                .pointers
                .get(&(reader.vtable + input.command_bindings.target_getter_slot))
                .and_then(|at| input.declarations.functions.get(at))
            else {
                continue;
            };
            if let Ok(rows) = decode_arm64(&body.code, body.address)
                && let Some(mask) = declarations::constant_return(&rows)
            {
                commands.insert(name, mask);
            }
        }
        let table = targets::getter_table(input);
        let named = |addresses: &BTreeSet<u64>| {
            input
                .symbols
                .iter()
                .filter(|symbol| addresses.contains(&symbol.address))
                .filter_map(|symbol| {
                    table
                        .get(&symbol.address)
                        .map(|getter| (symbol.name.clone(), getter.clone()))
                })
                .collect()
        };
        Ok(Census {
            scope_names: input.declarations.scope_names.clone(),
            commands,
            getters: named(&input.command_bindings.target_getters),
            accessors: named(
                &input
                    .command_bindings
                    .scope_accessors
                    .keys()
                    .copied()
                    .collect(),
            ),
        })
    })
}
