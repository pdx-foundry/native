//! Localization contexts, their commands and links, and the scope types that select each context,
//! from the engine's text tables.
//!
//! The engine's localization language is organized by context: the kind of object that a text
//! statement currently points at, such as a country or a dead fleet. The text object's constructor
//! fills one table entry per context for each of three functions: a getter of the context's link
//! rows ("promotions"), the function that follows a link, and a getter of the context's command
//! rows ("properties"). A row is a name and the index that the engine passes to the function.
//!
//! The method runs the constructor, then each getter, and reads the rows. It follows each link by
//! running the link function for the row's index along every path, and reads the context that the
//! text object points at when the path returns. The link function first checks run-time state, so
//! every path is followed and the paths must agree; a path that could not be followed makes the
//! output unresolved. A link that hands the text object to the engine's scope-object setter takes
//! its context from the run-time object, so its output is `Various`.
//!
//! The scope join runs the scope-object setter once for each scope type, with a reference to an
//! existing object of that type. The context that the setter selects is the join. Nothing here
//! establishes whether a command gives useful text at run time, its arguments, or how contexts
//! fall back to each other.
use std::collections::BTreeSet;

use super::InputError;
use super::declarations::ScopeType;
use super::evaluate::{Call, Code, Exit, Machine, Path, ReadOnlyData};
use super::stop::Unresolved;

/// Name and revision of this static method.
pub const METHOD: &str = "localization-declarations/v1";

/// A context value that no setter writes, so a path that leaves it did not change the context.
const UNCHANGED: u64 = 0xffff_ffff;

/// Scratch size for a text object and a scope-object reference. It covers every field that the
/// runs write on the catalogued build; a field outside it is unknown, never zero.
const SCRATCH: u64 = 0x1000;

/// A row is an 8-byte name pointer and a 4-byte index, padded to 16 bytes.
const ROW_SIZE: u64 = 16;

/// The most rows that one getter may declare.
const ROW_LIMIT: u64 = 4096;

/// Entry addresses of the engine functions that the method runs or recognizes.
pub struct LocalizationFunctions {
    /// The text object's constructor.
    pub text_constructor: u64,
    /// The function that writes a context's display name.
    pub context_name: u64,
    /// String constructors from a C string, where the name run stops.
    pub string_from_literal: BTreeSet<u64>,
    /// The setter that selects a context from a run-time scope object.
    pub scope_object: u64,
    /// Setters of the text object that the method follows when code calls them.
    pub setters: BTreeSet<u64>,
    /// Scope-object reference getters, which return the referenced object.
    pub scope_object_getters: BTreeSet<u64>,
}

/// Layout of the text object and the scope-object reference on one exact build.
#[derive(Debug, Clone, Copy)]
pub struct TextLayout {
    /// Offset of the current context, a 32-bit value.
    pub context_offset: u64,
    /// Offsets of the three per-context function tables.
    pub promotion_targets: u64,
    pub promote: u64,
    pub property_targets: u64,
    /// Number of entries in each table.
    pub context_count: u64,
    /// Offset of the scope-type value in a scope-object reference.
    pub scope_reference_type_offset: u64,
}

/// Executable-derived input for the localization method.
pub struct LocalizationInput {
    pub functions: LocalizationFunctions,
    pub layout: TextLayout,
    /// The constructor, getters, name function, link functions and setters. The scope-object
    /// setter is left out, so a jump to it reaches the call handler.
    pub code: Code,
    /// The scope-object setter and the setters that it jumps to.
    pub scope_object_code: Code,
    /// Memory that is read-only once loaded, with pointers resolved.
    pub data: ReadOnlyData,
    /// The initial image of the data that holds the rows, with pointers resolved. Only row reads
    /// use it; code never loads from it, because writable data can change at run time.
    pub rows: ReadOnlyData,
    /// Scope names indexed by scope-type bit, or `None` when the table was not read.
    pub scope_names: Option<Vec<String>>,
}

/// Every context with a table entry or selected by a link or a scope type, and the scope join.
pub struct LocalizationResult {
    /// Contexts in engine order.
    pub contexts: Vec<Context>,
    /// Each named scope type and the context that it selects.
    pub joins: Vec<(ScopeType, Join)>,
    pub scope_table_missing: bool,
}

/// One localization context and what it declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// The engine's context value.
    pub value: u64,
    /// The engine's display name, when the name function could be followed.
    pub name: Option<String>,
    /// Command names in row order, or why the rows could not be read.
    pub commands: Result<Vec<String>, &'static str>,
    /// Link names in row order with their outputs, or why the rows could not be read.
    pub links: Result<Vec<(String, Output)>, &'static str>,
}

/// The context that a link changes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// These context values; usually one.
    Contexts(BTreeSet<u64>),
    /// The engine selects the context from the run-time object.
    Various,
    /// Every path returns without changing the context.
    Unchanged,
    Unresolved(Unresolved),
}

/// The context that a scope type selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Join {
    Context(u64),
    /// Every path returned without selecting a context.
    NoContext,
    Unresolved(Unresolved),
}

/// The three table entries of one context. `None` is an entry that could not be read; `Some(0)`
/// is an empty entry.
#[derive(Debug, Clone, Copy)]
struct Entries {
    promotion_targets: Option<u64>,
    promote: Option<u64>,
    property_targets: Option<u64>,
}

impl Entries {
    fn is_empty(&self) -> bool {
        [self.promotion_targets, self.promote, self.property_targets]
            .iter()
            .all(|entry| *entry == Some(0))
    }
}

/// Read every context's commands and links, and the scope join.
pub fn analyze(input: &LocalizationInput) -> Result<LocalizationResult, InputError> {
    let tables = tables(input)?;
    let mut contexts = tables
        .into_iter()
        .filter(|(_, entries)| !entries.is_empty())
        .map(|(value, entries)| context(input, value, entries))
        .collect::<Vec<_>>();
    if contexts.is_empty() {
        return Err(InputError("the text constructor fills no context".into()));
    }

    let joins = joins(input);
    for value in selected_without_tables(&contexts, &joins) {
        contexts.push(Context {
            value,
            name: context_name(input, value),
            commands: Ok(Vec::new()),
            links: Ok(Vec::new()),
        });
    }
    contexts.sort_by_key(|context| context.value);

    Ok(LocalizationResult {
        contexts,
        joins,
        scope_table_missing: input.scope_names.is_none(),
    })
}

/// Contexts that a link output or a scope type selects but that have no table entry, so that
/// every reference in the result names a context of the result.
fn selected_without_tables(contexts: &[Context], joins: &[(ScopeType, Join)]) -> BTreeSet<u64> {
    let mut selected = BTreeSet::new();

    for links in contexts
        .iter()
        .filter_map(|context| context.links.as_ref().ok())
    {
        for (_, output) in links {
            if let Output::Contexts(values) = output {
                selected.extend(values.iter().copied());
            }
        }
    }

    for (_, join) in joins {
        if let Join::Context(value) = join {
            selected.insert(*value);
        }
    }

    selected.retain(|value| !contexts.iter().any(|context| context.value == *value));
    selected
}

/// Run the constructor and read each context's three table entries.
fn tables(input: &LocalizationInput) -> Result<Vec<(u64, Entries)>, InputError> {
    let layout = input.layout;
    let mut machine = Machine::new(&input.code, &input.data);
    let text = machine.allocate(SCRATCH);
    machine.set_register(0, text);

    let exit = machine
        .run(input.functions.text_constructor, &mut |_, _| Ok(Call::Stop))
        .map_err(|Unresolved { reason, .. }| InputError(format!("text constructor: {reason}")))?;
    if exit != Exit::Returned {
        return Err(InputError("text constructor makes a call".into()));
    }

    let entry = |table: u64, value: u64| machine.read(text + table + value * 8, 8);
    Ok((0..layout.context_count)
        .map(|value| {
            let entries = Entries {
                promotion_targets: entry(layout.promotion_targets, value),
                promote: entry(layout.promote, value),
                property_targets: entry(layout.property_targets, value),
            };
            (value, entries)
        })
        .collect())
}

fn context(input: &LocalizationInput, value: u64, entries: Entries) -> Context {
    let commands = rows(input, entries.property_targets)
        .map(|rows| rows.into_iter().map(|(name, _)| name).collect());
    let links = rows(input, entries.promotion_targets).and_then(|rows| {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let promote = match entries.promote {
            Some(0) | None => return Err("link-function"),
            Some(promote) => promote,
        };
        Ok(rows
            .into_iter()
            .map(|(name, index)| (name, output(input, promote, index)))
            .collect())
    });

    Context {
        value,
        name: context_name(input, value),
        commands,
        links,
    }
}

/// Run a row getter and read its rows as names and indexes.
fn rows(
    input: &LocalizationInput,
    getter: Option<u64>,
) -> Result<Vec<(String, u64)>, &'static str> {
    let getter = match getter {
        None => return Err("table-entry"),
        Some(0) => return Ok(Vec::new()),
        Some(getter) => getter,
    };

    let mut machine = Machine::new(&input.code, &input.data);
    let count_address = machine.allocate(8);
    machine.set_register(0, count_address);
    let exit = machine
        .run(getter, &mut |_, _| Ok(Call::Stop))
        .map_err(|Unresolved { reason, .. }| reason)?;
    if exit != Exit::Returned {
        return Err("row-getter-call");
    }

    let array = machine.register(0).ok_or("row-array")?;
    let count = machine.read(count_address, 4).ok_or("row-count")?;
    if count > ROW_LIMIT {
        return Err("row-count");
    }

    (0..count)
        .map(|index| {
            let row = array + index * ROW_SIZE;
            let name = input
                .rows
                .read(row, 8)
                .and_then(|pointer| input.data.string(pointer))
                .ok_or("row-name")?;
            let index = input.rows.read(row + 8, 4).ok_or("row-index")?;
            Ok((name, index))
        })
        .collect()
}

/// Follow the name function to the C string that it passes to the string constructor.
fn context_name(input: &LocalizationInput, value: u64) -> Option<String> {
    let mut machine = Machine::new(&input.code, &input.data);
    let result = machine.allocate(SCRATCH);
    machine.set_register(0, value);
    machine.set_register(8, result);

    let mut literals = BTreeSet::new();
    let paths = machine.run_paths(input.functions.context_name, &mut |target, machine| {
        if !target.is_some_and(|target| input.functions.string_from_literal.contains(&target)) {
            return Err(Unresolved::new("name-call"));
        }

        literals.insert(machine.register(1));
        Ok(Call::Stop)
    });
    if paths.iter().any(|path| path.end.is_err()) {
        return None;
    }

    match literals.into_iter().collect::<Vec<_>>().as_slice() {
        [Some(literal)] => input.data.string(*literal),
        _ => None,
    }
}

/// Follow one link along every path and combine the contexts that the paths leave.
fn output(input: &LocalizationInput, promote: u64, index: u64) -> Output {
    let mut machine = Machine::new(&input.code, &input.data);
    let text = machine.allocate(SCRATCH);
    let field = text + input.layout.context_offset;
    machine.write(field, 4, UNCHANGED);
    machine.set_register(1, text);
    machine.set_register(2, index);

    let scope_object = input.functions.scope_object;
    let paths = machine.run_paths(promote, &mut |target, machine| {
        if target == Some(scope_object) {
            return Ok(Call::Stop);
        }

        update_context_for_call(input, field, target, machine)
    });

    let mut contexts = BTreeSet::new();
    let mut various = false;
    let mut unchanged = false;
    for path in paths {
        match path_context(&path, field, scope_object) {
            Err(unresolved) => return Output::Unresolved(unresolved),
            Ok(PathContext::ScopeObject) => various = true,
            Ok(PathContext::Unchanged) => unchanged = true,
            Ok(PathContext::Trapped) => {}
            Ok(PathContext::Selected(value)) => {
                contexts.insert(value);
            }
        }
    }

    let changed = various || !contexts.is_empty();
    match (changed, unchanged) {
        (true, true) => {
            Output::Unresolved(Unresolved::new("some-paths-leave-the-context-unchanged"))
        }
        (false, true) => Output::Unchanged,
        (false, false) => Output::Unresolved(Unresolved::new("no-path-returns")),
        (true, false) if various => Output::Various,
        (true, false) => Output::Contexts(contexts),
    }
}

/// Run each scope type through the scope-object setter.
fn joins(input: &LocalizationInput) -> Vec<(ScopeType, Join)> {
    let names = input.scope_names.as_deref().unwrap_or_default();
    names
        .iter()
        .enumerate()
        .filter(|(bit, name)| !name.is_empty() && *bit < 64)
        .map(|(bit, name)| {
            let scope = ScopeType {
                bit,
                name: name.clone(),
            };
            (scope, join(input, bit))
        })
        .collect()
}

fn join(input: &LocalizationInput, bit: usize) -> Join {
    let layout = input.layout;
    let mut machine = Machine::new(&input.scope_object_code, &input.data);
    let text = machine.allocate(SCRATCH);
    let field = text + layout.context_offset;
    machine.write(field, 4, UNCHANGED);
    let reference = machine.allocate(SCRATCH);
    machine.write(reference + layout.scope_reference_type_offset, 8, 1 << bit);
    let object = machine.allocate(SCRATCH);
    machine.set_register(0, text);
    machine.set_register(1, reference);

    let getters = &input.functions.scope_object_getters;
    let paths = machine.run_paths(input.functions.scope_object, &mut |target, machine| {
        if target.is_some_and(|target| getters.contains(&target)) {
            return Ok(Call::Return(Some(object)));
        }

        update_context_for_call(input, field, target, machine)
    });

    let mut contexts = BTreeSet::new();
    for path in &paths {
        match path_context(path, field, input.functions.scope_object) {
            Err(unresolved) => return Join::Unresolved(unresolved),
            Ok(PathContext::ScopeObject) => {
                return Join::Unresolved(Unresolved::new("scope-object-recursion"));
            }
            Ok(PathContext::Unchanged | PathContext::Trapped) => {}
            Ok(PathContext::Selected(value)) => {
                contexts.insert(value);
            }
        }
    }

    match contexts.into_iter().collect::<Vec<_>>().as_slice() {
        [] => Join::NoContext,
        [value] => Join::Context(*value),
        _ => Join::Unresolved(Unresolved::new("several-contexts")),
    }
}

/// A call from a link function or the scope-object setter. A known setter runs on a copy of the
/// machine and its context is kept; any other call, including one through a register whose
/// target is unknown, may change the context, so the context becomes unknown.
fn update_context_for_call(
    input: &LocalizationInput,
    field: u64,
    target: Option<u64>,
    machine: &mut Machine<'_>,
) -> Result<Call, Unresolved> {
    let Some(target) = target.filter(|target| input.functions.setters.contains(target)) else {
        machine.forget(field, 4);
        return Ok(Call::Return(None));
    };

    let mut setter = machine.clone();
    let exit = setter.run(target, &mut |_, setter| {
        setter.forget(field, 4);
        Ok(Call::Return(None))
    })?;
    if exit != Exit::Returned {
        return Err(Unresolved::new("setter-stopped"));
    }

    match setter.read(field, 4) {
        Some(value) => machine.write(field, 4, value),
        None => machine.forget(field, 4),
    }
    Ok(Call::Return(None))
}

enum PathContext {
    Selected(u64),
    /// The path returned without selecting a context.
    Unchanged,
    /// The path does not return, so it gives no text.
    Trapped,
    ScopeObject,
}

fn path_context(path: &Path<'_>, field: u64, scope_object: u64) -> Result<PathContext, Unresolved> {
    match path.end.clone() {
        Err(unresolved) => Err(unresolved),
        Ok(Exit::Stopped(target)) if target == scope_object => Ok(PathContext::ScopeObject),
        Ok(Exit::Stopped(_) | Exit::Reached | Exit::Looped) => Err(Unresolved::new("stopped")),
        Ok(Exit::Trapped) => Ok(PathContext::Trapped),
        Ok(Exit::Returned) => match path.machine.read(field, 4) {
            None => Err(Unresolved::new("context-unknown")),
            Some(UNCHANGED) => Ok(PathContext::Unchanged),
            Some(value) => Ok(PathContext::Selected(value)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::decode::Instruction;

    const LAYOUT: TextLayout = TextLayout {
        context_offset: 0x8,
        promotion_targets: 0x20,
        promote: 0x40,
        property_targets: 0x60,
        context_count: 3,
        scope_reference_type_offset: 0x8,
    };
    const SCOPE_OBJECT: u64 = 0x9000;
    const SCOPE_GETTER: u64 = 0x9800;
    const STRING_FROM_LITERAL: u64 = 0x9900;
    const COUNTRY_SETTER: u64 = 0x4000;
    const SILENT_SETTER: u64 = 0x4100;
    const FLEET_SETTER: u64 = 0x4200;

    fn instructions(lines: &[(u64, &str, &str)]) -> Vec<Instruction> {
        lines
            .iter()
            .map(|(address, operation, operands)| Instruction {
                address: *address,
                bytes: [0; 4],
                operation: (*operation).into(),
                operands: (*operands).into(),
            })
            .collect()
    }

    /// Setters that write context 1, nothing, and context 2.
    fn setters() -> Vec<Instruction> {
        instructions(&[
            (0x4000, "mov", "w8,#1"),
            (0x4004, "str", "w8,[x0,#0x8]"),
            (0x4008, "ret", ""),
            (0x4100, "ret", ""),
            (0x4200, "mov", "w8,#2"),
            (0x4204, "str", "w8,[x0,#0x8]"),
            (0x4208, "ret", ""),
        ])
    }

    /// The constructor fills context 0 with commands, context 1 with commands and links, and
    /// context 2 with a command getter whose row name is unmapped.
    fn functions() -> Vec<Instruction> {
        instructions(&[
            (0x1000, "adrp", "x9,#0x2000"),
            (0x1004, "add", "x9,x9,#0x100"),
            (0x1008, "str", "x9,[x0,#0x60]"),
            (0x100c, "adrp", "x9,#0x2000"),
            (0x1010, "add", "x9,x9,#0x200"),
            (0x1014, "str", "x9,[x0,#0x68]"),
            (0x1018, "adrp", "x9,#0x2000"),
            (0x101c, "add", "x9,x9,#0x300"),
            (0x1020, "str", "x9,[x0,#0x28]"),
            (0x1024, "adrp", "x9,#0x3000"),
            (0x1028, "str", "x9,[x0,#0x48]"),
            (0x102c, "adrp", "x9,#0x2000"),
            (0x1030, "add", "x9,x9,#0x400"),
            (0x1034, "str", "x9,[x0,#0x70]"),
            (0x1038, "ret", ""),
            // Row getters.
            (0x2100, "mov", "w8,#1"),
            (0x2104, "str", "w8,[x0]"),
            (0x2108, "adrp", "x0,#0x5000"),
            (0x210c, "ret", ""),
            (0x2200, "mov", "w8,#2"),
            (0x2204, "str", "w8,[x0]"),
            (0x2208, "adrp", "x0,#0x5000"),
            (0x220c, "add", "x0,x0,#0x100"),
            (0x2210, "ret", ""),
            (0x2300, "mov", "w8,#7"),
            (0x2304, "str", "w8,[x0]"),
            (0x2308, "adrp", "x0,#0x5000"),
            (0x230c, "add", "x0,x0,#0x200"),
            (0x2310, "ret", ""),
            (0x2400, "mov", "w8,#1"),
            (0x2404, "str", "w8,[x0]"),
            (0x2408, "adrp", "x0,#0x5000"),
            (0x240c, "add", "x0,x0,#0x300"),
            (0x2410, "ret", ""),
            // Link function: index 0 checks run-time state; one side selects a context and the
            // other traps.
            (0x3000, "cmp", "w2,#0"),
            (0x3004, "b.ne", "#0x3020"),
            (0x3008, "ldr", "x8,[x9]"),
            (0x300c, "cbz", "x8,#0x3018"),
            (0x3010, "mov", "x0,x1"),
            (0x3014, "b", "#0x4000"),
            (0x3018, "brk", "#0x1"),
            // Index 1 hands the text to the scope-object setter.
            (0x3020, "cmp", "w2,#1"),
            (0x3024, "b.ne", "#0x3030"),
            (0x3028, "mov", "x0,x1"),
            (0x302c, "b", "#0x9000"),
            // Index 2 reaches a setter that leaves the context unchanged.
            (0x3030, "cmp", "w2,#2"),
            (0x3034, "b.ne", "#0x3040"),
            (0x3038, "mov", "x0,x1"),
            (0x303c, "b", "#0x4100"),
            // Index 3 makes an indirect call and returns.
            (0x3040, "cmp", "w2,#3"),
            (0x3044, "b.ne", "#0x3050"),
            (0x3048, "blr", "x8"),
            (0x304c, "ret", ""),
            // Index 4 calls an unknown function that may change the context.
            (0x3050, "cmp", "w2,#4"),
            (0x3054, "b.ne", "#0x3060"),
            (0x3058, "bl", "#0x7000"),
            (0x305c, "ret", ""),
            // Index 5 calls a setter and returns.
            (0x3060, "cmp", "w2,#5"),
            (0x3064, "b.ne", "#0x3074"),
            (0x3068, "mov", "x0,x1"),
            (0x306c, "bl", "#0x4000"),
            (0x3070, "ret", ""),
            // Index 6 selects a context on one side of a run-time check and returns on the other.
            (0x3074, "cbz", "x10,#0x3080"),
            (0x3078, "mov", "x0,x1"),
            (0x307c, "b", "#0x4000"),
            (0x3080, "ret", ""),
            // Context names.
            (0x4800, "adrp", "x9,#0x8000"),
            (0x4804, "ldr", "x1,[x9,x0,lsl#3]"),
            (0x4808, "b", "#0x9900"),
        ])
    }

    /// The scope-object setter: scope bit 2 selects context 1 through a reference getter; bit 3
    /// reaches two contexts on a run-time check; bit 4 selects context 5, which has no table
    /// entry; bit 0 selects nothing.
    fn scope_object() -> Vec<Instruction> {
        instructions(&[
            (0x9000, "ldr", "x8,[x1,#0x8]"),
            (0x9004, "cmp", "x8,#4"),
            (0x9008, "b.ne", "#0x901c"),
            (0x900c, "mov", "x19,x0"),
            (0x9010, "bl", "#0x9800"),
            (0x9014, "mov", "x0,x19"),
            (0x9018, "b", "#0x4000"),
            (0x901c, "cmp", "x8,#8"),
            (0x9020, "b.ne", "#0x9030"),
            (0x9024, "cbz", "x10,#0x902c"),
            (0x9028, "b", "#0x4000"),
            (0x902c, "b", "#0x4200"),
            (0x9030, "cmp", "x8,#0x10"),
            (0x9034, "b.ne", "#0x9040"),
            (0x9038, "mov", "w8,#5"),
            (0x903c, "str", "w8,[x0,#0x8]"),
            (0x9040, "ret", ""),
        ])
    }

    fn row(name: u64, index: u64) -> Vec<u8> {
        [name.to_le_bytes(), index.to_le_bytes()].concat()
    }

    fn input() -> LocalizationInput {
        let strings = [
            (0x6000, "GetYear"),
            (0x6010, "GetName"),
            (0x6020, "GetAdj"),
            (0x6030, "Capital"),
            (0x6040, "Root"),
            (0x6050, "Broken"),
            (0x6060, "Indirect"),
            (0x6070, "Lost"),
            (0x6080, "Owner"),
            (0x6090, "Mixed"),
            (0x6100, "Base Scope"),
            (0x6110, "Country"),
        ];
        let mut data: Vec<_> = strings
            .iter()
            .map(|(address, text)| (*address, format!("{text}\0").into_bytes()))
            .collect();
        data.push((
            0x8000,
            [0x6100u64.to_le_bytes(), 0x6110u64.to_le_bytes()].concat(),
        ));

        let rows = ReadOnlyData::new(vec![
            (0x5000, row(0x6000, 0)),
            (0x5100, [row(0x6010, 0), row(0x6020, 1)].concat()),
            (
                0x5200,
                [
                    row(0x6030, 0),
                    row(0x6040, 1),
                    row(0x6050, 2),
                    row(0x6060, 3),
                    row(0x6070, 4),
                    row(0x6080, 5),
                    row(0x6090, 6),
                ]
                .concat(),
            ),
            (0x5300, row(0xdead0, 0)),
        ]);

        LocalizationInput {
            functions: LocalizationFunctions {
                text_constructor: 0x1000,
                context_name: 0x4800,
                string_from_literal: BTreeSet::from([STRING_FROM_LITERAL]),
                scope_object: SCOPE_OBJECT,
                setters: BTreeSet::from([COUNTRY_SETTER, SILENT_SETTER, FLEET_SETTER]),
                scope_object_getters: BTreeSet::from([SCOPE_GETTER]),
            },
            layout: LAYOUT,
            code: Code::from_rows(functions().into_iter().chain(setters())),
            scope_object_code: Code::from_rows(scope_object().into_iter().chain(setters())),
            data: ReadOnlyData::new(data),
            rows,
            scope_names: Some(vec![
                "zero".into(),
                String::new(),
                "country".into(),
                "split".into(),
                "hidden".into(),
            ]),
        }
    }

    fn context(result: &LocalizationResult, value: u64) -> &Context {
        result
            .contexts
            .iter()
            .find(|context| context.value == value)
            .expect("context is present")
    }

    fn link<'a>(context: &'a Context, name: &str) -> &'a Output {
        let links = context.links.as_ref().expect("links are readable");
        &links
            .iter()
            .find(|(link, _)| link == name)
            .expect("link is present")
            .1
    }

    #[test]
    fn contexts_come_from_the_constructor_tables() {
        let result = analyze(&input()).unwrap();

        let values: Vec<_> = result
            .contexts
            .iter()
            .map(|context| context.value)
            .collect();
        assert_eq!(values, [0, 1, 2, 5]);
        assert_eq!(context(&result, 0).name.as_deref(), Some("Base Scope"));
        assert_eq!(context(&result, 1).name.as_deref(), Some("Country"));
        assert_eq!(context(&result, 0).commands, Ok(vec!["GetYear".into()]));
        assert_eq!(context(&result, 0).links, Ok(Vec::new()));
        assert_eq!(
            context(&result, 1).commands,
            Ok(vec!["GetName".into(), "GetAdj".into()])
        );
    }

    #[test]
    fn a_link_output_needs_every_path_to_agree() {
        let result = analyze(&input()).unwrap();
        let country = context(&result, 1);

        assert_eq!(
            link(country, "Capital"),
            &Output::Contexts(BTreeSet::from([1]))
        );
        assert_eq!(
            link(country, "Owner"),
            &Output::Contexts(BTreeSet::from([1]))
        );
        assert_eq!(link(country, "Root"), &Output::Various);
    }

    #[test]
    fn unfollowed_links_are_unresolved_and_links_that_select_nothing_are_unchanged() {
        let result = analyze(&input()).unwrap();
        let country = context(&result, 1);

        assert_eq!(link(country, "Broken"), &Output::Unchanged);
        assert_eq!(
            link(country, "Mixed"),
            &Output::Unresolved(Unresolved::new("some-paths-leave-the-context-unchanged")),
            "a link that may leave the context unchanged has no definite output"
        );
        assert_eq!(
            link(country, "Indirect"),
            &Output::Unresolved(Unresolved::new("context-unknown")),
            "an indirect call may change the context"
        );
        assert_eq!(
            link(country, "Lost"),
            &Output::Unresolved(Unresolved::new("context-unknown"))
        );
    }

    #[test]
    fn unreadable_rows_and_names_are_kept_as_failures() {
        let result = analyze(&input()).unwrap();
        let unnamed = context(&result, 2);

        assert_eq!(unnamed.name, None);
        assert_eq!(unnamed.commands, Err("row-name"));
    }

    #[test]
    fn scope_types_join_the_context_that_they_select() {
        let result = analyze(&input()).unwrap();
        let joins: Vec<_> = result
            .joins
            .iter()
            .map(|(scope, join)| (scope.name.as_str(), join.clone()))
            .collect();

        assert_eq!(
            joins,
            [
                ("zero", Join::NoContext),
                ("country", Join::Context(1)),
                (
                    "split",
                    Join::Unresolved(Unresolved::new("several-contexts"))
                ),
                ("hidden", Join::Context(5)),
            ]
        );
        let hidden = context(&result, 5);
        assert_eq!(
            (&hidden.commands, &hidden.links),
            (&Ok(Vec::new()), &Ok(Vec::new())),
            "a selected context without table entries is still a context of the result"
        );
        assert!(!result.scope_table_missing);
    }

    #[test]
    fn names_and_strings_come_from_read_only_data() {
        let mut input = input();
        input.data = ReadOnlyData::default();

        let result = analyze(&input).unwrap();
        assert_eq!(context(&result, 1).name, None);
        assert_eq!(context(&result, 1).commands, Err("row-name"));
    }

    #[test]
    fn a_constructor_that_fills_nothing_is_refused() {
        let mut input = input();
        input.code = Code::from_rows(instructions(&[(0x1000, "ret", "")]));

        assert!(analyze(&input).is_err());
    }
}
