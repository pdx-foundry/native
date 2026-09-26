//! Local storage selections. Loop joins retain possible collection elements and unknown origins.
//! The enclosing reachability/selection predicate is deliberately unresolved; these are not
//! complete runtime rules. No use-time predicate is added to a loader alternative.
use super::tokens::{number, register};
use super::{CollectionField, FieldInput, RootField, StorageSelection};
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Unknown,
    Aligned(u32),
    Owner(i64),
    Buffer(usize),
    Element {
        collection: usize,
        site: u64,
        offset: i64,
    },
    Selected {
        collection: usize,
        site: u64,
        offset: i64,
    },
    Loaded(Box<Origin>, u8),
}
type Values = BTreeSet<Origin>;
type Registers = BTreeMap<String, Values>;
const LIMIT: usize = 16;

fn unknown() -> Values {
    BTreeSet::from([Origin::Unknown])
}
fn value(registers: &Registers, operand: &str) -> Values {
    if let Some(immediate) = number(operand).filter(|_| operand.starts_with('#')) {
        return BTreeSet::from([Origin::Aligned((immediate as u64).trailing_zeros())]);
    }
    register(operand)
        .and_then(|key| registers.get(&key).cloned())
        .unwrap_or_else(unknown)
}
fn set(registers: &mut Registers, destination: &str, mut values: Values) {
    if let Some(key) = register(destination) {
        if destination.starts_with('w') {
            values = values
                .into_iter()
                .map(|origin| match origin {
                    Origin::Loaded(base, width) => Origin::Loaded(base, width.min(4)),
                    Origin::Aligned(bits) => Origin::Aligned(bits.min(32)),
                    _ => Origin::Unknown,
                })
                .collect();
        }
        if values.len() > LIMIT {
            values = unknown();
        }
        registers.insert(key, values);
    }
}
fn offset(value: &Origin, amount: i64) -> Origin {
    match value {
        Origin::Aligned(bits) => Origin::Aligned((*bits).min((amount as u64).trailing_zeros())),
        Origin::Owner(at) => at
            .checked_add(amount)
            .map(Origin::Owner)
            .unwrap_or(Origin::Unknown),
        Origin::Element {
            collection,
            site,
            offset,
        } => offset
            .checked_add(amount)
            .map(|offset| Origin::Element {
                collection: *collection,
                site: *site,
                offset,
            })
            .unwrap_or(Origin::Unknown),
        Origin::Selected {
            collection,
            site,
            offset,
        } => offset
            .checked_add(amount)
            .map(|offset| Origin::Selected {
                collection: *collection,
                site: *site,
                offset,
            })
            .unwrap_or(Origin::Unknown),
        _ => Origin::Unknown,
    }
}
fn address(operand: &str) -> Option<(&str, &str)> {
    let body = operand.strip_prefix('[')?.strip_suffix(']')?;
    let (base, rest) = body.split_once(',').unwrap_or((body, "#0"));
    Some((base, rest))
}
fn load(
    base: &Origin,
    amount: Option<i64>,
    aligned: bool,
    width: u8,
    site: u64,
    collections: &[CollectionField],
) -> Origin {
    if let Origin::Buffer(collection) = base {
        return if width == 8 && aligned {
            Origin::Element {
                collection: *collection,
                site,
                offset: 0,
            }
        } else {
            Origin::Unknown
        };
    }
    let Some(amount) = amount else {
        return Origin::Unknown;
    };
    let at = offset(base, amount);
    if let Origin::Owner(at) = at
        && width == 8
        && let Some(index) = collections.iter().position(|collection| {
            collection
                .data_offset
                .is_some_and(|data| collection.offset.checked_add(data) == u64::try_from(at).ok())
        })
    {
        return Origin::Buffer(index);
    }
    match at {
        Origin::Owner(_) | Origin::Element { .. } | Origin::Selected { .. } => {
            Origin::Loaded(Box::new(at), width)
        }
        _ => Origin::Unknown,
    }
}

fn loaded_values(
    row: &Instruction,
    to: &str,
    memory: &str,
    registers: &Registers,
    collections: &[CollectionField],
) -> Values {
    let Some((base, index)) = address(memory) else {
        return unknown();
    };
    let amount = number(index);
    let (index_register, shift) = index.split_once(',').unwrap_or((index, ""));
    let scale = shift.strip_prefix("lsl").and_then(number).unwrap_or(0);
    let aligned = match amount {
        Some(amount) => amount % 8 == 0,
        None => scale >= 3
            || value(registers, index_register).iter().all(
                |origin| matches!(origin, Origin::Aligned(bits) if i64::from(*bits) + scale >= 3),
            ),
    };
    let width = match row.operation.as_str() {
        "ldrb" | "ldrsb" => 1,
        "ldrh" => 2,
        "ldrsw" => 4,
        _ if to.starts_with('x') => 8,
        _ => 4,
    };
    value(registers, base)
        .iter()
        .map(|origin| load(origin, amount, aligned, width, row.address, collections))
        .collect()
}

fn transfer(row: &Instruction, registers: &mut Registers, collections: &[CollectionField]) {
    let args: Vec<_> = row.operands.split(',').collect();
    match (row.operation.as_str(), args.as_slice()) {
        ("mov", [to, from]) => set(registers, to, value(registers, from)),
        ("add" | "sub", [to, from, immediate]) => {
            let amount = number(immediate).map(|amount| {
                if row.operation == "sub" {
                    -amount
                } else {
                    amount
                }
            });
            let values = value(registers, from)
                .iter()
                .map(|origin| amount.map_or(Origin::Unknown, |amount| offset(origin, amount)))
                .collect();
            set(registers, to, values);
        }
        ("ldr" | "ldrb" | "ldrh" | "ldrsb" | "ldrsw", _) => {
            if let Some((to, memory)) = row.operands.split_once(',') {
                let values = loaded_values(row, to, memory, registers, collections);
                set(registers, to, values);
            }
        }
        ("lsl", [to, from, shift]) => {
            let shift = number(shift).filter(|shift| (0..64).contains(shift));
            let values = value(registers, from)
                .iter()
                .map(|origin| match (origin, shift) {
                    (Origin::Aligned(bits), Some(shift)) => {
                        Origin::Aligned((*bits + shift as u32).min(64))
                    }
                    (_, Some(shift)) => Origin::Aligned(shift as u32),
                    _ => Origin::Unknown,
                })
                .collect();
            set(registers, to, values);
        }
        ("csel", [to, left, right, _]) => {
            let mut values = value(registers, left);
            values.extend(value(registers, right));
            set(registers, to, values);
        }
        ("bl" | "blr", _) => {
            for index in 0..19 {
                set(registers, &format!("x{index}"), unknown());
            }
        }
        ("ldp", [left, right, ..]) => {
            set(registers, left, unknown());
            set(registers, right, unknown());
        }
        (
            "str" | "strb" | "strh" | "stp" | "cmp" | "cmn" | "tst" | "nop" | "ret" | "b" | "cbz"
            | "cbnz" | "tbz" | "tbnz",
            _,
        ) => {}
        (operation, _) if operation.starts_with("b.") => {}
        (_, [to, ..]) => set(registers, to, unknown()),
        _ => {
            registers.clear();
        }
    }
    // A pre/post-index address changes its base even when the value transfer is unsupported.
    if (row.operands.contains("]!") || row.operands.contains("],#"))
        && let Some(memory) = row.operands.split('[').nth(1)
        && let Some(base) = memory.split([',', ']']).next()
    {
        set(registers, base, unknown());
    }
}

fn successors(rows: &[Instruction], indexes: &BTreeMap<u64, usize>, pc: usize) -> Vec<usize> {
    let row = &rows[pc];
    let target = row
        .operands
        .rsplit(',')
        .next()
        .and_then(number)
        .and_then(|at| indexes.get(&(at as u64)))
        .copied();
    if matches!(row.operation.as_str(), "ret" | "br" | "brk") {
        return vec![];
    }
    if row.operation == "b" {
        return target.into_iter().collect();
    }
    let mut next: Vec<_> = (pc + 1 < rows.len())
        .then_some(pc + 1)
        .into_iter()
        .collect();
    if row.operation.starts_with("b.")
        || matches!(row.operation.as_str(), "cbz" | "cbnz" | "tbz" | "tbnz")
    {
        next.extend(target);
    }
    next
}
fn merge(destination: &mut Registers, source: &Registers) -> bool {
    let previous = destination.clone();
    let keys: BTreeSet<_> = destination.keys().chain(source.keys()).cloned().collect();
    for key in keys {
        let mut values = value(destination, &key);
        values.extend(value(source, &key));
        set(destination, &key, values);
    }
    *destination != previous
}
fn flow(
    rows: &[Instruction],
    indexes: &BTreeMap<u64, usize>,
    collections: &[CollectionField],
) -> Option<Vec<Option<Registers>>> {
    let mut states = vec![None; rows.len()];
    states[0] = Some(BTreeMap::from([(
        "x0".into(),
        BTreeSet::from([Origin::Owner(0)]),
    )]));
    let mut pending = VecDeque::from([0]);
    let mut steps = 0;
    while let Some(pc) = pending.pop_front() {
        steps += 1;
        if steps > rows.len() * 64 {
            return None;
        }
        let mut state = states[pc].clone().unwrap();
        transfer(&rows[pc], &mut state, collections);
        for next in successors(rows, indexes, pc) {
            match &mut states[next] {
                Some(previous) => {
                    if merge(previous, &state) {
                        pending.push_back(next);
                    }
                }
                destination @ None => {
                    *destination = Some(state.clone());
                    pending.push_back(next);
                }
            }
        }
    }
    Some(states)
}

fn storage(field: &RootField) -> Option<i64> {
    let mut offsets = BTreeSet::new();
    for join in &field.readers {
        offsets.insert(crate::engine::analysis::readers::destination(join)?);
    }
    (offsets.len() == 1).then(|| *offsets.first().unwrap())
}
fn named(
    origin: &Origin,
    fields: &[RootField],
    collections: &[CollectionField],
) -> Option<Vec<String>> {
    let (members, at, mut path) = match origin {
        Origin::Owner(at) => (fields, *at, Vec::new()),
        Origin::Selected {
            collection, offset, ..
        } => {
            let collection = &collections[*collection];
            let parent = fields
                .iter()
                .find(|field| field.token == collection.token)?;
            (
                collection.fields.fields.as_slice(),
                *offset,
                vec![parent.name.clone()],
            )
        }
        _ => return None,
    };
    let mut matches = members.iter().filter(|field| storage(field) == Some(at));
    let field = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    path.push(field.name.clone());
    Some(path)
}
fn element(origin: &Origin) -> Option<(usize, u64)> {
    match origin {
        Origin::Selected {
            collection, site, ..
        } => Some((*collection, *site)),
        _ => None,
    }
}
fn same_element(origin: &Origin, tested: Option<(usize, u64)>) -> bool {
    matches!(origin, Origin::Owner(_))
        || element(origin).is_some_and(|element| Some(element) == tested)
}

struct TestedField {
    path: Vec<String>,
    element: Option<(usize, u64)>,
}

fn tested(
    values: Values,
    fields: &[RootField],
    collections: &[CollectionField],
) -> Vec<TestedField> {
    values
        .iter()
        .filter_map(|value| match value {
            Origin::Loaded(origin, 1) => {
                let path = named(origin, fields, collections)?;
                let members = match origin.as_ref() {
                    Origin::Owner(_) => fields,
                    Origin::Selected { collection, .. } => &collections[*collection].fields.fields,
                    _ => return None,
                };
                let field = members
                    .iter()
                    .find(|field| Some(&field.name) == path.last())?;
                (crate::engine::analysis::readers::classify(&field.readers).kind
                    == crate::ReaderKind::Boolean)
                    .then_some(TestedField {
                        path,
                        element: element(origin),
                    })
            }
            _ => None,
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Storage<'a> {
    fields: &'a [RootField],
    collections: &'a [CollectionField],
}

/// Whether the signature establishes a direct const member of this owner class.
/// Unqualified members may be static; nested-class methods have a different receiver.
pub(crate) fn has_owner_receiver(name: &str, owner: &str) -> bool {
    let Some(member) = name
        .strip_prefix(owner)
        .and_then(|suffix| suffix.strip_prefix("::"))
    else {
        return false;
    };
    let Some((method, parameters)) = member.split_once('(') else {
        return false;
    };
    if method.contains("::") {
        return false;
    }
    let mut depth = 1;
    for (index, character) in parameters.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return &parameters[index + 1..] == " const";
                }
            }
            _ => {}
        }
    }
    false
}

pub(super) fn discover(
    input: &FieldInput,
    fields: &[RootField],
    collections: &[CollectionField],
) -> Vec<StorageSelection> {
    if collections.is_empty() {
        return Vec::new();
    }
    let receivers: BTreeSet<_> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.name.ends_with(") const"))
        .map(|symbol| symbol.address)
        .collect();
    let storage = Storage {
        fields,
        collections,
    };
    let mut found = Vec::new();
    for function in input
        .functions
        .iter()
        .filter(|function| has_owner_receiver(&function.name, &input.selection.owner_candidate))
    {
        let Ok(rows) = decode_arm64(&function.code, function.address) else {
            continue;
        };
        if rows.is_empty() {
            continue;
        }
        let indexes = rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.address, index))
            .collect();
        let Some(states) = flow(&rows, &indexes, collections) else {
            continue;
        };
        let mut predecessors = vec![Vec::new(); rows.len()];
        for pc in 0..rows.len() {
            for next in successors(&rows, &indexes, pc) {
                predecessors[next].push(pc);
            }
        }
        let method = Method {
            rows: &rows,
            indexes: &indexes,
            states: &states,
            predecessors: &predecessors,
            storage,
            receivers: &receivers,
        };
        for (pc, row) in rows.iter().enumerate() {
            let selections = match row.operation.as_str() {
                "cbz" | "cbnz" => method.branch(pc),
                "csel" => method.conditional_select(pc),
                _ => Vec::new(),
            };
            found.extend(
                selections
                    .into_iter()
                    .map(|(field, tested, zero)| StorageSelection {
                        method: function.name.clone(),
                        field,
                        tested,
                        zero,
                    }),
            );
        }
    }
    found.sort_by(|left, right| {
        (&left.method, &left.field, &left.tested, left.zero).cmp(&(
            &right.method,
            &right.field,
            &right.tested,
            right.zero,
        ))
    });
    found.dedup();
    found
}

struct Method<'a> {
    rows: &'a [Instruction],
    indexes: &'a BTreeMap<u64, usize>,
    states: &'a [Option<Registers>],
    predecessors: &'a [Vec<usize>],
    storage: Storage<'a>,
    receivers: &'a BTreeSet<u64>,
}

type Selection = (Vec<String>, Vec<String>, bool);

impl Method<'_> {
    /// Rebase one live receiver at the flag load. A loop load site supplies possible
    /// collection provenance, but only copies from this snapshot prove object identity.
    fn flag_state(&self, gate: usize, tested_register: &str) -> Option<Registers> {
        let mut pc = gate;
        for _ in 0..32 {
            let [previous] = self.predecessors[pc].as_slice() else {
                return None;
            };
            if *previous + 1 != pc || successors(self.rows, self.indexes, *previous) != [pc] {
                return None;
            }
            pc = *previous;
            let row = &self.rows[pc];
            if matches!(row.operation.as_str(), "bl" | "blr") {
                return None;
            }
            let Some((destination, memory)) = row.operands.split_once(',') else {
                continue;
            };
            if row.operation != "ldrb" || register(destination) != register(tested_register) {
                continue;
            }
            let (base, _) = address(memory)?;
            let mut state = self.states[pc].clone()?;
            let selected = value(&state, base)
                .into_iter()
                .map(|origin| match origin {
                    Origin::Element {
                        collection, offset, ..
                    } => Origin::Selected {
                        collection,
                        offset,
                        site: row.address,
                    },
                    other => other,
                })
                .collect();
            set(&mut state, base, selected);
            for row in &self.rows[pc..gate] {
                transfer(row, &mut state, self.storage.collections);
            }
            return Some(state);
        }
        None
    }

    fn branch(&self, pc: usize) -> Vec<Selection> {
        let row = &self.rows[pc];
        let Some((register, target)) = row.operands.split_once(',') else {
            return Vec::new();
        };
        let Some(state) = self.flag_state(pc, register) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for tested in tested(
            value(&state, register),
            self.storage.fields,
            self.storage.collections,
        ) {
            let branch = number(target)
                .and_then(|at| self.indexes.get(&(at as u64)))
                .copied()
                .unwrap_or(self.rows.len());
            for (target, zero) in [
                (pc + 1, row.operation == "cbnz"),
                (branch, row.operation == "cbz"),
            ] {
                for field in arm(
                    self.rows,
                    self.indexes,
                    target,
                    state.clone(),
                    self.storage,
                    tested.element,
                    self.receivers,
                ) {
                    found.push((field, tested.path.clone(), zero));
                }
            }
        }
        found
    }

    fn conditional_select(&self, pc: usize) -> Vec<Selection> {
        let args: Vec<_> = self.rows[pc].operands.split(',').collect();
        if args.len() != 4
            || !matches!(args[3], "eq" | "ne")
            || pc == 0
            || self.predecessors[pc] != [pc - 1]
        {
            return Vec::new();
        }
        let compare = &self.rows[pc - 1];
        let Some((register, immediate)) = compare.operands.split_once(',') else {
            return Vec::new();
        };
        if compare.operation != "cmp" || number(immediate) != Some(0) {
            return Vec::new();
        }
        let Some(state) = self.flag_state(pc, register) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for tested in tested(
            value(&state, register),
            self.storage.fields,
            self.storage.collections,
        ) {
            for (operand, zero) in [(args[1], args[3] == "eq"), (args[2], args[3] == "ne")] {
                for origin in value(&state, operand) {
                    if let Some(field) =
                        named(&origin, self.storage.fields, self.storage.collections)
                        && same_element(&origin, tested.element)
                    {
                        found.push((field, tested.path.clone(), zero));
                    }
                }
            }
        }
        found
    }
}

/// Follow a bounded local branch until a field-consuming call, return, or external tail call.
/// Other calls clobber their scratch registers and traversal continues.
fn arm(
    rows: &[Instruction],
    indexes: &BTreeMap<u64, usize>,
    start: usize,
    registers: Registers,
    storage: Storage<'_>,
    subject: Option<(usize, u64)>,
    receivers: &BTreeSet<u64>,
) -> BTreeSet<Vec<String>> {
    let Storage {
        fields,
        collections,
    } = storage;
    let mut pending = vec![(start, registers, BTreeSet::new())];
    let mut found = BTreeSet::new();
    let mut steps = 0;
    while let Some((pc, mut state, mut seen)) = pending.pop() {
        steps += 1;
        if steps > 256 {
            break;
        }
        let Some(row) = rows.get(pc) else { continue };
        if !seen.insert(pc) {
            continue;
        }
        if matches!(row.operation.as_str(), "bl" | "blr" | "ret")
            || row.operation == "b" && successors(rows, indexes, pc).is_empty()
        {
            // A const member symbol proves x0 is the receiver. Other argument registers
            // may contain unused caller scratch values, so they establish no field use.
            let consumes_receiver = matches!(row.operation.as_str(), "bl" | "b")
                && number(&row.operands).is_some_and(|target| receivers.contains(&(target as u64)));
            if row.operation == "ret" || consumes_receiver {
                for origin in value(&state, "x0") {
                    if let Some(path) = named(&origin, fields, collections)
                        && same_element(&origin, subject)
                    {
                        found.insert(path);
                    }
                }
            }
            if row.operation == "ret" || row.operation == "b" || !found.is_empty() {
                continue;
            }
        }
        transfer(row, &mut state, collections);
        for next in successors(rows, indexes, pc) {
            pending.push((next, state.clone(), seen.clone()));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::fields::{Function, ReaderJoin, RegistryFieldResult, Value};
    use crate::engine::analysis::{
        assembler::arm64,
        discovery::{Symbol, candidates},
    };
    fn member(name: &str, token: i64, at: i64, callee: &str) -> RootField {
        RootField {
            name: name.into(),
            token,
            constructor: 0,
            paths: Vec::new(),
            readers: vec![ReaderJoin::Joined {
                callee: callee.into(),
                arguments: [
                    ("x0".into(), Value::Reader(0)),
                    ("x1".into(), Value::Owner(at)),
                ]
                .into(),
                tail: true,
            }],
        }
    }
    fn input(code: Vec<u8>) -> FieldInput {
        let symbols = vec![Symbol { name: "TSingleObjectGameDatabase<CExampleDatabase, CExample, false>::LoadFile(char const*, bool)".into(), address: 0x7000 }];
        FieldInput {
            persistent: None,
            selection: candidates(&symbols).remove(0),
            symbols,
            functions: vec![Function {
                name: "CExample::Select() const".into(),
                address: 0x1000,
                code,
            }],
            strings: Default::default(),
            read_only_data: vec![],
            objects: vec![],
            gaps: vec![],
        }
    }
    fn collections() -> Vec<CollectionField> {
        vec![CollectionField {
            token: 7,
            offset: 0x40,
            data_offset: Some(8),
            class: "CChild".into(),
            fields: Box::new(RegistryFieldResult {
                persistent: Default::default(),
                fields: vec![
                    member("inherit", 8, 0x10, "CReader::Read(bool&)"),
                    member("text", 9, 0x18, "CReader::Read(CString&, bool)"),
                ],
                paths: vec![],
                gaps: vec![],
                partition_accounted: true,
                collections: vec![],
                uses: vec![],
            }),
        }]
    }
    #[test]
    fn a_loop_keeps_possible_elements_beside_an_unknown_selection() {
        let code = arm64!(at 0x1000;
            mov x20, x0;
            mov x21, x3; // initial selection is unknown
            ldr x8, [x20, #0x48]; // collection buffer
            ldr x22, [x8, x4, lsl #3]; // one possible element
            cbz w5, extern 0x1018;
            mov x21, x22;
            cbnz w6, extern 0x1008; // selection loop
            ldrb w8, [x21, #0x10]; // inheritance flag
            cbnz w8, extern 0x102c;
            add x0, x21, #0x18; // selected text
            ret;
            mov x0, x20;
            ret
        );
        let fields = vec![member("children", 7, 0x40, "CReader::Read(CPersistent&)")];
        let found = discover(&input(code), &fields, &collections());
        assert!(
            found
                .iter()
                .any(|selection| selection.field == ["children", "text"]
                    && selection.tested == ["children", "inherit"]
                    && selection.zero)
        );
    }
    #[test]
    fn conditional_select_retains_both_storage_alternatives() {
        let code = arm64!(at 0x1000;
            mov x20, x0;
            ldr x8, [x20, #0x48];
            ldr x21, [x8, x4, lsl #3];
            ldrb w8, [x21, #0x10];
            add x9, x21, #0x18;
            add x10, x20, #0x80;
            cmp w8, #0;
            csel x0, x9, x10, eq;
            ret
        );
        let fields = vec![
            member("children", 7, 0x40, "CReader::Read(CPersistent&)"),
            member("base", 10, 0x80, "CReader::Read(CString&, bool)"),
        ];
        let found = discover(&input(code), &fields, &collections());
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .any(|selection| selection.field == ["base"] && !selection.zero)
        );
    }
    #[test]
    fn unused_call_registers_do_not_establish_field_uses() {
        let code = arm64!(at 0x1000;
            mov x20, x0;
            ldr x8, [x20, #0x48];
            ldr x21, [x8, x4, lsl #3];
            ldrb w8, [x21, #0x10];
            cbnz w8, extern 0x101c;
            add x8, x21, #0x18; // scratch value, not a call argument
            bl extern 0x9000;
            mov x0, x20;
            ret
        );
        let fields = vec![member("children", 7, 0x40, "CReader::Read(CPersistent&)")];
        let mut input = input(code);
        input.symbols.push(Symbol {
            name: "NoArguments()".into(),
            address: 0x9000,
        });
        assert!(discover(&input, &fields, &collections()).is_empty());
    }

    #[test]
    fn unknown_receiver_does_not_acquire_a_field_from_its_offset() {
        let code = arm64!(at 0x1000;
            ldrb w8, [x21, #0x10];
            cbnz w8, extern 0x100c;
            add x0, x21, #0x18;
            ret
        );
        let fields = vec![member("children", 7, 0x40, "CReader::Read(CPersistent&)")];
        assert!(discover(&input(code), &fields, &collections()).is_empty());
    }
    #[test]
    fn separate_loop_iterations_do_not_prove_the_same_receiver() {
        let code = arm64!(at 0x1000;
            mov x20, x0;
            ldr x8, [x20, #0x48];
            ldr x21, [x8, x4, lsl #3];
            cbz w5, extern 0x1018;
            mov x22, x21; // saved element from an earlier iteration
            b extern 0x1008;
            ldrb w8, [x22, #0x10];
            cbnz w8, extern 0x1028;
            add x0, x21, #0x18; // current iteration's element
            ret;
            mov x0, x20;
            ret
        );
        let fields = vec![member("children", 7, 0x40, "CReader::Read(CPersistent&)")];
        assert!(discover(&input(code), &fields, &collections()).is_empty());
    }

    #[test]
    fn a_branch_cannot_bypass_the_comparison_for_a_conditional_select() {
        let code = arm64!(at 0x1000;
            mov x20, x0;
            ldr x8, [x20, #0x48];
            ldr x21, [x8, x4, lsl #3];
            ldrb w8, [x21, #0x10];
            add x9, x21, #0x18;
            add x10, x20, #0x80;
            cbz w4, extern 0x1020;
            cmp w8, #0;
            csel x0, x9, x10, eq;
            ret
        );
        let fields = vec![
            member("children", 7, 0x40, "CReader::Read(CPersistent&)"),
            member("base", 10, 0x80, "CReader::Read(CString&, bool)"),
        ];
        assert!(discover(&input(code), &fields, &collections()).is_empty());
    }
    #[test]
    fn owner_scope_alone_does_not_establish_a_receiver() {
        let code = arm64!(at 0x1000;
            mov x20, x0;
            ldr x8, [x20, #0x48];
            ldr x21, [x8, x4, lsl #3];
            ldrb w8, [x21, #0x10];
            cbnz w8, extern 0x101c;
            add x0, x21, #0x18;
            ret;
            mov x0, x20;
            ret
        );
        let fields = vec![member("children", 7, 0x40, "CReader::Read(CPersistent&)")];
        let mut input = input(code);
        assert!(!discover(&input, &fields, &collections()).is_empty());
        for name in [
            "CExample::Select(void*)",
            "CExample::Nested::Select() const",
            "CExample::Select()::Local::Select() const",
            "CExample::Select() const::Local::Select() const",
        ] {
            input.functions[0].name = name.into();
            assert!(
                discover(&input, &fields, &collections()).is_empty(),
                "{name}"
            );
        }
    }
}
