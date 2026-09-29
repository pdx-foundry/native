//! Duration groups among a registry's fields, for the duration population run. Registry field
//! answers do not report durations. Not a consumer API.
//!
//! ```no_run
//! use pdx_native::Native;
//! use pdx_native::internals::duration_groups;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! for group in duration_groups::registry(&native, "common/opinion_modifiers")? {
//!     println!("{:?}: {:?}", group.path, group.group.combination);
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use std::collections::BTreeMap;

use super::Native;
use crate::engine::analysis::fields::RegistryFieldResult;
use crate::engine::analysis::stop::Unresolved;
use crate::{Error, Operation};

pub use crate::engine::analysis::durations::{Combination, Consumption, Group, Unit};

/// One duration group and the field path of its owner: empty for the registry item itself.
#[derive(Debug, Clone)]
pub struct RegistryGroup {
    /// Collection fields from the registry item to the group's owner.
    pub path: Vec<String>,
    /// The group, with its unresolved parts.
    pub group: Group,
}

/// Run the duration grouping over one registry's root fields and nested collections. Registry
/// items have no execute body, so a shared factor stays unresolved.
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
        let mut groups = Vec::new();

        collect(&result, &[], &code, &mut groups);

        Ok(groups)
    })
}

fn collect(
    result: &RegistryFieldResult,
    path: &[String],
    code: &crate::engine::analysis::durations::CodeAt<'_>,
    groups: &mut Vec<RegistryGroup>,
) {
    let no_countdown = Err(Unresolved::new("duration-execute-body"));

    groups.extend(
        crate::engine::analysis::durations::groups(
            &result.fields,
            &result.paths,
            code,
            &BTreeMap::new(),
            None,
            &no_countdown,
        )
        .into_iter()
        .map(|group| RegistryGroup {
            path: path.to_vec(),
            group,
        }),
    );

    for collection in &result.collections {
        let name = result
            .fields
            .iter()
            .find(|field| field.token == collection.token)
            .map_or_else(|| collection.class.clone(), |field| field.name.clone());
        let mut child = path.to_vec();
        child.push(name);

        collect(&collection.fields, &child, code, groups);
    }
}
