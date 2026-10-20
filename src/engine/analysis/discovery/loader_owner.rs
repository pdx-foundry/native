//! The owner of a registry that its database's own loader names: the one class with a member
//! reader that the loader builds or reads, or the classes that the root key of each statement
//! selects between.
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
//! constant and one class or none for every other token. Otherwise the owner is the one built
//! class whose object a route function reads: it stores the class's address point to the object's
//! first word and passes the object in `x0`, with the reader of its one statement read in `x1`, to
//! a call. The registers are followed in code order, not by path; a constructor call does not
//! count as that store. Any other shape is a gap; the method never selects one of several. A class
//! that the database builds outside the route, such as a scope object on the stack of a query
//! function, is not on the route and does not count.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::decode::{
    Instruction, add_immediate, adrp, call_target, general_register, number, wide_register,
    written_registers,
};

/// What a custom loader builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoaderOwner {
    /// Exactly one class with a member reader that the loader builds, or the one of several that
    /// it reads with the statement reader.
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
    /// Several classes with a member reader, with no root keys that select between them and not
    /// one that the loader reads; none is selected.
    Several(Vec<String>),
}

impl LoaderOwner {
    /// Every class that the loader builds with a member reader.
    pub(crate) fn classes(&self) -> Vec<&String> {
        match self {
            Self::Owner(owner) => vec![owner],
            Self::ByRootKey { keyed, other } => keyed.values().chain(other).collect(),
            Self::NoOwner => vec![],
            Self::Several(classes) => classes.iter().collect(),
        }
    }
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
    route_owner(functions, loader_entries(functions, reads), reads, builders)
}

/// The functions of `functions` that call the file enumeration: where a loader's route starts.
fn loader_entries(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    reads: &LoaderReads<'_>,
) -> Vec<u64> {
    functions
        .iter()
        .filter(|(_, rows)| {
            !FunctionAddresses::of(rows)
                .calls
                .is_disjoint(reads.enumerations)
        })
        .map(|(&start, _)| start)
        .collect()
}

/// The starts of the functions on the route from `entries`: the entries and every function of
/// `functions` that the route calls directly. A called function that calls the file enumeration
/// itself is not followed.
fn route(
    addresses: &BTreeMap<u64, FunctionAddresses>,
    entries: Vec<u64>,
    reads: &LoaderReads<'_>,
) -> BTreeSet<u64> {
    let follows = |call: &u64| {
        addresses
            .get(call)
            .is_some_and(|callee| callee.calls.is_disjoint(reads.enumerations))
    };
    let mut pending = entries;
    let mut route = BTreeSet::new();
    while let Some(start) = pending.pop() {
        if route.insert(start) {
            pending.extend(addresses[&start].calls.iter().filter(|call| follows(call)));
        }
    }

    route
}

/// The owner that the loader in `functions` builds for the directory that the enumeration calls
/// at `calls` pass: the route of each call starts at the call and ends at the next enumeration
/// call of the same function. Calls whose routes give different owners are
/// [`LoaderOwner::Several`], with every class that they build.
pub fn directory_owner(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    reads: &LoaderReads<'_>,
    builders: &BTreeMap<u64, String>,
    calls: &[u64],
) -> LoaderOwner {
    let owners: Vec<LoaderOwner> = calls
        .iter()
        .map(|&call| call_owner(functions, reads, builders, call))
        .collect();

    match owners.split_first() {
        None => LoaderOwner::NoOwner,
        Some((first, rest)) if rest.iter().all(|owner| owner == first) => first.clone(),
        Some(_) => {
            let classes: BTreeSet<&String> = owners.iter().flat_map(LoaderOwner::classes).collect();

            LoaderOwner::Several(classes.into_iter().cloned().collect())
        }
    }
}

/// The owner that the route from the enumeration call at `call` builds; no owner when no
/// function of `functions` makes that call.
fn call_owner(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    reads: &LoaderReads<'_>,
    builders: &BTreeMap<u64, String>,
    call: u64,
) -> LoaderOwner {
    let Some(code) = functions
        .values()
        .find_map(|rows| directory_code(rows, reads, call))
    else {
        return LoaderOwner::NoOwner;
    };
    let mut route_functions = functions.clone();
    route_functions.insert(call, code.to_vec());

    route_owner(&route_functions, vec![call], reads, builders)
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
/// directly, builds. A called function that calls the file enumeration itself is not followed:
/// it loads its own directory, as `CRandomNameDatabase::FillDatabase()` calls
/// `ReadRandomNames()` after it reads its own files.
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
    let route = route(&addresses, entries, reads);
    let built: BTreeSet<&String> = route
        .iter()
        .flat_map(|start| addresses[start].calls.union(&addresses[start].formed))
        .filter_map(|address| builders.get(address))
        .collect();
    let mut built: Vec<String> = built.into_iter().cloned().collect();
    match built.len() {
        0 => LoaderOwner::NoOwner,
        1 => LoaderOwner::Owner(built.remove(0)),
        _ => {
            if let Some((keyed, other)) = route
                .iter()
                .find_map(|start| {
                    root_key_owners(&functions[start], &addresses[start], reads, builders)
                })
                .filter(|(keyed, other)| selects_every_class(keyed, other.as_ref(), &built))
            {
                return LoaderOwner::ByRootKey { keyed, other };
            }

            let read: BTreeSet<String> = route
                .iter()
                .flat_map(|start| {
                    object_reads(&functions[start], &addresses[start], reads, builders)
                })
                .map(|read| read.class)
                .collect();
            match read.into_iter().collect::<Vec<_>>()[..] {
                [ref owner] => LoaderOwner::Owner(owner.clone()),
                _ => LoaderOwner::Several(built),
            }
        }
    }
}

/// One built object that a loader function reads with the reader of its statement read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRead {
    /// The class whose address point the function stores to the object's first word.
    pub class: String,
    /// The position of that store in the function's rows.
    pub store: usize,
    /// The register that holds the object at the store.
    pub register: usize,
    /// The address of the call that reads the object.
    pub call: u64,
}

/// The one object that the loader in `functions`, every decoded function of one database by its
/// start, reads with its statement reader on its route from the file enumeration, with the start
/// of the function that reads it. `None` when no function or several read one.
pub fn read_object(
    functions: &BTreeMap<u64, Vec<Instruction>>,
    reads: &LoaderReads<'_>,
    builders: &BTreeMap<u64, String>,
) -> Option<(u64, ObjectRead)> {
    let addresses: BTreeMap<u64, FunctionAddresses> = functions
        .iter()
        .map(|(&start, rows)| (start, FunctionAddresses::of(rows)))
        .collect();
    let route = route(&addresses, loader_entries(functions, reads), reads);
    let mut found = route.into_iter().flat_map(|start| {
        object_reads(&functions[&start], &addresses[&start], reads, builders)
            .into_iter()
            .map(move |read| (start, read))
    });
    let read = found.next()?;

    found.next().is_none().then_some(read)
}

/// The built objects that `rows` passes, with the reader of its one statement read, to a call.
/// An object is built when the address point of its class is stored to its first word; the call
/// receives the object in `x0` and the reader in `x1`, as a virtual `Read(CReader&)` does. The
/// registers are followed in code order, not by path. Empty unless the function has one statement
/// read.
fn object_reads(
    rows: &[Instruction],
    addresses: &FunctionAddresses,
    reads: &LoaderReads<'_>,
    builders: &BTreeMap<u64, String>,
) -> Vec<ObjectRead> {
    let mut read = Vec::new();
    let Some(statement) = statement_read(rows, addresses, reads) else {
        return read;
    };
    let Some(reader) = argument(&rows[..statement], 0) else {
        return read;
    };
    // The class whose address point each register holds, and the object whose first word holds
    // it: its class, the store and the register at the store.
    let mut points: BTreeMap<usize, &String> = BTreeMap::new();
    let mut objects: BTreeMap<usize, (&String, usize, usize)> = BTreeMap::new();

    for (position, row) in rows.iter().enumerate() {
        let operands: Vec<&str> = row.operands.split(',').collect();
        let is_call = row.operation == "bl" || row.operation.starts_with("blr");
        if is_call
            && let Some(&(class, store, register)) = objects.get(&0)
            && argument(&rows[..position], 1) == Some(reader)
        {
            read.push(ObjectRead {
                class: class.clone(),
                store,
                register,
                call: row.address,
            });
        }

        if let ("str", [value, base]) = (row.operation.as_str(), operands.as_slice())
            && let Some(class) = wide_register(value).and_then(|value| points.get(&value))
            && let Some(base) = base
                .strip_prefix('[')
                .and_then(|base| base.strip_suffix(']'))
                .and_then(wide_register)
        {
            objects.insert(base, (*class, position, base));
        }

        let copied = match (row.operation.as_str(), operands.as_slice()) {
            ("mov", [destination, source]) => wide_register(destination)
                .zip(wide_register(source).and_then(|source| objects.get(&source).copied())),
            _ => None,
        };
        for register in written_registers(&row.operation, &row.operands) {
            points.remove(&register);
            objects.remove(&register);
        }
        objects.extend(copied);
        if let Some((destination, _, _)) = add_immediate(u32::from_le_bytes(row.bytes))
            && let Some(class) = addresses.targets[position].and_then(|point| builders.get(&point))
        {
            points.insert(destination, class);
        }
    }

    read
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
    let read = statement_read(rows, addresses, reads)?;
    let load = rows.get(read + 1)?;
    let (base, offset) = argument(&rows[..read], 0)?;
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

/// The position of the one call to the statement read in `rows`; `None` when there is none or
/// several.
fn statement_read(
    rows: &[Instruction],
    addresses: &FunctionAddresses,
    reads: &LoaderReads<'_>,
) -> Option<usize> {
    let mut statement_reads = addresses
        .targets
        .iter()
        .enumerate()
        .filter(|(position, target)| {
            rows[*position].operation == "bl"
                && target.is_some_and(|target| reads.statement_reads.contains(&target))
        })
        .map(|(position, _)| position);
    let read = statement_reads.next()?;

    statement_reads.next().is_none().then_some(read)
}

/// The base register and offset of the address that a call at the end of `rows` receives in the
/// argument register `x<register>`: its last write is `add x<register>, base, #offset` or
/// `mov x<register>, base`.
fn argument(rows: &[Instruction], register: usize) -> Option<(&str, u64)> {
    let writer = rows
        .iter()
        .rev()
        .find(|row| written_registers(&row.operation, &row.operands).contains(&register))?;
    let operands: Vec<&str> = writer.operands.split(',').collect();
    let (destination, base, offset) = match (writer.operation.as_str(), operands.as_slice()) {
        ("add", [destination, base, offset]) => (destination, base, number(offset)?),
        ("mov", [destination, base]) => (destination, base, 0),
        _ => return None,
    };

    (wide_register(destination) == Some(register)).then_some((base, offset))
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
        let directory = |calls: &[u64]| {
            directory_owner(
                &functions,
                &LoaderReads {
                    enumerations: &BTreeSet::from([ENUMERATE]),
                    statement_reads: &BTreeSet::from([READ_STATEMENT]),
                    token_offset: TOKEN_OFFSET,
                    token_names: &BTreeMap::new(),
                },
                &builders(),
                calls,
            )
        };

        assert_eq!(directory(&[LOADER]), LoaderOwner::Owner("COwner".into()));
        assert_eq!(
            directory(&[second_call]),
            LoaderOwner::Owner("COther".into())
        );
        // Two calls of one directory that build different classes select neither.
        assert_eq!(
            directory(&[LOADER, second_call]),
            LoaderOwner::Several(vec!["COther".into(), "COwner".into()])
        );
        // A call that no decoded function makes builds nothing.
        assert_eq!(directory(&[QUERY]), LoaderOwner::NoOwner);
        assert_eq!(
            owner(functions.clone()),
            LoaderOwner::Several(vec!["COther".into(), "COwner".into()])
        );
    }

    #[test]
    fn a_directory_route_does_not_follow_a_call_into_another_directory_loader() {
        let mut first = Arm64::at(LOADER);
        first.call(ENUMERATE);
        builds_owner(&mut first);
        first.call(READ_FILE);
        arm64!(first; ret);
        let mut second = Arm64::at(READ_FILE);
        second.call(ENUMERATE);
        builds_other(&mut second);
        arm64!(second; ret);
        let functions = BTreeMap::from([decoded(first, LOADER), decoded(second, READ_FILE)]);
        let reads = LoaderReads {
            enumerations: &BTreeSet::from([ENUMERATE]),
            statement_reads: &BTreeSet::from([READ_STATEMENT]),
            token_offset: TOKEN_OFFSET,
            token_names: &BTreeMap::new(),
        };

        assert_eq!(
            directory_owner(&functions, &reads, &builders(), &[LOADER]),
            LoaderOwner::Owner("COwner".into())
        );
        assert_eq!(
            directory_owner(&functions, &reads, &builders(), &[READ_FILE]),
            LoaderOwner::Owner("COther".into())
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
    fn the_object_that_the_loader_reads_with_the_statement_reader_selects_its_class() {
        let read_with = |reader_offset: u32| {
            let mut code = Arm64::at(READ_FILE);
            let statement = code.here();
            arm64!(code; add x0, sp, #0x70);
            code.call(READ_STATEMENT);
            arm64!(code; add x0, sp, #16);
            code.call(OTHER_CONSTRUCTOR);
            code.address(8, OWNER_POINT);
            arm64!(code;
                str x8, [x19]
                ; ldr x8, [x19]
                ; ldr x8, [x8, #0x20]
                ; add x1, sp, #reader_offset
                ; mov x0, x19
                ; blr x8
                ; b extern statement as usize
            );

            owner([loader(READ_FILE), decoded(code, READ_FILE)])
        };

        assert_eq!(read_with(0x70), LoaderOwner::Owner("COwner".into()));
        // The call reads the object with another reader than the statement's.
        assert_eq!(
            read_with(0xa0),
            LoaderOwner::Several(vec!["COther".into(), "COwner".into()])
        );
    }

    #[test]
    fn the_read_object_is_read_on_the_loader_route() {
        let mut code = Arm64::at(READ_FILE);
        let statement = code.here();
        arm64!(code; add x0, sp, #0x70);
        code.call(READ_STATEMENT);
        code.address(8, OWNER_POINT);
        arm64!(code;
            str x8, [x19]
            ; ldr x8, [x19]
            ; ldr x8, [x8, #0x20]
            ; add x1, sp, #0x70
            ; mov x0, x19
            ; blr x8
            ; b extern statement as usize
        );
        let reads = LoaderReads {
            enumerations: &BTreeSet::from([ENUMERATE]),
            statement_reads: &BTreeSet::from([READ_STATEMENT]),
            token_offset: TOKEN_OFFSET,
            token_names: &BTreeMap::new(),
        };
        let read_file = decoded(code, READ_FILE);
        let read_from = |next: u64| {
            let functions = BTreeMap::from([loader(next), read_file.clone()]);

            read_object(&functions, &reads, &builders()).map(|(start, read)| (start, read.class))
        };

        assert_eq!(read_from(READ_FILE), Some((READ_FILE, "COwner".into())));
        // A method of the database that the loader does not call reads no item.
        assert_eq!(read_from(QUERY), None);
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
