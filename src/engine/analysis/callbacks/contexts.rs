//! The context pass: the scope that one call site passes, along every path from the entry of its
//! function.
//!
//! The pass runs [`Machine::run_paths_to`] with unknown arguments. It follows the calls that
//! build a scope: a fresh constructor gives the scope object type 0 and self-links, and a setter
//! writes its type or links. Each such call runs on a copy of the machine, and its writes to the
//! type and link slots are kept when every path of the call agrees. A scope object that a fresh
//! constructor built is *tracked*; only a tracked object's slots are read at the site.
//!
//! A callee cannot reach a fresh stack object until its address leaves the function's own
//! registers and the link slots of tracked objects. So a tracked object *escapes* when its
//! address is an argument of a call that the pass does not follow, is stored where the call can
//! reach it, or is linked from an escaped object. A call reads the argument registers that its
//! signature uses, or all of them when the signature is not known; a virtual call on the object
//! that a proven instance pointer holds has a known target ([`super::instances`]), so its
//! signature is known. A call reads `x8`, the address of a returned object, only when its target
//! may read `x8` before writing it; this relies on the ABI, as the signature rule does: compiled
//! code never reads a caller-saved register after a call for its value before the call. A
//! *reader*, such as a call that fires an on_action, receives a scope but changes no type or link
//! and keeps no pointer, so a scope that it receives does not escape.
//!
//! A call reaches all memory outside the stack, and the stack from each live stack address that it
//! receives, or that is stored outside the stack, through an unknown address or in what it
//! reaches, up to the top of the frame that holds that address. What one call could reach stays
//! reachable for the later calls on the path, down to the stack pointer. At each call that the
//! pass does not follow, and at each reader, every known byte that the call reaches becomes
//! unknown, except the register saves of each function's prologue
//! ([`Machine::is_register_save`]) and the slots of a tracked object that has not escaped. A
//! tracked object whose address is in that memory escapes first: the call may leave the address
//! where a later call finds it. From that call on, every call that the pass does not follow makes
//! the slots of every escaped object unknown. Until it escapes, a store to an unknown address
//! leaves its slots known.
//!
//! The pass makes three assumptions. A callee that receives a pointer to another member of a scope
//! object does not write its type or links. No stack array indexed by an unknown value reaches a
//! scope object. A copy, a destructor, a string function or a lookup, which the pass recognizes
//! without running it, writes only its own object; of a scope object, the pass reads only the type
//! and link slots, and of a string object, nothing.
//! The pass releases a range that an entered call exposed only at a call made after that call
//! returns, so a second callee entered first at the same addresses forgets them at its own calls.
//! This only makes memory unknown; give exposures a frame identity if a block run loses an answer
//! to it.
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use super::{CallbackLayout, Context, Slot};
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use crate::engine::analysis::stop::{Unresolved, sort_and_dedup};

use super::names::StringFunctions;

/// Label kinds, in the top byte of a label key. The low bytes hold an object address.
const SCOPE: u64 = 1 << 56;
const NAME: u64 = 2 << 56;
const LIST: u64 = 3 << 56;
/// A stack address that a call could reach.
const EXPOSED: u64 = 4 << 56;

/// Values of a scope label.
const TRACKED: u64 = 1;
const ESCAPED: u64 = 2;

/// How deep the pass follows scope functions that call each other.
const CALL_DEPTH: usize = 3;

/// Scratch size of a list that a lookup returns.
const LIST_SIZE: u64 = 16;

/// The scope functions that the context pass follows or recognizes.
#[derive(Debug, Clone, Default)]
pub struct ScopeFunctions {
    /// Constructors of a new scope: type 0 and self-links.
    pub fresh_constructors: BTreeSet<u64>,
    /// Members that write the type or the links: typed setters, `Set`, `ClearRootFromPrev`.
    pub setters: BTreeSet<u64>,
    /// Copy and move constructors and assignment: the destination's slots become unknown.
    pub copies: BTreeSet<u64>,
    /// Destructors: the object is no longer tracked.
    pub destructors: BTreeSet<u64>,
    /// Functions that receive a scope but change no scope's type or links and keep no pointer
    /// to one: `const` members, `AccessVariables` and the calls that fire an on_action.
    pub readers: BTreeSet<u64>,
}

/// Where the site holds the subject of the call.
#[derive(Debug, Clone, Copy)]
pub(super) enum Subject {
    /// A string object in this register.
    String(usize),
    /// A list that a lookup returned, in this register.
    List(usize),
    /// Nothing that the path names, such as a game rule.
    None,
}

/// What the paths that arrive at one site pass.
#[derive(Debug, Clone, Default)]
pub(super) struct SiteContexts {
    /// For each arriving path, the literal that it proves for the subject, and its context.
    pub reached: Vec<(Option<u64>, Context)>,
    /// Why some path could not be followed to the site.
    pub unresolved: Option<&'static str>,
}

/// The calls that a run for a registry field block treats as evaluations of a block.
pub(super) struct BlockCalls<'a> {
    /// Trigger evaluators and effect executors: each takes its block in `x0` and its scope in
    /// [`EVALUATED_SCOPE`].
    pub evaluators: &'a BTreeSet<u64>,
    /// Other trigger and effect code that receives a scope, such as a tooltip builder. By
    /// assumption, neither this code nor an evaluation changes a scope object's type or links or
    /// keeps a pointer to one, so the pass treats both as scope readers.
    pub scope_users: &'a BTreeSet<u64>,
    /// Functions that pass a scope parameter on to an evaluation. The pass runs their code on
    /// the path, with the caller's arguments and memory.
    pub wrappers: &'a BTreeSet<u64>,
    /// Other functions that receive a scope. The pass runs their code on the path too, while
    /// the path is inside fewer than [`ENTER_LIMIT`] calls.
    pub receivers: &'a BTreeSet<u64>,
    /// Functions that never return: a path that calls one ends there.
    pub never_return: &'a BTreeSet<u64>,
}

/// How many calls deep a block run runs the code of functions that receive a scope.
pub(super) const ENTER_LIMIT: usize = 6;

/// The register in which an evaluator receives its scope.
pub(super) const EVALUATED_SCOPE: usize = 1;

/// What the selected call of a block run calls.
#[derive(Debug, Clone, Copy)]
pub(super) enum Selected {
    /// An evaluator: the selected call is itself an evaluation.
    Evaluator,
    /// A wrapper, which runs from the state that arrives at the call.
    Wrapper(u64),
}

/// What the evaluations that one selected call reaches receive.
#[derive(Debug, Clone, Default)]
pub(super) struct Evaluations {
    /// Each evaluator call that a path reached, with the context of the scope that it received.
    pub reached: Vec<(u64, Context)>,
    /// Why some path could not be followed to the selected call, or through it, other than a
    /// bound of the search, each once with where it stopped.
    pub unresolved: Vec<Unresolved>,
    /// The bounds of the search that some path reached, such as the path limit, each once.
    pub bounded: Vec<Unresolved>,
}

impl Evaluations {
    fn record(&mut self, unresolved: Unresolved) {
        if unresolved.is_bound() {
            self.bounded.push(unresolved);
        } else {
            self.unresolved.push(unresolved);
        }
    }
}

/// The inputs that every run of the pass shares.
pub(super) struct Runner<'a> {
    pub scope_code: &'a [Instruction],
    pub scopes: &'a ScopeFunctions,
    pub strings: &'a StringFunctions,
    pub lookups: &'a BTreeSet<u64>,
    pub data: &'a ReadOnlyData,
    pub layout: CallbackLayout,
    /// How many argument registers, from `x0`, each known function reads. A function that is not
    /// listed may read `x0` to `x7`.
    pub arguments: &'a BTreeMap<u64, usize>,
    /// How many argument registers a call through a pointer reads, by the call instruction, when
    /// the binding knows what the pointer holds.
    pub call_arguments: &'a BTreeMap<u64, usize>,
    /// Functions that ignore the `x8` that they receive.
    pub ignores_x8: &'a BTreeSet<u64>,
}

impl Runner<'_> {
    /// Code for one function together with the scope functions that the pass runs.
    pub fn code(&self, function: &[Instruction]) -> Code {
        Code::from_rows(function.iter().chain(self.scope_code).cloned())
    }

    /// Follow every path from `entry` to the call at `site`, and read the scope in register
    /// `scope` and the subject there.
    pub fn contexts(
        &self,
        code: &Code,
        entry: u64,
        site: u64,
        subject: Subject,
        scope: usize,
    ) -> SiteContexts {
        let machine = Machine::new(code, self.data);
        let paths = machine.run_paths_to(entry, site, &mut |target, machine| {
            self.call(target, machine, 0);
            Ok(Call::Return(self.returned(target, machine)))
        });

        let mut result = SiteContexts::default();
        for path in paths {
            match path.end {
                Ok(Exit::Reached) => {
                    let machine = &path.machine;
                    let literal = match subject {
                        Subject::String(register) => machine
                            .register(register)
                            .and_then(|object| machine.labelled(NAME | object)),
                        Subject::List(register) => machine
                            .register(register)
                            .and_then(|list| machine.labelled(LIST | list)),
                        Subject::None => None,
                    };
                    let context = self.read(machine, machine.register(scope));
                    result.reached.push((literal, context));
                }
                Ok(_) => result.unresolved = Some("left-the-site"),
                Err(Unresolved { reason, .. }) => result.unresolved = Some(reason),
            }
        }
        result
    }

    /// Follow every path from `entry` to the call at `site`, then through that call, and give
    /// the context that each evaluation reached through the call receives.
    ///
    /// Before the site, evaluations act only through their effects, so an evaluation of a scope
    /// that the function received gives nothing.
    pub fn evaluations(
        &self,
        code: &Code,
        entry: u64,
        site: u64,
        selected: Selected,
        calls: &BlockCalls<'_>,
    ) -> Evaluations {
        let machine = Machine::new(code, self.data);
        let prefix = machine.run_paths_to(entry, site, &mut |target, machine| {
            Ok(self.block_call(target, machine, calls))
        });

        let mut result = Evaluations::default();
        for path in prefix {
            match path.end {
                Ok(Exit::Reached) => {}
                Ok(Exit::Stopped(_) | Exit::Trapped) => continue,
                Ok(_) => {
                    result.unresolved.push(Unresolved::new("left-the-site"));
                    continue;
                }
                Err(unresolved) => {
                    result.record(unresolved);
                    continue;
                }
            }

            let machine = path.machine;
            let wrapper = match selected {
                Selected::Evaluator => {
                    let context = self.read(&machine, machine.register(EVALUATED_SCOPE));
                    result.reached.push((site, context));
                    continue;
                }
                Selected::Wrapper(wrapper) => wrapper,
            };

            let through = machine.run_paths(wrapper, &mut |target, machine| {
                if target.is_some_and(|target| calls.evaluators.contains(&target)) {
                    let context = self.read(machine, machine.register(EVALUATED_SCOPE));
                    result.reached.push((machine.pc(), context));
                }
                Ok(self.block_call(target, machine, calls))
            });
            for path in through {
                if let Err(unresolved) = path.end {
                    result.record(unresolved);
                }
            }
        }

        sort_and_dedup(&mut result.unresolved);
        sort_and_dedup(&mut result.bounded);
        result
    }

    /// Apply one call on a path of a block run: a function that never returns ends the path, a
    /// wrapper or a scope receiver runs on the path, trigger and effect code reads the scope, and
    /// every other call is applied as in [`Runner::contexts`].
    fn block_call(
        &self,
        target: Option<u64>,
        machine: &mut Machine<'_>,
        calls: &BlockCalls<'_>,
    ) -> Call {
        match target {
            Some(target) if calls.never_return.contains(&target) => Call::Stop,
            Some(target) if calls.wrappers.contains(&target) => Call::Enter,
            Some(target)
                if calls.receivers.contains(&target)
                    && machine.entered_calls().count() < ENTER_LIMIT =>
            {
                Call::Enter
            }
            Some(target)
                if calls.evaluators.contains(&target) || calls.scope_users.contains(&target) =>
            {
                self.forget_reached(machine, Some(target), BTreeSet::new());
                Call::Return(None)
            }
            _ => {
                self.call(target, machine, 0);
                Call::Return(self.returned(target, machine))
            }
        }
    }

    /// Run a rule forwarder from its entry with `base` as its receiver and `probe` in register
    /// `enumeration`, and give the rule object that each arriving path passes in `x0`.
    pub fn probe(
        &self,
        code: &Code,
        entry: u64,
        site: u64,
        enumeration: usize,
        probe: u64,
    ) -> (u64, Vec<Option<u64>>) {
        let mut machine = Machine::new(code, self.data);
        let base = machine.allocate(16);
        machine.set_register(0, base);
        machine.set_register(enumeration, probe);
        let paths = machine.run_paths_to(entry, site, &mut |_, _| Ok(Call::Return(None)));
        let passed = paths
            .iter()
            .map(|path| match path.end {
                Ok(Exit::Reached) => path.machine.register(0),
                _ => None,
            })
            .collect();
        (base, passed)
    }

    /// Apply one call on a path of the pass.
    fn call(&self, target: Option<u64>, machine: &mut Machine<'_>, depth: usize) {
        let Some(target) = target else {
            self.unfollowed(machine, None);
            return;
        };
        let object = machine.register(0);
        let scopes = self.scopes;

        if scopes.fresh_constructors.contains(&target) || scopes.setters.contains(&target) {
            self.follow(target, machine, depth);
        } else if scopes.copies.contains(&target) {
            if let Some(object) = object {
                self.forget_slots(machine, object);
            }
        } else if scopes.destructors.contains(&target) {
            if let Some(object) = object {
                self.untrack(machine, object);
            }
        } else if scopes.readers.contains(&target) {
            self.forget_reached(machine, Some(target), BTreeSet::new());
        } else if self.strings.from_literal.contains(&target) {
            if let Some(object) = object {
                match machine.register(1) {
                    Some(literal) => machine.label(NAME | object, literal),
                    None => machine.unlabel(NAME | object),
                }
            }
        } else if self.strings.copy.contains(&target) {
            let source = machine
                .register(1)
                .and_then(|source| machine.labelled(NAME | source));
            if let Some(object) = object {
                match source {
                    Some(literal) => machine.label(NAME | object, literal),
                    None => machine.unlabel(NAME | object),
                }
            }
        } else if self.strings.destructors.contains(&target) {
            if let Some(object) = object {
                machine.unlabel(NAME | object);
            }
        } else if !self.lookups.contains(&target) {
            self.unfollowed(machine, Some(target));
        }
    }

    /// The value that a call returns: a lookup returns a new list labelled with its name.
    fn returned(&self, target: Option<u64>, machine: &mut Machine<'_>) -> Option<u64> {
        let target = target?;
        if !self.lookups.contains(&target) {
            return None;
        }
        let literal = machine
            .register(1)
            .and_then(|name| machine.labelled(NAME | name));
        let list = machine.allocate(LIST_SIZE);
        if let Some(literal) = literal {
            machine.label(LIST | list, literal);
        }
        Some(list)
    }

    /// Run a scope function on a copy of the machine, and keep its writes to the object's slots
    /// where every path agrees. A path that passes the object itself to a call that the pass does
    /// not follow, or that reaches the call depth, makes the slots unknown. Another call that the
    /// pass does not follow makes the memory that it reaches unknown, except the object's slots.
    fn follow(&self, target: u64, machine: &mut Machine<'_>, depth: usize) {
        let Some(object) = machine.register(0) else {
            self.unfollowed(machine, Some(target));
            return;
        };

        let mut copy = machine.without_entered_calls();
        copy.label(SCOPE | object, TRACKED);
        let paths = copy.run_paths(target, &mut |callee, inner| {
            let followed = callee.is_some_and(|callee| {
                self.scopes.fresh_constructors.contains(&callee)
                    || self.scopes.setters.contains(&callee)
            });
            match callee {
                Some(callee) if followed && depth < CALL_DEPTH => {
                    self.follow(callee, inner, depth + 1);
                    Ok(Call::Return(None))
                }
                _ if followed => Err(Unresolved::new("call-depth")),
                _ if self
                    .passed(callee, inner.pc())
                    .any(|index| inner.register(index) == Some(object)) =>
                {
                    Err(Unresolved::new("scope-passed-on"))
                }
                _ => {
                    self.forget_reached(inner, callee, BTreeSet::new());
                    Ok(Call::Return(None))
                }
            }
        });

        let fresh = self.scopes.fresh_constructors.contains(&target);
        if fresh {
            self.untrack(machine, object);
        }

        for (offset, width) in self.slots() {
            let mut values = BTreeSet::new();
            let mut known = true;
            for path in &paths {
                match path.end {
                    Ok(Exit::Trapped) => {}
                    Ok(Exit::Returned) => {
                        values.insert(path.machine.read(object + offset, width));
                    }
                    _ => known = false,
                }
            }
            match values.into_iter().collect::<Vec<_>>().as_slice() {
                [Some(value)] if known => machine.write(object + offset, width, *value),
                _ => machine.forget(object + offset, width),
            }
        }

        if fresh {
            machine.label(SCOPE | object, TRACKED);
            for (offset, width) in self.slots() {
                machine.protect(object + offset, width);
            }
        }
    }

    /// The registers that the call at `call` to `target` can read its arguments from: the
    /// argument registers that its signature uses, and `x8`, which holds the address of a
    /// returned object, unless the target ignores it.
    fn passed(&self, target: Option<u64>, call: u64) -> impl Iterator<Item = usize> + use<> {
        let count = target
            .and_then(|target| self.arguments.get(&target))
            .or_else(|| self.call_arguments.get(&call))
            .copied()
            .unwrap_or(8);
        let result_address = target.is_none_or(|target| !self.ignores_x8.contains(&target));

        (0..count.min(8)).chain(result_address.then_some(8))
    }

    /// A call that the pass does not follow may change any object that has escaped and the other
    /// memory that it reaches, and it changes a string object that it receives as its object.
    fn unfollowed(&self, machine: &mut Machine<'_>, target: Option<u64>) {
        if let Some(object) = machine.register(0) {
            machine.unlabel(NAME | object);
        }

        let escaping: BTreeSet<u64> = machine
            .labels()
            .range(SCOPE..NAME)
            .filter(|(_, state)| **state == ESCAPED)
            .map(|(key, _)| key & !SCOPE)
            .chain(self.arguments(machine, target))
            .chain(machine.unknown_stores().iter().copied())
            .collect();
        self.forget_reached(machine, target, escaping);
    }

    /// The values in the registers that the call at the present instruction reads.
    fn arguments(&self, machine: &Machine<'_>, target: Option<u64>) -> BTreeSet<u64> {
        self.passed(target, machine.pc())
            .filter_map(|index| machine.register(index))
            .collect()
    }

    /// Make unknown each known byte that the call at the present instruction reaches, except a
    /// register save and the slots of a tracked object that has not escaped. First the tracked
    /// objects in `escaping` escape, with each one whose address is in that memory outside a link
    /// and each one that an escaped object links.
    fn forget_reached(
        &self,
        machine: &mut Machine<'_>,
        target: Option<u64>,
        escaping: BTreeSet<u64>,
    ) {
        let tracked: BTreeMap<u64, u64> = machine
            .labels()
            .range(SCOPE..NAME)
            .map(|(key, value)| (key & !SCOPE, *value))
            .collect();
        let links: BTreeSet<u64> = tracked
            .keys()
            .flat_map(|object| self.link_offsets().map(move |offset| object + offset))
            .collect();
        let arguments = self.arguments(machine, target);
        let words = pointer_words(machine);
        let reachable = expose_reachable_stack(machine, &words, &arguments);
        let stored = words
            .iter()
            .filter(|(address, _)| {
                !links.contains(address) && reaches(machine, &reachable, *address)
            })
            .map(|(_, value)| *value);

        let mut escaped: BTreeSet<u64> = escaping
            .into_iter()
            .chain(stored)
            .filter(|object| tracked.contains_key(object))
            .collect();
        loop {
            let linked: BTreeSet<u64> = escaped
                .iter()
                .flat_map(|object| self.link_offsets().map(move |offset| object + offset))
                .filter_map(|slot| machine.read(slot, 8))
                .filter(|target| tracked.contains_key(target) && !escaped.contains(target))
                .collect();
            if linked.is_empty() {
                break;
            }
            escaped.extend(linked);
        }

        for object in escaped {
            machine.label(SCOPE | object, ESCAPED);
            self.forget_slots(machine, object);
        }

        let kept: Vec<Range<u64>> = machine
            .labels()
            .range(SCOPE..NAME)
            .filter(|(_, state)| **state == TRACKED)
            .flat_map(|(key, _)| {
                let object = key & !SCOPE;
                self.slots()
                    .map(move |(offset, width)| object + offset..object + offset + width)
            })
            .collect();
        let reached: Vec<u64> = machine
            .known_bytes(0, u64::MAX)
            .into_keys()
            .filter(|address| {
                reaches(machine, &reachable, *address)
                    && !machine.is_register_save(address & !7)
                    && !kept.iter().any(|range| range.contains(address))
            })
            .collect();
        for address in reached {
            machine.forget(address, 1);
        }
    }

    fn forget_slots(&self, machine: &mut Machine<'_>, object: u64) {
        for (offset, width) in self.slots() {
            machine.release(object + offset);
            machine.forget(object + offset, width);
        }
    }

    fn untrack(&self, machine: &mut Machine<'_>, object: u64) {
        if machine.labelled(SCOPE | object).is_some() {
            self.forget_slots(machine, object);
            machine.unlabel(SCOPE | object);
        }
    }

    /// The type slot and the three link slots.
    fn slots(&self) -> impl Iterator<Item = (u64, u64)> + use<> {
        let layout = self.layout;
        [
            (layout.scope_type_offset, 8),
            (layout.scope_root_offset, 8),
            (layout.scope_from_offset, 8),
            (layout.scope_prev_offset, 8),
        ]
        .into_iter()
    }

    fn link_offsets(&self) -> impl Iterator<Item = u64> + use<> {
        let layout = self.layout;
        [
            layout.scope_root_offset,
            layout.scope_from_offset,
            layout.scope_prev_offset,
        ]
        .into_iter()
    }

    /// The context of the scope at `scope` when the path arrives at the site.
    fn read(&self, machine: &Machine<'_>, scope: Option<u64>) -> Context {
        let Some(scope) = scope.filter(|scope| self.is_readable(machine, *scope)) else {
            return Context {
                this: Slot::Unresolved,
                root: Slot::Unresolved,
                from: vec![Slot::Unresolved],
                prev: vec![Slot::Unresolved],
            };
        };

        let layout = self.layout;
        let this = self.scope_type(machine, scope);
        let root = match machine.read(scope + layout.scope_root_offset, 8) {
            None => Slot::Unresolved,
            Some(linked) if linked == scope => Slot::SelfLink,
            Some(linked) if self.is_readable(machine, linked) => self.scope_type(machine, linked),
            Some(_) => Slot::Unresolved,
        };
        // `fromfrom` requires `from` to have a type (`IsFromFromSet`), but `prevprev` follows the
        // links without testing the scope between them.
        let from = self.chain(machine, scope, layout.scope_from_offset, |slot| {
            matches!(slot, Slot::Scope(_))
        });
        let prev = self.chain(machine, scope, layout.scope_prev_offset, |slot| {
            matches!(slot, Slot::Scope(_) | Slot::NotSet)
        });

        Context {
            this,
            root,
            from,
            prev,
        }
    }

    /// The slots along the chain of links at `link_offset` from `scope`. The chain ends after the
    /// first slot that `continues` rejects; a link back into the chain is unresolved.
    fn chain(
        &self,
        machine: &Machine<'_>,
        scope: u64,
        link_offset: u64,
        continues: fn(Slot) -> bool,
    ) -> Vec<Slot> {
        let mut slots = Vec::new();
        let mut visited = vec![scope];
        let mut holder = scope;
        loop {
            let link = machine.read(holder + link_offset, 8);
            let slot = match link {
                None => Slot::Unresolved,
                Some(linked) if linked == holder => Slot::SelfLink,
                Some(linked) if visited.contains(&linked) => Slot::Unresolved,
                Some(linked) if self.is_readable(machine, linked) => {
                    self.scope_type(machine, linked)
                }
                Some(_) => Slot::Unresolved,
            };
            slots.push(slot);
            if !continues(slot) {
                break;
            }

            holder = link.expect("a scope link");
            visited.push(holder);
        }

        slots
    }

    /// Only a tracked object that has not escaped has slots that the pass can read.
    fn is_readable(&self, machine: &Machine<'_>, object: u64) -> bool {
        machine.labelled(SCOPE | object) == Some(TRACKED)
    }

    fn scope_type(&self, machine: &Machine<'_>, object: u64) -> Slot {
        match machine.read(object + self.layout.scope_type_offset, 8) {
            None => Slot::Unresolved,
            Some(0) => Slot::NotSet,
            Some(value) if value.is_power_of_two() => Slot::Scope(value.trailing_zeros()),
            Some(_) => Slot::Unresolved,
        }
    }
}

/// The known 8-byte words that may hold a pointer that a call follows: every known word except a
/// register save.
fn pointer_words(machine: &Machine<'_>) -> Vec<(u64, u64)> {
    machine
        .known_words()
        .into_iter()
        .filter(|(address, _)| !machine.is_register_save(*address))
        .collect()
}

/// Whether a call reaches `address`: it is outside the stack or in a reachable stack range.
fn reaches(machine: &Machine<'_>, reachable: &[Range<u64>], address: u64) -> bool {
    !machine.is_stack(address) || reachable.iter().any(|range| range.contains(&address))
}

/// Record on the path, as exposed to later calls, and return the stack ranges that a call
/// receiving `arguments` reaches: from each live stack address up to the top of its frame, where
/// the address is one that the call receives, or one that is stored outside the stack, through an
/// unknown address or in a range that the call reaches; and each range that an earlier call on
/// the path could reach, from the stack pointer on. A stack address below the stack pointer is in
/// memory that a returned frame or a restored stack pointer has released.
fn expose_reachable_stack(
    machine: &mut Machine<'_>,
    words: &[(u64, u64)],
    arguments: &BTreeSet<u64>,
) -> Vec<Range<u64>> {
    let stack_pointer = machine.stack_pointer();
    let live = |value: &u64| machine.is_stack(*value) && *value >= stack_pointer;
    let frame = |start: u64| start..machine.frame_top(start);
    let exposed: Vec<(u64, u64)> = machine
        .labels()
        .range(EXPOSED..EXPOSED + (1 << 56))
        .map(|(key, end)| (key & !EXPOSED, *end))
        .collect();
    let outside = words
        .iter()
        .filter(|(address, _)| !machine.is_stack(*address))
        .map(|(_, value)| *value);
    let mut pending: Vec<Range<u64>> = arguments
        .iter()
        .copied()
        .chain(outside)
        .chain(machine.unknown_stores().iter().copied())
        .filter(live)
        .map(frame)
        .chain(
            exposed
                .iter()
                .filter(|(_, end)| *end > stack_pointer)
                .map(|(start, end)| *start.max(&stack_pointer)..*end),
        )
        .collect();

    let mut reachable = BTreeMap::<u64, u64>::new();
    while let Some(range) = pending.pop() {
        if reachable
            .get(&range.start)
            .is_some_and(|end| *end >= range.end)
        {
            continue;
        }
        reachable.insert(range.start, range.end);
        pending.extend(
            words
                .iter()
                .filter(|(address, value)| range.contains(address) && live(value))
                .map(|(_, value)| frame(*value)),
        );
    }

    for (start, _) in exposed {
        machine.unlabel(EXPOSED | start);
    }
    for (start, end) in &reachable {
        machine.label(EXPOSED | start, *end);
    }
    reachable
        .into_iter()
        .map(|(start, end)| start..end)
        .collect()
}
