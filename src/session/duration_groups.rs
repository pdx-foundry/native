//! Duration groups among a registry's fields, for the duration population run. Registry field
//! answers do not report durations. Not a consumer API.
//!
//! ```no_run
//! use pdx_native::Native;
//! use pdx_native::internals::duration_groups;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! for owner in duration_groups::registry(&native, "common/opinion_modifiers")? {
//!     println!("{:?}: {} groups", owner.path, owner.inventory.groups.len());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use std::collections::BTreeMap;

use super::Native;
use crate::engine::analysis::fields::RegistryFieldResult;
use crate::engine::analysis::stop::Unresolved;
use crate::{Error, Operation};

pub use crate::engine::analysis::durations::{Combination, Consumption, Group, Inventory, Unit};

/// The duration inventory of one owner and its field path: empty for the registry item itself.
#[derive(Debug, Clone)]
pub struct RegistryGroup {
    /// Collection fields from the registry item to the owner.
    pub path: Vec<String>,
    /// The owner's groups and unclassified candidates.
    pub inventory: Inventory,
}

/// Run the duration grouping over one registry's root fields and nested collections, one entry for
/// each owner. Registry items have no execute body, so a shared factor stays unresolved.
pub fn registry(native: &Native, registry: &str) -> Result<Vec<RegistryGroup>, Error> {
    native.method_result(Operation::RegistryFields, || {
        let (input, result) = native.registry_field_input_and_result(registry)?;
        let functions: BTreeMap<u64, &[u8]> = input
            .functions
            .iter()
            .map(|function| (function.address, function.code.as_slice()))
            .collect();
        let code = |address: u64| {
            functions
                .range(..=address)
                .next_back()
                .map(|(&start, code)| (start, *code))
                .filter(|(start, code)| address < start + code.len() as u64)
        };
        let scoped = native.scoped_numeric_facts(Operation::RegistryFields)?;
        let numeric = native.numeric_facts(Operation::RegistryFields)?;
        let storage = crate::engine::analysis::durations::scoped_storage(scoped, numeric);
        let mut groups = Vec::new();

        collect(&result, &[], &code, &storage, &mut groups);

        Ok(groups)
    })
}

fn collect(
    result: &RegistryFieldResult,
    path: &[String],
    code: &crate::engine::analysis::durations::CodeAt<'_>,
    storage: &BTreeMap<u64, Result<u64, Unresolved>>,
    groups: &mut Vec<RegistryGroup>,
) {
    let no_countdown = Err(Unresolved::new("duration-execute-body"));

    let scoped_storage = result
        .scoped_destinations
        .iter()
        .filter_map(|(&offset, point)| Some((offset, storage.get(point)?.clone())))
        .collect();
    groups.push(RegistryGroup {
        path: path.to_vec(),
        inventory: crate::engine::analysis::durations::groups(
            &result.fields,
            &result.paths,
            code,
            &BTreeMap::new(),
            None,
            &no_countdown,
            &scoped_storage,
        ),
    });

    for collection in &result.collections {
        let name = result
            .fields
            .iter()
            .find(|field| field.token == collection.token)
            .map_or_else(|| collection.class.clone(), |field| field.name.clone());
        let mut child = path.to_vec();
        child.push(name);

        collect(&collection.fields, &child, code, storage, groups);
    }
}
