//! Dynamic names: the integer-flag stores that commands define, remove and read.
//!
//! A command takes part when its assign or member reader stores the 16-bit index that the
//! engine's flag interner returns at one owner offset. Each further fact comes from a join of
//! its own:
//!
//! - **Role.** The command's execute or evaluate slot runs with a stand-in command object whose
//!   stored index holds a marker. A path that reaches the setter or the remover with the marker
//!   as its flag argument, and with a store that an accessor call returned, defines or removes.
//!   A complete-function membership-scan shape whose compared value loads from the stored index
//!   reads. A scan loop cannot be proved by running it, so reads need the shape.
//! - **Store.** The accessor that the role reached is resolved for each scope type that the
//!   command declares (see [`routes`]).
//! - **Dynamic form.** A reader that splits `name@target` stores the name and the target at
//!   owner offsets on its dynamic branch and does not intern there. The form is `name@target`
//!   only when the role slot passes that name and target to the call whose result it uses as the
//!   flag.
//!
//! A fact that no join establishes stays unresolved with its reason. No command, class or scope
//! is selected by name: the flag functions come from the binding by signature.
pub mod routes;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::LazyLock;

use super::declarations::{
    self, DeclarationInput, DeclarationResult, ScopeOutcome, ScopeType, Site, number,
};
use super::decode::{Instruction, decode_arm64};
use super::evaluate::{Call, Code, Exit, Machine, Path};
use super::references::shapes::{Shape, canonical};
use super::stop::Unresolved;
use crate::{DeclarationKind, DynamicNameForm};
use routes::{Route, RouteInput, Routes, STAND_IN_STRIDE, position_or_push, stand_in};

/// Name and revision of this static method.
pub const METHOD: &str = "dynamic-names/v1";

/// Bytes of a stand-in command object that the method tracks.
const OBJECT_SPAN: u64 = 0x1000;

/// The flag index that the interner returns in a run; it marks where the reader stores it.
const INDEX_MARKER: u64 = 0xa5c3;

/// The flag index that the dynamic-name call returns in a role run.
const DYNAMIC_MARKER: u64 = 0x5a3c;

/// First stand-in value of the store that an accessor call returns in a role run.
const ACCESSOR_BASE: u64 = 0x5000_0000_0000;

/// Path labels of a reader run.
const READER_CALLED: u64 = 1;
const NAME_DESTINATION: u64 = 2;
const TARGET_DESTINATION: u64 = 3;
const INTERNED_AFTER_READER: u64 = 4;
const INTERNED_WITHOUT_READER: u64 = 5;

static MEMBERSHIP_SCAN: LazyLock<Shape> =
    LazyLock::new(|| Shape::parse(include_str!("dynamic_names/membership_scan.shape")));

/// The engine functions that the method recognizes, located by the binding.
#[derive(Debug, Clone, Copy)]
pub struct FlagFunctions {
    /// Splits a `name@target` value. It returns true for a dynamic name, and writes the name to
    /// its `x1` destination and the target to its `x2` destination.
    pub name_reader: u64,
    /// Interns a flag name and returns its 16-bit index.
    pub interner: u64,
    /// Sets the flag whose index is in `x1` in the store in `x0`.
    pub setter: u64,
    /// Clears the flag whose index is in `x1` from the store in `x0`.
    pub remover: u64,
}

/// Virtual slots of a command, relative to its vtable address point.
#[derive(Debug, Clone, Copy)]
pub struct CommandSlots {
    /// The reader of a command's assigned value.
    pub assign: u64,
    /// The execute slot of an effect or the evaluate slot of a trigger.
    pub role: u64,
}

/// One command family: its registrations and the code that their receivers reach.
pub struct CommandFamily {
    pub kind: DeclarationKind,
    pub declarations: DeclarationInput,
    pub inventory: DeclarationResult,
    pub slots: CommandSlots,
}

/// Executable-derived input for the dynamic-name method.
pub struct DynamicNameInput {
    pub families: Vec<CommandFamily>,
    pub functions: FlagFunctions,
    /// Offset of a scope object's type field, which holds the type's mask bit.
    pub scope_type_offset: u64,
    /// Demangled names by address and pointer slot, for the canonical lines of a shape.
    pub names: BTreeMap<u64, String>,
}

/// What a command does with a flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    /// The command sets the flag in the store.
    Defines,
    /// The command clears the flag from the store.
    Removes,
    /// The command tests whether the store holds the flag.
    Reads,
}

/// One role of a command in the store of one scope type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleUse {
    /// What the command does with the flag.
    pub role: Role,
    /// The declared scope type whose store the route was resolved for.
    pub scope: ScopeType,
    /// The store's route, or why it was not established.
    pub route: Result<Route, Unresolved>,
}

/// A command that stores an interned flag name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagCommand {
    /// Whether the command accepts `name@target`.
    pub form: DynamicNameForm,
    /// Each established role, once for each declared scope type.
    pub uses: Vec<RoleUse>,
    /// Why a role, a scope set or the dynamic form was not established.
    pub stops: Vec<Unresolved>,
}

/// The method's result for one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameOutcome {
    /// Neither reader of the command calls the name reader or the interner.
    NotFlag,
    /// The command stores an interned flag index.
    Flag(FlagCommand),
    /// The command's registration or command object was not joined, so its readers were not
    /// examined.
    NotExamined(Unresolved),
    /// The command's readers name a flag, but no one stored index was established.
    Unresolved(Unresolved),
}

/// One registered command and its outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandNames {
    /// The command's family.
    pub kind: DeclarationKind,
    /// The registered name.
    pub name: String,
    /// What the command does with a flag name.
    pub outcome: NameOutcome,
}

/// Who calls a function whose route is resolved: a command with this vtable address point, or a
/// scope object's own accessor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Caller {
    Command { vtable: u64 },
    Scope,
}

/// Examine every named command of every family.
pub fn analyze(input: &DynamicNameInput) -> Vec<CommandNames> {
    let mut commands = Vec::new();
    for family in &input.families {
        let mut examiner = Examiner::new(input, family);
        for (name, registration) in registrations(&family.inventory) {
            let outcome = match registration {
                Ok((factory, scopes)) => examiner.command(factory, &scopes),
                Err(stop) => NameOutcome::NotExamined(stop),
            };
            commands.push(CommandNames {
                kind: family.kind,
                name,
                outcome,
            });
        }
    }

    commands
}

/// Each registered name with its one factory and declared scopes.
fn registrations(
    inventory: &DeclarationResult,
) -> BTreeMap<String, Result<(u64, ScopeOutcome), Unresolved>> {
    let mut factories = BTreeMap::<String, BTreeSet<u64>>::new();
    let mut scopes = BTreeMap::<String, ScopeOutcome>::new();
    let mut unreadable = BTreeSet::new();
    for (_, site) in &inventory.sites {
        match site {
            Site::Declared {
                name,
                factory,
                scopes: declared,
                ..
            } => {
                factories.entry(name.clone()).or_default().insert(*factory);
                scopes.insert(name.clone(), declared.clone());
            }
            Site::Unreadable {
                name: Some(name), ..
            } => {
                unreadable.insert(name.clone());
            }
            _ => {}
        }
    }

    factories
        .into_iter()
        .map(|(name, factories)| {
            let registration = if unreadable.contains(&name) {
                Err(Unresolved::new("command-registration"))
            } else if factories.len() > 1 {
                Err(Unresolved::new("ambiguous-command-factory"))
            } else {
                let factory = *factories.first().expect("a declared name has a factory");
                Ok((factory, scopes[&name].clone()))
            };
            (name, registration)
        })
        .collect()
}

/// What the slot functions of one command store, before the role run.
#[derive(Debug, Default)]
struct NameRead {
    /// Whether a slot function calls the name reader or the interner.
    names_flags: bool,
    /// Owner offsets where a static-name path stored the interned index.
    indexes: BTreeSet<u64>,
    /// Owner offsets of the name reader's name and target destinations.
    destinations: BTreeSet<(u64, u64)>,
    /// Whether the interner ran after the name reader on a static-name path.
    interned_after_reader: bool,
    /// Whether the interner ran on a path that did not call the name reader.
    interned_without_reader: bool,
    /// Whether the interner ran on a dynamic-name path.
    dynamic_interns: bool,
}

impl NameRead {
    fn merge(&mut self, other: &Self) {
        self.names_flags |= other.names_flags;
        self.indexes.extend(&other.indexes);
        self.destinations.extend(&other.destinations);
        self.interned_after_reader |= other.interned_after_reader;
        self.interned_without_reader |= other.interned_without_reader;
        self.dynamic_interns |= other.dynamic_interns;
    }

    /// The name and target offsets of a dynamic branch that keeps them and does not intern.
    fn kept_dynamic_name(&self) -> Option<(u64, u64)> {
        let [destination] = self.destinations.iter().collect::<Vec<_>>()[..] else {
            return None;
        };

        (self.interned_after_reader && !self.dynamic_interns).then_some(*destination)
    }

    /// Record the returned paths of a run whose name reader reported a static name.
    fn record_static_run<'m, 'a: 'm>(
        &mut self,
        returned: impl Iterator<Item = &'m Machine<'a>>,
        command: u64,
    ) {
        for machine in returned {
            if let (Some(name), Some(target)) = (
                machine.labelled(NAME_DESTINATION),
                machine.labelled(TARGET_DESTINATION),
            ) {
                self.destinations.insert((name, target));
            }

            let after_reader = machine.labelled(INTERNED_AFTER_READER).is_some();
            let without_reader = machine.labelled(INTERNED_WITHOUT_READER).is_some();
            self.interned_after_reader |= after_reader;
            self.interned_without_reader |= without_reader;
            if after_reader || without_reader {
                self.indexes.extend(
                    (0..OBJECT_SPAN - 1)
                        .filter(|&offset| machine.read(command + offset, 2) == Some(INDEX_MARKER)),
                );
            }
        }
    }

    /// Record the returned paths of a run whose name reader reported a dynamic name.
    fn record_dynamic_run<'m, 'a: 'm>(&mut self, returned: impl Iterator<Item = &'m Machine<'a>>) {
        for machine in returned {
            self.dynamic_interns |= machine.labelled(INTERNED_AFTER_READER).is_some();
        }
    }
}

/// The stand-in answers of a reader run: the name reader reports a static or a dynamic name,
/// and the interner returns [`INDEX_MARKER`]. Each path labels what it called.
struct ReaderCalls {
    functions: FlagFunctions,
    command: u64,
    dynamic: bool,
}

impl ReaderCalls {
    fn answer(&self, target: Option<u64>, machine: &mut Machine<'_>) -> Call {
        match target {
            Some(target) if target == self.functions.name_reader => self.read_name(machine),
            Some(target) if target == self.functions.interner => Self::intern(machine),
            _ => {
                forget_if_passed(machine, self.command);
                Call::Return(None)
            }
        }
    }

    /// Label the name and target destinations when both are in the command object. The reader
    /// may write any of the object.
    fn read_name(&self, machine: &mut Machine<'_>) -> Call {
        let command = self.command;
        let destination = |register| {
            let value = machine.register(register)?;
            (command..command + OBJECT_SPAN)
                .contains(&value)
                .then(|| value - command)
        };
        if let (Some(name), Some(target)) = (destination(1), destination(2)) {
            machine.label(NAME_DESTINATION, name);
            machine.label(TARGET_DESTINATION, target);
        }

        machine.label(READER_CALLED, 1);
        machine.forget(command, OBJECT_SPAN);

        Call::Return(Some(u64::from(self.dynamic)))
    }

    fn intern(machine: &mut Machine<'_>) -> Call {
        let key = match machine.labelled(READER_CALLED) {
            Some(_) => INTERNED_AFTER_READER,
            None => INTERNED_WITHOUT_READER,
        };
        machine.label(key, 1);

        Call::Return(Some(INDEX_MARKER))
    }
}

/// The roles that one role slot establishes.
#[derive(Debug, Default)]
struct RoleSlot {
    /// Each role with the accessor that gave its store and the store's offset in the result.
    roles: BTreeSet<(Role, u64, u64)>,
    /// Whether the slot passes the stored dynamic name to the call whose result is its flag.
    dynamic_joined: bool,
    stops: Vec<Unresolved>,
}

/// The stand-in answers of a role run, and the setter and remover calls that its paths made.
struct RoleCalls {
    functions: FlagFunctions,
    command: u64,
    scope: u64,
    /// The name and target offsets that the command keeps for a dynamic name.
    dynamic: Option<(u64, u64)>,
    /// Each accessor that the run called; its position selects the stand-in store it returns.
    accessors: Vec<u64>,
    /// Each setter or remover call with its store and its 16-bit flag argument.
    flag_calls: Vec<(Role, Option<u64>, Option<u64>)>,
}

impl RoleCalls {
    /// Record a setter or remover call. An accessor call with the command and the scope returns
    /// a stand-in store, and a call with the kept name and target returns [`DYNAMIC_MARKER`].
    fn answer(&mut self, target: Option<u64>, machine: &mut Machine<'_>) -> Call {
        let register = |index| machine.register(index);
        let role = match target {
            Some(target) if target == self.functions.setter => Some(Role::Defines),
            Some(target) if target == self.functions.remover => Some(Role::Removes),
            _ => None,
        };
        if let Some(role) = role {
            let flag = register(1).map(|flag| flag & 0xffff);
            self.flag_calls.push((role, register(0), flag));
            return Call::Return(None);
        }

        if let Some(target) = target
            && register(0) == Some(self.command)
            && register(1) == Some(self.scope)
        {
            let accessor = position_or_push(&mut self.accessors, target);
            return Call::Return(Some(ACCESSOR_BASE + accessor * STAND_IN_STRIDE));
        }

        if let Some((name, target)) = self.dynamic
            && register(1) == Some(self.command + target)
            && register(2) == Some(self.command + name)
        {
            return Call::Return(Some(DYNAMIC_MARKER));
        }

        forget_if_passed(machine, self.command);
        Call::Return(None)
    }

    /// The roles that the recorded flag calls establish, with the stops of the paths and calls.
    fn into_role_slot(self, paths: Vec<Path<'_>>) -> RoleSlot {
        let mut slot = RoleSlot::default();
        for path in paths {
            if let Err(stop) = path.end
                && !slot.stops.contains(&stop)
            {
                slot.stops.push(stop);
            }
        }

        for (role, store, flag) in self.flag_calls {
            let accessor = store.and_then(|store| stand_in(store, ACCESSOR_BASE, &self.accessors));
            match (accessor, flag) {
                (Some((accessor, offset)), Some(INDEX_MARKER)) => {
                    slot.roles.insert((role, accessor, offset));
                }
                (Some(_), Some(DYNAMIC_MARKER)) => slot.dynamic_joined = true,
                (None, _) => slot.stops.push(Unresolved::new("role-store")),
                (Some(_), _) => slot.stops.push(Unresolved::new("role-flag")),
            }
        }

        slot
    }
}

/// One family's commands, with the runs that commands share.
struct Examiner<'a> {
    input: &'a DynamicNameInput,
    family: &'a CommandFamily,
    reads: HashMap<u64, NameRead>,
    routes: Routes<'a>,
}

impl<'a> Examiner<'a> {
    fn new(input: &'a DynamicNameInput, family: &'a CommandFamily) -> Self {
        let declarations = &family.declarations;
        let routes = Routes::new(RouteInput {
            functions: &declarations.functions,
            pointers: &declarations.pointers,
            data: declarations.pointer_data(),
            scope_type_offset: input.scope_type_offset,
        });
        Self {
            input,
            family,
            reads: HashMap::new(),
            routes,
        }
    }

    fn command(&mut self, factory: u64, scopes: &ScopeOutcome) -> NameOutcome {
        let declarations = &self.family.declarations;
        let reader = match declarations::command_reader(declarations, factory) {
            Ok(reader) => reader,
            Err(stop) => return NameOutcome::NotExamined(stop),
        };
        let read = self.command_name_read(reader.vtable, reader.member);
        if !read.names_flags {
            return NameOutcome::NotFlag;
        }
        let index = match read.indexes.iter().collect::<Vec<_>>()[..] {
            [index] => *index,
            [] => return NameOutcome::Unresolved(Unresolved::new("index-store")),
            _ => return NameOutcome::Unresolved(Unresolved::new("index-stores")),
        };
        let dynamic = read.kept_dynamic_name();

        let slot = self.command_role_slot(reader.vtable, index, dynamic);
        let mut stops = slot.stops;
        if slot.roles.is_empty() {
            stops.push(Unresolved::new("no-role"));
        }
        let form = dynamic_form(&read, dynamic, slot.dynamic_joined);
        if form == DynamicNameForm::Unresolved {
            stops.push(Unresolved::new("dynamic-form"));
        }
        let uses = match self.scope_types(scopes) {
            Ok(types) => self.role_uses(&slot.roles, reader.vtable, &types),
            Err(stop) => {
                stops.push(stop);
                Vec::new()
            }
        };

        NameOutcome::Flag(FlagCommand { form, uses, stops })
    }

    /// What the command's assign and member readers do with a flag name.
    fn command_name_read(&mut self, vtable: u64, member: u64) -> NameRead {
        let assign = self
            .family
            .declarations
            .pointers
            .get(&(vtable + self.family.slots.assign))
            .copied();
        let functions: BTreeSet<u64> = assign.into_iter().chain([member]).collect();
        let mut read = NameRead::default();
        for function in functions {
            read.merge(self.name_read(function));
        }

        read
    }

    /// The roles of the command's execute or evaluate slot.
    fn command_role_slot(&self, vtable: u64, index: u64, dynamic: Option<(u64, u64)>) -> RoleSlot {
        let slot = vtable + self.family.slots.role;
        match self.family.declarations.pointers.get(&slot) {
            Some(&function) => self.role_slot(function, vtable, index, dynamic),
            None => RoleSlot {
                stops: vec![Unresolved::new("role-slot")],
                ..RoleSlot::default()
            },
        }
    }

    /// Each role in the store of each scope type.
    fn role_uses(
        &mut self,
        roles: &BTreeSet<(Role, u64, u64)>,
        vtable: u64,
        types: &[ScopeType],
    ) -> Vec<RoleUse> {
        let caller = Caller::Command { vtable };
        let mut uses = Vec::new();
        for &(role, accessor, offset) in roles {
            for scope in types {
                let route = self
                    .routes
                    .route(accessor, caller, scope.bit)
                    .map(|route| route.plus(offset));
                uses.push(RoleUse {
                    role,
                    scope: scope.clone(),
                    route,
                });
            }
        }

        uses
    }

    /// The scope types that a declared scope set names; every type of the scope table for
    /// `Any`.
    fn scope_types(&self, scopes: &ScopeOutcome) -> Result<Vec<ScopeType>, Unresolved> {
        match scopes {
            ScopeOutcome::Listed(types) => Ok(types.clone()),
            ScopeOutcome::Unresolved(_) => Err(Unresolved::new("scope-set")),
            ScopeOutcome::Any => {
                let names = self
                    .family
                    .declarations
                    .scope_names
                    .as_ref()
                    .ok_or(Unresolved::new("scope-set"))?;
                Ok(names
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| !name.is_empty())
                    .map(|(bit, name)| ScopeType {
                        bit,
                        name: name.clone(),
                    })
                    .collect())
            }
        }
    }

    fn name_read(&mut self, function: u64) -> &NameRead {
        if !self.reads.contains_key(&function) {
            let read = self.read_slot(function);
            self.reads.insert(function, read);
        }

        &self.reads[&function]
    }

    /// Run a slot function once with a static-name and once with a dynamic-name reader result.
    fn read_slot(&self, function: u64) -> NameRead {
        let flags = self.input.functions;
        let Some(rows) = self.rows(function) else {
            return NameRead::default();
        };
        if !calls_any(&rows, &[flags.name_reader, flags.interner]) {
            return NameRead::default();
        }
        let code = Code::from_rows(rows);
        let mut read = NameRead {
            names_flags: true,
            ..NameRead::default()
        };
        for dynamic in [false, true] {
            let mut machine = Machine::new(&code, self.family.declarations.pointer_data());
            let command = machine.reserve(OBJECT_SPAN);
            machine.set_register(0, command);

            let calls = ReaderCalls {
                functions: flags,
                command,
                dynamic,
            };
            let paths = machine.run_paths(function, &mut |target, machine| {
                Ok(calls.answer(target, machine))
            });

            let returned = paths
                .iter()
                .filter(|path| matches!(path.end, Ok(Exit::Returned)))
                .map(|path| &path.machine);
            if dynamic {
                read.record_dynamic_run(returned);
            } else {
                read.record_static_run(returned, command);
            }
        }

        read
    }

    /// Run the role slot when it calls the setter or the remover, and match it against the read
    /// shape.
    fn role_slot(
        &self,
        function: u64,
        vtable: u64,
        index: u64,
        dynamic: Option<(u64, u64)>,
    ) -> RoleSlot {
        let Some(rows) = self.rows(function) else {
            return RoleSlot {
                stops: vec![Unresolved::new("role-code")],
                ..RoleSlot::default()
            };
        };
        let flags = self.input.functions;
        let mut slot = if calls_any(&rows, &[flags.setter, flags.remover]) {
            self.run_role(&rows, function, vtable, index, dynamic)
        } else {
            RoleSlot::default()
        };
        self.match_read(&rows, vtable, index, dynamic, &mut slot);

        slot
    }

    fn run_role(
        &self,
        rows: &[Instruction],
        function: u64,
        vtable: u64,
        index: u64,
        dynamic: Option<(u64, u64)>,
    ) -> RoleSlot {
        let code = Code::from_rows(rows.to_vec());
        let mut machine = Machine::new(&code, self.family.declarations.pointer_data());
        let command = stand_in_command(&mut machine, vtable);
        machine.write(command + index, 2, INDEX_MARKER);
        let scope = machine.reserve(OBJECT_SPAN);
        machine.set_register(0, command);
        machine.set_register(1, scope);

        let mut calls = RoleCalls {
            functions: self.input.functions,
            command,
            scope,
            dynamic,
            accessors: Vec::new(),
            flag_calls: Vec::new(),
        };
        let paths = machine.run_paths(function, &mut |target, machine| {
            Ok(calls.answer(target, machine))
        });

        calls.into_role_slot(paths)
    }

    /// A read when the whole slot is the membership scan and it compares the stored index.
    fn match_read(
        &self,
        rows: &[Instruction],
        vtable: u64,
        index: u64,
        dynamic: Option<(u64, u64)>,
        slot: &mut RoleSlot,
    ) {
        let Some(bindings) = MEMBERSHIP_SCAN.matches(&canonical(rows, &self.input.names)) else {
            return;
        };
        let offset = |name: &str| bindings.get(name).and_then(|text| number(text));
        if offset("index") != Some(index) {
            slot.stops.push(Unresolved::new("read-index"));
            return;
        }
        let accessor = offset("accessor").and_then(|slot| {
            self.family
                .declarations
                .pointers
                .get(&(vtable + slot))
                .copied()
        });
        let Some(accessor) = accessor else {
            slot.stops.push(Unresolved::new("accessor-slot"));
            return;
        };
        slot.roles.insert((Role::Reads, accessor, 0));
        if let Some((name, target)) = dynamic {
            slot.dynamic_joined |= offset("name") == Some(name) && offset("target") == Some(target);
        }
    }

    fn rows(&self, function: u64) -> Option<Vec<Instruction>> {
        let body = self.family.declarations.functions.get(&function)?;

        decode_arm64(&body.code, body.address).ok()
    }
}

/// A name that no path splits does not accept `name@target`; a split name accepts it when its
/// dynamic branch keeps the name and the role slot uses what it kept.
fn dynamic_form(read: &NameRead, kept: Option<(u64, u64)>, joined: bool) -> DynamicNameForm {
    if read.interned_without_reader && !read.interned_after_reader {
        DynamicNameForm::NotAccepted
    } else if kept.is_some() && joined {
        DynamicNameForm::TargetSuffix
    } else {
        DynamicNameForm::Unresolved
    }
}

/// Whether `rows` call or tail-call one of `targets`.
fn calls_any(rows: &[Instruction], targets: &[u64]) -> bool {
    rows.iter()
        .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
        .filter_map(|row| number(&row.operands))
        .any(|target| targets.contains(&target))
}

/// A command object whose vtable is `vtable` and whose other fields are unknown.
fn stand_in_command(machine: &mut Machine<'_>, vtable: u64) -> u64 {
    let command = machine.reserve(OBJECT_SPAN);
    machine.write(command, 8, vtable);

    command
}

/// An unknown call that receives a pointer into the command object may write any of it.
fn forget_if_passed(machine: &mut Machine<'_>, command: u64) {
    let passed = (0..8).any(|register| {
        machine
            .register(register)
            .is_some_and(|value| (command..command + OBJECT_SPAN).contains(&value))
    });
    if passed {
        machine.forget(command, OBJECT_SPAN);
    }
}

#[cfg(test)]
mod tests;
