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
//! address is an argument of a call that the pass does not follow, is stored anywhere else, or is
//! linked from an escaped object. From that call on, every call that the pass does not follow
//! makes the slots of every escaped object unknown. Until it escapes, a store to an unknown
//! address leaves its slots known. The pass assumes that a callee that receives a pointer to
//! another member of the object does not write its type or links, and that no stack array
//! indexed by an unknown value reaches a scope object.
use std::collections::{BTreeMap, BTreeSet};

use super::{CallbackLayout, Context, Slot};
use crate::engine::analysis::decode::Instruction;
use crate::engine::analysis::evaluate::{Call, Code, Exit, Machine, ReadOnlyData, Unresolved};

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

/// How many `from` links the pass reads.
const FROM_DEPTH: usize = 4;

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

/// The inputs that every run of the pass shares.
pub(super) struct Runner<'a> {
    pub scope_code: &'a [Instruction],
    pub scopes: &'a ScopeFunctions,
    pub strings: &'a StringFunctions,
    pub lookups: &'a BTreeSet<u64>,
    pub data: &'a ReadOnlyData,
    pub layout: CallbackLayout,
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
                Err(Unresolved(reason)) => result.unresolved = Some(reason),
            }
        }
        result
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
            self.unfollowed(machine);
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
            if let (Some(object), Some(literal)) = (object, machine.register(1)) {
                machine.label(NAME | object, literal);
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
            self.unfollowed(machine);
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
    /// where every path agrees.
    fn follow(&self, target: u64, machine: &mut Machine<'_>, depth: usize) {
        let Some(object) = machine.register(0) else {
            self.unfollowed(machine);
            return;
        };

        let copy = machine.clone();
        let paths = copy.run_paths(target, &mut |callee, inner| {
            let followed = callee.is_some_and(|callee| {
                self.scopes.fresh_constructors.contains(&callee)
                    || self.scopes.setters.contains(&callee)
            });
            if followed && depth < CALL_DEPTH {
                self.follow(callee.expect("followed callee"), inner, depth + 1);
            }
            Ok(Call::Return(None))
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

    /// A call that the pass does not follow may change any object that has escaped, and it
    /// changes a string object that it receives as its object.
    fn unfollowed(&self, machine: &mut Machine<'_>) {
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
        let arguments: BTreeSet<u64> = (0..=8)
            .filter_map(|index| machine.register(index))
            .collect();
        let stored: BTreeSet<u64> = machine
            .known_words()
            .into_iter()
            .filter(|(address, _)| !links.contains(address))
            .map(|(_, value)| value)
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

        let mut from = Vec::new();
        let mut chain = vec![scope];
        let mut holder = scope;
        loop {
            if from.len() == FROM_DEPTH {
                from.push(Slot::Unresolved);
                break;
            }
            let link = machine.read(holder + layout.scope_from_offset, 8);
            let slot = match link {
                None => Slot::Unresolved,
                Some(linked) if linked == holder => Slot::SelfLink,
                Some(linked) if chain.contains(&linked) => Slot::Unresolved,
                Some(linked) if self.is_readable(machine, linked) => {
                    self.scope_type(machine, linked)
                }
                Some(_) => Slot::Unresolved,
            };
            let ends = !matches!(slot, Slot::Scope(_));
            from.push(slot);
            if ends {
                break;
            }
            holder = link.expect("a scope link");
            chain.push(holder);
        }

        Context { this, root, from }
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
