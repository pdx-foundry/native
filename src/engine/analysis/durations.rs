//! Duration keys: sibling keys that set one count and the factor each key applies.
//!
//! A key is a duration unit only by mechanism. Its reader stores an integer or a scoped numeric
//! operand. After reading, the key either scales the count (`ScaledAtRead`) or stores a constant
//! word that the execute body multiplies by the operand (`SharedFactor`). An integer read through
//! a stack temporary joins at its final owner store, after following the temporary's scale.
//! Keys sharing a reader and final destination form a group when at least one applies a factor.
//! A key name, token or command never selects a result.
//!
//! The continuation follows straight-line code and local unconditional branches through the
//! register-restoring epilogue. Prefix word resets are harmless only when the reader overwrites
//! the same destination. Byte stores of zero or one do not establish word factors; they remain
//! recorded so a write overlapping a sibling's count or factor cannot mean preservation.
//! Other owner stores, calls or unfamiliar code leave a typed gap.
use std::collections::{BTreeMap, BTreeSet};

use super::declarations::number;
use super::decode::{Instruction, decode_arm64};
use super::fields::{ReaderJoin, RootField, TokenPath, Value};
use super::references::shapes::{Bindings, Shape, canonical, split_operands};
use super::stop::Unresolved;

/// The most instructions followed after one reader call.
const CONTINUATION_LIMIT: usize = 64;

/// Executable-bound inputs for duration combination.
#[cfg_attr(test, derive(Default))]
pub struct Input {
    /// Execute slot of an effect, or evaluate slot of a trigger, relative to its vtable point.
    pub execute_slot: u64,
    /// Demangled names by address and pointer slot, for the canonical execute body.
    pub names: BTreeMap<u64, String>,
    /// Span enclosing the proved selection fields and literal, by numeric subtype point.
    pub(crate) scoped_storage: BTreeMap<u64, Result<u64, Unresolved>>,
}

/// Sibling keys of one reader that set one duration count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// Each key and the factor it applies.
    pub units: Vec<Unit>,
    /// Owner offset of the count, or of the scoped operand for a shared factor.
    pub destination: i64,
    /// How keys combine; `SharedFactor` requires the matched execute body.
    pub combination: Result<Combination, Unresolved>,
    /// Factory-agreed initial bytes of the owner, for the omitted count.
    pub initial: BTreeMap<u64, u8>,
}

/// One duration key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// The child key.
    pub key: String,
    /// `Some(k)`: the key applies factor `k`. `None`: the key keeps the current shared factor.
    pub factor: Result<Option<i64>, Unresolved>,
}

/// How later keys combine with earlier ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combination {
    /// Each key scales its value in the count slot; the last key read replaces the count.
    ScaledAtRead,
    /// Keys assign one scoped operand; factor keys replace one factor slot. The execute body
    /// passes the 32-bit product of operand and factor.
    SharedFactor {
        /// Owner offset of the shared factor.
        factor_slot: i64,
        /// The factor before any key is read.
        initial_factor: i64,
    },
}

/// What one key's path does to owner storage after its reader call.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyEffect {
    /// In-place factor applied to the reader destination.
    scale: Option<i64>,
    /// Constants stored to other owner slots, by offset.
    constants: BTreeMap<i64, i64>,
    /// Byte writes that must not overlap a sibling's count or factor word.
    bytes: BTreeSet<i64>,
}

/// A key's name and what its paths do after reading.
type JoinedKey = (String, i64, Result<KeyEffect, Unresolved>);

/// A key's name and effect after reading.
type KeyResult = (String, Result<KeyEffect, Unresolved>);

/// A linear value of the continuation: a constant, or a 32-bit owner slot times a factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Term {
    Constant(i64),
    Scaled { slot: Destination, factor: i64 },
    Owner(i64),
    Stack(i64),
}

/// Storage written by the integer reader, before any transfer to the owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    Owner(i64),
    Stack(i64),
}

/// The execute-body proof that joins an operand and its factor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    /// Operand and factor offsets captured from the body.
    pub bindings: Bindings,
}

/// The function that holds an address: its start and code.
pub type CodeAt<'a> = dyn Fn(u64) -> Option<(u64, &'a [u8])> + 'a;

/// The duration groups among one reader's keys, and why other keys could not be classified.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inventory {
    /// Each group, with its unresolved parts.
    pub groups: Vec<Group>,
    /// One stop for each candidate without an established factor whose key code could not be
    /// followed: such a candidate may still be a duration group.
    pub unresolved: Vec<Unresolved>,
}

/// Every duration group among one reader's keys.
///
/// `execute` is the owner's execute body, when the owner is a command; nested readers have none.
pub fn groups(
    fields: &[RootField],
    paths: &[TokenPath],
    code: &CodeAt<'_>,
    initial: &BTreeMap<u64, u8>,
    execute: Option<Result<Execution, Unresolved>>,
    scoped_storage: &BTreeMap<i64, Result<u64, Unresolved>>,
) -> Inventory {
    let mut candidates: BTreeMap<(String, i64), Vec<KeyResult>> = BTreeMap::new();
    let mut inventory = Inventory::default();

    for field in fields {
        let Some(candidate) = key(field, paths, code) else {
            continue;
        };
        let (callee, destination, effect) = match candidate {
            Ok(candidate) => candidate,
            Err(stop) => {
                inventory.unresolved.push(stop);
                continue;
            }
        };

        candidates
            .entry((callee, destination))
            .or_default()
            .push((field.name.clone(), effect));
    }

    for ((_, destination), keys) in candidates {
        if keys.iter().any(|(_, effect)| applies_factor(effect)) {
            inventory.groups.push(group(
                destination,
                keys,
                initial,
                execute.as_ref(),
                scoped_storage.get(&destination),
            ));
        } else if let Some(stop) = keys.iter().find_map(|(_, effect)| effect.clone().err()) {
            inventory.unresolved.push(stop);
        }
    }

    inventory
}

/// Whether a reader can store a duration count: an integer or a scoped numeric operand. A key of
/// any other reader is never a duration unit, whatever its code does after reading.
fn counts(callee: &str) -> bool {
    matches!(
        super::readers::classify_callee(callee),
        crate::ReaderKind::Integer | crate::ReaderKind::ScopedNumeric
    )
}

/// Whether a key's effect establishes a factor.
fn applies_factor(effect: &Result<KeyEffect, Unresolved>) -> bool {
    effect.as_ref().is_ok_and(|effect| {
        effect.scale.is_some_and(|factor| factor != 1) || !effect.constants.is_empty()
    })
}

/// The key's single reader, destination and agreed effect, when every path joins one reader.
fn key(
    field: &RootField,
    paths: &[TokenPath],
    code: &CodeAt<'_>,
) -> Option<Result<JoinedKey, Unresolved>> {
    let mut joined = None;
    let mut effects = Vec::new();

    for (&index, join) in field.paths.iter().zip(&field.readers) {
        let ReaderJoin::Joined {
            callee, arguments, ..
        } = join
        else {
            return None;
        };

        if !counts(callee) {
            return None;
        }

        let source = super::readers::destination(join)
            .map(Destination::Owner)
            .or_else(|| match arguments.get("x1") {
                Some(Value::Stack(offset))
                    if super::readers::classify_callee(callee) == crate::ReaderKind::Integer
                        && super::readers::scalar_width(callee) == Some(4) =>
                {
                    Some(Destination::Stack(*offset))
                }
                _ => None,
            })?;
        let effect = key_effect(&paths[index], join, arguments, source, code);
        let (destination, effect) = match (source, effect) {
            (_, Ok((destination, effect))) => (destination, Ok(effect)),
            (Destination::Owner(destination), Err(stop)) => (destination, Err(stop)),
            (Destination::Stack(_), Err(stop)) => return Some(Err(stop)),
        };

        match &joined {
            None => joined = Some((callee.clone(), destination)),
            Some((known, at)) if known == callee && *at == destination => {}
            Some(_) => return Some(Err(Unresolved::new("duration-key-alternatives"))),
        }

        effects.push(effect);
    }

    let (callee, destination) = joined?;
    Some(Ok((callee, destination, agreed_effect(&effects))))
}

/// Alternatives must agree on the count and word factors. Their byte writes are a conservative
/// union: a conditional presence write may affect a sibling word on any one path.
fn agreed_effect(effects: &[Result<KeyEffect, Unresolved>]) -> Result<KeyEffect, Unresolved> {
    let Some((first, rest)) = effects.split_first() else {
        return Err(Unresolved::new("duration-key-alternatives"));
    };
    let first = match first {
        Ok(effect) => effect,
        Err(stop) if rest.iter().all(|effect| effect == &Err(stop.clone())) => {
            return Err(stop.clone());
        }
        Err(_) => return Err(Unresolved::new("duration-key-alternatives")),
    };
    let mut agreed = first.clone();
    for effect in rest {
        let Ok(effect) = effect else {
            return Err(Unresolved::new("duration-key-alternatives"));
        };
        if effect.scale != agreed.scale || effect.constants != agreed.constants {
            return Err(Unresolved::new("duration-key-alternatives"));
        }
        agreed.bytes.extend(&effect.bytes);
    }

    Ok(agreed)
}

/// The stores on one path, including overwritten prefix resets and the reader continuation.
fn key_effect(
    path: &TokenPath,
    join: &ReaderJoin,
    arguments: &BTreeMap<String, Value>,
    destination: Destination,
    code: &CodeAt<'_>,
) -> Result<(i64, KeyEffect), Unresolved> {
    let ReaderJoin::Joined { callee, tail, .. } = join else {
        return Err(Unresolved::new("duration-key"));
    };

    let overwrites_word = super::readers::classify_callee(callee) == crate::ReaderKind::Integer
        && super::readers::scalar_width(callee) == Some(4);
    let prefix_bytes = prefix_stores(path, destination, overwrites_word, code)?;

    if *tail {
        let Destination::Owner(destination) = destination else {
            return Err(Unresolved::new("duration-stack-store"));
        };

        return Ok((
            destination,
            KeyEffect {
                scale: None,
                constants: BTreeMap::new(),
                bytes: prefix_bytes,
            },
        ));
    }

    let (destination, mut effect) = continuation(path.terminal, arguments, destination, code)?;
    effect.bytes.extend(prefix_bytes);

    Ok((destination, effect))
}

/// Byte writes before the call; only word resets overwritten by the reader are discarded.
fn prefix_stores(
    path: &TokenPath,
    destination: Destination,
    overwrites_word: bool,
    code: &CodeAt<'_>,
) -> Result<BTreeSet<i64>, Unresolved> {
    let mut bytes = BTreeSet::new();
    let mut registers =
        BTreeMap::from([("x0".into(), Term::Owner(0)), ("sp".into(), Term::Stack(0))]);

    for &address in path.instructions.iter().filter(|&&at| at != path.terminal) {
        let row = instruction(address, code)?;
        let operands = split_operands(&row.operands);

        if row.operation.starts_with("st") && !stores_to_frame(&operands) {
            let [from, memory] = operands.as_slice() else {
                return Err(Unresolved::new("duration-prefix-store"));
            };
            let slot = owner_slot(&registers, memory);
            let constant = matches!(operand(&registers, from), Some(Term::Constant(_)));
            let overwritten = overwrites_word
                && slot.is_some_and(|slot| destination == Destination::Owner(slot))
                && matches!(row.operation.as_str(), "str" | "stur")
                && from.starts_with('w');
            let byte = matches!(row.operation.as_str(), "strb" | "sturb")
                && matches!(operand(&registers, from), Some(Term::Constant(0 | 1)));

            if byte
                && let Some(slot) = slot
                && !(overwrites_word
                    && matches!(destination, Destination::Owner(at) if (at..at + 4).contains(&slot)))
            {
                bytes.insert(slot);
            }

            if !constant || slot.is_none() || !(overwritten || byte) {
                return Err(Unresolved::new("duration-prefix-store"));
            }
        }

        match (row.operation.as_str(), operands.as_slice()) {
            ("mov", [to, from]) => {
                let value = operand(&registers, from);
                set(&mut registers, to, value);
            }
            ("add" | "sub", [to, from, amount]) => {
                let amount = number(amount).map(|amount| {
                    if row.operation == "sub" {
                        -(amount as i64)
                    } else {
                        amount as i64
                    }
                });
                let value = match (operand(&registers, from), amount) {
                    (Some(Term::Owner(offset)), Some(amount)) => Some(Term::Owner(offset + amount)),
                    (Some(Term::Stack(offset)), Some(amount)) => Some(Term::Stack(offset + amount)),
                    _ => None,
                };
                set(&mut registers, to, value);
            }
            ("bl" | "blr", _) => {
                registers.retain(|name, _| callee_preserves(name));
            }
            ("ldp", [first, second, ..]) => {
                set(&mut registers, first, None);
                set(&mut registers, second, None);
                if let Some(base) = written_back_base(&operands) {
                    set(&mut registers, base, None);
                }
            }
            (operation, [to, ..]) if writes_register_only(operation, to) => {
                set(&mut registers, to, None);
                if let Some(base) = written_back_base(&operands) {
                    set(&mut registers, base, None);
                }
            }
            _ => {}
        }
    }

    Ok(bytes)
}

fn instruction(address: u64, code: &CodeAt<'_>) -> Result<Instruction, Unresolved> {
    let (start, bytes) = code(address).ok_or(Unresolved::new("duration-code"))?;
    let offset = (address - start) as usize;
    let word = bytes
        .get(offset..offset + 4)
        .ok_or(Unresolved::new("duration-code"))?;

    decode_arm64(word, address)
        .ok()
        .and_then(|rows| rows.into_iter().next())
        .ok_or(Unresolved::new("duration-code"))
}

/// Follow the code after a non-tail reader call at `call` to `ret`.
fn continuation(
    call: u64,
    arguments: &BTreeMap<String, Value>,
    destination: Destination,
    code: &CodeAt<'_>,
) -> Result<(i64, KeyEffect), Unresolved> {
    let stop = |reason| Unresolved::new(reason);
    let (start, bytes) = code(call).ok_or(stop("duration-code"))?;
    let end = start + bytes.len() as u64;
    let mut registers = preserved(arguments);
    let mut stores: BTreeMap<i64, Term> = BTreeMap::new();
    let mut byte_stores = BTreeSet::new();
    let mut address = call + 4;

    for _ in 0..CONTINUATION_LIMIT {
        let row = instruction(address, code)?;
        let operands = split_operands(&row.operands);
        address += 4;

        match (row.operation.as_str(), operands.as_slice()) {
            ("ret", []) => return effect(destination, stores, byte_stores),
            ("b", [target]) => {
                let target = number(target).ok_or(stop("duration-post-read"))?;

                if !(start..end).contains(&target) {
                    return Err(stop("duration-post-read"));
                }

                address = target;
            }
            ("mov", [to, from]) => {
                let value = operand(&registers, from);
                set(&mut registers, to, value);
            }
            ("ldr" | "ldur", [to, memory]) if to.starts_with('w') => {
                let value =
                    address_slot(&registers, memory).map(|slot| Term::Scaled { slot, factor: 1 });
                set(&mut registers, to, value);
            }
            ("lsl", [to, from, amount]) => {
                let shift = number(amount).ok_or(stop("duration-post-read"))?;
                let value = scale(operand(&registers, from), 1 << shift);
                set(&mut registers, to, value);
            }
            ("mul", [to, left, right]) => {
                let value = product(operand(&registers, left), operand(&registers, right));
                set(&mut registers, to, value);
            }
            ("add" | "sub", [to, left, right, shift @ ..]) if !is_stack(to) => {
                let factor = shifted(shift).ok_or(stop("duration-post-read"))?;
                let right = scale(operand(&registers, right), factor);
                let right = if row.operation == "sub" {
                    scale(right, -1)
                } else {
                    right
                };
                let value = sum(operand(&registers, left), right);
                set(&mut registers, to, value);
            }
            ("add", [to, from, amount]) if is_stack(to) && is_stack(from) => {
                let amount = number(amount).ok_or(stop("duration-post-read"))? as i64;
                let value = match operand(&registers, from) {
                    Some(Term::Stack(offset)) => Some(Term::Stack(offset + amount)),
                    _ => None,
                };
                set(&mut registers, to, value);
            }
            (operation, _) if operation.starts_with("st") && stores_to_frame(&operands) => {
                if matches!(destination, Destination::Stack(_)) {
                    return Err(stop("duration-stack-overwrite"));
                }
            }
            ("strb" | "sturb", [from, memory]) => {
                let slot =
                    owner_slot(&registers, memory).ok_or(stop("duration-post-read-store"))?;
                if !matches!(operand(&registers, from), Some(Term::Constant(0 | 1)))
                    || matches!(destination, Destination::Owner(at) if (at..at + 4).contains(&slot))
                    || stores.keys().any(|&at| (at..at + 4).contains(&slot))
                {
                    return Err(stop("duration-post-read-store"));
                }
                byte_stores.insert(slot);
            }
            ("str" | "stur", [from, memory]) => {
                let slot =
                    owner_slot(&registers, memory).ok_or(stop("duration-post-read-store"))?;
                let value = from
                    .starts_with('w')
                    .then(|| operand(&registers, from))
                    .flatten()
                    .ok_or(stop("duration-post-read"))?;
                stores.retain(|&known, _| known + 4 <= slot || known >= slot + 4);
                byte_stores.retain(|byte| !(slot..slot + 4).contains(byte));
                stores.insert(slot, value);
            }
            ("ldp", [first, second, memory, ..])
                if memory_base(memory).is_some_and(|base| base == "sp") =>
            {
                registers.remove(&index(first));
                registers.remove(&index(second));
            }
            (operation, [to, ..]) if writes_register_only(operation, to) => {
                set(&mut registers, to, None);
            }
            _ => return Err(stop("duration-post-read")),
        }
        if let Some(base) = written_back_base(&operands) {
            set(&mut registers, base, None);
        }
    }

    Err(Unresolved::new("duration-post-read-limit"))
}

/// Whether a store addresses the stack or frame, which holds no owner storage.
fn stores_to_frame(operands: &[&str]) -> bool {
    operands
        .iter()
        .find(|operand| operand.starts_with('['))
        .and_then(|memory| memory_base(memory))
        .is_some_and(|base| is_stack(base) || base == "x29")
}

/// An instruction that only writes its first register operand: not a store, a branch, a call or
/// a return, which could change owner storage or leave the straight-line code.
fn writes_register_only(operation: &str, destination: &str) -> bool {
    let control = matches!(operation, "b" | "bl" | "blr" | "br" | "ret")
        || ["b.", "bl", "br", "cb", "tb", "ret"]
            .iter()
            .any(|prefix| operation.starts_with(prefix));
    let register = destination
        .strip_prefix(['w', 'x'])
        .is_some_and(|number| number.parse::<u8>().is_ok());

    !control && !operation.starts_with("st") && register
}

/// The base register of a pre- or post-indexed access, which the access also writes.
fn written_back_base<'a>(operands: &[&'a str]) -> Option<&'a str> {
    let memory = operands.iter().find(|operand| operand.starts_with('['))?;
    let written_back = memory.ends_with('!') || operands.last() != Some(memory);

    written_back.then(|| memory_base(memory)).flatten()
}

/// Whether a call preserves this register's value.
fn callee_preserves(name: &str) -> bool {
    name == "sp"
        || name == "x29"
        || name
            .strip_prefix('x')
            .and_then(|index| index.parse::<u8>().ok())
            .is_some_and(|index| (19..=28).contains(&index))
}

/// The owner and stack addresses in preserved registers; a call clobbers the others.
fn preserved(arguments: &BTreeMap<String, Value>) -> BTreeMap<String, Term> {
    arguments
        .iter()
        .filter_map(|(name, value)| {
            let term = match value {
                Value::Owner(offset) => Term::Owner(*offset),
                Value::Stack(offset) => Term::Stack(*offset),
                _ => return None,
            };
            callee_preserves(name).then(|| (name.clone(), term))
        })
        .collect()
}

/// The final owner slot and effects, including a stack count transferred to one owner slot.
fn effect(
    source: Destination,
    stores: BTreeMap<i64, Term>,
    bytes: BTreeSet<i64>,
) -> Result<(i64, KeyEffect), Unresolved> {
    let destination = match source {
        Destination::Owner(destination) => destination,
        Destination::Stack(_) => {
            let destinations: Vec<_> = stores
                .iter()
                .filter_map(|(&at, term)| {
                    matches!(term, Term::Scaled { slot, .. } if *slot == source).then_some(at)
                })
                .collect();
            let [destination] = destinations.as_slice() else {
                return Err(Unresolved::new("duration-stack-store"));
            };
            *destination
        }
    };
    let mut effect = KeyEffect {
        scale: None,
        constants: BTreeMap::new(),
        bytes,
    };

    for (slot, term) in stores {
        match term {
            Term::Scaled { slot: read, factor } if slot == destination && read == source => {
                effect.scale = Some(factor);
            }
            Term::Constant(value) if slot != destination => {
                effect.constants.insert(slot, value);
            }
            _ => return Err(Unresolved::new("duration-post-read-store")),
        }
    }

    Ok((destination, effect))
}

/// One group from its keys' effects, the factory state and the execute body.
fn group(
    destination: i64,
    keys: Vec<KeyResult>,
    initial: &BTreeMap<u64, u8>,
    execute: Option<&Result<Execution, Unresolved>>,
    scoped_storage: Option<&Result<u64, Unresolved>>,
) -> Group {
    let scaled = keys
        .iter()
        .any(|(_, effect)| effect.as_ref().is_ok_and(|effect| effect.scale.is_some()));
    let slots: BTreeSet<i64> = keys
        .iter()
        .filter_map(|(_, effect)| effect.as_ref().ok())
        .flat_map(|effect| effect.constants.keys().copied())
        .collect();
    let units = keys
        .iter()
        .map(|(key, effect)| Unit {
            key: key.clone(),
            factor: effect.clone().and_then(|effect| {
                byte_disjointness(destination, scaled, &slots, &effect.bytes, scoped_storage)?;

                unit_factor(&effect, scaled)
            }),
        })
        .collect::<Vec<_>>();
    let unresolved_key = units.iter().find_map(|unit| unit.factor.clone().err());

    let combination = match (scaled, slots.len(), unresolved_key) {
        (_, _, Some(stop)) => Err(stop),
        (true, 0, None) => Ok(Combination::ScaledAtRead),
        (false, 1, None) => {
            let factor = *slots.first().unwrap();
            shared_factor(destination, factor, initial, execute)
        }
        _ => Err(Unresolved::new("duration-mixed-keys")),
    };

    Group {
        units,
        destination,
        combination,
        initial: initial.clone(),
    }
}

/// Byte writes must miss both factor words and the count's proved storage.
fn byte_disjointness(
    destination: i64,
    scaled: bool,
    factors: &BTreeSet<i64>,
    bytes: &BTreeSet<i64>,
    scoped_storage: Option<&Result<u64, Unresolved>>,
) -> Result<(), Unresolved> {
    if bytes.is_empty() {
        return Ok(());
    }
    if bytes.iter().any(|byte| {
        (destination..destination + 4).contains(byte)
            || factors.iter().any(|slot| (*slot..*slot + 4).contains(byte))
    }) {
        return Err(Unresolved::new("duration-byte-factor"));
    }
    if scaled {
        return Ok(());
    }
    let end = scoped_storage
        .and_then(|span| span.as_ref().ok())
        .and_then(|span| i64::try_from(*span).ok())
        .and_then(|span| destination.checked_add(span))
        .ok_or(Unresolved::new("duration-byte-storage"))?;
    if bytes.iter().any(|byte| (destination..end).contains(byte)) {
        return Err(Unresolved::new("duration-byte-factor"));
    }

    Ok(())
}

/// A conservative span enclosing all proved selection fields and the numeric literal.
pub(crate) fn scoped_storage(
    facts: &super::scoped_numeric::Facts,
    numeric: &super::numeric::NumericFacts,
) -> BTreeMap<u64, Result<u64, Unresolved>> {
    use super::scoped_numeric::Subtype;

    facts
        .subtypes
        .iter()
        .map(|(&point, subtype)| {
            let span = (|| {
                let Ok(Subtype::Numeric {
                    token_reader,
                    literal,
                }) = subtype
                else {
                    return Err(Unresolved::new("duration-byte-storage"));
                };
                let layout = facts
                    .shared
                    .selection
                    .as_ref()
                    .map_err(|_| Unresolved::new("duration-byte-storage"))?;
                let conversion = numeric
                    .token_readers
                    .get(token_reader)
                    .and_then(|reader| match &reader.conversion {
                        crate::GrammarProperty::Known(Some(conversion))
                        | crate::GrammarProperty::Partial(Some(conversion)) => Some(conversion),
                        _ => None,
                    })
                    .ok_or(Unresolved::new("duration-byte-storage"))?;
                let crate::GrammarProperty::Known(width) = conversion.width_bits else {
                    return Err(Unresolved::new("duration-byte-storage"));
                };
                if !matches!(width, 32 | 64) || *literal != layout.literal {
                    return Err(Unresolved::new("duration-byte-storage"));
                }
                let width = u64::from(width / 8);
                [
                    (0, 8),
                    (*literal, width),
                    (layout.location, 24),
                    (layout.variable, 24),
                    (layout.trigger, 8),
                    (layout.script_value, 8),
                    (layout.modifier, 8),
                ]
                .into_iter()
                .map(|(offset, width)| {
                    offset
                        .checked_add(width)
                        .ok_or(Unresolved::new("duration-byte-storage"))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .max()
                .ok_or(Unresolved::new("duration-byte-storage"))
            })();
            (point, span)
        })
        .collect()
}

/// The factor one key applies: a scale, one shared-factor constant, or none.
fn unit_factor(effect: &KeyEffect, scaled: bool) -> Result<Option<i64>, Unresolved> {
    match (effect.scale, effect.constants.len()) {
        (Some(factor), 0) => Ok(Some(factor)),
        (None, 0) if scaled => Ok(Some(1)),
        (None, 0) => Ok(None),
        (None, 1) => Ok(effect.constants.values().next().copied()),
        _ => Err(Unresolved::new("duration-mixed-keys")),
    }
}

/// A shared factor is established only when the execute body multiplies this operand by this slot.
fn shared_factor(
    operand: i64,
    factor: i64,
    initial: &BTreeMap<u64, u8>,
    execute: Option<&Result<Execution, Unresolved>>,
) -> Result<Combination, Unresolved> {
    let execute = execute
        .ok_or(Unresolved::new("duration-execute-body"))?
        .as_ref()
        .map_err(Clone::clone)?;
    let joined = offset(&execute.bindings, "operand") == Some(operand)
        && offset(&execute.bindings, "factor") == Some(factor);

    if !joined {
        return Err(Unresolved::new("duration-execute-join"));
    }

    word(initial, factor)
        .map(|initial_factor| Combination::SharedFactor {
            factor_slot: factor,
            initial_factor,
        })
        .ok_or(Unresolved::new("duration-initial-state"))
}

/// The signed 32-bit word at `offset` in the factory state.
pub fn word(initial: &BTreeMap<u64, u8>, offset: i64) -> Option<i64> {
    let offset = u64::try_from(offset).ok()?;
    let mut bytes = [0; 4];

    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = *initial.get(&(offset + index as u64))?;
    }

    Some(i32::from_le_bytes(bytes).into())
}

/// Match the execute body that passes the operand times the factor to the flag setter.
pub fn execute(
    rows: &[Instruction],
    names: &BTreeMap<u64, String>,
) -> Result<Execution, Unresolved> {
    let lines = canonical(rows, names);
    for shape in [
        include_str!("durations/shapes/execute.txt"),
        include_str!("durations/shapes/relation_execute.txt"),
        include_str!("durations/shapes/trait_execute.txt"),
    ] {
        if let Some(bindings) = Shape::parse(shape).matches(&lines) {
            return Ok(Execution { bindings });
        }
    }

    Err(Unresolved::new("duration-execute-body"))
}

fn offset(bindings: &Bindings, name: &str) -> Option<i64> {
    bindings
        .get(name)
        .and_then(|text| number(text))
        .map(|value| value as i64)
}

fn memory_base(operand: &str) -> Option<&str> {
    let interior = operand.strip_prefix('[')?;
    let end = interior.find([',', ']'])?;

    Some(&interior[..end])
}

fn is_stack(register: &str) -> bool {
    register == "sp"
}

/// The register family: `w8` and `x8` are one register.
fn index(register: &str) -> String {
    match register.strip_prefix('w') {
        Some(number) => format!("x{number}"),
        None => register.to_string(),
    }
}

fn operand(registers: &BTreeMap<String, Term>, text: &str) -> Option<Term> {
    if text.starts_with('#') {
        return number(text).map(|value| Term::Constant(value as i64));
    }

    if text == "wzr" || text == "xzr" {
        return Some(Term::Constant(0));
    }

    registers.get(&index(text)).copied()
}

fn set(registers: &mut BTreeMap<String, Term>, register: &str, value: Option<Term>) {
    let register = index(register);

    match value {
        Some(value) => registers.insert(register, value),
        None => registers.remove(&register),
    };
}

/// The owner slot that a `[base,#offset]` operand addresses.
fn owner_slot(registers: &BTreeMap<String, Term>, memory: &str) -> Option<i64> {
    let Destination::Owner(offset) = address_slot(registers, memory)? else {
        return None;
    };

    Some(offset)
}

/// The owner or stack slot addressed by a non-indexed memory operand.
fn address_slot(registers: &BTreeMap<String, Term>, memory: &str) -> Option<Destination> {
    let interior = memory.strip_prefix('[')?.strip_suffix(']')?;
    let (base, amount) = interior.split_once(',').unwrap_or((interior, "#0"));
    let offset = number(amount)? as i64;
    match registers.get(&index(base))? {
        Term::Owner(owner) => Some(Destination::Owner(owner + offset)),
        Term::Stack(stack) => Some(Destination::Stack(stack + offset)),
        _ => None,
    }
}

/// The factor of an optional `lsl#n` shift operand.
fn shifted(shift: &[&str]) -> Option<i64> {
    match shift {
        [] => Some(1),
        [amount] => Some(1 << number(amount.strip_prefix("lsl")?)?),
        _ => None,
    }
}

fn scale(term: Option<Term>, factor: i64) -> Option<Term> {
    match term? {
        Term::Constant(value) => Some(Term::Constant(value * factor)),
        Term::Scaled { slot, factor: base } => Some(Term::Scaled {
            slot,
            factor: base * factor,
        }),
        Term::Owner(_) | Term::Stack(_) => None,
    }
}

fn product(left: Option<Term>, right: Option<Term>) -> Option<Term> {
    match (left?, right?) {
        (Term::Constant(factor), term) | (term, Term::Constant(factor)) => {
            scale(Some(term), factor)
        }
        _ => None,
    }
}

fn sum(left: Option<Term>, right: Option<Term>) -> Option<Term> {
    match (left?, right?) {
        (Term::Constant(left), Term::Constant(right)) => Some(Term::Constant(left + right)),
        (
            Term::Scaled { slot, factor: left },
            Term::Scaled {
                slot: other,
                factor: right,
            },
        ) if slot == other => Some(Term::Scaled {
            slot,
            factor: left + right,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
