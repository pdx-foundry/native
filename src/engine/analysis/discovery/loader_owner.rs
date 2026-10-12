//! The owner of a registry that its database's own loader names: the one class with a member
//! reader that the loader builds.
//!
//! A template registry's owner is a template argument of its loader. A custom loader has no such
//! argument, so the method reads the loader's code. The loader route is every function of the
//! database that calls the file enumeration, and every function of the database that the route
//! calls directly. A class is built on the route when the route calls one of its constructors or
//! forms its primary vtable address point with `adrp` and `add`, which an inlined constructor
//! does. Only classes with a member reader count; the binding gives their constructors and
//! address points.
//!
//! Exactly one built class is the owner. None, or several, is a gap; the method never selects one
//! of several. A class that the database builds outside the route, such as a scope object on the
//! stack of a query function, is not on the route and does not count.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::decode::{
    Instruction, add_immediate, adrp, call_target, written_registers,
};

/// What a custom loader builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderOwner {
    /// Exactly one class with a member reader.
    Owner(String),
    /// No function calls the file enumeration, or the route builds no class with a member
    /// reader.
    NoOwner,
    /// Several classes with a member reader; none is selected.
    Several(Vec<String>),
}

/// The owner that the loader in `functions`, every decoded function of one database by its
/// start, builds. `enumerations` are the file enumeration's bodies, and `builders` gives the
/// class that each constructor body or primary vtable address point builds.
pub fn loader_owner(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    enumerations: &BTreeSet<u64>,
    builders: &BTreeMap<u64, String>,
) -> LoaderOwner {
    let addresses: BTreeMap<u64, FunctionAddresses> = functions
        .iter()
        .map(|(&start, rows)| (start, FunctionAddresses::of(rows)))
        .collect();
    let mut pending: Vec<u64> = addresses
        .iter()
        .filter(|(_, function)| !function.calls.is_disjoint(enumerations))
        .map(|(&start, _)| start)
        .collect();
    let mut route = BTreeSet::new();
    while let Some(start) = pending.pop() {
        if route.insert(start) {
            let calls = &addresses[&start].calls;
            pending.extend(calls.iter().filter(|call| addresses.contains_key(call)));
        }
    }

    let built: BTreeSet<&String> = route
        .iter()
        .flat_map(|start| addresses[start].calls.union(&addresses[start].formed))
        .filter_map(|address| builders.get(address))
        .collect();
    let mut built: Vec<String> = built.into_iter().cloned().collect();
    match built.len() {
        0 => LoaderOwner::NoOwner,
        1 => LoaderOwner::Owner(built.remove(0)),
        _ => LoaderOwner::Several(built),
    }
}

/// The direct call targets of one function and the addresses that it forms with `adrp` and `add`.
struct FunctionAddresses {
    calls: BTreeSet<u64>,
    formed: BTreeSet<u64>,
}

impl FunctionAddresses {
    fn of(rows: &[Instruction]) -> Self {
        let mut addresses = Self {
            calls: BTreeSet::new(),
            formed: BTreeSet::new(),
        };
        // The page that an `adrp` left in each register, until the register is written again.
        let mut pages: BTreeMap<usize, u64> = BTreeMap::new();
        for row in rows {
            let word = u32::from_le_bytes(row.bytes);
            if let Some((destination, page)) = adrp(word, row.address) {
                pages.insert(destination, page);
                continue;
            }

            if let Some(target) = call_target(word, row.address) {
                addresses.calls.insert(target);
            }
            if let Some((_, source, offset)) = add_immediate(word)
                && let Some(page) = pages.get(&source)
            {
                addresses.formed.insert(page + offset);
            }
            for register in written_registers(&row.operation, &row.operands) {
                pages.remove(&register);
            }
        }

        addresses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::assembler::{Arm64, arm64};
    use crate::engine::analysis::decode::decode_arm64;

    const ENUMERATE: u64 = 0x9000;
    const LOADER: u64 = 0x1000;
    const READ_FILE: u64 = 0x2000;
    const QUERY: u64 = 0x3000;
    const OWNER_POINT: u64 = 0x5_0010;
    const OTHER_CONSTRUCTOR: u64 = 0x8000;
    const SCOPE_CONSTRUCTOR: u64 = 0x8100;

    fn builders() -> BTreeMap<u64, String> {
        BTreeMap::from([
            (OWNER_POINT, "COwner".to_owned()),
            (OTHER_CONSTRUCTOR, "COther".to_owned()),
            (SCOPE_CONSTRUCTOR, "CScope".to_owned()),
        ])
    }

    fn decoded(code: Arm64, start: u64) -> (u64, Vec<Instruction>) {
        (start, decode_arm64(&code.bytes(), start).unwrap())
    }

    /// A loader that enumerates its files and then calls `next`.
    fn loader(next: u64) -> (u64, Vec<Instruction>) {
        let mut code = Arm64::at(LOADER);
        code.call(ENUMERATE).call(next);
        arm64!(code; ret);

        decoded(code, LOADER)
    }

    /// A file reader that allocates an object and inlines its constructor.
    fn inlined_owner() -> (u64, Vec<Instruction>) {
        let mut code = Arm64::at(READ_FILE);
        code.address(8, OWNER_POINT);
        arm64!(code; str x8, [x0]; ret);

        decoded(code, READ_FILE)
    }

    /// A query that builds a scope object on its stack.
    fn scoped_query() -> (u64, Vec<Instruction>) {
        let mut code = Arm64::at(QUERY);
        arm64!(code; add x0, sp, #16);
        code.call(SCOPE_CONSTRUCTOR);
        arm64!(code; ret);

        decoded(code, QUERY)
    }

    fn owner(functions: impl IntoIterator<Item = (u64, Vec<Instruction>)>) -> LoaderOwner {
        loader_owner(
            &functions.into_iter().collect(),
            &BTreeSet::from([ENUMERATE]),
            &builders(),
        )
    }

    #[test]
    fn the_owner_is_the_class_that_the_loader_route_builds() {
        assert_eq!(
            owner([loader(READ_FILE), inlined_owner(), scoped_query()]),
            LoaderOwner::Owner("COwner".into())
        );
    }

    #[test]
    fn two_built_classes_are_not_one_owner() {
        let mut code = Arm64::at(READ_FILE);
        code.address(8, OWNER_POINT).call(OTHER_CONSTRUCTOR);
        arm64!(code; ret);

        assert_eq!(
            owner([loader(READ_FILE), decoded(code, READ_FILE)]),
            LoaderOwner::Several(vec!["COther".into(), "COwner".into()])
        );
    }

    #[test]
    fn a_class_built_off_the_route_or_without_an_enumeration_is_not_the_owner() {
        assert_eq!(
            owner([loader(READ_FILE), scoped_query()]),
            LoaderOwner::NoOwner
        );
        assert_eq!(owner([inlined_owner()]), LoaderOwner::NoOwner);
    }

    #[test]
    fn a_page_that_a_load_replaced_forms_no_address() {
        let mut code = Arm64::at(READ_FILE);
        code.load(8, OWNER_POINT - 8);
        arm64!(code;
            add x8, x8, #0x10 // the stale page plus 0x10 would be the owner's point
            ; ret
        );

        assert_eq!(
            owner([loader(READ_FILE), decoded(code, READ_FILE)]),
            LoaderOwner::NoOwner
        );
    }
}
