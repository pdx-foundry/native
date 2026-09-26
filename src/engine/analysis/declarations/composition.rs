//! Registrations whose token the code composes at run time.
//!
//! A script list registers its commands through helpers. The list's registry helper composes each
//! command's name and documentation from the list's name and description with the command's
//! `GenerateTokenName` and `GenerateDocumentation`, adds the name to the lexer as a dynamic token,
//! and passes the token and the documentation to a helper that registers the entry. The token is
//! not a literal at the registration call, so the method follows the call's arguments.
//!
//! The method runs the function that contains the registration call from its entry, with the
//! evaluator and the string model, and reads the name and the documentation at the call. When
//! the run does not establish them, it runs from each caller of that function instead, through
//! the call, up to [`CALLER_DEPTH`] callers. Each chain of calls that establishes them is one
//! registration, so a helper that two callers use registers two commands.
//!
//! The run enters only the composers. `AddDynamicToken` returns a new token that is labelled with
//! its name, `operator new` returns fresh memory, and the database's `CreateInstance` returns.
//! Every other call returns an unknown value, and any string that it receives becomes unknown.
//! Every path that arrives at the call must agree. A chain that is not established at the
//! greatest depth is a gap: the method never names a registration from a partial run.
use std::collections::{BTreeMap, BTreeSet};

use super::{DeclarationInput, Function, Site, decode, scopes, split_documentation, target};
use crate::engine::analysis::{
    decode::Instruction,
    evaluate::{Call, Code, Exit, Machine, PATH_LIMIT, ReadOnlyData},
    families::{Arena, Effect, Model, StringFunctions, StringLayout},
    stop::Unresolved,
};

/// The most callers that the method follows above the function that contains a registration call.
pub const CALLER_DEPTH: usize = 2;

/// The first token that `AddDynamicToken` returns in a run. The executable's literal tokens are
/// below it, and every token fits in the 32-bit register that carries it.
const DYNAMIC_TOKENS: u64 = 0xf000_0000;

/// The largest allocation that `operator new` makes in a run, in bytes.
const ALLOCATION_LIMIT: u64 = 0x1000;

/// The code that composes tokens at run time, and the functions that the method follows.
pub struct Composition {
    /// The whole body of each function that contains a registration call, of its callers up to
    /// [`CALLER_DEPTH`], and of each composer, by start address.
    pub bodies: BTreeMap<u64, Function>,
    /// For each body, the calls and tail calls to it: the call's address and the calling body.
    pub callers: BTreeMap<u64, Vec<(u64, u64)>>,
    /// Functions that compose a command's name or documentation. The method runs them.
    pub composers: BTreeSet<u64>,
    /// `CStaticLexer::AddDynamicToken`: the name's string object in `x0`; it returns the token.
    pub dynamic_token: BTreeSet<u64>,
    /// The database's `CreateInstance`, which a registration calls when the database does not
    /// exist yet. It takes no argument, so it changes no string that the run follows.
    pub create_database: BTreeSet<u64>,
    pub strings: StringFunctions,
    pub layout: StringLayout,
    pub data: ReadOnlyData,
}

/// The registration calls of one analysis, with the bodies that it has decoded.
pub(super) struct Composer<'a> {
    input: &'a DeclarationInput,
    rows: BTreeMap<u64, Vec<Instruction>>,
}

/// One step of a chain: a function, and the call in it that the run arrives at.
type Step = (u64, u64);

impl<'a> Composer<'a> {
    pub fn new(input: &'a DeclarationInput) -> Self {
        Self {
            input,
            rows: BTreeMap::new(),
        }
    }

    /// Every registration that the call at `call` makes, one for each chain of callers, or a gap.
    pub fn registrations(&mut self, call: u64) -> Vec<Site> {
        let Some(function) = self.enclosing(call) else {
            return vec![Site::RuntimeToken {
                obstacle: "registering-function",
            }];
        };
        let mut chains = vec![vec![(function, call)]];
        let mut sites = Vec::new();
        for depth in 0..=CALLER_DEPTH {
            let mut longer = Vec::new();
            for chain in chains {
                let obstacle = match self.evaluate(&chain) {
                    Ok(site) => {
                        sites.push(site);
                        continue;
                    }
                    Err(Unresolved {
                        reason: obstacle, ..
                    }) => obstacle,
                };
                let callers: Vec<Step> = self
                    .input
                    .composition
                    .callers
                    .get(&chain[0].0)
                    .into_iter()
                    .flatten()
                    .filter(|(_, caller)| chain.iter().all(|(function, _)| function != caller))
                    .map(|&(call, caller)| (caller, call))
                    .collect();
                if depth == CALLER_DEPTH || callers.is_empty() {
                    sites.push(Site::RuntimeToken { obstacle });
                    continue;
                }
                for caller in callers {
                    let mut chain = chain.clone();
                    chain.insert(0, caller);
                    longer.push(chain);
                }
            }
            chains = longer;
        }
        sites
    }

    /// The body that contains `address`.
    fn enclosing(&self, address: u64) -> Option<u64> {
        let (&start, body) = self
            .input
            .composition
            .bodies
            .range(..=address)
            .next_back()?;
        (address < start + body.code.len() as u64).then_some(start)
    }

    fn rows(&mut self, function: u64) -> Result<&[Instruction], Unresolved> {
        if !self.rows.contains_key(&function) {
            let body = self
                .input
                .composition
                .bodies
                .get(&function)
                .ok_or(Unresolved::new("function-code"))?;
            let rows = decode(body).map_err(|_| Unresolved::new("function-code"))?;
            self.rows.insert(function, rows);
        }
        Ok(&self.rows[&function])
    }

    /// The code of the chain's functions and of every composer that they reach.
    fn code(&mut self, chain: &[Step]) -> Result<Code, Unresolved> {
        let composers = &self.input.composition.composers;
        let mut pending: Vec<u64> = chain.iter().map(|(function, _)| *function).collect();
        let mut functions = BTreeSet::new();
        while let Some(function) = pending.pop() {
            if !functions.insert(function) {
                continue;
            }
            pending.extend(
                self.rows(function)?
                    .iter()
                    .filter(|row| row.operation == "bl")
                    .filter_map(target)
                    .filter(|target| composers.contains(target)),
            );
        }
        let mut rows = Vec::new();
        for function in functions {
            rows.extend_from_slice(self.rows(function)?);
        }
        Ok(Code::from_rows(rows))
    }

    /// Run the chain to its last call, and read the registration there.
    fn evaluate(&mut self, chain: &[Step]) -> Result<Site, Unresolved> {
        let code = self.code(chain)?;
        let input = self.input;
        let composition = &input.composition;
        let run = Run {
            input,
            model: Model {
                functions: &composition.strings,
                layout: composition.layout,
                data: &composition.data,
                key: "",
            },
        };
        let mut arena = Arena::default();
        let mut tokens = 0;

        let mut machines = vec![Machine::new(&code, &composition.data)];
        for &(function, call) in chain {
            let mut arrived = Vec::new();
            for machine in machines {
                let paths = machine.run_paths_to(function, call, &mut |target, machine| {
                    run.call(target, machine, &mut arena, &mut tokens)
                });
                for path in paths {
                    match path.end? {
                        Exit::Reached => arrived.push(path.machine),
                        Exit::Trapped => {}
                        Exit::Stopped(target)
                            if composition.strings.never_return.contains(&target) => {}
                        Exit::Returned | Exit::Stopped(_) | Exit::Looped => {
                            return Err(Unresolved::new("call-skipped"));
                        }
                    }
                }
            }
            if arrived.is_empty() {
                return Err(Unresolved::new("call-unreached"));
            }
            if arrived.len() > PATH_LIMIT {
                return Err(Unresolved::new("path-limit"));
            }
            machines = arrived;
        }

        let mut registrations = BTreeSet::new();
        for machine in &machines {
            registrations.insert(run.registration(machine, &mut arena)?);
        }
        let (Some((name, factory, documentation)), None) =
            (registrations.pop_first(), registrations.pop_first())
        else {
            return Err(Unresolved::new("paths-disagree"));
        };
        let (description, usage) = split_documentation(&documentation);
        Ok(Site::Declared {
            name,
            description,
            usage,
            scopes: scopes(input, factory),
            factory,
        })
    }
}

/// What one chain's run does at each call.
struct Run<'a> {
    input: &'a DeclarationInput,
    model: Model<'a>,
}

impl Run<'_> {
    fn call(
        &self,
        target: Option<u64>,
        machine: &mut Machine,
        arena: &mut Arena,
        tokens: &mut u64,
    ) -> Result<Call, Unresolved> {
        let composition = &self.input.composition;
        match target {
            Some(target) if composition.composers.contains(&target) => {
                let exit = machine.run(target, &mut |target, machine| {
                    self.call(Some(target), machine, arena, tokens)
                })?;
                match exit {
                    Exit::Returned => Ok(Call::Return(machine.register(0))),
                    _ => Err(Unresolved::new("composer")),
                }
            }
            Some(target) if composition.dynamic_token.contains(&target) => {
                let name = self.model.object_node(machine, machine.register(0), arena);
                let token = DYNAMIC_TOKENS + *tokens;
                *tokens += 1;
                machine.label(token, name);
                Ok(Call::Return(Some(token)))
            }
            Some(target) if composition.create_database.contains(&target) => Ok(Call::Return(None)),
            Some(target) if self.input.operator_new.contains(&target) => {
                let size = machine.register(0).filter(|size| *size <= ALLOCATION_LIMIT);
                Ok(Call::Return(size.map(|size| machine.allocate(size))))
            }
            _ => Ok(match self.model.call(target, machine, arena)? {
                Effect::Followed(call) => call,
                Effect::Other => Call::Return(None),
            }),
        }
    }

    /// The name, the entry's factory and the documentation at a registration call: the token in
    /// `w1`, and the entry in `x2`, which holds the factory and then the documentation text.
    fn registration(
        &self,
        machine: &Machine,
        arena: &mut Arena,
    ) -> Result<(String, u64, String), Unresolved> {
        let token = machine.known_register(1, "token")? & 0xffff_ffff;
        let name = if token >= DYNAMIC_TOKENS {
            let node = machine.labelled(token).ok_or(Unresolved::new("token"))?;
            arena
                .node(node)
                .literal_text()
                .ok_or(Unresolved::new("name"))?
        } else {
            self.input
                .tokens
                .get(&token)
                .cloned()
                .ok_or(Unresolved::new("token-table"))?
        };
        let entry = machine.known_register(2, "entry")?;
        let factory = machine
            .read(entry, 8)
            .ok_or(Unresolved::new("entry-shape"))?;
        let text = machine.read(entry + 8, 8);
        let node = self.model.text_node(machine, text, arena);
        let documentation = arena
            .node(node)
            .literal_text()
            .ok_or(Unresolved::new("documentation"))?;
        Ok((name, factory, documentation))
    }
}
