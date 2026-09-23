//! Engine callbacks: the on_actions that the engine fires by name and the game rules that it
//! evaluates, with the scope that each call site supplies for `this`, `root` and the `from` chain.
//!
//! The engine fires an on_action by passing a name string and a scope object to its on_action
//! database, directly or through a command that fires later, and it evaluates a game rule by
//! passing a rule object and a scope object to the rule. The binding lists every direct call to
//! these functions. Each call site gets two passes:
//!
//! - The **name pass** ([`names`]) runs over the whole function without choosing a path. It
//!   gives every literal that the name string can hold at the site, a list that a lookup or the
//!   database's cached pulse lists give, or the offset of the rule object in the rule set. So a
//!   name stays known when its context cannot be followed.
//! - The **context pass** ([`contexts`]) follows every path from the function entry to the site
//!   and reads the scope object there. A path that the pass cannot follow gives no context.
//!
//! A scope object has a type and three links: root, from and prev. A fresh scope has type 0 and
//! each link points back to the scope itself; the engine tests the type of the linked scope, not
//! the pointer, to decide whether a link is set. So the pass reports a self-link as
//! [`Slot::SelfLink`], never as absent. Different contexts for one name stay separate.
//!
//! A small set of forwarders, which take a name or rule from their caller, are pinned by the
//! binding and checked here before their callers are read. A site whose name the method cannot
//! recover is an unnamed site; the method never guesses its name.
mod contexts;
mod names;

use std::collections::{BTreeMap, BTreeSet};

use super::InputError;
use super::decode::Instruction;
use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};

pub use contexts::ScopeFunctions;
pub use names::StringFunctions;

use contexts::{Runner, SiteContexts, Subject};
use names::{Fact, State};

/// Name and revision of this static method.
pub const METHOD: &str = "callbacks/v1";

/// A value that no rule enumeration reaches, for the probe of a rule forwarder.
const PROBE: u64 = 7;

/// Consecutive missing enumerations after which the rule table ends.
const TABLE_MISSES: u64 = 64;

/// Layout facts of one exact build.
#[derive(Debug, Clone, Copy)]
pub struct CallbackLayout {
    /// Offsets of the scope type and of the root, from and prev links in a scope object.
    pub scope_type_offset: u64,
    pub scope_root_offset: u64,
    pub scope_from_offset: u64,
    pub scope_prev_offset: u64,
    /// Where each family's rule objects are in the rule set.
    pub scripted_rules: RuleArray,
    pub weighted_rules: RuleArray,
    /// Offset of the token in a rule declaration row.
    pub declaration_token_offset: u64,
}

/// An array of rule objects in the rule set: its offset and the size of one rule.
#[derive(Debug, Clone, Copy)]
pub struct RuleArray {
    pub base: u64,
    pub stride: u64,
}

/// A family of game rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleFamily {
    /// A rule that evaluates a trigger.
    Scripted,
    /// A rule that computes a weight.
    Weighted,
}

/// One direct call that the method reads.
#[derive(Debug, Clone)]
pub struct Site {
    /// The call instruction.
    pub address: u64,
    /// The start of the function that holds it.
    pub function: u64,
    pub call: SiteCall,
}

/// What a site calls, and which registers hold its arguments.
#[derive(Debug, Clone, Copy)]
pub enum SiteCall {
    /// Fire an on_action by name, now or later.
    Fire { name: usize, scope: SiteScope },
    /// Look up an on_action list by name.
    Lookup { name: usize },
    /// Fire an on_action list.
    FireList { list: usize, scope: usize },
    /// Evaluate a rule object.
    Rule {
        family: RuleFamily,
        rule: usize,
        scope: usize,
    },
    /// Call a pinned forwarder.
    Forwarded { forwarder: usize },
}

/// Where a firing site's scope comes from.
#[derive(Debug, Clone, Copy)]
pub enum SiteScope {
    /// The scope object in this register.
    Register(usize),
    /// The callee builds the scope itself; the method does not follow it.
    BuiltByCallee,
}

/// A function that takes a name or rule from its caller and passes it to a site of its own.
#[derive(Debug, Clone)]
pub struct Forwarder {
    pub function: u64,
    pub kind: ForwarderKind,
}

#[derive(Debug, Clone, Copy)]
pub enum ForwarderKind {
    /// Fires the on_action named by the string in register `name`, with the caller's scope in
    /// register `scope`, or with a scope that the forwarder builds when `scope` is `None`.
    Fire { name: usize, scope: Option<usize> },
    /// Evaluates the rule of `family` whose enumeration is in register `enumeration`, with a
    /// scope that the forwarder builds.
    Rule {
        family: RuleFamily,
        enumeration: usize,
    },
}

/// The on_action database's cached lists: its load function, the string comparison that it
/// uses, and the global that holds the database.
#[derive(Debug, Clone)]
pub struct Pulse {
    pub init: u64,
    pub string_compare: BTreeSet<u64>,
    pub instance: u64,
}

/// The rule declaration tables: the initializer that fills them and the lookup of each family.
#[derive(Debug, Clone)]
pub struct RuleTables {
    pub code: Vec<Instruction>,
    pub initializer: u64,
    pub finders: Vec<(RuleFamily, u64)>,
}

/// Executable-derived input for the callback method.
pub struct CallbacksInput {
    /// Decoded rows of every function that holds a site, and of the pulse load function.
    pub functions: BTreeMap<u64, Vec<Instruction>>,
    /// Decoded rows of the scope functions that the context pass runs.
    pub scope_code: Vec<Instruction>,
    pub scope_functions: ScopeFunctions,
    pub strings: StringFunctions,
    /// The lookup of an on_action list by name.
    pub lookups: BTreeSet<u64>,
    pub sites: Vec<Site>,
    pub forwarders: Vec<Forwarder>,
    /// Starts of the rule set's own functions, whose receiver is the rule set.
    pub rule_owners: BTreeSet<u64>,
    /// Sites where script content fires an on_action that it names.
    pub script_fired_sites: usize,
    pub pulse: Option<Pulse>,
    pub rule_tables: RuleTables,
    /// Literal token names by token.
    pub tokens: BTreeMap<u64, String>,
    /// Scope names indexed by scope-type bit, or `None` when the table was not read.
    pub scope_names: Option<Vec<String>>,
    pub data: ReadOnlyData,
    pub layout: CallbackLayout,
}

/// One entry scope at a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    /// A scope of the type with this bit.
    Scope(u32),
    /// A scope of type 0.
    NotSet,
    /// The link points back to the scope that holds it.
    SelfLink,
    Unresolved,
}

/// The scopes that one path supplies at a site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Context {
    pub this: Slot,
    pub root: Slot,
    /// from, fromfrom, …; ends after the first slot that is not a scope.
    pub from: Vec<Slot>,
}

/// What the method established for one name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Findings {
    pub contexts: BTreeSet<Context>,
    /// Why some site of this name gave no context, or not every context.
    pub unresolved: BTreeSet<&'static str>,
}

/// A site whose subject the method could not name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Unnamed {
    pub family: Family,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    OnAction,
    GameRule,
}

/// Everything that the method established.
#[derive(Debug, Clone, Default)]
pub struct CallbacksResult {
    pub on_actions: BTreeMap<String, Findings>,
    pub rules: BTreeMap<String, (RuleFamily, Findings)>,
    pub unnamed: Vec<Unnamed>,
    pub script_fired_sites: usize,
    /// Why the rule tables could not be read.
    pub rule_tables_missing: Option<&'static str>,
}

impl SiteCall {
    fn family(self, forwarders: &[Forwarder]) -> Family {
        match self {
            Self::Fire { .. } | Self::Lookup { .. } | Self::FireList { .. } => Family::OnAction,
            Self::Rule { .. } => Family::GameRule,
            Self::Forwarded { forwarder } => forwarders[forwarder].kind.family(),
        }
    }
}

impl ForwarderKind {
    fn family(self) -> Family {
        match self {
            Self::Fire { .. } => Family::OnAction,
            Self::Rule { .. } => Family::GameRule,
        }
    }
}

/// Run the method for the sites of one family.
pub fn analyze(input: &CallbacksInput, family: Family) -> Result<CallbacksResult, InputError> {
    let in_family = |site: &&Site| site.call.family(&input.forwarders) == family;
    if !input.sites.iter().any(|site| in_family(&site)) {
        return Err(InputError(format!("no {family:?} call site")));
    }

    let runner = Runner {
        scope_code: &input.scope_code,
        scopes: &input.scope_functions,
        strings: &input.strings,
        lookups: &input.lookups,
        data: &input.data,
        layout: input.layout,
    };
    let states = site_states(input);
    let rule_names = match family {
        Family::GameRule => rule_names(input),
        Family::OnAction => Ok(BTreeMap::new()),
    };
    let pulse = match family {
        Family::OnAction => pulse_names(input),
        Family::GameRule => BTreeMap::new(),
    };
    let mut assembly = Assembly::default();
    if let Err(reason) = rule_names {
        assembly.result.rule_tables_missing = Some(reason);
    }
    let rule_names = rule_names.unwrap_or_default();

    let forwarders: BTreeMap<u64, usize> = input
        .forwarders
        .iter()
        .enumerate()
        .map(|(index, forwarder)| (forwarder.function, index))
        .collect();
    let mut codes: BTreeMap<u64, Code> = BTreeMap::new();

    // Forwarders first: their own sites give the contexts and the check of their callers.
    let mut inner: Vec<Option<SiteContexts>> = vec![None; input.forwarders.len()];
    for (index, forwarder) in input.forwarders.iter().enumerate() {
        if forwarder.kind.family() != family {
            continue;
        }
        let sites: Vec<&Site> = input
            .sites
            .iter()
            .filter(|site| site.function == forwarder.function)
            .collect();
        let code = code_for(&mut codes, &runner, input, forwarder.function);
        inner[index] = forwarder_contexts(input, &runner, code, forwarder, &sites, &states);
    }

    let lookup_uses = lookup_uses(input, &states);
    let lookup_names: BTreeMap<u64, Result<BTreeSet<String>, &'static str>> = input
        .sites
        .iter()
        .filter_map(|site| match site.call {
            SiteCall::Lookup { name } => {
                let state = states.get(&site.address)?;
                Some((site.address, literal_names(input, state, name)))
            }
            _ => None,
        })
        .collect();

    for site in input.sites.iter().filter(in_family) {
        if forwarders.contains_key(&site.function) {
            continue;
        }
        let Some(state) = states.get(&site.address) else {
            assembly.unnamed(Family::OnAction, "site-not-decoded");
            continue;
        };
        let code = code_for(&mut codes, &runner, input, site.function);

        match site.call {
            SiteCall::Fire { name, scope } => {
                let names = literal_names(input, state, name);
                match scope {
                    SiteScope::Register(scope) => {
                        let found = runner.contexts(
                            code,
                            site.function,
                            site.address,
                            Subject::String(name),
                            scope,
                        );
                        assembly.on_action_site(input, names, found);
                    }
                    SiteScope::BuiltByCallee => {
                        assembly.on_action_names(names, Some("scope-built-by-callee"));
                    }
                }
            }
            SiteCall::Lookup { name } => {
                if !lookup_uses.contains(&site.address) {
                    let names = literal_names(input, state, name);
                    assembly.on_action_names(names, Some("looked-up-only"));
                }
            }
            SiteCall::FireList { list, scope } => {
                let names = list_names(input, state, list, &pulse, &lookup_names);
                let found = runner.contexts(
                    code,
                    site.function,
                    site.address,
                    Subject::List(list),
                    scope,
                );
                assembly.on_action_site(input, names, found);
            }
            SiteCall::Rule {
                family,
                rule,
                scope,
            } => {
                let name = rule_name(input, site, state, family, rule, &rule_names);
                let found =
                    runner.contexts(code, site.function, site.address, Subject::None, scope);
                assembly.rule_site(family, name, found);
            }
            SiteCall::Forwarded { forwarder } => {
                let Some(inner) = &inner[forwarder] else {
                    let family = match input.forwarders[forwarder].kind {
                        ForwarderKind::Fire { .. } => Family::OnAction,
                        ForwarderKind::Rule { .. } => Family::GameRule,
                    };
                    assembly.unnamed(family, "forwarder-not-verified");
                    continue;
                };
                match input.forwarders[forwarder].kind {
                    ForwarderKind::Fire { name, scope } => {
                        let names = literal_names(input, state, name);
                        let found = match scope {
                            Some(scope) => runner.contexts(
                                code,
                                site.function,
                                site.address,
                                Subject::String(name),
                                scope,
                            ),
                            None => without_subject(inner),
                        };
                        assembly.on_action_site(input, names, found);
                    }
                    ForwarderKind::Rule {
                        family,
                        enumeration,
                    } => {
                        let name = match constant(state.register(enumeration)) {
                            Some(value) => rule_names
                                .get(&(family, value))
                                .cloned()
                                .ok_or("rule-not-declared"),
                            None => Err("rule-not-constant"),
                        };
                        assembly.rule_site(family, name, inner.clone());
                    }
                }
            }
        }
    }

    for ((family, _), name) in &rule_names {
        let entry = assembly
            .result
            .rules
            .entry(name.clone())
            .or_insert_with(|| (*family, Findings::default()));
        if entry.1.contexts.is_empty() && entry.1.unresolved.is_empty() {
            entry.1.unresolved.insert("no-site");
        }
    }
    for (_, name) in pulse {
        if let Some(name) = input.data.string(name) {
            assembly
                .result
                .on_actions
                .entry(name)
                .or_default()
                .unresolved
                .insert("cached-list-not-fired");
        }
    }
    for findings in assembly.result.on_actions.values_mut() {
        if !findings.contexts.is_empty() {
            findings.unresolved.remove("cached-list-not-fired");
        }
    }

    assembly.result.script_fired_sites = input.script_fired_sites;
    Ok(assembly.result)
}

#[derive(Default)]
struct Assembly {
    result: CallbacksResult,
}

impl Assembly {
    fn unnamed(&mut self, family: Family, reason: &'static str) {
        self.result.unnamed.push(Unnamed { family, reason });
    }

    fn on_action_names(
        &mut self,
        names: Result<BTreeSet<String>, &'static str>,
        reason: Option<&'static str>,
    ) {
        match names {
            Ok(names) => {
                for name in names {
                    let findings = self.result.on_actions.entry(name).or_default();
                    findings.unresolved.extend(reason);
                }
            }
            Err(reason) => self.unnamed(Family::OnAction, reason),
        }
    }

    /// Give each arriving path's context to the name that the path proves, or to the site's one
    /// name.
    fn on_action_site(
        &mut self,
        input: &CallbacksInput,
        names: Result<BTreeSet<String>, &'static str>,
        found: SiteContexts,
    ) {
        let mut names = match names {
            Ok(names) => names,
            Err(reason) => {
                self.unnamed(Family::OnAction, reason);
                BTreeSet::new()
            }
        };
        let mut attributed: BTreeMap<String, BTreeSet<Context>> = BTreeMap::new();
        let mut unattributed = false;
        for (literal, context) in found.reached {
            let proved = literal.and_then(|literal| input.data.string(literal));
            let name = match proved {
                Some(name) => {
                    names.insert(name.clone());
                    Some(name)
                }
                None if names.len() == 1 => names.first().cloned(),
                None => None,
            };
            match name {
                Some(name) => {
                    attributed.entry(name).or_default().insert(context);
                }
                None => unattributed = true,
            }
        }

        let reached_none = attributed.is_empty() && !unattributed;
        if names.is_empty() {
            self.unnamed(Family::OnAction, "name-not-proved");
        }
        for name in names {
            let findings = self.result.on_actions.entry(name.clone()).or_default();
            findings
                .contexts
                .extend(attributed.remove(&name).unwrap_or_default());
            findings.unresolved.extend(found.unresolved);
            if unattributed {
                findings.unresolved.insert("context-not-attributed");
            }
            if reached_none && found.unresolved.is_none() {
                findings.unresolved.insert("site-not-reached");
            }
        }
    }

    fn rule_site(
        &mut self,
        family: RuleFamily,
        name: Result<String, &'static str>,
        found: SiteContexts,
    ) {
        let name = match name {
            Ok(name) => name,
            Err(reason) => {
                self.unnamed(Family::GameRule, reason);
                return;
            }
        };
        let entry = self
            .result
            .rules
            .entry(name)
            .or_insert_with(|| (family, Findings::default()));
        entry.1.unresolved.extend(found.unresolved);
        if found.reached.is_empty() && found.unresolved.is_none() {
            entry.1.unresolved.insert("site-not-reached");
        }
        entry
            .1
            .contexts
            .extend(found.reached.into_iter().map(|(_, context)| context));
    }
}

/// The decoded code of one function with the scope functions, built once per function.
fn code_for<'c>(
    codes: &'c mut BTreeMap<u64, Code>,
    runner: &Runner<'_>,
    input: &CallbacksInput,
    function: u64,
) -> &'c Code {
    codes
        .entry(function)
        .or_insert_with(|| runner.code(input.functions.get(&function).map_or(&[], Vec::as_slice)))
}

/// The name-pass state before every site, for every function that holds a site.
fn site_states(input: &CallbacksInput) -> BTreeMap<u64, State> {
    let addresses: BTreeSet<u64> = input.sites.iter().map(|site| site.address).collect();
    let functions: BTreeSet<u64> = input.sites.iter().map(|site| site.function).collect();
    let mut states = BTreeMap::new();
    for function in functions {
        let Some(rows) = input.functions.get(&function) else {
            continue;
        };
        names::each_state(rows, &input.strings, |row, state| {
            if addresses.contains(&row.address) {
                states.insert(row.address, state.clone());
            }
        });
    }
    states
}

/// The names that the string in register `name` can hold at a site.
fn literal_names(
    input: &CallbacksInput,
    state: &State,
    name: usize,
) -> Result<BTreeSet<String>, &'static str> {
    let literals = state.string_literals(name).ok_or("name-not-a-literal")?;
    literals
        .into_iter()
        .map(|literal| input.data.string(literal).ok_or("name-not-readable"))
        .collect()
}

/// The names of a list: the names that a lookup in the same function was given, or a cached
/// pulse list.
fn list_names(
    input: &CallbacksInput,
    state: &State,
    list: usize,
    pulse: &BTreeMap<i64, u64>,
    lookups: &BTreeMap<u64, Result<BTreeSet<String>, &'static str>>,
) -> Result<BTreeSet<String>, &'static str> {
    let facts = state.register(list).as_ref().ok_or("list-unknown")?;
    let mut names = BTreeSet::new();
    for fact in facts {
        match *fact {
            Fact::Field(global, offset)
                if input
                    .pulse
                    .as_ref()
                    .is_some_and(|pulse| pulse.instance == global) =>
            {
                let literal = pulse.get(&offset).copied().ok_or("cached-list-unknown")?;
                names.insert(input.data.string(literal).ok_or("name-not-readable")?);
            }
            Fact::Result(call) => {
                let looked_up = lookups.get(&call).ok_or("list-unknown")?.clone()?;
                names.extend(looked_up);
            }
            _ => return Err("list-unknown"),
        }
    }
    Ok(names)
}

/// Lookup calls whose list a firing site in the same function uses.
fn lookup_uses(input: &CallbacksInput, states: &BTreeMap<u64, State>) -> BTreeSet<u64> {
    input
        .sites
        .iter()
        .filter_map(|site| match site.call {
            SiteCall::FireList { list, .. } => states.get(&site.address)?.register(list).clone(),
            _ => None,
        })
        .flatten()
        .filter_map(|fact| match fact {
            Fact::Result(call) => Some(call),
            _ => None,
        })
        .collect()
}

/// The rule that a rule set's own function evaluates: its receiver plus the rule's offset.
fn rule_name(
    input: &CallbacksInput,
    site: &Site,
    state: &State,
    family: RuleFamily,
    rule: usize,
    names: &BTreeMap<(RuleFamily, u64), String>,
) -> Result<String, &'static str> {
    if !input.rule_owners.contains(&site.function) {
        return Err("rule-outside-the-rule-set");
    }
    let facts = state.register(rule).as_ref().ok_or("rule-unknown")?;
    let offset = match facts.iter().collect::<Vec<_>>().as_slice() {
        [Fact::Argument(0, offset)] => *offset,
        _ => return Err("rule-unknown"),
    };
    let enumeration = enumeration(input.layout, family, offset).ok_or("rule-offset")?;
    names
        .get(&(family, enumeration))
        .cloned()
        .ok_or("rule-not-declared")
}

fn enumeration(layout: CallbackLayout, family: RuleFamily, offset: i64) -> Option<u64> {
    let array = match family {
        RuleFamily::Scripted => layout.scripted_rules,
        RuleFamily::Weighted => layout.weighted_rules,
    };
    let relative = u64::try_from(offset).ok()?.checked_sub(array.base)?;
    relative
        .is_multiple_of(array.stride)
        .then(|| relative / array.stride)
}

fn constant(value: &names::Value) -> Option<u64> {
    match value.as_ref()?.iter().collect::<Vec<_>>().as_slice() {
        [Fact::Constant(value)] => Some(*value),
        _ => None,
    }
}

/// Check a forwarder against its own site, and give the contexts of the scope that it builds.
/// `None` when the check fails.
fn forwarder_contexts(
    input: &CallbacksInput,
    runner: &Runner<'_>,
    code: &Code,
    forwarder: &Forwarder,
    sites: &[&Site],
    states: &BTreeMap<u64, State>,
) -> Option<SiteContexts> {
    let [site] = sites else {
        return None;
    };
    let state = states.get(&site.address)?;
    let is_argument = |register: usize, argument: usize| {
        state
            .register(register)
            .as_ref()
            .is_some_and(|facts| facts.iter().collect::<Vec<_>>() == [&Fact::Argument(argument, 0)])
    };

    match (forwarder.kind, site.call) {
        (
            ForwarderKind::Fire { name, scope },
            SiteCall::Fire {
                name: inner_name,
                scope: SiteScope::Register(inner_scope),
            },
        ) => {
            if !is_argument(inner_name, name) {
                return None;
            }
            match scope {
                Some(scope) => is_argument(inner_scope, scope).then(SiteContexts::default),
                None => Some(runner.contexts(
                    code,
                    forwarder.function,
                    site.address,
                    Subject::None,
                    inner_scope,
                )),
            }
        }
        (
            ForwarderKind::Rule {
                family,
                enumeration,
            },
            SiteCall::Rule {
                family: inner_family,
                scope,
                ..
            },
        ) if family == inner_family => {
            let array = match family {
                RuleFamily::Scripted => input.layout.scripted_rules,
                RuleFamily::Weighted => input.layout.weighted_rules,
            };
            let (base, passed) =
                runner.probe(code, forwarder.function, site.address, enumeration, PROBE);
            let expected = base + array.base + PROBE * array.stride;
            if passed.is_empty() || passed.iter().any(|rule| *rule != Some(expected)) {
                return None;
            }
            Some(runner.contexts(code, forwarder.function, site.address, Subject::None, scope))
        }
        _ => None,
    }
}

/// A forwarder's own contexts, which belong to whatever name its caller passes.
fn without_subject(inner: &SiteContexts) -> SiteContexts {
    SiteContexts {
        reached: inner
            .reached
            .iter()
            .map(|(_, context)| (None, context.clone()))
            .collect(),
        unresolved: inner.unresolved,
    }
}

/// Run the rule initializer, then each family's lookup for each enumeration until the table
/// ends, and name each rule by its token.
fn rule_names(input: &CallbacksInput) -> Result<BTreeMap<(RuleFamily, u64), String>, &'static str> {
    let tables = &input.rule_tables;
    let code = Code::from_rows(tables.code.iter().cloned());
    let mut filled = Machine::new(&code, &input.data);
    let exit = filled
        .run(tables.initializer, &mut |_, _| Ok(Call::Return(None)))
        .map_err(|_| "rule-initializer")?;
    if exit != Exit::Returned {
        return Err("rule-initializer");
    }

    let mut names = BTreeMap::new();
    for &(family, finder) in &tables.finders {
        let row = |enumeration: u64| -> Result<u64, &'static str> {
            let mut machine = filled.clone();
            machine.set_register(0, enumeration);
            machine
                .run(finder, &mut |_, _| Ok(Call::Return(None)))
                .map_err(|_| "rule-lookup")?;
            machine.register(0).ok_or("rule-lookup")
        };
        let missing = row(u64::from(u32::MAX))?;
        let mut misses = 0;
        let mut enumeration = 0;
        while misses < TABLE_MISSES {
            let found = row(enumeration)?;
            if found == missing {
                misses += 1;
            } else {
                misses = 0;
                let token = filled
                    .read(found + input.layout.declaration_token_offset, 4)
                    .ok_or("rule-token")?;
                let name = input.tokens.get(&token).ok_or("rule-token")?;
                names.insert((family, enumeration), name.clone());
            }
            enumeration += 1;
        }
    }
    if names.is_empty() {
        return Err("rule-table-empty");
    }
    Ok(names)
}

/// The cached lists of the on_action database: in its load function, each comparison of a list
/// name with a literal is followed along its equal edge to the store of the list into the
/// database. Gives the database offset and the literal.
fn pulse_names(input: &CallbacksInput) -> BTreeMap<i64, u64> {
    let Some(pulse) = &input.pulse else {
        return BTreeMap::new();
    };
    let Some(rows) = input.functions.get(&pulse.init) else {
        return BTreeMap::new();
    };

    let mut compares: Vec<(usize, u64)> = Vec::new();
    let mut stores: BTreeMap<usize, Option<i64>> = BTreeMap::new();
    let position: BTreeMap<u64, usize> = rows
        .iter()
        .enumerate()
        .map(|(position, row)| (row.address, position))
        .collect();
    names::each_state(rows, &input.strings, |row, state| {
        let at = position[&row.address];
        if row.operation == "bl"
            && names::immediate(&row.operands)
                .is_some_and(|target| pulse.string_compare.contains(&(target as u64)))
            && let Some(literal) = constant(state.register(1))
        {
            compares.push((at, literal));
        }
        if row.operation == "str" {
            stores.insert(at, database_store(row, state));
        }
    });

    let mut found: BTreeMap<i64, BTreeSet<u64>> = BTreeMap::new();
    for (at, literal) in compares {
        let Some(edge) = equal_edge(rows, &position, at) else {
            continue;
        };
        let store = (edge..rows.len())
            .take_while(|&index| index == edge || !ends_block(&rows[index - 1]))
            .find_map(|index| stores.get(&index).copied().flatten());
        if let Some(offset) = store {
            found.entry(offset).or_default().insert(literal);
        }
    }
    found
        .into_iter()
        .filter_map(|(offset, literals)| match literals.len() {
            1 => Some((offset, *literals.first()?)),
            _ => None,
        })
        .collect()
}

/// The position where the comparison at `at` continues when its strings are equal: the result
/// is tested with `cbz` or `cbnz` right after the call.
fn equal_edge(rows: &[Instruction], position: &BTreeMap<u64, usize>, at: usize) -> Option<usize> {
    let test = rows.get(at + 1)?;
    let operands = names::split(&test.operands);
    let [register, target] = operands.as_slice() else {
        return None;
    };
    if names::register(register) != Some(0) {
        return None;
    }
    let target = position.get(&(names::immediate(target)? as u64)).copied()?;
    match test.operation.as_str() {
        "cbz" => Some(target),
        "cbnz" => Some(at + 2),
        _ => None,
    }
}

/// The database offset of a 64-bit store through the load function's receiver.
fn database_store(row: &Instruction, state: &State) -> Option<i64> {
    let operands = names::split(&row.operands);
    let [source, memory] = operands.as_slice() else {
        return None;
    };
    if !source.starts_with('x') {
        return None;
    }
    let inner = memory.strip_prefix('[')?.strip_suffix(']')?;
    let mut parts = inner.split(',');
    let base = names::register(parts.next()?)?;
    let displacement = match parts.next() {
        None => 0,
        Some(text) => names::immediate(text)?,
    };
    let facts = state.register(base).as_ref()?;
    match facts.iter().collect::<Vec<_>>().as_slice() {
        [Fact::Argument(0, offset)] => Some(offset + displacement),
        _ => None,
    }
}

fn ends_block(row: &Instruction) -> bool {
    let operation = row.operation.as_str();
    operation.starts_with("b.")
        || matches!(
            operation,
            "b" | "br" | "ret" | "cbz" | "cbnz" | "tbz" | "tbnz" | "brk"
        )
}

#[cfg(test)]
mod tests;
