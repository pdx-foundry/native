//! Whether the engine runs an item's post-read code for every item that its database loads.
//!
//! An item root (`<Owner>::InitPostRead`) receives one item, and the engine calls it through the
//! item's vtable. The method establishes that call from the code that constructs items. Every
//! function that calls a constructor of the item's class, directly or through a constructor
//! that delegates, must be a function of the database's own classes (the loader, the readers of
//! a new or an existing entry, the database constructor) or the null object's initializer. No
//! other function may form an address of the item's vtables: such a function can construct an
//! item without a constructor call, which the method does not follow.
//!
//! Each function of the database's classes runs from its entry with fresh registers, the
//! database as its constructor leaves it, an item array of unknown length, and unknown memory
//! for each pointer argument. After an item constructor, the item holds the address points of
//! its class's vtables, and the run enters the functions in those vtables, such as
//! `CPersistent::Read`. A thunk of an item root adjusts `this` from the subobject whose vtable
//! holds it to the item and runs the root, so a call of the thunk with that subobject of the item
//! is a call of the root with the item, also when the thunk holds a copy of the root's body.
//!
//! States join at loop heads (`Machine::run_paths_joining`), so the run covers every pass of a
//! loop. The call is established when, on every path that returns, each item that the path
//! constructed received a call of every item root with the item in `x0`. A path that ends at a
//! loop head is covered by a path with the same pending items, which goes on.
//!
//! Assumptions: the null object (`TPdxNullObject<Owner>::Initialize`) is not an item of the
//! database; a store to an unknown address, or a call that the run does not enter, changes
//! neither the database's fields other than its item array nor an item's vtable pointers. A path
//! that ends in a function that never returns, or in a trap, loads no item. Outside the method:
//! an address of the vtables that code forms in a way that the binding's scan does not read.
use std::collections::{BTreeMap, BTreeSet};

use super::super::evaluate::{Call, Code, Exit, Machine};
use super::super::stop::Unresolved;
use super::{DATABASE_SPAN, FamilyInput, ITEM_SPAN};

/// Bytes of unknown memory that each pointer argument of a function receives.
const ARGUMENT_SPAN: u64 = 0x400;

/// Labels at or above this value mark a constructed item that some item root has not received:
/// the label of `ITEMS | item` has bit `n` set when item root `n` received it. The label goes
/// when every item root received the item, so paths that read their items join at loop heads.
const ITEMS: u64 = 1 << 62;

/// The code that constructs a registry's items.
pub struct Loading {
    /// The functions of the database's own classes that construct an item.
    pub loaders: Vec<Function>,
    /// Every body of the database's constructor.
    pub database_constructors: Vec<Function>,
    /// The functions of other classes that construct an item, other than the null object's
    /// initializer.
    pub elsewhere: Vec<String>,
    /// The functions other than the item's constructors and destructors that form an address of
    /// the item's vtables: they may construct an item without a constructor call.
    pub inline_constructions: Vec<String>,
    /// Every body of every constructor of the item's class.
    pub constructors: BTreeSet<u64>,
    /// The address point of each vtable of the item's class, by the offset of its subobject.
    pub vtables: BTreeMap<u64, u64>,
    /// The item root of each thunk, and the offset of the subobject whose vtable holds it: the
    /// thunk adjusts `this` from that subobject to the item and runs its root, from a branch to
    /// the root or from a copy of its body.
    pub thunks: BTreeMap<u64, (u64, u64)>,
    /// The functions in the item's vtables, other than the item roots and their thunks.
    pub dispatch: BTreeSet<u64>,
    /// The functions that return new memory.
    pub allocations: BTreeSet<u64>,
    /// The loaders, the database constructors and the dispatch functions. The item roots are left
    /// out, so that a branch into one reaches the run's call handler.
    pub code: Code,
}

/// A function that the method runs from its entry.
pub struct Function {
    pub address: u64,
    pub name: String,
    /// The argument registers that hold a pointer or a reference.
    pub pointers: Vec<usize>,
}

/// Why the method did not establish that the engine runs the item roots for every item.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NotEstablished {
    /// No function of the database's classes constructs an item.
    NoLoader,
    /// The item's class has no vtable in the executable's data.
    NoVtable,
    /// A function of another class constructs an item.
    ConstructedElsewhere { function: String },
    /// A function forms an address of the item's vtables without calling its constructor.
    InlineConstruction { function: String },
    /// A path of the function keeps an item that it constructed without calling every item
    /// root with it.
    Unread { function: String },
    /// A path of the function could not be followed.
    Unfollowed {
        function: String,
        unresolved: Unresolved,
    },
}

/// Whether the functions that construct the registry's items call each of `roots` with each
/// item on every path.
pub fn every_item(
    input: &FamilyInput,
    loading: &Loading,
    roots: &BTreeSet<u64>,
) -> Result<(), NotEstablished> {
    if let Some(function) = loading.elsewhere.first() {
        return Err(NotEstablished::ConstructedElsewhere {
            function: function.clone(),
        });
    }
    if let Some(function) = loading.inline_constructions.first() {
        return Err(NotEstablished::InlineConstruction {
            function: function.clone(),
        });
    }
    if loading.loaders.is_empty() {
        return Err(NotEstablished::NoLoader);
    }
    if loading.vtables.is_empty() {
        return Err(NotEstablished::NoVtable);
    }

    let run = Run {
        input,
        loading,
        roots: roots.iter().copied().collect(),
    };
    let databases = run.databases()?;
    for loader in &loading.loaders {
        for database in &databases {
            run.loader(database, loader)?;
        }
    }
    Ok(())
}

struct Run<'r> {
    input: &'r FamilyInput,
    loading: &'r Loading,
    /// The item roots, in the order of their bits in an item's label.
    roots: Vec<u64>,
}

impl<'r> Run<'r> {
    /// The database as each returned path of each constructor body leaves it, once for each
    /// distinct content.
    fn databases(&self) -> Result<Vec<Database>, NotEstablished> {
        let mut databases = Vec::new();
        for constructor in &self.loading.database_constructors {
            let mut machine = Machine::new(&self.loading.code, &self.input.data);
            let database = machine.reserve(DATABASE_SPAN);
            machine.set_register(0, database);
            let paths = machine.run_paths(constructor.address, &mut |target, machine| {
                Ok(match target {
                    Some(target) if self.loading.allocations.contains(&target) => {
                        Call::Return(Some(machine.reserve(ITEM_SPAN)))
                    }
                    Some(target) if self.input.strings.never_return.contains(&target) => Call::Stop,
                    _ => Call::Return(None),
                })
            });
            for path in paths {
                match self.ending(&path.end) {
                    Ending::Returned | Ending::Covered => {
                        let bytes = database_bytes(&path.machine, database);
                        if !databases
                            .iter()
                            .any(|other: &Database| other.bytes == bytes)
                        {
                            databases.push(Database {
                                bytes,
                                reserved: path.machine.clone().reserve(0),
                            });
                        }
                    }
                    Ending::Ignored => {}
                    Ending::Failed(unresolved) => {
                        return Err(NotEstablished::Unfollowed {
                            function: constructor.name.clone(),
                            unresolved,
                        });
                    }
                }
            }
        }

        match self.loading.database_constructors.first() {
            None => Err(NotEstablished::Unfollowed {
                function: "the database constructor".into(),
                unresolved: Unresolved::new("no-symbol"),
            }),
            Some(constructor) if databases.is_empty() => Err(NotEstablished::Unfollowed {
                function: constructor.name.clone(),
                unresolved: Unresolved::new("never-returns"),
            }),
            Some(_) => Ok(databases),
        }
    }

    /// Run `loader` on the database, and require every path to read each item it constructed.
    fn loader(&self, constructed: &Database, loader: &Function) -> Result<(), NotEstablished> {
        let Database { bytes, reserved } = constructed;
        let mut machine = Machine::new(&self.loading.code, &self.input.data);
        let database = machine.reserve(DATABASE_SPAN);
        // The database may point at memory that its constructor reserved: new memory follows it.
        let next = machine.reserve(0);
        machine.reserve(reserved.saturating_sub(next));
        for (offset, byte) in (0..).zip(bytes) {
            if let Some(byte) = byte {
                machine.write(database + offset, 1, *byte);
            }
        }
        let layout = self.input.database;
        machine.protect(database, DATABASE_SPAN);
        machine.forget(
            database + layout.items_offset,
            layout.count_offset + 4 - layout.items_offset,
        );
        machine.set_register(0, database);
        for &register in &loader.pointers {
            let argument = machine.reserve(ARGUMENT_SPAN);
            machine.set_register(register, argument);
        }

        let paths = machine.run_paths_joining(loader.address, &mut |target, machine| {
            self.call(target, machine)
        });
        for path in paths {
            match self.ending(&path.end) {
                Ending::Returned if unread(&path.machine) => {
                    return Err(NotEstablished::Unread {
                        function: loader.name.clone(),
                    });
                }
                Ending::Returned | Ending::Covered | Ending::Ignored => {}
                Ending::Failed(unresolved) => {
                    return Err(NotEstablished::Unfollowed {
                        function: loader.name.clone(),
                        unresolved,
                    });
                }
            }
        }
        Ok(())
    }

    fn call(&self, target: Option<u64>, machine: &mut Machine<'r>) -> Result<Call, Unresolved> {
        let Some(target) = target else {
            return Ok(Call::Return(None));
        };
        if self.loading.allocations.contains(&target) {
            return Ok(Call::Return(Some(machine.reserve(ITEM_SPAN))));
        }
        if self.loading.constructors.contains(&target) {
            let item = machine.known_register(0, "item-address")?;
            for (&offset, &point) in &self.loading.vtables {
                machine.write(item + offset, 8, point);
                machine.protect(item + offset, 8);
            }
            machine.label(ITEMS | item, 0);
            return Ok(Call::Return(Some(item)));
        }
        if let Some(bit) = self.roots.iter().position(|&root| root == target) {
            self.reach(machine, bit, 0);
            return Ok(Call::Return(None));
        }
        if let Some((root, offset)) = self.loading.thunks.get(&target) {
            if let Some(bit) = self.roots.iter().position(|other| other == root) {
                self.reach(machine, bit, *offset);
            }
            return Ok(Call::Return(None));
        }
        if self.loading.dispatch.contains(&target) {
            return Ok(Call::Enter);
        }
        if self.input.strings.never_return.contains(&target) {
            return Ok(Call::Stop);
        }
        Ok(Call::Return(None))
    }

    fn ending(&self, end: &Result<Exit, Unresolved>) -> Ending {
        match end {
            Ok(Exit::Returned) => Ending::Returned,
            Ok(Exit::Looped) => Ending::Covered,
            Ok(Exit::Trapped) => Ending::Ignored,
            Ok(Exit::Stopped(target)) if self.input.strings.never_return.contains(target) => {
                Ending::Ignored
            }
            Ok(Exit::Stopped(_) | Exit::Reached) => Ending::Failed(Unresolved::new("stopped")),
            Err(unresolved) => Ending::Failed(*unresolved),
        }
    }

    /// Item root `bit` received the item whose subobject at `offset` is in `x0`.
    fn reach(&self, machine: &mut Machine, bit: usize, offset: u64) {
        let Some(item) = machine.register(0).map(|this| this.wrapping_sub(offset)) else {
            return;
        };
        let Some(reached) = machine.labelled(ITEMS | item) else {
            return;
        };
        let reached = reached | 1 << bit;
        if reached == self.every_root() {
            machine.unlabel(ITEMS | item);
        } else {
            machine.label(ITEMS | item, reached);
        }
    }

    /// The label bits of an item that every item root received.
    fn every_root(&self) -> u64 {
        (1 << self.roots.len()) - 1
    }
}

/// Whether the path constructed an item that some item root did not receive.
fn unread(machine: &Machine) -> bool {
    machine.labels().range(ITEMS..).next().is_some()
}

/// The database as its constructor leaves it.
struct Database {
    /// Each byte, when it is known.
    bytes: Vec<Option<u64>>,
    /// The end of the scratch memory that the constructor's run reserved.
    reserved: u64,
}

/// Each byte of the database, when it is known.
fn database_bytes(machine: &Machine, database: u64) -> Vec<Option<u64>> {
    (0..DATABASE_SPAN)
        .map(|offset| machine.read(database + offset, 1))
        .collect()
}

enum Ending {
    Returned,
    /// The path ended at a loop head where a path with the same labels goes on from a state
    /// that covers its own. That path's end checks the items that both hold.
    Covered,
    /// The path does not finish: it throws or traps.
    Ignored,
    Failed(Unresolved),
}

#[cfg(test)]
mod tests {
    use super::super::{DatabaseLayout, StringFunctions, StringLayout};
    use super::*;
    use crate::engine::analysis::decode::Instruction;
    use crate::engine::analysis::evaluate::ReadOnlyData;

    const DATABASE_CONSTRUCTOR: u64 = 0x100;
    const LOADER: u64 = 0x200;
    const READ: u64 = 0x400;
    const THUNK: u64 = 0x500;
    const ROOT: u64 = 0x600;
    const OTHER_ROOT: u64 = 0x640;
    const ITEM_CONSTRUCTOR: u64 = 0x800;
    const ALLOCATE: u64 = 0x900;
    const INSERT: u64 = 0x904;
    const NEVER_RETURNS: u64 = 0x908;
    /// The item's primary vtable and the vtable of its reader base at `+0x38`.
    const PRIMARY: u64 = 0x7000;
    const READER_BASE: u64 = 0x7100;

    type Rows = Vec<(u64, &'static str, String)>;

    fn rows(start: u64, lines: &[(&'static str, &str)]) -> Rows {
        lines
            .iter()
            .enumerate()
            .map(|(index, (operation, operands))| {
                (start + index as u64 * 4, *operation, (*operands).to_owned())
            })
            .collect()
    }

    /// Stores `mode` in the database's word at `+0x70` and empties its item array.
    fn database_constructor(mode: u64) -> Rows {
        rows(
            DATABASE_CONSTRUCTOR,
            &[
                ("stp", "xzr,xzr,[x0,#0x48]"),
                ("mov", &format!("w8,#{mode}")),
                ("str", "w8,[x0,#0x70]"),
                ("ret", ""),
            ],
        )
    }

    /// Reads statements until the reader's word at `+0` is zero. For each, allocates an item,
    /// constructs it and, unless the database's word at `+0x70` is 1, calls slot `0x20` of its
    /// reader base, then inserts it. `gate` is two instructions before the read that may branch
    /// to `SKIP`, past it.
    fn loader(gate: [(&'static str, &str); 2]) -> Rows {
        let [first, second] = gate;
        rows(
            LOADER,
            &[
                ("mov", "x20,x0"),
                ("mov", "x21,x1"),
                ("mov", "x22,#0"),
                // 0x20c: the statement loop.
                ("ldr", "w8,[x21]"),
                ("cbz", "w8,#0x258"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("mov", "x19,x0"),
                ("bl", &format!("#{ITEM_CONSTRUCTOR:#x}")),
                ("ldr", "w8,[x20,#0x70]"),
                ("cmp", "w8,#1"),
                ("b.eq", "#0x24c"),
                first,
                second,
                ("mov", "x0,x19"),
                ("ldr", "x8,[x0,#0x38]!"),
                ("ldr", "x8,[x8,#0x20]"),
                ("mov", "x1,x21"),
                ("blr", "x8"),
                ("mov", "x22,#1"),
                // 0x24c: SKIP
                ("mov", "x0,x19"),
                ("bl", &format!("#{INSERT:#x}")),
                ("b", "#0x20c"),
                // 0x258
                ("ret", ""),
            ],
        )
    }

    const NO_GATE: [(&str, &str); 2] = [("nop", ""), ("nop", "")];

    /// `CPersistent::Read`: calls slot `0x30` of `this`.
    fn read() -> Rows {
        rows(
            READ,
            &[
                ("stp", "x20,x30,[sp,#-0x10]!"),
                ("mov", "x20,x0"),
                ("bl", "#0x90c"),
                ("ldr", "x8,[x20]"),
                ("ldr", "x8,[x8,#0x30]"),
                ("mov", "x0,x20"),
                ("blr", "x8"),
                ("ldp", "x20,x30,[sp],#0x10"),
                ("ret", ""),
            ],
        )
    }

    fn data() -> ReadOnlyData {
        let mut bytes = vec![0; 0x200];
        let mut word = |at: usize, value: u64| {
            bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
        };
        word(0x120, READ);
        word(0x130, THUNK);
        ReadOnlyData::new(vec![(PRIMARY, bytes)])
    }

    fn input() -> FamilyInput {
        FamilyInput {
            registration: 0,
            category_offset: 0,
            database: DatabaseLayout {
                items_offset: 0x48,
                count_offset: 0x54,
            },
            definitions: None,
            strings: StringFunctions {
                never_return: [NEVER_RETURNS].into(),
                ..StringFunctions::default()
            },
            layout: StringLayout { flag_byte: 0x17 },
            data: data(),
        }
    }

    fn code(parts: &[Rows]) -> Code {
        Code::from_rows(
            parts
                .iter()
                .flatten()
                .map(|(address, operation, operands)| Instruction {
                    address: *address,
                    bytes: [0; 4],
                    operation: (*operation).into(),
                    operands: operands.clone(),
                }),
        )
    }

    fn function(address: u64, name: &str, pointers: &[usize]) -> Function {
        Function {
            address,
            name: name.into(),
            pointers: pointers.to_vec(),
        }
    }

    /// The loader and database constructor with `mode`, and the thunk of `ROOT` in the reader
    /// base's slot `0x30`.
    fn loading(loader: Rows, mode: u64) -> Loading {
        Loading {
            loaders: vec![function(LOADER, "CDb::Load(CReader&)", &[1])],
            database_constructors: vec![function(DATABASE_CONSTRUCTOR, "CDb::CDb()", &[])],
            elsewhere: Vec::new(),
            inline_constructions: Vec::new(),
            constructors: [ITEM_CONSTRUCTOR].into(),
            vtables: [(0, PRIMARY + 0x10), (0x38, READER_BASE)].into(),
            thunks: [(THUNK, (ROOT, 0x38))].into(),
            dispatch: [READ].into(),
            allocations: [ALLOCATE].into(),
            code: code(&[database_constructor(mode), loader, read()]),
        }
    }

    fn check(loading: &Loading, roots: &[u64]) -> Result<(), NotEstablished> {
        every_item(&input(), loading, &roots.iter().copied().collect())
    }

    fn unread(function: &str) -> Result<(), NotEstablished> {
        Err(NotEstablished::Unread {
            function: function.into(),
        })
    }

    #[test]
    fn a_loop_that_reads_each_item_through_its_vtable_calls_the_root_for_every_item() {
        let loading = loading(loader(NO_GATE), 0);
        assert_eq!(check(&loading, &[ROOT]), Ok(()));
    }

    #[test]
    fn a_loop_that_skips_the_read_of_some_items_does_not() {
        // The reader's word at `+8` decides.
        let on_reader = loading(loader([("ldr", "w9,[x21,#8]"), ("cbnz", "w9,#0x24c")]), 0);
        assert_eq!(check(&on_reader, &[ROOT]), unread("CDb::Load(CReader&)"));

        // Only the items after the first skip it: the first pass reads its item.
        let after_first = loading(loader([("cbnz", "x22,#0x24c"), ("nop", "")]), 0);
        assert_eq!(check(&after_first, &[ROOT]), unread("CDb::Load(CReader&)"));
    }

    #[test]
    fn a_database_whose_constructor_selects_no_read_does_not() {
        let loading = loading(loader(NO_GATE), 1);
        assert_eq!(check(&loading, &[ROOT]), unread("CDb::Load(CReader&)"));
    }

    #[test]
    fn a_database_constructor_that_adds_an_item_without_reading_it_does_not() {
        let mut loading = loading(loader(NO_GATE), 0);
        let constructor = rows(
            DATABASE_CONSTRUCTOR,
            &[
                ("mov", "x20,x0"),
                ("str", "wzr,[x20,#0x70]"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("bl", &format!("#{ITEM_CONSTRUCTOR:#x}")),
                ("mov", "x0,x20"),
                ("bl", &format!("#{INSERT:#x}")),
                ("ret", ""),
            ],
        );
        loading.code = code(&[constructor, loader(NO_GATE), read()]);
        loading
            .loaders
            .push(function(DATABASE_CONSTRUCTOR, "CDb::CDb()", &[]));
        assert_eq!(check(&loading, &[ROOT]), unread("CDb::CDb()"));
    }

    #[test]
    fn every_item_root_must_receive_each_item() {
        let loading = loading(loader(NO_GATE), 0);
        assert_eq!(
            check(&loading, &[ROOT, OTHER_ROOT]),
            unread("CDb::Load(CReader&)")
        );
    }

    /// An item constructed after a loop that counts to three, and never read.
    #[test]
    fn an_item_after_a_counted_loop_is_followed() {
        let mut loading = loading(loader(NO_GATE), 0);
        let counted = rows(
            0x300,
            &[
                ("mov", "x9,#0"),
                ("add", "x9,x9,#1"),
                ("cmp", "x9,#3"),
                ("b.ne", "#0x304"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("bl", &format!("#{ITEM_CONSTRUCTOR:#x}")),
                ("ret", ""),
            ],
        );
        loading.code = code(&[database_constructor(0), loader(NO_GATE), read(), counted]);
        loading.loaders.push(function(0x300, "CDb::Counted()", &[]));
        assert_eq!(check(&loading, &[ROOT]), unread("CDb::Counted()"));
    }

    #[test]
    fn an_item_that_other_code_constructs_is_not_established() {
        let mut loading = loading(loader(NO_GATE), 0);
        loading.elsewhere = vec!["CFactory::Make()".into()];
        assert_eq!(
            check(&loading, &[ROOT]),
            Err(NotEstablished::ConstructedElsewhere {
                function: "CFactory::Make()".into()
            })
        );
    }

    /// Allocates and constructs one item, then runs `rows`.
    fn single(rows_after: &[(&'static str, &str)]) -> Loading {
        let mut lines = vec![
            ("stp", "x19,x30,[sp,#-0x10]!".to_owned()),
            ("bl", format!("#{ALLOCATE:#x}")),
            ("mov", "x19,x0".to_owned()),
            ("bl", format!("#{ITEM_CONSTRUCTOR:#x}")),
        ];
        lines.extend(
            rows_after
                .iter()
                .map(|(op, operands)| (*op, (*operands).to_owned())),
        );
        lines.push(("ldp", "x19,x30,[sp],#0x10".into()));
        lines.push(("ret", String::new()));
        let borrowed: Vec<(&str, &str)> = lines
            .iter()
            .map(|(op, operands)| (*op, operands.as_str()))
            .collect();
        let mut loading = loading(loader(NO_GATE), 0);
        loading.code = code(&[database_constructor(0), rows(0x300, &borrowed), read()]);
        loading.loaders = vec![function(0x300, "CDb::Single()", &[])];
        loading
    }

    /// A thunk counts for the item only with `this` at the subobject whose vtable holds it.
    #[test]
    fn a_thunk_counts_only_with_its_own_adjustment() {
        let thunk = format!("#{THUNK:#x}");
        let at_base = single(&[("add", "x0,x19,#0x38"), ("bl", &thunk)]);
        assert_eq!(check(&at_base, &[ROOT]), Ok(()));

        let complete = single(&[("mov", "x0,x19"), ("bl", &thunk)]);
        assert_eq!(check(&complete, &[ROOT]), unread("CDb::Single()"));

        let other_base = single(&[("add", "x0,x19,#0x40"), ("bl", &thunk)]);
        assert_eq!(check(&other_base, &[ROOT]), unread("CDb::Single()"));
    }

    /// The database constructor leaves `x2` zero; the loader's own `x2` is a flag that it did not
    /// receive from the constructor.
    #[test]
    fn a_loader_starts_with_its_own_arguments() {
        let mut loading = loading(loader([("cbnz", "w2,#0x24c"), ("nop", "")]), 0);
        let mut constructor = database_constructor(0);
        constructor.insert(3, (0x10c, "mov", "x2,#0".into()));
        constructor[4].0 = 0x110;
        loading.code = code(&[
            constructor,
            loader([("cbnz", "w2,#0x24c"), ("nop", "")]),
            read(),
        ]);
        assert_eq!(check(&loading, &[ROOT]), unread("CDb::Load(CReader&)"));
    }

    /// The database constructor keeps a pointer to memory that it allocated. The loader reads its
    /// new item only when the item is that memory, which it never is.
    #[test]
    fn new_items_are_not_memory_that_the_database_constructor_reserved() {
        let mut loading = loading(loader(NO_GATE), 0);
        let constructor = rows(
            DATABASE_CONSTRUCTOR,
            &[
                ("stp", "x20,x30,[sp,#-0x10]!"),
                ("mov", "x20,x0"),
                ("str", "wzr,[x20,#0x70]"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("str", "x0,[x20,#0x80]"),
                ("ldp", "x20,x30,[sp],#0x10"),
                ("ret", ""),
            ],
        );
        let compared = rows(
            0x300,
            &[
                ("stp", "x20,x30,[sp,#-0x20]!"),
                ("str", "x19,[sp,#0x10]"),
                ("mov", "x20,x0"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("mov", "x19,x0"),
                ("bl", &format!("#{ITEM_CONSTRUCTOR:#x}")),
                ("ldr", "x9,[x20,#0x80]"),
                ("cmp", "x9,x19"),
                ("b.ne", "#0x330"),
                ("add", "x0,x19,#0x38"),
                ("bl", &format!("#{THUNK:#x}")),
                ("nop", ""),
                // 0x330
                ("ldr", "x19,[sp,#0x10]"),
                ("ldp", "x20,x30,[sp],#0x20"),
                ("ret", ""),
            ],
        );
        loading.code = code(&[constructor, compared, read()]);
        loading.loaders = vec![function(0x300, "CDb::Compare()", &[])];
        assert_eq!(check(&loading, &[ROOT]), unread("CDb::Compare()"));
    }

    /// The loop head branches on unknown flags; its exit constructs an item and returns.
    #[test]
    fn an_item_behind_a_branch_at_a_loop_head_is_followed() {
        let mut loading = loading(loader(NO_GATE), 0);
        let looping = rows(
            0x300,
            &[
                ("cmp", "x7,#0"),
                ("b.eq", "#0x30c"),
                ("b", "#0x304"),
                ("bl", &format!("#{ALLOCATE:#x}")),
                ("bl", &format!("#{ITEM_CONSTRUCTOR:#x}")),
                ("ret", ""),
            ],
        );
        loading.code = code(&[database_constructor(0), loader(NO_GATE), read(), looping]);
        loading.loaders = vec![function(0x300, "CDb::Looping()", &[])];
        assert_eq!(check(&loading, &[ROOT]), unread("CDb::Looping()"));
    }

    #[test]
    fn an_item_that_code_constructs_inline_is_not_established() {
        let mut loading = loading(loader(NO_GATE), 0);
        loading.inline_constructions = vec!["CDb::ReadNewEntry()".into()];
        assert_eq!(
            check(&loading, &[ROOT]),
            Err(NotEstablished::InlineConstruction {
                function: "CDb::ReadNewEntry()".into()
            })
        );
    }

    #[test]
    fn a_path_that_throws_loads_no_item() {
        let never_returns = format!("#{NEVER_RETURNS:#x}");
        let throwing = loader([("cbz", "x3,#0x230"), ("bl", &never_returns)]);
        let loading = loading(throwing, 0);
        assert_eq!(check(&loading, &[ROOT]), Ok(()));
    }
}
