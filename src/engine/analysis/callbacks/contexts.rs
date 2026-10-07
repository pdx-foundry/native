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
//! signature is known. It reaches all memory outside the stack, and the stack from each stack
//! address that it receives, or that is stored in what it reaches, up to the top of the frame
//! that holds that address, except the register saves of each function's prologue
//! ([`Machine::is_register_save`]). From that call on, every call that
//! the pass does not follow makes the slots of every escaped object unknown. Until it escapes, a
//! store to an unknown address leaves its slots known. The pass assumes that a callee that
//! receives a pointer to another member of the object does not write its type or links, and that
//! no stack array indexed by an unknown value reaches a scope object.
use std::collections::{BTreeMap, BTreeSet};

use super::{CallbackLayout, Context, Slot};
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use crate::engine::analysis::stop::Unresolved;

use super::names::StringFunctions;

/// Label kinds, in the top byte of a label key. The low bytes hold an object address.
const SCOPE: u64 = 1 << 56;
const NAME: u64 = 2 << 56;
const LIST: u64 = 3 << 56;

/// Values of a scope label.
const TRACKED: u64 = 1;
const ESCAPED: u64 = 2;

/// How deep the pass follows scope functions that call each other.
const CALL_DEPTH: usize = 3;

/// How many `from` or `prev` links the pass reads.
const CHAIN_DEPTH: usize = 4;

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
    /// `const` members, which write nothing.
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
    /// bound of the search.
    pub unresolved: BTreeSet<&'static str>,
    /// The bounds of the search that some path reached, such as the path limit.
    pub bounded: BTreeSet<&'static str>,
}

impl Evaluations {
    fn record(&mut self, unresolved: &Unresolved) {
        if unresolved.is_bound() {
            self.bounded.insert(unresolved.reason);
        } else {
            self.unresolved.insert(unresolved.reason);
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
                    result.unresolved.insert("left-the-site");
                    continue;
                }
                Err(unresolved) => {
                    result.record(&unresolved);
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
                    result.record(&unresolved);
                }
            }
        }

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
    /// not follow, or that reaches the call depth, makes the slots unknown.
    fn follow(&self, target: u64, machine: &mut Machine<'_>, depth: usize) {
        let Some(object) = machine.register(0) else {
            self.unfollowed(machine, Some(target));
            return;
        };

        let copy = machine.without_entered_calls();
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
                _ => Ok(Call::Return(None)),
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
    /// returned object.
    fn passed(&self, target: Option<u64>, call: u64) -> impl Iterator<Item = usize> + use<> {
        let count = target
            .and_then(|target| self.arguments.get(&target))
            .or_else(|| self.call_arguments.get(&call))
            .copied()
            .unwrap_or(8);
        (0..count.min(8)).chain([8])
    }

    /// A call that the pass does not follow may change any object that has escaped, and it
    /// changes a string object that it receives as its object.
    fn unfollowed(&self, machine: &mut Machine<'_>, target: Option<u64>) {
        if let Some(object) = machine.register(0) {
            machine.unlabel(NAME | object);
        }

        let tracked: BTreeMap<u64, u64> = machine
            .labels()
            .range(SCOPE..NAME)
            .map(|(key, value)| (key & !SCOPE, *value))
            .collect();
        let links: BTreeSet<u64> = tracked
            .keys()
            .flat_map(|object| self.link_offsets().map(move |offset| object + offset))
            .collect();
        let arguments: BTreeSet<u64> = self
            .passed(target, machine.pc())
            .filter_map(|index| machine.register(index))
            .collect();
        let words: Vec<(u64, u64)> = machine
            .known_words()
            .into_iter()
            .filter(|(address, _)| !machine.is_register_save(*address))
            .collect();
        let reachable = reachable_stack(machine, &words, &arguments);
        let stored: BTreeSet<u64> = words
            .iter()
            .filter(|(address, _)| {
                !links.contains(address)
                    && (!machine.is_stack(*address)
                        || reachable.iter().any(|range| range.contains(address)))
            })
            .map(|(_, value)| *value)
            .chain(machine.unknown_stores().iter().copied())
            .collect();

        let mut escaped: BTreeSet<u64> = tracked
            .iter()
            .filter(|(object, state)| {
                **state == ESCAPED || arguments.contains(object) || stored.contains(object)
            })
            .map(|(object, _)| *object)
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
    /// first slot that `continues` rejects, and after [`CHAIN_DEPTH`] links; a link back into the
    /// chain is unresolved.
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
            if slots.len() == CHAIN_DEPTH {
                slots.push(Slot::Unresolved);
                break;
            }

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

/// The stack ranges that a call receiving `arguments` reaches: from each stack address that it
/// receives, or that is stored in a range that it reaches, up to the top of that address's frame.
fn reachable_stack(
    machine: &Machine<'_>,
    words: &[(u64, u64)],
    arguments: &BTreeSet<u64>,
) -> Vec<std::ops::Range<u64>> {
    let mut starts: BTreeSet<u64> = arguments
        .iter()
        .copied()
        .filter(|value| machine.is_stack(*value))
        .collect();
    let mut pending: Vec<u64> = starts.iter().copied().collect();
    while let Some(start) = pending.pop() {
        let range = start..machine.frame_top(start);
        let pointed = words
            .iter()
            .filter(|(address, value)| range.contains(address) && machine.is_stack(*value))
            .map(|(_, value)| *value);
        for value in pointed.collect::<Vec<_>>() {
            if starts.insert(value) {
                pending.push(value);
            }
        }
    }

    starts
        .into_iter()
        .map(|start| start..machine.frame_top(start))
        .collect()
}
