//! The owner of a registry that its database's own loader names: the one class with a member
//! reader that the loader builds, or the classes that the root key of each statement selects
//! between.
//!
//! A template registry's owner is a template argument of its loader. A custom loader has no such
//! argument, so the method reads the loader's code. The loader route is every function of the
//! database that calls the file enumeration, and every function of the database that the route
//! calls directly. A class is built on the route when the route calls one of its constructors or
//! forms its primary vtable address point with `adrp` and `add`, which an inlined constructor
//! does. Only classes with a member reader count; the binding gives their constructors and
//! address points.
//!
//! A loader that enumerates several directories builds the items of each in its own code:
//! [`directory_owner`] reads the route from one enumeration call up to the next enumeration call
//! of the same function. This assumes that the compiler places each directory's loop after its
//! call and before the next one, as it does on M452.
//!
//! Exactly one built class is the owner. Several built classes are owners by root key when a route
//! function compares the token of each statement that it reads with a chain of constants, and the
//! code that each comparison selects, up to the next statement read, builds one class for each
//! constant and one class or none for every other token. Any other shape is a gap; the method
//! never selects one of several. A class that the database builds outside the route, such as a
//! scope object on the stack of a query function, is not on the route and does not count.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::decode::{
    Instruction, add_immediate, adrp, call_target, general_register, number, written_registers,
};

/// What a custom loader builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderOwner {
    /// Exactly one class with a member reader.
    Owner(String),
    /// A class for each of some root keys, and one class or none for every other key.
    ByRootKey {
        /// The class that the loader builds for a statement with each key, by key.
        keyed: BTreeMap<String, String>,
        /// The class that the loader builds for a statement with any other key; `None` when it
        /// builds none.
        other: Option<String>,
    },
    /// No function calls the file enumeration, or the route builds no class with a member
    /// reader.
    NoOwner,
    /// Several classes with a member reader and no root keys that select between them; none is
    /// selected.
    Several(Vec<String>),
}

/// The calls that a custom loader makes to read its files.
pub struct LoaderReads<'a> {
    /// Entry addresses of the file enumeration.
    pub enumerations: &'a BTreeSet<u64>,
    /// Entry addresses of the function that reads one statement into the reader.
    pub statement_reads: &'a BTreeSet<u64>,
    /// The reader offset of the statement's token word.
    pub token_offset: u64,
    /// The literal name of each token, by 32-bit identity.
    pub token_names: &'a BTreeMap<u64, String>,
}

/// The owner that the loader in `functions`, every decoded function of one database by its
/// start, builds. `builders` gives the class that each constructor body or primary vtable address
/// point builds.
pub fn loader_owner(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    reads: &LoaderReads<'_>,
    builders: &BTreeMap<u64, String>,
) -> LoaderOwner {
    let entries = functions
        .iter()
        .filter(|(_, rows)| {
            !FunctionAddresses::of(rows)
                .calls
                .is_disjoint(reads.enumerations)
        })
        .map(|(&start, _)| start)
        .collect();

    route_owner(functions, entries, reads, builders)
}

/// The owner that the loader in `functions` builds for the directory that the enumeration calls
/// at `calls` pass: the route starts at each call and ends at the next enumeration call of the
/// same function.
pub fn directory_owner(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    reads: &LoaderReads<'_>,
    builders: &BTreeMap<u64, String>,
    calls: &[u64],
) -> LoaderOwner {
    let mut route_functions = functions.clone();
    for &call in calls {
        if let Some(code) = functions
            .values()
            .find_map(|rows| directory_code(rows, reads, call))
        {
            route_functions.insert(call, code.to_vec());
        }
    }

    route_owner(&route_functions, calls.to_vec(), reads, builders)
}

/// The rows of `rows` from the enumeration call at `call` up to the next enumeration call.
fn directory_code<'a>(
    rows: &'a [Instruction],
    reads: &LoaderReads<'_>,
    call: u64,
) -> Option<&'a [Instruction]> {
    let first = rows.iter().position(|row| row.address == call)?;
    let enumerates = |row: &Instruction| {
        call_target(u32::from_le_bytes(row.bytes), row.address)
            .is_some_and(|target| reads.enumerations.contains(&target))
    };
    let end = rows[first + 1..]
        .iter()
        .position(enumerates)
        .map_or(rows.len(), |offset| first + 1 + offset);

    Some(&rows[first..end])
}

/// The owner that the route from `entries`, and every function of `functions` that it calls
/// directly, builds.
fn route_owner(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    entries: Vec<u64>,
    reads: &LoaderReads<'_>,
    builders: &BTreeMap<u64, String>,
) -> LoaderOwner {
    let addresses: BTreeMap<u64, FunctionAddresses> = functions
        .iter()
        .map(|(&start, rows)| (start, FunctionAddresses::of(rows)))
        .collect();
    let mut pending = entries;
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
        _ => route
            .iter()
            .find_map(|start| {
                root_key_owners(&functions[start], &addresses[start], reads, builders)
            })
            .filter(|(keyed, other)| selects_every_class(keyed, other.as_ref(), &built))
            .map_or(LoaderOwner::Several(built), |(keyed, other)| {
                LoaderOwner::ByRootKey { keyed, other }
            }),
    }
}

/// Whether the classes of a root switch are distinct and are exactly the classes in `built`.
fn selects_every_class(
    keyed: &BTreeMap<String, String>,
    other: Option<&String>,
    built: &[String],
) -> bool {
    let selected: Vec<&String> = keyed.values().chain(other).collect();
    let distinct: BTreeSet<&String> = selected.iter().copied().collect();

    distinct.len() == selected.len() && distinct.into_iter().eq(built.iter())
}

/// The root keys whose tokens `rows` compares each statement's token with, the class that it
/// builds for each, and the class that it builds for any other key, if any. `None` unless the
/// function has one statement read; the next instruction loads its token; a chain of comparisons
/// with named tokens follows, each with a branch on equality, the last one possibly on
/// inequality; each key's side builds exactly one class, and the other side builds at most one.
fn root_key_owners(
    rows: &[Instruction],
    addresses: &FunctionAddresses,
    reads: &LoaderReads<'_>,
    builders: &BTreeMap<u64, String>,
) -> Option<(BTreeMap<String, String>, Option<String>)> {
    let mut statement_reads = addresses
        .targets
        .iter()
        .enumerate()
        .filter(|(position, target)| {
            rows[*position].operation == "bl"
                && target.is_some_and(|target| reads.statement_reads.contains(&target))
        });
    let (read, _) = statement_reads.next()?;
    if statement_reads.next().is_some() {
        return None;
    }

    let load = rows.get(read + 1)?;
    let (base, offset) = reader_argument(&rows[..read])?;
    let (token, word) = loaded_word(load)?;
    if word != (base, offset + reads.token_offset) {
        return None;
    }

    let (keys, other) = token_chain(rows, read + 2, token, reads.token_names)?;
    let loop_head = rows[read].address;
    let built = |side| side_targets(rows, addresses, side, loop_head);
    let keyed = keys
        .into_iter()
        .map(|(key, side)| Some((key, one_class(built(side), builders)?)))
        .collect::<Option<BTreeMap<_, _>>>()?;
    let other = match classes(built(other), builders).as_slice() {
        [] => None,
        [class] => Some(class.clone()),
        _ => return None,
    };

    Some((keyed, other))
}

/// The comparisons of `token` that start at `first`: the side of each named key, and the side of
/// every other token. `None` when no comparison starts there, a constant has no name, or a branch
/// is not on equality or, for the last comparison, inequality.
fn token_chain(
    rows: &[Instruction],
    first: usize,
    token: &str,
    token_names: &BTreeMap<u64, String>,
) -> Option<(Vec<(String, usize)>, usize)> {
    let positions: BTreeMap<u64, usize> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| (row.address, position))
        .collect();
    let mut keys = Vec::new();
    let mut position = first;

    loop {
        let Some((_, operand)) = rows
            .get(position)
            .filter(|compare| compare.operation == "cmp")
            .and_then(|compare| compare.operands.split_once(','))
            .filter(|(compared, _)| *compared == token)
        else {
            return (!keys.is_empty()).then_some((keys, position));
        };
        let constant = match number(operand) {
            Some(constant) => constant,
            None => constant_before(&rows[..position], operand)?,
        };
        let key = token_names.get(&(constant & 0xffff_ffff))?.clone();
        let branch = rows.get(position + 1)?;
        let target = *positions.get(&number(&branch.operands)?)?;

        match branch.operation.as_str() {
            "b.eq" => {
                keys.push((key, target));
                position += 2;
            }
            "b.ne" => {
                keys.push((key, position + 2));
                return Some((keys, target));
            }
            _ => return None,
        }
    }
}

/// The base register and offset of the reader that the call at the end of `rows` receives: the
/// last write to `x0` is `add x0, base, #offset` or `mov x0, base`.
fn reader_argument(rows: &[Instruction]) -> Option<(&str, u64)> {
    let writer = rows
        .iter()
        .rev()
        .find(|row| written_registers(&row.operation, &row.operands).contains(&0))?;
    let operands: Vec<&str> = writer.operands.split(',').collect();
    match (writer.operation.as_str(), operands.as_slice()) {
        ("add", ["x0", base, offset]) => Some((base, number(offset)?)),
        ("mov", ["x0", base]) => Some((base, 0)),
        _ => None,
    }
}

/// The destination of a 32-bit load and the base register and offset that it reads.
fn loaded_word(row: &Instruction) -> Option<(&str, (&str, u64))> {
    if !matches!(row.operation.as_str(), "ldr" | "ldur") {
        return None;
    }

    let (destination, address) = row.operands.split_once(',')?;
    let inside = address.strip_prefix('[')?.strip_suffix(']')?;
    let (base, offset) = inside.split_once(',').unwrap_or((inside, "#0"));

    destination
        .starts_with('w')
        .then_some((destination, (base, number(offset)?)))
}

/// The immediate that the last write to `register` in `rows` moves into it, in code order. A call
/// writes the caller-saved registers, so a constant that a call replaces has none.
fn constant_before(rows: &[Instruction], register: &str) -> Option<u64> {
    let number_of = general_register(register)?;
    let writer = rows
        .iter()
        .rev()
        .find(|row| written_registers(&row.operation, &row.operands).contains(&number_of))?;
    let (destination, value) = writer.operands.split_once(',')?;
    if writer.operation != "mov" || general_register(destination) != Some(number_of) {
        return None;
    }

    number(value)
}

/// The call targets and formed addresses of the code that runs from `start` until it returns,
/// leaves the function or reaches the statement loop at or before `loop_head`.
fn side_targets(
    rows: &[Instruction],
    addresses: &FunctionAddresses,
    start: usize,
    loop_head: u64,
) -> BTreeSet<u64> {
    let positions: BTreeMap<u64, usize> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| (row.address, position))
        .collect();
    let mut visited = BTreeSet::new();
    let mut pending = vec![start];
    let mut targets = BTreeSet::new();

    while let Some(position) = pending.pop() {
        let Some(row) = rows.get(position) else {
            continue;
        };
        if row.address <= loop_head || !visited.insert(position) {
            continue;
        }

        targets.extend(addresses.targets[position]);
        let operation = row.operation.as_str();
        let branch_target = row
            .operands
            .rsplit(',')
            .next()
            .and_then(number)
            .and_then(|target| positions.get(&target).copied());
        let conditional =
            operation.starts_with("b.") || matches!(operation, "cbz" | "cbnz" | "tbz" | "tbnz");
        if conditional || operation == "b" {
            pending.extend(branch_target);
        }
        if !matches!(operation, "b" | "br" | "ret") {
            pending.push(position + 1);
        }
    }

    targets
}

/// The one class that `targets` build.
fn one_class(targets: BTreeSet<u64>, builders: &BTreeMap<u64, String>) -> Option<String> {
    let [class] = classes(targets, builders).try_into().ok()?;

    Some(class)
}

/// The distinct classes that `targets` build.
fn classes(targets: BTreeSet<u64>, builders: &BTreeMap<u64, String>) -> Vec<String> {
    let classes: BTreeSet<&String> = targets.iter().filter_map(|t| builders.get(t)).collect();

    classes.into_iter().cloned().collect()
}

/// The direct call targets of one function and the addresses that it forms with `adrp` and `add`.
struct FunctionAddresses {
    calls: BTreeSet<u64>,
    formed: BTreeSet<u64>,
    /// The call target or formed address of each instruction, by position.
    targets: Vec<Option<u64>>,
}

impl FunctionAddresses {
    fn of(rows: &[Instruction]) -> Self {
        let mut addresses = Self {
            calls: BTreeSet::new(),
            formed: BTreeSet::new(),
            targets: vec![None; rows.len()],
        };
        // The page that an `adrp` left in each register, until the register is written again.
        let mut pages: BTreeMap<usize, u64> = BTreeMap::new();
        for (position, row) in rows.iter().enumerate() {
            let word = u32::from_le_bytes(row.bytes);
            if let Some((destination, page)) = adrp(word, row.address) {
                pages.insert(destination, page);
                continue;
            }

            if let Some(target) = call_target(word, row.address) {
                addresses.calls.insert(target);
                addresses.targets[position] = Some(target);
            }
            if let Some((_, source, offset)) = add_immediate(word)
                && let Some(page) = pages.get(&source)
            {
                addresses.formed.insert(page + offset);
                addresses.targets[position] = Some(page + offset);
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
    const READ_STATEMENT: u64 = 0x9100;
    const TOKEN_OFFSET: u64 = 0x38;
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
            &LoaderReads {
                enumerations: &BTreeSet::from([ENUMERATE]),
                statement_reads: &BTreeSet::from([READ_STATEMENT]),
                token_offset: TOKEN_OFFSET,
                token_names: &BTreeMap::from([
                    (0x2714, "random_list".to_owned()),
                    (0x2715, "fixed_list".to_owned()),
                ]),
            },
            &builders(),
        )
    }

    /// A file reader that reads each statement of the reader at `sp + 0x70`, compares its token
    /// with `random_list`, and runs `keyed` when it is that key and `other` when it is not.
    fn root_switch(
        token_load: u32,
        keyed: impl Fn(&mut Arm64),
        other: impl Fn(&mut Arm64),
    ) -> (u64, Vec<Instruction>) {
        let mut code = Arm64::at(READ_FILE);
        arm64!(code; mov w23, #0x2714); // token 0x2714: random_list
        let statement = code.here();
        arm64!(code; add x0, sp, #0x70);
        code.call(READ_STATEMENT);
        arm64!(code;
            ldr w8, [sp, #token_load]
            ; cmp w8, w23
            ; b.ne >other
        );
        keyed(&mut code);
        arm64!(code; b extern statement as usize; other:);
        other(&mut code);
        arm64!(code; b extern statement as usize);

        decoded(code, READ_FILE)
    }

    fn builds_owner(code: &mut Arm64) {
        code.address(8, OWNER_POINT);
    }

    fn builds_other(code: &mut Arm64) {
        code.call(OTHER_CONSTRUCTOR);
    }

    #[test]
    fn one_root_key_selects_one_of_two_built_classes() {
        let switch = root_switch(0xa8, builds_owner, builds_other);

        assert_eq!(
            owner([loader(READ_FILE), switch]),
            LoaderOwner::ByRootKey {
                keyed: BTreeMap::from([("random_list".into(), "COwner".into())]),
                other: Some("COther".into()),
            }
        );
    }

    #[test]
    fn a_chain_of_root_keys_selects_one_class_for_each_key() {
        let mut code = Arm64::at(READ_FILE);
        arm64!(code; mov w23, #0x2714; mov w24, #0x2715); // tokens random_list and fixed_list
        let statement = code.here();
        arm64!(code; reject:; add x0, sp, #0x70);
        code.call(READ_STATEMENT);
        arm64!(code;
            ldr w8, [sp, #0xa8]
            ; cmp w8, w23
            ; b.eq >first
            ; cmp w8, w24
            ; b.ne <reject
        );
        builds_other(&mut code);
        arm64!(code; b extern statement as usize; first:);
        builds_owner(&mut code);
        arm64!(code; b extern statement as usize);

        assert_eq!(
            owner([loader(READ_FILE), decoded(code, READ_FILE)]),
            LoaderOwner::ByRootKey {
                keyed: BTreeMap::from([
                    ("fixed_list".into(), "COther".into()),
                    ("random_list".into(), "COwner".into()),
                ]),
                other: None,
            }
        );
    }

    #[test]
    fn each_directory_of_a_loader_is_owned_by_the_class_that_its_own_code_builds() {
        let mut code = Arm64::at(LOADER);
        code.call(ENUMERATE);
        builds_owner(&mut code);
        let second_call = code.here();
        code.call(ENUMERATE).call(OTHER_CONSTRUCTOR);
        arm64!(code; ret);
        let functions = BTreeMap::from([decoded(code, LOADER)]);
        let directory = |call| {
            directory_owner(
                &functions,
                &LoaderReads {
                    enumerations: &BTreeSet::from([ENUMERATE]),
                    statement_reads: &BTreeSet::from([READ_STATEMENT]),
                    token_offset: TOKEN_OFFSET,
                    token_names: &BTreeMap::new(),
                },
                &builders(),
                &[call],
            )
        };

        assert_eq!(directory(LOADER), LoaderOwner::Owner("COwner".into()));
        assert_eq!(directory(second_call), LoaderOwner::Owner("COther".into()));
        assert_eq!(
            owner(functions.clone()),
            LoaderOwner::Several(vec!["COther".into(), "COwner".into()])
        );
    }

    #[test]
    fn two_classes_without_a_token_switch_between_them_are_several() {
        let both = |code: &mut Arm64| {
            builds_owner(code);
            builds_other(code);
        };
        let several = LoaderOwner::Several(vec!["COther".into(), "COwner".into()]);

        assert_eq!(
            owner([loader(READ_FILE), root_switch(0xa8, both, builds_other)]),
            several
        );
        // The comparison reads a word of the reader that is not the statement's token.
        assert_eq!(
            owner([
                loader(READ_FILE),
                root_switch(0xa0, builds_owner, builds_other)
            ]),
            several
        );
    }

    #[test]
    fn one_class_on_both_sides_of_the_switch_is_no_root_key() {
        let mut query = Arm64::at(QUERY);
        builds_other(&mut query);
        arm64!(query; ret);
        let mut first = Arm64::at(LOADER);
        first.call(ENUMERATE).call(READ_FILE).call(QUERY);
        arm64!(first; ret);

        assert_eq!(
            owner([
                decoded(first, LOADER),
                root_switch(0xa8, builds_owner, builds_owner),
                decoded(query, QUERY),
            ]),
            LoaderOwner::Several(vec!["COther".into(), "COwner".into()])
        );
    }

    #[test]
    fn a_constant_that_the_statement_read_replaces_selects_no_root_key() {
        let mut code = Arm64::at(READ_FILE);
        arm64!(code; mov w9, #0x2714); // token 0x2714: random_list, in a caller-saved register
        let statement = code.here();
        arm64!(code; add x0, sp, #0x70);
        code.call(READ_STATEMENT);
        arm64!(code; ldr w8, [sp, #0xa8]; cmp w8, w9; b.ne >other);
        builds_owner(&mut code);
        arm64!(code; b extern statement as usize; other:);
        builds_other(&mut code);
        arm64!(code; b extern statement as usize);

        assert_eq!(
            owner([loader(READ_FILE), decoded(code, READ_FILE)]),
            LoaderOwner::Several(vec!["COther".into(), "COwner".into()])
        );
    }

    #[test]
    fn a_constant_changed_after_its_move_selects_no_root_key() {
        let mut code = Arm64::at(READ_FILE);
        arm64!(code; mov w23, #0x2714; add w23, w23, #1);
        let statement = code.here();
        arm64!(code; add x0, sp, #0x70);
        code.call(READ_STATEMENT);
        arm64!(code; ldr w8, [sp, #0xa8]; cmp w8, w23; b.ne >other);
        builds_owner(&mut code);
        arm64!(code; b extern statement as usize; other:);
        builds_other(&mut code);
        arm64!(code; b extern statement as usize);

        assert_eq!(
            owner([loader(READ_FILE), decoded(code, READ_FILE)]),
            LoaderOwner::Several(vec!["COther".into(), "COwner".into()])
        );
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
