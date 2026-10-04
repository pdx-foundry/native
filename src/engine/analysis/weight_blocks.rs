//! Weight blocks: the keys, arithmetic operations and conditions that a shared mean-time member
//! reader accepts.
//!
//! Weight, `random_weight` and script-value fields share one engine reader. Its member reader
//! dispatches some keys before a compiled token-to-enum switch selects an arithmetic operation,
//! and it allocates an entry object for each nested block. The symbolic dispatch walker stops at
//! those calls, so this method runs the member reader concretely, along every path, for every
//! token value from zero to the largest literal token and for the first value after it, which
//! stands for every token created at run time (the token domain of [`super::scopes`]).
//!
//! A path is accepted only through one of four shapes: a shared reader joined with an owner
//! destination, an allocated entry inserted into an owner array, an operation stored after the
//! switch returned, or a delegate to a shared trigger or effect family. A path with none of them
//! is rejected when it wrote an engine diagnostic, and unresolved otherwise. Calls that receive
//! neither the owner nor the reader change no script state; a call that receives either and has
//! no recognized shape stops the path.
//!
//! An accepted path is an operation when it called the switch with its key token and a word of
//! the owner or of the inserted entry, written after the switch returned, holds the returned
//! value. This is a validated rule, not a data-flow proof: a word that holds the same value by
//! coincidence, most likely zero, would also satisfy it.
//!
//! The unnamed token values decide what other keys are: rejected, or trigger conditions in one
//! scope. A named key with that same disposition is one of the other keys.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    decode::decode_arm64,
    discovery::Symbol,
    evaluate::{Call, Code, Exit, Machine, Path, ReadOnlyData},
    fields::{
        ConcreteReader, Condition, PathOutcome, ReaderJoin, RootField, Token, TokenPath, Value,
    },
    readers,
    stop::Unresolved,
};
use crate::{ReaderKind, RepeatBehavior};

const SPAN: u64 = 0x10000;
/// Stands for the scope that a scoped member reader receives in `x3`.
const ENCLOSING: u64 = 0xe5c0_0000_0000_0001;
/// The deepest nested entry reader that the method follows.
const NESTING_LIMIT: usize = 2;
/// The most member delegates that one reader's code includes.
const DELEGATE_LIMIT: usize = 8;

/// Weight readers bound to the executable, with the call sets the method needs.
pub(crate) struct WeightBlockInput {
    /// Constructor-installed address points of weight readers, with their read and member names.
    pub points: BTreeMap<u64, ConcreteReader>,
    /// Constructors that the weight member readers call, by entry, with the vtable each installs
    /// at each offset.
    pub constructors: BTreeMap<u64, BTreeMap<u64, u64>>,
    pub operator_new: BTreeSet<u64>,
    pub symbols: Vec<Symbol>,
    /// Literal tokens by value.
    pub tokens: BTreeMap<i64, Token>,
    /// Member readers that delegate a key to a shared command family.
    pub families: BTreeMap<String, crate::BlockFamily>,
    pub pointers: BTreeMap<u64, u64>,
    /// Read-only data, with the target of every pointer slot.
    pub data: ReadOnlyData,
    /// Offset of the key token within the reader.
    pub reader_token_offset: u64,
    /// Offset of the assigned value token within the reader.
    pub value_token_offset: u64,
}

/// The grammar of each weight reader, by its address point.
#[derive(Default)]
pub(crate) struct WeightBlockFacts {
    pub points: BTreeMap<u64, Result<Grammar, Unresolved>>,
}

impl WeightBlockFacts {
    /// The owner-relative offsets of the stored scopes that each address point's grammar reads.
    /// A word that the read entry may write before the block is read is not a stored scope.
    pub(crate) fn stored_scopes(&self) -> BTreeMap<u64, BTreeSet<u64>> {
        self.points
            .iter()
            .filter_map(|(&point, grammar)| {
                let grammar = grammar.as_ref().ok()?;
                let written = &grammar.read_entry.as_ref().ok()?.writes;
                let offsets: BTreeSet<_> = grammar
                    .scopes()
                    .filter_map(|scope| match scope {
                        Value::Load(base, 8) => match **base {
                            Value::Owner(offset) => u64::try_from(offset).ok(),
                            _ => None,
                        },
                        _ => None,
                    })
                    .filter(|offset| !written.contains(&(offset & !7)))
                    .collect();
                (!offsets.is_empty()).then_some((point, offsets))
            })
            .collect()
    }
}

/// What one member reader accepts.
#[derive(Debug, Clone)]
pub(crate) struct Grammar {
    /// What the block's read entry does before it reads the block.
    pub read_entry: Result<ReadEntry, Unresolved>,
    /// Fixed keys, other than nested entries and operations.
    pub fields: Vec<RootField>,
    pub paths: Vec<TokenPath>,
    /// Keys that insert an entry object, with the entry's reader.
    pub nested: BTreeMap<String, Nested>,
    pub operations: Vec<Operation>,
    pub operation_repeat: RepeatBehavior,
    pub other_keys: OtherKeys,
    /// The scope argument of each key's read, operand or entry read.
    pub key_scopes: BTreeMap<String, Value>,
    /// Stops by key, or `None` for the block itself.
    pub stops: Vec<(Option<String>, Unresolved)>,
    /// Whether some named token stopped before the method knew if it is a fixed key or an
    /// operation, so neither list is complete.
    pub undetermined_keys: bool,
}

impl Grammar {
    fn scopes(&self) -> impl Iterator<Item = &Value> {
        let other = match &self.other_keys {
            OtherKeys::Triggers(Some(scope)) => Some(scope),
            _ => None,
        };
        self.key_scopes.values().chain(other).chain(
            self.operations
                .iter()
                .filter_map(|operation| operation.operand.as_ref()?.scope.as_ref()),
        )
    }
}

/// The bare value that a block's read entry accepts, and the owner words it writes first.
#[derive(Debug, Clone)]
pub(crate) struct ReadEntry {
    /// The reader of a bare value, or `None` when every path reads a block.
    pub scalar: Option<ReaderJoin>,
    /// Owner words, by 8-byte-aligned offset, that some path writes before it reads.
    pub writes: BTreeSet<u64>,
}

/// An entry object that a key allocates and inserts, read by its own member reader.
#[derive(Debug, Clone)]
pub(crate) struct Nested {
    pub reader: ConcreteReader,
    pub grammar: Result<Box<Grammar>, Unresolved>,
    /// The entry's known 8-byte words when it was inserted, by offset.
    pub words: BTreeMap<u64, u64>,
}

/// A key that selects an arithmetic operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Operation {
    pub key: String,
    pub operand: Option<Operand>,
}

/// The reader of an operation's value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Operand {
    pub callee: String,
    pub scope: Option<Value>,
    /// The address point of the value object that the reader fills, when the path knows it.
    pub point: Option<u64>,
    /// The owner offset of that value object, when it is in the owner.
    pub destination: Option<i64>,
}

/// What a key that is neither a fixed key nor an operation does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OtherKeys {
    Rejected,
    /// A trigger delegate, with its scope argument.
    Triggers(Option<Value>),
    Unresolved(Unresolved),
}

/// Analyze every weight reader in `input`. `bodies` gives the code of the function that starts
/// at an address.
pub(crate) fn analyze<'a>(
    input: &'a WeightBlockInput,
    bodies: &'a dyn Fn(u64) -> Option<&'a [u8]>,
) -> WeightBlockFacts {
    let context = Context::new(input, bodies);
    let mut grammars = Grammars::new();
    WeightBlockFacts {
        points: input
            .points
            .iter()
            .map(|(&point, reader)| {
                let grammar = context
                    .cached(&reader.member, 0, &mut grammars)
                    .map(|grammar| Grammar {
                        read_entry: context.read_entry(&reader.read),
                        ..*grammar
                    });
                (point, grammar)
            })
            .collect(),
    }
}

/// The grammar of each member reader at each nesting depth, which several readers share.
type Grammars = BTreeMap<(String, usize), Result<Box<Grammar>, Unresolved>>;

struct Context<'a> {
    input: &'a WeightBlockInput,
    bodies: &'a dyn Fn(u64) -> Option<&'a [u8]>,
    names: BTreeMap<u64, &'a str>,
    last_token: u64,
}

/// A member reader's code, with the delegates and enum switches it calls.
struct Member {
    entry: u64,
    code: Code,
    scoped: bool,
    enums: BTreeMap<u64, Code>,
}

impl<'a> Context<'a> {
    fn new(input: &'a WeightBlockInput, bodies: &'a dyn Fn(u64) -> Option<&'a [u8]>) -> Self {
        let names = input
            .symbols
            .iter()
            .map(|symbol| (symbol.address, symbol.name.as_str()))
            .collect();
        let last_token = input
            .tokens
            .keys()
            .copied()
            .filter(|token| *token >= 0)
            .max()
            .unwrap_or(0) as u64;
        Self {
            input,
            bodies,
            names,
            last_token,
        }
    }

    fn name(&self, address: u64) -> &'a str {
        self.names.get(&address).copied().unwrap_or("")
    }

    /// The one entry of the symbol `name`, excluding cold parts.
    fn entry(&self, name: &str) -> Result<u64, Unresolved> {
        let entries: BTreeSet<_> = self
            .input
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.address)
            .collect();
        match entries.len() {
            1 => Ok(*entries.first().unwrap()),
            _ => Err(Unresolved::new("weight-member-symbol")),
        }
    }

    fn body(&self, entry: u64) -> Result<&'a [u8], Unresolved> {
        (self.bodies)(entry).ok_or(Unresolved::new("weight-member-body"))
    }

    /// The member reader `name` with every member delegate and enum switch that it calls
    /// directly, transitively through the delegates.
    fn member(&self, name: &str) -> Result<Member, Unresolved> {
        let entry = self.entry(name)?;
        let mut bodies = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut pending = vec![entry];
        while let Some(address) = pending.pop() {
            if bodies.contains_key(&address) {
                continue;
            }
            if bodies.len() > DELEGATE_LIMIT {
                return Err(Unresolved::new("weight-delegate-limit"));
            }
            let body = self.body(address)?;
            let rows =
                decode_arm64(body, address).map_err(|_| Unresolved::new("weight-member-code"))?;
            for row in rows
                .iter()
                .filter(|row| matches!(row.operation.as_str(), "bl" | "b"))
            {
                let Some(target) = super::declarations::number(&row.operands) else {
                    continue;
                };
                let callee = self.name(target);
                if is_enum_switch(callee) && !enums.contains_key(&target) {
                    let rows = decode_arm64(self.body(target)?, target)
                        .map_err(|_| Unresolved::new("weight-enum-code"))?;
                    enums.insert(target, Code::from_rows(rows));
                } else if readers::is_member(callee) && !self.input.families.contains_key(callee) {
                    pending.push(target);
                }
            }
            bodies.insert(address, body);
        }
        let ranges: Vec<_> = bodies.into_iter().collect();
        Ok(Member {
            entry,
            code: Code::decode(&ranges).map_err(|_| Unresolved::new("weight-member-code"))?,
            scoped: name.ends_with(", EScopeType)"),
            enums,
        })
    }

    fn grammar(
        &self,
        name: &str,
        depth: usize,
        grammars: &mut Grammars,
    ) -> Result<Grammar, Unresolved> {
        let member = self.member(name)?;
        let mut other_keys = None;
        for token in self.unnamed_tokens() {
            let disposition = disposition(&self.evaluate(&member, token));
            other_keys = Some(match other_keys {
                None => disposition,
                Some(known) if known == disposition => known,
                Some(_) => OtherKeys::Unresolved(Unresolved::new("weight-other-keys")),
            });
        }
        let other_keys =
            other_keys.unwrap_or(OtherKeys::Unresolved(Unresolved::new("weight-other-keys")));
        let mut grammar = Grammar {
            read_entry: Err(Unresolved::new("weight-scalar")),
            fields: Vec::new(),
            paths: Vec::new(),
            nested: BTreeMap::new(),
            operations: Vec::new(),
            operation_repeat: RepeatBehavior::Unknown,
            other_keys,
            key_scopes: BTreeMap::new(),
            stops: Vec::new(),
            undetermined_keys: false,
        };
        if let OtherKeys::Unresolved(stop) = &grammar.other_keys {
            grammar.stops.push((None, stop.clone()));
        }
        let mut repeats = BTreeSet::new();
        for (&token, literal) in self.input.tokens.range(0..=self.last_token as i64) {
            let ends = self.evaluate(&member, token as u64);
            if disposition(&ends) == grammar.other_keys {
                continue;
            }
            if literal.ambiguous {
                grammar
                    .stops
                    .push((None, Unresolved::new("weight-token-name")));
                grammar.undetermined_keys = true;
                continue;
            }
            match key(&ends) {
                Key::Operation(operand, repeat) => {
                    repeats.insert(repeat);
                    if let Some(scope) = operand.as_ref().and_then(|operand| operand.scope.clone())
                    {
                        grammar.key_scopes.insert(literal.name.clone(), scope);
                    }
                    grammar.operations.push(Operation {
                        key: literal.name.clone(),
                        operand,
                    });
                }
                Key::Nested(child) => {
                    let entry = self.nested(&child, depth, grammars);
                    if let Some(scope) = &child.scope {
                        grammar
                            .key_scopes
                            .insert(literal.name.clone(), scope.clone());
                    }
                    self.push_field(&mut grammar, token as u64, literal, &ends);
                    grammar.nested.insert(literal.name.clone(), entry);
                }
                Key::Field => {
                    if let Some(scope) = ends.iter().find_map(|end| end.scope.clone()) {
                        grammar.key_scopes.insert(literal.name.clone(), scope);
                    }
                    self.push_field(&mut grammar, token as u64, literal, &ends);
                }
                Key::Unresolved(stop) => {
                    grammar.stops.push((Some(literal.name.clone()), stop));
                    grammar.undetermined_keys = true;
                }
            }
        }
        // A key that might be an operation could store differently from the known ones.
        grammar.operation_repeat = match (grammar.undetermined_keys, repeats.len(), repeats.first())
        {
            (false, 1, Some(Repeat::Accumulate)) => RepeatBehavior::Accumulate,
            (false, 1, Some(Repeat::Replace(_))) => RepeatBehavior::Replace,
            _ => RepeatBehavior::Unknown,
        };
        Ok(grammar)
    }

    fn unnamed_tokens(&self) -> impl Iterator<Item = u64> + '_ {
        (0..=self.last_token)
            .filter(|token| !self.input.tokens.contains_key(&(*token as i64)))
            .chain([self.last_token + 1])
    }

    fn cached(
        &self,
        member: &str,
        depth: usize,
        grammars: &mut Grammars,
    ) -> Result<Box<Grammar>, Unresolved> {
        let key = (member.to_owned(), depth);
        if let Some(known) = grammars.get(&key) {
            return known.clone();
        }
        let grammar = self.grammar(member, depth, grammars).map(Box::new);
        grammars.insert(key, grammar.clone());
        grammar
    }

    fn nested(&self, child: &Child, depth: usize, grammars: &mut Grammars) -> Nested {
        let reader = ConcreteReader {
            read: child.read.clone(),
            member: child.member.clone(),
            family: crate::BlockFamily::Weight,
        };
        let grammar = if depth + 1 >= NESTING_LIMIT {
            Err(Unresolved::new("weight-nesting-limit"))
        } else {
            self.cached(&child.member, depth + 1, grammars)
        };
        Nested {
            reader,
            grammar,
            words: child.words.clone(),
        }
    }

    /// Add the key `literal` with one path for each distinct outcome of its ends. Outcomes that
    /// every end shares hold unconditionally.
    fn push_field(&self, grammar: &mut Grammar, token: u64, literal: &Token, ends: &[End]) {
        let mut alternatives: Vec<(PathOutcome, &End)> = Vec::new();
        for end in ends {
            let outcome = end.outcome();
            if !alternatives.iter().any(|(known, _)| *known == outcome) {
                alternatives.push((outcome, end));
            }
        }

        let unconditional = alternatives.len() == 1;
        let mut indices = Vec::new();
        let mut readers = Vec::new();
        for (outcome, end) in alternatives {
            match &outcome {
                PathOutcome::Reader(join) => readers.push(join.clone()),
                PathOutcome::Gap(stop) => {
                    readers.push(ReaderJoin::Missing(stop.clone()));
                    grammar
                        .stops
                        .push((Some(literal.name.clone()), stop.clone()));
                }
                PathOutcome::Rejected => {}
            }

            indices.push(grammar.paths.len());
            grammar.paths.push(TokenPath {
                domain: [token as i64, token as i64],
                conditions: if unconditional {
                    Vec::new()
                } else {
                    end.conditions.clone()
                },
                instructions: Vec::new(),
                terminal: end.terminal,
                outcome,
            });
        }

        grammar.fields.push(RootField {
            name: literal.name.clone(),
            token: token as i64,
            constructor: literal.constructor,
            paths: indices,
            readers,
        });
    }

    /// What the read entry `name` does before it reads a block: the reader of a bare value it
    /// accepts instead, and the owner words it writes.
    fn read_entry(&self, name: &str) -> Result<ReadEntry, Unresolved> {
        let entry = self.entry(name)?;
        let rows = decode_arm64(self.body(entry)?, entry)
            .map_err(|_| Unresolved::new("weight-read-code"))?;
        let code = Code::from_rows(rows);
        let data = &self.input.data;
        let mut machine = Machine::new(&code, data);
        let owner = machine.reserve(SPAN);
        let reader = machine.reserve(SPAN);
        machine.set_register(0, owner);
        machine.set_register(1, reader);
        let mut scalar = None;
        let mut writes = BTreeSet::new();
        let mut block = false;
        for path in machine.run_paths(entry, &mut |target, machine| {
            let callee = self.name(target.ok_or(Unresolved::new("weight-read-call"))?);
            let arguments = Seeds {
                owner,
                reader,
                token: None,
            }
            .arguments(machine);
            let joined = readers::arguments_join(
                callee,
                &arguments,
                false,
                Some(self.input.value_token_offset as i64),
            );
            let block = callee == "CPersistent::Read(CReader&)"
                && arguments.get("x0") == Some(&Value::Owner(0))
                && arguments.get("x1") == Some(&Value::Reader(0));
            if block || (joined && is_reader(callee)) {
                return Ok(Call::Stop);
            }
            Err(Unresolved::new("weight-read-call"))
        }) {
            let Exit::Stopped(target) = path.end? else {
                return Err(Unresolved::new("weight-read-terminal"));
            };
            writes.extend(
                path.machine
                    .written_words(owner, owner + SPAN)
                    .into_iter()
                    .map(|word| (word - owner) & !7),
            );
            let callee = self.name(target);
            if callee == "CPersistent::Read(CReader&)" {
                block = true;
                continue;
            }
            let join = ReaderJoin::Joined {
                callee: callee.into(),
                arguments: Seeds {
                    owner,
                    reader,
                    token: None,
                }
                .arguments(&path.machine),
                tail: path.machine.is_tail_call(),
            };
            if scalar.as_ref().is_some_and(|known| *known != join) {
                return Err(Unresolved::new("weight-scalar"));
            }
            scalar = Some(join);
        }
        if !block {
            return Err(Unresolved::new("weight-read-block"));
        }
        Ok(ReadEntry { scalar, writes })
    }

    /// Every path of `member` for one token.
    fn evaluate(&self, member: &Member, token: u64) -> Vec<End> {
        let data = &self.input.data;
        let mut machine = Machine::new(&member.code, data);
        let owner = machine.reserve(SPAN);
        let reader = machine.reserve(SPAN);
        machine.write(
            reader + self.input.reader_token_offset,
            4,
            token & 0xffff_ffff,
        );
        machine.set_register(0, owner);
        machine.set_register(1, reader);
        machine.set_register(2, token);
        if member.scoped {
            machine.set_register(3, ENCLOSING);
        }
        machine.watch_reads(owner, SPAN);
        let seeds = Seeds {
            owner,
            reader,
            token: Some(token),
        };
        let paths = machine.run_paths(member.entry, &mut |target, machine| {
            self.call(member, seeds, target, machine)
        });
        paths
            .into_iter()
            .map(|path| self.end(seeds, path))
            .collect()
    }

    /// What one path of the member does at a call.
    fn call(
        &self,
        member: &Member,
        seeds: Seeds,
        target: Option<u64>,
        machine: &mut Machine<'_>,
    ) -> Result<Call, Unresolved> {
        let reaches_state = (0..8).any(|index| seeds.reaches(machine.register(index)));
        let Some(target) = target else {
            if reaches_state {
                return Err(Unresolved::new("weight-indirect-call"));
            }
            return Ok(Call::Return(None));
        };
        let callee = self.name(target);
        let arguments = seeds.arguments(machine);
        if EMISSIONS.contains(&callee) {
            machine.label(EMITTED, 1);
            return Ok(Call::Return(None));
        }
        if readers::is_member(callee) && readers::arguments_join(callee, &arguments, true, None) {
            if self.input.families.contains_key(callee) {
                return Ok(Call::Stop);
            }
            return Ok(Call::Enter);
        }
        let value_token = self.input.value_token_offset as i64;
        if is_reader(callee)
            && readers::arguments_join(callee, &arguments, false, Some(value_token))
        {
            return Ok(Call::Stop);
        }
        if callee == "CVariableValue::Read(CReader&, EScopeType)"
            && arguments.get("x1") == Some(&Value::Reader(0))
        {
            machine.label(OPERAND, target);
            machine.label(OPERAND_SCOPE, scope_label(seeds.scope(machine, 2).as_ref()));
            let destination = machine.known_register(0, "weight-operand-destination")?;
            if let Some(point) = machine.read(destination, 8) {
                machine.label(OPERAND_POINT, point);
            }
            return Ok(Call::Return(None));
        }
        if readers::conversion_kind(callee).is_some()
            && arguments.get("x0") == Some(&Value::Reader(value_token))
        {
            machine.label(CONVERSION, target);
            snapshot(machine, CONVERSION_WRITTEN, seeds.owner);
            return Ok(Call::Return(None));
        }
        if let Some(code) = member.enums.get(&target) {
            let value = self.enum_value(code, target, seeds, machine)?;
            machine.label(ENUM, value);
            snapshot(machine, ENUM_WRITTEN, seeds.owner);
            return Ok(Call::Return(Some(value)));
        }
        if self.input.operator_new.contains(&target) {
            let size = machine.known_register(0, "weight-allocation-size")?;
            if size == 0 || size > SPAN {
                return Err(Unresolved::new("weight-allocation-bound"));
            }
            let object = machine.reserve(size);
            machine.label(object, size);
            return Ok(Call::Return(Some(object)));
        }
        if let Some(vtables) = self.input.constructors.get(&target) {
            let receiver = machine.known_register(0, "weight-constructor-receiver")?;
            let end = allocation(machine, receiver).map_or_else(
                || receiver + vtables.keys().max().copied().unwrap_or(0) + 8,
                |(start, size)| start + size,
            );
            super::receivers::install_vtables(
                machine,
                receiver,
                end,
                vtables,
                super::stop::CauseKind::Invalidated,
            )
            .ok_or(Unresolved::new("weight-constructor-bound"))?;
            return Ok(Call::Return(None));
        }
        if is_insert(callee) && seeds.in_owner(machine.register(0)) {
            let array = machine.known_register(0, "weight-insert-array")?;
            let slot = machine.known_register(2, "weight-insert-value")?;
            let child = machine
                .read(slot, 8)
                .filter(|&child| allocation(machine, child).is_some())
                .ok_or(Unresolved::new("weight-insert-value"))?;
            machine.label(INSERT_ARRAY, array - seeds.owner);
            machine.label(INSERT_CHILD, child);
            machine.forget(array, 0x18);
            return Ok(Call::Return(None));
        }
        let receiver = machine.register(0);
        if machine.register(1) == Some(seeds.reader)
            && receiver.is_some_and(|receiver| allocation(machine, receiver).is_some())
        {
            if machine.labelled(CHILD_READ).is_some() {
                return Err(Unresolved::new("weight-child-read"));
            }
            machine.label(CHILD_READ, target);
            machine.label(CHILD_RECEIVER, receiver.unwrap());
            let scope = callee
                .ends_with(", EScopeType)")
                .then(|| seeds.scope(machine, 2))
                .flatten();
            machine.label(CHILD_SCOPE, scope_label(scope.as_ref()));
            return Ok(Call::Return(None));
        }
        if LOCATIONS.contains(&callee) {
            return Ok(Call::Return(None));
        }
        if reaches_state {
            return Err(Unresolved::new("weight-call"));
        }
        Ok(Call::Return(None))
    }

    /// The value that the enum switch `code` returns for the key token. The switch must receive
    /// the address of a word that holds the key token.
    fn enum_value(
        &self,
        code: &Code,
        entry: u64,
        seeds: Seeds,
        machine: &Machine<'_>,
    ) -> Result<u64, Unresolved> {
        let argument = machine.known_register(0, "weight-enum-argument")?;
        let token = seeds.token.ok_or(Unresolved::new("weight-enum-argument"))?;
        if machine.read(argument, 4) != Some(token & 0xffff_ffff) {
            return Err(Unresolved::new("weight-enum-argument"));
        }
        let mut lookup = Machine::new(code, &self.input.data);
        let word = lookup.allocate(4);
        lookup.write(word, 4, token & 0xffff_ffff);
        lookup.set_register(0, word);
        match lookup.run(entry, &mut |_, _| Err(Unresolved::new("weight-enum-call")))? {
            Exit::Returned => Ok(lookup.known_register(0, "weight-enum-value")? & 0xffff_ffff),
            _ => Err(Unresolved::new("weight-enum-terminal")),
        }
    }

    fn end(&self, seeds: Seeds, path: Path<'_>) -> End {
        let machine = &path.machine;
        let conditions = machine
            .decisions()
            .into_iter()
            .map(|decision| Condition {
                at: decision.instruction,
                value: None,
                zero: decision.side == 0,
            })
            .collect();
        let mut end = End {
            shape: None,
            operation: None,
            emitted: machine.labelled(EMITTED).is_some(),
            stop: None,
            scope: None,
            conditions,
            terminal: machine.pc(),
        };
        let exit = match path.end {
            Ok(exit) => exit,
            Err(stop) => {
                end.stop = Some(stop);
                return end;
            }
        };
        match self.shape(seeds, machine, exit) {
            Ok(shape) => end.shape = shape,
            Err(stop) => {
                end.stop = Some(stop);
                return end;
            }
        }
        end.scope = match &end.shape {
            Some(Shape::Read(ReaderJoin::Joined {
                callee, arguments, ..
            })) => readers::scope_argument(callee, arguments).cloned(),
            Some(Shape::Family { scope, .. }) => scope.clone(),
            Some(Shape::Inserted { child, .. }) => {
                child.as_ref().and_then(|child| child.scope.clone())
            }
            _ => None,
        };
        end.operation = self.operation(seeds, machine, &end.shape);
        end
    }

    fn shape(
        &self,
        seeds: Seeds,
        machine: &Machine<'_>,
        exit: Exit,
    ) -> Result<Option<Shape>, Unresolved> {
        if let Exit::Stopped(target) = exit {
            let callee = self.name(target);
            let arguments = seeds.arguments(machine);
            if let Some(&family) = self.input.families.get(callee) {
                return Ok(Some(Shape::Family {
                    family,
                    scope: readers::scope_argument(callee, &arguments).cloned(),
                }));
            }
            return Ok(Some(Shape::Read(ReaderJoin::Joined {
                callee: callee.into(),
                arguments,
                tail: machine.is_tail_call(),
            })));
        }
        if exit != Exit::Returned {
            return Err(Unresolved::new("weight-terminal"));
        }
        if let Some(child) = machine.labelled(INSERT_CHILD) {
            let array = machine.labelled(INSERT_ARRAY).unwrap_or(0) as i64;
            let read = machine
                .labelled(CHILD_READ)
                .filter(|_| machine.labelled(CHILD_RECEIVER).is_some());
            let child = match read {
                Some(read) => Some(self.child(machine, read, child)?),
                None => None,
            };
            return Ok(Some(Shape::Inserted { array, child }));
        }
        if let Some(conversion) = machine.labelled(CONVERSION) {
            let destination = written_since(machine, CONVERSION_WRITTEN, seeds.owner)
                .ok_or(Unresolved::new("weight-conversion-destination"))?;
            return Ok(Some(Shape::Converted {
                callee: self.name(conversion).into(),
                kind: readers::conversion_kind(self.name(conversion))
                    .unwrap_or(ReaderKind::Unknown),
                destination,
            }));
        }
        Ok(None)
    }

    /// The reader of an inserted entry: the read call it received, and the member reader in
    /// the vtable at the receiver's address point.
    fn child(&self, machine: &Machine<'_>, read: u64, object: u64) -> Result<Child, Unresolved> {
        let receiver = machine.labelled(CHILD_RECEIVER).unwrap_or(0);
        if allocation(machine, receiver).map(|(start, _)| start) != Some(object) {
            return Err(Unresolved::new("weight-child-receiver"));
        }
        let point = machine
            .read(receiver, 8)
            .ok_or(Unresolved::new("weight-child-vtable"))?;
        let members: BTreeSet<_> = (0..64)
            .map_while(|slot| self.input.pointers.get(&(point + slot * 8)))
            .map(|&target| thunk_target(self.name(target)))
            .filter(|name| readers::is_member(name))
            .collect();
        let [member] = <[_; 1]>::try_from(members.into_iter().collect::<Vec<_>>())
            .map_err(|_| Unresolved::new("weight-child-member"))?;
        let (_, size) =
            allocation(machine, object).ok_or(Unresolved::new("weight-child-receiver"))?;
        let words = (0..size / 8)
            .filter_map(|word| Some((word * 8, machine.read(object + word * 8, 8)?)))
            .filter(|(_, value)| *value != 0)
            .collect();
        Ok(Child {
            read: self.name(read).into(),
            member: member.into(),
            scope: scope_from_label(machine.labelled(CHILD_SCOPE).unwrap_or(NO_SCOPE)),
            words,
        })
    }

    /// The operation of an accepted path, with whether its entries accumulate or replace.
    fn operation(
        &self,
        seeds: Seeds,
        machine: &Machine<'_>,
        shape: &Option<Shape>,
    ) -> Option<(Option<Operand>, Repeat)> {
        let value = machine.labelled(ENUM)?;
        let operand = |join: Option<&ReaderJoin>| match join {
            Some(ReaderJoin::Joined {
                callee, arguments, ..
            }) if callee == "CVariableValue::Read(CReader&, EScopeType)" => Some(Operand {
                callee: callee.clone(),
                scope: readers::scope_argument(callee, arguments).cloned(),
                point: None,
                destination: readers::destination(join?),
            }),
            _ => machine.labelled(OPERAND).map(|target| Operand {
                callee: self.name(target).into(),
                scope: scope_from_label(machine.labelled(OPERAND_SCOPE).unwrap_or(NO_SCOPE)),
                point: machine.labelled(OPERAND_POINT),
                destination: None,
            }),
        };
        match shape {
            Some(Shape::Inserted { .. }) => {
                let child = machine.labelled(INSERT_CHILD)?;
                let (_, size) = allocation(machine, child)?;
                let holds =
                    (0..size / 4).any(|word| machine.read(child + word * 4, 4) == Some(value));
                holds.then(|| (operand(None), Repeat::Accumulate))
            }
            None | Some(Shape::Read(_)) => {
                let join = match shape {
                    Some(Shape::Read(join)) => Some(join),
                    _ => None,
                };
                let before = written(machine, ENUM_WRITTEN);
                let slot = machine
                    .written_words(seeds.owner, seeds.owner + SPAN)
                    .into_iter()
                    .filter(|word| !before.contains(word))
                    .find(|&word| machine.read(word, 4) == Some(value))?;
                Some((operand(join), Repeat::Replace(slot - seeds.owner)))
            }
            _ => None,
        }
    }
}

/// The registers that a weight path starts with.
#[derive(Clone, Copy)]
struct Seeds {
    owner: u64,
    reader: u64,
    token: Option<u64>,
}

impl Seeds {
    fn in_owner(&self, value: Option<u64>) -> bool {
        value.is_some_and(|value| (self.owner..self.owner + SPAN).contains(&value))
    }

    /// Whether `value` points into the owner or the reader.
    fn reaches(&self, value: Option<u64>) -> bool {
        self.in_owner(value)
            || value.is_some_and(|value| (self.reader..self.reader + SPAN).contains(&value))
    }

    /// The call arguments `x0` to `x3` that the shared reader table can test.
    fn arguments(&self, machine: &Machine<'_>) -> BTreeMap<String, Value> {
        (0..4)
            .filter_map(|index| {
                let value = match machine.register(index) {
                    Some(ENCLOSING) => Value::EnclosingScope,
                    Some(value) if (self.reader..self.reader + SPAN).contains(&value) => {
                        Value::Reader((value - self.reader) as i64)
                    }
                    Some(value) if self.in_owner(Some(value)) => {
                        Value::Owner((value - self.owner) as i64)
                    }
                    Some(value) if index == 2 && Some(value) == self.token => Value::Token,
                    Some(value) if machine.is_stack(value) => Value::Stack(0),
                    Some(value) => Value::Constant(value as i64),
                    None => owner_load(machine, index)?,
                };
                Some((format!("x{index}"), value))
            })
            .collect()
    }

    /// The scope argument in register `index`.
    fn scope(&self, machine: &Machine<'_>, index: usize) -> Option<Value> {
        match machine.register(index) {
            Some(ENCLOSING) => Some(Value::EnclosingScope),
            Some(value) => Some(Value::Constant(value as i64)),
            None => owner_load(machine, index),
        }
    }
}

/// An unknown register loaded whole from one word of the owner.
fn owner_load(machine: &Machine<'_>, index: usize) -> Option<Value> {
    let sources = machine.register_sources(index);
    let first = *sources.first()?;
    let whole = sources.len() == 8 && sources.iter().copied().eq(first..first + 8);
    whole.then(|| Value::Load(Box::new(Value::Owner(first as i64)), 8))
}

/// One path's end, in the terms the grammar needs.
#[derive(Debug, Clone)]
struct End {
    shape: Option<Shape>,
    operation: Option<(Option<Operand>, Repeat)>,
    emitted: bool,
    stop: Option<Unresolved>,
    scope: Option<Value>,
    conditions: Vec<Condition>,
    terminal: u64,
}

impl End {
    fn accepted(&self) -> bool {
        self.stop.is_none() && (self.shape.is_some() || self.operation.is_some())
    }

    fn outcome(&self) -> PathOutcome {
        if let Some(stop) = &self.stop {
            return PathOutcome::Gap(stop.clone());
        }
        match &self.shape {
            Some(Shape::Read(join)) => PathOutcome::Reader(join.clone()),
            Some(Shape::Converted {
                callee,
                kind,
                destination,
            }) => PathOutcome::Reader(ReaderJoin::Stored {
                callee: callee.clone(),
                kind: *kind,
                destination: *destination,
                repeat: RepeatBehavior::Replace,
            }),
            Some(Shape::Inserted {
                array,
                child: Some(child),
            }) => PathOutcome::Reader(ReaderJoin::Stored {
                callee: child.read.clone(),
                kind: ReaderKind::Block,
                destination: *array,
                repeat: RepeatBehavior::Accumulate,
            }),
            _ if self.emitted => PathOutcome::Rejected,
            _ => PathOutcome::Gap(Unresolved::new("weight-acceptance")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Shape {
    Read(ReaderJoin),
    Converted {
        callee: String,
        kind: ReaderKind,
        destination: i64,
    },
    Inserted {
        array: i64,
        child: Option<Child>,
    },
    Family {
        family: crate::BlockFamily,
        scope: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Child {
    read: String,
    member: String,
    scope: Option<Value>,
    words: BTreeMap<u64, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Repeat {
    Accumulate,
    /// One operation slot at this owner offset.
    Replace(u64),
}

/// How a token's paths classify it.
enum Key {
    Operation(Option<Operand>, Repeat),
    Nested(Child),
    Field,
    Unresolved(Unresolved),
}

/// The disposition of a token that names no fixed key or operation.
fn disposition(ends: &[End]) -> OtherKeys {
    if ends
        .iter()
        .all(|end| end.stop.is_none() && end.shape.is_none() && end.emitted)
    {
        return OtherKeys::Rejected;
    }
    let scopes: BTreeSet<_> = ends
        .iter()
        .map(|end| match (&end.stop, &end.shape, &end.operation) {
            (
                None,
                Some(Shape::Family {
                    family: crate::BlockFamily::Trigger,
                    scope,
                }),
                None,
            ) => Some(scope.clone()),
            _ => None,
        })
        .collect();
    match <[_; 1]>::try_from(scopes.into_iter().collect::<Vec<_>>()) {
        Ok([Some(scope)]) => OtherKeys::Triggers(scope),
        _ => OtherKeys::Unresolved(Unresolved::new("weight-key-disposition")),
    }
}

fn key(ends: &[End]) -> Key {
    let accepted: Vec<_> = ends.iter().filter(|end| end.accepted()).collect();
    if accepted.is_empty() {
        return Key::Unresolved(
            ends.iter()
                .find_map(|end| end.stop.clone())
                .unwrap_or(Unresolved::new("weight-acceptance")),
        );
    }
    let operations: BTreeSet<_> = accepted.iter().map(|end| end.operation.clone()).collect();
    if operations.iter().any(Option::is_some) {
        let complete = ends.iter().all(|end| end.stop.is_none());
        return match (
            complete,
            <[_; 1]>::try_from(operations.into_iter().collect::<Vec<_>>()),
        ) {
            (true, Ok([Some((operand, repeat))])) => Key::Operation(operand, repeat),
            _ => Key::Unresolved(Unresolved::new("weight-operation")),
        };
    }
    let children: BTreeSet<_> = accepted
        .iter()
        .map(|end| match &end.shape {
            Some(Shape::Inserted { child, .. }) => Some(child.clone()),
            _ => None,
        })
        .collect();
    match <[_; 1]>::try_from(children.into_iter().collect::<Vec<_>>()) {
        Ok([None]) => Key::Field,
        Ok([Some(Some(child))]) => Key::Nested(child),
        _ => Key::Unresolved(Unresolved::new("weight-entry")),
    }
}

/// Calls that write an engine diagnostic.
const EMISSIONS: &[&str] = &[
    "CPdxLogFileAndLine::operator()(char const*, ...)",
    "CLogger::Log(char const*, unsigned int, int)",
    "CReader::ReportMalformed()",
    "CReader::ReportUnexpected()",
];

/// Calls that locate a diagnostic in the reader and change no script state.
const LOCATIONS: &[&str] = &[
    "CReader::GetFileName() const",
    "CReader::GetLineNumber() const",
    "CReader::GetFileLocationDescription() const",
];

/// Labels that keep one path's facts.
const EMITTED: u64 = 1;
const ENUM: u64 = 2;
const OPERAND: u64 = 3;
const OPERAND_SCOPE: u64 = 4;
const CONVERSION: u64 = 5;
const INSERT_ARRAY: u64 = 6;
const INSERT_CHILD: u64 = 7;
const CHILD_READ: u64 = 8;
const CHILD_RECEIVER: u64 = 9;
const CHILD_SCOPE: u64 = 10;
const OPERAND_POINT: u64 = 11;
/// Tags of the owner words written before an event; the label key adds the word's address.
const ENUM_WRITTEN: u64 = 1 << 63;
const CONVERSION_WRITTEN: u64 = 1 << 62;

fn snapshot(machine: &mut Machine<'_>, tag: u64, owner: u64) {
    for word in machine.written_words(owner, owner + SPAN) {
        machine.label(tag | word, 1);
    }
}

fn written(machine: &Machine<'_>, tag: u64) -> BTreeSet<u64> {
    machine
        .labels()
        .keys()
        .filter(|key| *key & (ENUM_WRITTEN | CONVERSION_WRITTEN) == tag)
        .map(|key| key & !tag)
        .collect()
}

/// The owner offset of the one 8-byte store made after the snapshot `tag`.
fn written_since(machine: &Machine<'_>, tag: u64, owner: u64) -> Option<i64> {
    let before = written(machine, tag);
    let after: Vec<_> = machine
        .written_words(owner, owner + SPAN)
        .into_iter()
        .filter(|word| !before.contains(word))
        .collect();
    match after.as_slice() {
        [first] | [first, _] if after.last()? - first <= 4 => Some((first - owner) as i64),
        _ => None,
    }
}

/// The allocation that holds `address`, as its start and size.
fn allocation(machine: &Machine<'_>, address: u64) -> Option<(u64, u64)> {
    machine
        .labels()
        .range(..=address)
        .next_back()
        .filter(|&(&start, &size)| start > 0xffff && address < start + size)
        .map(|(&start, &size)| (start, size))
}

/// Machine labels hold integers, so a scope argument kept on a path is a tag in the low byte with
/// an owner offset above it. Only the received scope and a word loaded from the owner are kept;
/// `NO_SCOPE` stands for any other argument.
const NO_SCOPE: u64 = 0;
const RECEIVED_SCOPE: u64 = 1;
const STORED_SCOPE: u64 = 2;

fn scope_label(scope: Option<&Value>) -> u64 {
    match scope {
        Some(Value::EnclosingScope) => RECEIVED_SCOPE,
        Some(Value::Load(base, 8)) => match **base {
            Value::Owner(offset) if offset >= 0 => STORED_SCOPE | (offset as u64) << 8,
            _ => NO_SCOPE,
        },
        _ => NO_SCOPE,
    }
}

fn scope_from_label(label: u64) -> Option<Value> {
    match label & 0xff {
        RECEIVED_SCOPE => Some(Value::EnclosingScope),
        STORED_SCOPE => Some(Value::Load(Box::new(Value::Owner((label >> 8) as i64)), 8)),
        _ => None,
    }
}

fn is_reader(callee: &str) -> bool {
    readers::classify_callee(callee) != ReaderKind::Unknown
        || super::references::reader(callee).is_some()
}

fn is_enum_switch(callee: &str) -> bool {
    callee.contains(" TokenToEnum<") && callee.ends_with("(int const&)")
}

fn is_insert(callee: &str) -> bool {
    callee.contains("CPdxArray<") && callee.contains("::InsertAtEmplace<")
}

/// The function that a thunk symbol runs, or the name itself.
fn thunk_target(name: &str) -> &str {
    if let Some(target) = name.strip_prefix("non-virtual thunk to ") {
        return target;
    }
    name.strip_prefix("{virtual override thunk(")
        .and_then(|rest| rest.strip_suffix(")}"))
        .and_then(|rest| rest.split_once("}, "))
        .map_or(name, |(_, target)| target)
}

#[cfg(test)]
mod tests;
