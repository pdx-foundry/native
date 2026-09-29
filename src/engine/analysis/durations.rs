//! Duration keys: sibling keys that set one count, the factor each key applies, and the flag-store
//! countdown that consumes a shared-factor count.
//!
//! A key is a duration unit only by mechanism. After its reader returns, the key either scales the
//! value it read in place (`ScaledAtRead`) or stores a constant to one other owner slot, which the
//! command's execute body multiplies by the operand (`SharedFactor`). Keys whose joins share one
//! reader and destination form a group when at least one of them applies a factor. A key name, a
//! token or a command never selects a result.
//!
//! The code after a reader call is followed straight-line through unconditional branches and the
//! register-restoring epilogue until `ret`. Every owner store on the key's whole path counts: a
//! store before a tail call is a reset, not preservation. Any other instruction leaves the key
//! unresolved.
use std::collections::BTreeMap;

use super::declarations::number;
use super::decode::{Instruction, decode_arm64};
use super::fields::{ReaderJoin, RootField, TokenPath, Value};
use super::references::shapes::{Bindings, Shape, canonical, split_operands};
use super::stop::Unresolved;

/// The most instructions followed after one reader call.
const CONTINUATION_LIMIT: usize = 64;

/// Executable-bound inputs for duration consumption.
pub struct Input {
    /// Execute slot of an effect, or evaluate slot of a trigger, relative to its vtable point.
    pub execute_slot: u64,
    /// Demangled names by address and pointer slot, for the canonical execute body.
    pub names: BTreeMap<u64, String>,
    /// The flag-store countdown proof, shared by every command.
    pub countdown: Result<Countdown, Unresolved>,
}

#[cfg(test)]
impl Default for Input {
    fn default() -> Self {
        Self {
            execute_slot: 0,
            names: BTreeMap::new(),
            countdown: Err(Unresolved::new("duration-flag-setter")),
        }
    }
}

/// `SetFlag` replaces an existing flag's date and count in mode 0; `UpdateFlags` skips negative
/// counts, decrements the others, and removes a flag whose decremented count is zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Countdown {
    /// Offset of the count array that both bodies use.
    pub counts: u64,
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
    /// What consumes the count.
    pub consumption: Result<Consumption, Unresolved>,
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

/// What consumes a duration count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consumption {
    /// The execute body passes the count to the flag setter, and the flag store counts it down.
    FlagCountdown,
}

/// What one key's path does to owner storage after its reader call.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyEffect {
    /// In-place factor applied to the reader destination.
    scale: Option<i64>,
    /// Constants stored to other owner slots, by offset.
    constants: BTreeMap<i64, i64>,
}

/// A key's name and what its paths do after reading.
type KeyResult = (String, Result<KeyEffect, Unresolved>);

/// A linear value of the continuation: a constant, or a 32-bit owner slot times a factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Term {
    Constant(i64),
    Scaled { slot: i64, factor: i64 },
    Owner(i64),
}

/// The function that holds an address: its start and code.
pub type CodeAt<'a> = dyn Fn(u64) -> Option<(u64, &'a [u8])> + 'a;

/// Every duration group among one reader's keys.
///
/// `execute` is the owner's execute body, when the owner is a command; nested readers have none.
pub fn groups(
    fields: &[RootField],
    paths: &[TokenPath],
    code: &CodeAt<'_>,
    initial: &BTreeMap<u64, u8>,
    execute: Option<Result<Bindings, Unresolved>>,
    countdown: &Result<Countdown, Unresolved>,
) -> Vec<Group> {
    let mut candidates: BTreeMap<(String, i64), Vec<KeyResult>> = BTreeMap::new();

    for field in fields {
        let Some((callee, destination, effect)) = key(field, paths, code) else {
            continue;
        };

        candidates
            .entry((callee, destination))
            .or_default()
            .push((field.name.clone(), effect));
    }

    candidates
        .into_iter()
        .filter(|(_, keys)| keys.iter().any(|(_, effect)| applies_factor(effect)))
        .map(|((_, destination), keys)| {
            group(destination, keys, initial, execute.as_ref(), countdown)
        })
        .collect()
}

/// Whether a key's effect establishes a factor.
fn applies_factor(effect: &Result<KeyEffect, Unresolved>) -> bool {
    effect
        .as_ref()
        .is_ok_and(|effect| effect.scale.is_some() || !effect.constants.is_empty())
}

/// The key's single reader, destination and agreed effect, when every path joins one reader.
fn key(
    field: &RootField,
    paths: &[TokenPath],
    code: &CodeAt<'_>,
) -> Option<(String, i64, Result<KeyEffect, Unresolved>)> {
    let mut joined = None;
    let mut effects = Vec::new();

    for (&index, join) in field.paths.iter().zip(&field.readers) {
        let ReaderJoin::Joined {
            callee, arguments, ..
        } = join
        else {
            return None;
        };
        let destination = super::readers::destination(join)?;

        match &joined {
            None => joined = Some((callee.clone(), destination)),
            Some((known, at)) if known == callee && *at == destination => {}
            Some(_) => return None,
        }

        effects.push(key_effect(
            &paths[index],
            join,
            arguments,
            destination,
            code,
        ));
    }

    let (callee, destination) = joined?;
    let effect = match effects.split_first() {
        Some((first, rest)) if rest.iter().all(|effect| effect == first) => first.clone(),
        Some(_) => Err(Unresolved::new("duration-key-alternatives")),
        None => return None,
    };

    Some((callee, destination, effect))
}

/// The owner stores on one path: none may precede the call, and the continuation is followed.
fn key_effect(
    path: &TokenPath,
    join: &ReaderJoin,
    arguments: &BTreeMap<String, Value>,
    destination: i64,
    code: &CodeAt<'_>,
) -> Result<KeyEffect, Unresolved> {
    let ReaderJoin::Joined { tail, .. } = join else {
        return Err(Unresolved::new("duration-key"));
    };

    if prefix_stores_owner(path, code)? {
        return Err(Unresolved::new("duration-prefix-store"));
    }

    if *tail {
        return Ok(KeyEffect {
            scale: None,
            constants: BTreeMap::new(),
        });
    }

    continuation(path.terminal, arguments, destination, code)
}

/// Whether an instruction before the call stores through a base other than the stack or frame.
fn prefix_stores_owner(path: &TokenPath, code: &CodeAt<'_>) -> Result<bool, Unresolved> {
    for &address in path.instructions.iter().filter(|&&at| at != path.terminal) {
        let row = instruction(address, code)?;
        let operands = split_operands(&row.operands);

        if row.operation.starts_with("st")
            && operands
                .iter()
                .find(|operand| operand.starts_with('['))
                .and_then(|address| memory_base(address))
                .is_some_and(|base| base != "sp" && base != "x29")
        {
            return Ok(true);
        }
    }

    Ok(false)
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
    destination: i64,
    code: &CodeAt<'_>,
) -> Result<KeyEffect, Unresolved> {
    let stop = |reason| Unresolved::new(reason);
    let (start, bytes) = code(call).ok_or(stop("duration-code"))?;
    let end = start + bytes.len() as u64;
    let mut registers = preserved(arguments);
    let mut stores: BTreeMap<i64, Term> = BTreeMap::new();
    let mut address = call + 4;

    for _ in 0..CONTINUATION_LIMIT {
        let row = instruction(address, code)?;
        let operands = split_operands(&row.operands);
        address += 4;

        match (row.operation.as_str(), operands.as_slice()) {
            ("ret", []) => return effect(destination, stores),
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
                    owner_slot(&registers, memory).map(|slot| Term::Scaled { slot, factor: 1 });
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
                number(amount).ok_or(stop("duration-post-read"))?;
            }
            ("str" | "stur", [from, memory]) => {
                let slot = owner_slot(&registers, memory).ok_or(stop("duration-post-read"))?;
                let value = from
                    .starts_with('w')
                    .then(|| operand(&registers, from))
                    .flatten()
                    .ok_or(stop("duration-post-read"))?;
                stores.retain(|&known, _| known + 4 <= slot || known >= slot + 4);
                stores.insert(slot, value);
            }
            ("ldp", [first, second, memory, ..])
                if memory_base(memory).is_some_and(|base| base == "sp") =>
            {
                registers.remove(&index(first));
                registers.remove(&index(second));
            }
            _ => return Err(stop("duration-post-read")),
        }
    }

    Err(Unresolved::new("duration-post-read-limit"))
}

/// The owner-derived callee-saved registers at the call; a call clobbers the others.
fn preserved(arguments: &BTreeMap<String, Value>) -> BTreeMap<String, Term> {
    arguments
        .iter()
        .filter_map(|(name, value)| {
            let index: u8 = name.strip_prefix('x')?.parse().ok()?;
            let Value::Owner(offset) = value else {
                return None;
            };

            (19..=28)
                .contains(&index)
                .then(|| (format!("x{index}"), Term::Owner(*offset)))
        })
        .collect()
}

/// The effect that the final owner stores establish.
fn effect(destination: i64, stores: BTreeMap<i64, Term>) -> Result<KeyEffect, Unresolved> {
    let mut effect = KeyEffect {
        scale: None,
        constants: BTreeMap::new(),
    };

    for (slot, term) in stores {
        match term {
            Term::Scaled { slot: read, factor } if slot == destination && read == destination => {
                effect.scale = Some(factor);
            }
            Term::Constant(value) if slot != destination => {
                effect.constants.insert(slot, value);
            }
            _ => return Err(Unresolved::new("duration-post-read-store")),
        }
    }

    Ok(effect)
}

/// One group from its keys' effects, the factory state and the execute body.
fn group(
    destination: i64,
    keys: Vec<KeyResult>,
    initial: &BTreeMap<u64, u8>,
    execute: Option<&Result<Bindings, Unresolved>>,
    countdown: &Result<Countdown, Unresolved>,
) -> Group {
    let scaled = keys
        .iter()
        .any(|(_, effect)| effect.as_ref().is_ok_and(|effect| effect.scale.is_some()));
    let slots: std::collections::BTreeSet<i64> = keys
        .iter()
        .filter_map(|(_, effect)| effect.as_ref().ok())
        .flat_map(|effect| effect.constants.keys().copied())
        .collect();
    let units = keys
        .iter()
        .map(|(key, effect)| Unit {
            key: key.clone(),
            factor: effect
                .clone()
                .and_then(|effect| unit_factor(&effect, scaled)),
        })
        .collect::<Vec<_>>();
    let unresolved_key = units.iter().find_map(|unit| unit.factor.clone().err());

    let (combination, consumption) = match (scaled, slots.len(), unresolved_key) {
        (_, _, Some(stop)) => (Err(stop.clone()), Err(stop)),
        (true, 0, None) => (
            Ok(Combination::ScaledAtRead),
            Err(Unresolved::new("duration-consumption")),
        ),
        (false, 1, None) => {
            let factor = *slots.first().unwrap();
            shared_factor(destination, factor, initial, execute, countdown)
        }
        _ => {
            let stop = Unresolved::new("duration-mixed-keys");
            (Err(stop.clone()), Err(stop))
        }
    };

    Group {
        units,
        destination,
        combination,
        consumption,
        initial: initial.clone(),
    }
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
    execute: Option<&Result<Bindings, Unresolved>>,
    countdown: &Result<Countdown, Unresolved>,
) -> (
    Result<Combination, Unresolved>,
    Result<Consumption, Unresolved>,
) {
    let execute = match execute {
        Some(Ok(bindings)) => bindings,
        Some(Err(stop)) => return (Err(stop.clone()), Err(stop.clone())),
        None => {
            let stop = Unresolved::new("duration-execute-body");
            return (Err(stop.clone()), Err(stop));
        }
    };
    let joined =
        offset(execute, "operand") == Some(operand) && offset(execute, "factor") == Some(factor);

    if !joined {
        let stop = Unresolved::new("duration-execute-join");
        return (Err(stop.clone()), Err(stop));
    }

    let combination = word(initial, factor)
        .map(|initial_factor| Combination::SharedFactor {
            factor_slot: factor,
            initial_factor,
        })
        .ok_or(Unresolved::new("duration-initial-state"));
    let consumption = countdown.clone().map(|_| Consumption::FlagCountdown);

    (combination, consumption)
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
) -> Result<Bindings, Unresolved> {
    Shape::parse(include_str!("durations/shapes/execute.txt"))
        .matches(&canonical(rows, names))
        .ok_or(Unresolved::new("duration-execute-body"))
}

/// Prove the flag-store countdown from the setter and the daily update bodies.
pub fn countdown(
    set_flag: &[Instruction],
    update_flags: &[Instruction],
    names: &BTreeMap<u64, String>,
) -> Result<Countdown, Unresolved> {
    let set = Shape::parse(include_str!("durations/shapes/set_flag.txt"))
        .matches(&canonical(set_flag, names))
        .ok_or(Unresolved::new("duration-flag-setter"))?;
    let update = Shape::parse(include_str!("durations/shapes/update_flags.txt"))
        .matches(&canonical(update_flags, names))
        .ok_or(Unresolved::new("duration-flag-update"))?;
    let agreed = ["flag_count", "flags", "dates", "counts"]
        .iter()
        .all(|name| set.contains_key(*name) && set.get(*name) == update.get(*name));
    let counts = offset(&set, "counts").filter(|_| agreed);

    counts
        .and_then(|counts| u64::try_from(counts).ok())
        .map(|counts| Countdown { counts })
        .ok_or(Unresolved::new("duration-flag-layout"))
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
    let interior = memory.strip_prefix('[')?.strip_suffix(']')?;
    let (base, amount) = interior.split_once(',').unwrap_or((interior, "#0"));
    let Term::Owner(owner) = registers.get(&index(base))? else {
        return None;
    };

    Some(owner + number(amount)? as i64)
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
        Term::Owner(_) => None,
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
