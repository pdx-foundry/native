//! Engine callbacks: the on_actions that the engine fires by name and the game rules that it
//! evaluates, with the scope that each call site supplies for `this`, `root`, the `from` chain and
//! the `prev` chain.
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
//! When a site fires or evaluates a scope that its function received as a parameter, the context
//! pass instead runs from the function's direct callers that build the scope, through the same
//! caller climb as the block method ([`climb`]). The site's name still comes only from the name
//! pass at the site: a string label that a caller's path carries does not name it.
//!
//! A scope object has a type and three links: root, from and prev. A fresh scope has type 0 and
//! each link points back to the scope itself; the engine tests the type of the linked scope, not
//! the pointer, to decide whether a link is set. So the pass reports a self-link as
//! [`Slot::SelfLink`], never as absent. Different contexts for one name stay separate.
//!
//! A small set of forwarders, which take a name or rule from their caller, are pinned by the
//! binding and checked here before their callers are read. A site whose name the method cannot
//! recover is an unnamed site; the method never guesses its name.
pub mod blocks;
pub mod climb;
mod contexts;
pub mod instances;
mod names;

use std::collections::{BTreeMap, BTreeSet};

use super::InputError;
use super::decode::{Instruction, general_register};
use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use super::stop::Unresolved;

pub use contexts::ScopeFunctions;
pub use names::StringFunctions;

use climb::{CallSite, Decoded, Wrapper};
use contexts::{CallReads, EntryCalls, Read, Runner, SiteContexts, Subject};
use names::{Fact, State};

/// Name and revision of this static method.
pub const METHOD: &str = "callbacks/v4";

/// A value that no rule enumeration reaches, for the probe of a rule forwarder.
const PROBE: u64 = 7;

/// Consecutive missing enumerations after which the rule table ends.
const TABLE_MISSES: u64 = 64;

/// The callback method's name pass follows no offset getter; the block method's does.
const NO_GETTERS: &BTreeMap<u64, i64> = &BTreeMap::new();

/// Layout facts of one exact build.
#[derive(Debug, Clone, Copy)]
pub struct CallbackLayout {
    /// Offsets of the scope type and of the root, from and prev links in a scope object.
    pub scope_type_offset: u64,
    pub scope_root_offset: u64,
    pub scope_from_offset: u64,
    pub scope_prev_offset: u64,
    /// Size of a scope object, as `CopyInternalScopes` allocates it.
    pub scope_size: u64,
    /// Where each family's rule objects are in the rule set.
    pub scripted_rules: RuleArray,
    pub weighted_rules: RuleArray,
    /// Offset of the token in a rule declaration row.
    pub declaration_token_offset: u64,
}

impl CallbackLayout {
    fn rule_array(self, family: RuleFamily) -> RuleArray {
        match family {
            RuleFamily::Scripted => self.scripted_rules,
            RuleFamily::Weighted => self.weighted_rules,
        }
    }
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
    /// Decoded rows of every function that holds a site, of the pulse load function, and of the
    /// direct callers in `callers`.
    pub functions: BTreeMap<u64, Vec<Instruction>>,
    /// The direct calls to each function of [`climbing_functions`] and to its callers short of
    /// [`climb::CALLER_DEPTH`].
    pub callers: BTreeMap<u64, Vec<CallSite>>,
    /// Functions that never return.
    pub never_return: BTreeSet<u64>,
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
    /// How many argument registers, from `x0`, each known function reads.
    pub arguments: BTreeMap<u64, usize>,
    /// How many argument registers a call through a pointer reads, by the call instruction.
    pub call_arguments: BTreeMap<u64, usize>,
    /// Functions that ignore the `x8` that they receive: no path reads it before writing it. A
    /// call to one does not receive the address of a returned object.
    pub ignores_x8: BTreeSet<u64>,
    /// The vtable address point of the object that each proven instance pointer holds
    /// ([`instances`]). `data` holds the pointer slots that name these pointers and the slots of
    /// their vtables; the method places the objects, so a virtual call on one has a known target.
    pub instances: BTreeMap<u64, u64>,
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
    /// A slot that the pass could not read.
    Unresolved,
}

/// The scopes that one path supplies at a site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Context {
    /// The scope itself.
    pub this: Slot,
    /// The scope's root link.
    pub root: Slot,
    /// from, fromfrom, …; ends after the first slot that is not a scope.
    pub from: Vec<Slot>,
    /// prev, prevprev, …; passes a scope of type 0 and ends after the first other slot that is
    /// not a scope.
    pub prev: Vec<Slot>,
}

impl Context {
    /// Whether every slot of the context is established.
    pub fn is_established(&self) -> bool {
        std::iter::once(&self.this)
            .chain([&self.root])
            .chain(&self.from)
            .chain(&self.prev)
            .all(|slot| *slot != Slot::Unresolved)
    }
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
    /// Rules by name and family; a scripted and a weighted rule can share a name.
    pub rules: BTreeMap<(String, RuleFamily), Findings>,
    pub unnamed: Vec<Unnamed>,
    pub script_fired_sites: usize,
    /// Why the rule tables could not be read.
    pub rule_tables_missing: Option<&'static str>,
}

impl SiteCall {
    /// Where the call holds the scope that it fires or evaluates, and its subject; `None` for a
    /// call that holds no scope.
    fn read(self) -> Option<Read> {
        match self {
            Self::Fire {
                name,
                scope: SiteScope::Register(scope),
            } => Some(Read {
                scope,
                subject: Subject::String(name),
            }),
            Self::FireList { list, scope } => Some(Read {
                scope,
                subject: Subject::List(list),
            }),
            Self::Rule { scope, .. } => Some(Read {
                scope,
                subject: Subject::None,
            }),
            Self::Fire {
                scope: SiteScope::BuiltByCallee,
                ..
            }
            | Self::Lookup { .. }
            | Self::Forwarded { .. } => None,
        }
    }

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

    let data = instances::with_objects(&input.data, &input.instances);
    let runner = Runner {
        scope_code: &input.scope_code,
        scopes: &input.scope_functions,
        strings: &input.strings,
        lookups: &input.lookups,
        data: &data,
        layout: input.layout,
        arguments: &input.arguments,
        call_arguments: &input.call_arguments,
        ignores_x8: &input.ignores_x8,
    };
    let states = climb::states_at(
        &input.functions,
        &input.strings,
        NO_GETTERS,
        input.sites.iter().map(|site| (site.function, site.address)),
    );
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

    // Forwarders first: their own sites give the contexts and the check of their callers.
    let inner = verified_forwarders(input, &runner, &states, family);
    let mut climbed = climbed_contexts(
        input,
        &runner,
        &states,
        input.sites.iter().filter(in_family),
    );

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
    let sources = NameSources {
        pulse: &pulse,
        rule_names: &rule_names,
        lookup_uses: &lookup_uses,
        lookup_names: &lookup_names,
    };

    let mut codes: BTreeMap<u64, Code> = BTreeMap::new();
    for site in input.sites.iter().filter(in_family) {
        if forwarders.contains_key(&site.function) {
            continue;
        }
        let Some(state) = states.get(&site.address) else {
            assembly.unnamed(family, "site-not-decoded");
            continue;
        };
        let code = code_for(&mut codes, &runner, input, site.function);
        let climbed = climbed.remove(&site.address);
        let arrival = Arrival {
            runner: &runner,
            code,
            climbed,
        };
        if let Some(finding) = site_finding(input, arrival, site, state, &inner, &sources) {
            assembly.record(input, finding);
        }
    }

    for ((family, _), name) in &rule_names {
        let findings = assembly
            .result
            .rules
            .entry((name.clone(), *family))
            .or_default();
        if findings.contexts.is_empty() && findings.unresolved.is_empty() {
            findings.unresolved.insert("no-site");
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

/// The functions that hold a site whose scope the method follows up the function's direct
/// callers: a site outside the pinned forwarders that fires or evaluates a scope that its function
/// received as a parameter, on every path. The binding gives these functions' callers.
pub fn climbing_functions(
    sites: &[Site],
    forwarders: &[Forwarder],
    functions: &BTreeMap<u64, Vec<Instruction>>,
    strings: &StringFunctions,
) -> BTreeSet<u64> {
    let states = climb::states_at(
        functions,
        strings,
        NO_GETTERS,
        sites.iter().map(|site| (site.function, site.address)),
    );
    climbing_sites(sites.iter(), forwarders, &states)
        .into_keys()
        .map(|wrapper| wrapper.function)
        .collect()
}

/// The sites of [`climbing_functions`], by the wrapper that their function is.
fn climbing_sites<'s>(
    sites: impl Iterator<Item = &'s Site>,
    forwarders: &[Forwarder],
    states: &BTreeMap<u64, State>,
) -> BTreeMap<Wrapper, BTreeSet<u64>> {
    let pinned: BTreeSet<u64> = forwarders
        .iter()
        .map(|forwarder| forwarder.function)
        .collect();
    let mut wrappers: BTreeMap<Wrapper, BTreeSet<u64>> = BTreeMap::new();
    for site in sites.filter(|site| !pinned.contains(&site.function)) {
        let Some(read) = site.call.read() else {
            continue;
        };
        let parameter = states
            .get(&site.address)
            .and_then(|state| climb::parameter(state, read.scope));
        let Some(parameter) = parameter else {
            continue;
        };

        let wrapper = Wrapper {
            function: site.function,
            parameter,
        };
        wrappers.entry(wrapper).or_default().insert(site.address);
    }
    wrappers
}

/// The contexts that the callers of each climbing site among `sites` give it, by the site's
/// address. The climb's charges and every reason that a run from an entry stopped, a bound of the
/// search or not, are unresolved reasons of the sites that the entry reaches.
fn climbed_contexts<'s>(
    input: &CallbacksInput,
    runner: &Runner<'_>,
    states: &BTreeMap<u64, State>,
    sites: impl Iterator<Item = &'s Site> + Clone,
) -> BTreeMap<u64, SiteContexts> {
    let wrappers = climbing_sites(sites.clone(), &input.forwarders, states);
    let climbing: BTreeSet<u64> = wrappers.values().flatten().copied().collect();
    let reads: BTreeMap<u64, (u64, Read)> = sites
        .filter(|site| climbing.contains(&site.address))
        .filter_map(|site| Some((site.address, (site.address, site.call.read()?))))
        .collect();
    let decoded = Decoded {
        functions: &input.functions,
        callers: &input.callers,
        scope_code: &input.scope_code,
        strings: &input.strings,
        getters: NO_GETTERS,
    };
    let climb = climb::climb(&decoded, wrappers);

    let mut found: BTreeMap<u64, SiteContexts> = climbing
        .iter()
        .map(|&site| (site, SiteContexts::default()))
        .collect();
    for (site, reason) in climb.charges {
        found.entry(site).or_default().unresolved.insert(reason);
    }

    let none = BTreeSet::new();
    let calls = EntryCalls {
        evaluators: &none,
        scope_users: &none,
        wrappers: &climb.wrappers,
        receivers: &none,
        never_return: &input.never_return,
    };
    for entry in climb.entries {
        let run = match climb::entry_code(&decoded, &climb.wrappers, entry.function) {
            Some(code) => runner.read_calls(
                &code,
                entry.function,
                entry.site,
                entry.selected,
                &calls,
                &entry.reads(&reads),
            ),
            None => CallReads {
                unresolved: vec![Unresolved::new("caller-not-decoded")],
                ..CallReads::default()
            },
        };

        for (site, literal, context) in run.reached {
            let site = found.entry(site).or_default();
            site.reached.push((literal, context));
        }
        for reason in run.unresolved.iter().chain(&run.bounded) {
            for site in &entry.reaches {
                let site = found.entry(*site).or_default();
                site.unresolved.insert(reason.reason);
            }
        }
    }
    found
}

/// The contexts at the own site of each forwarder of `family` whose check succeeds, by the
/// forwarder's index in `input.forwarders`. Every other forwarder has `None`.
fn verified_forwarders(
    input: &CallbacksInput,
    runner: &Runner<'_>,
    states: &BTreeMap<u64, State>,
    family: Family,
) -> Vec<Option<SiteContexts>> {
    let mut codes: BTreeMap<u64, Code> = BTreeMap::new();
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
        let code = code_for(&mut codes, runner, input, forwarder.function);
        inner[index] = forwarder_contexts(input, runner, code, forwarder, &sites, states);
    }

    inner
}

/// The names that the name pass established outside single sites.
struct NameSources<'a> {
    /// The on_action database's cached pulse lists.
    pulse: &'a BTreeMap<i64, u64>,
    /// The declared rules by family and offset in the rule set.
    rule_names: &'a BTreeMap<(RuleFamily, u64), String>,
    /// Lookup calls whose list a firing site in the same function uses.
    lookup_uses: &'a BTreeSet<u64>,
    /// The names that each lookup call is given.
    lookup_names: &'a BTreeMap<u64, Result<BTreeSet<String>, &'static str>>,
}

/// What one call site establishes about the names and contexts of its family.
enum SiteFinding {
    /// On_action names, with the contexts that arrive at the site.
    OnActionContexts {
        names: Result<BTreeSet<String>, &'static str>,
        found: SiteContexts,
    },
    /// On_action names whose contexts the site does not give, and why.
    OnActionNames {
        names: Result<BTreeSet<String>, &'static str>,
        reason: &'static str,
    },
    /// A rule name, with the contexts that arrive at the site.
    Rule {
        family: RuleFamily,
        name: Result<String, &'static str>,
        found: SiteContexts,
    },
    /// A site whose subject the method could not name.
    Unnamed {
        family: Family,
        reason: &'static str,
    },
}

/// How the contexts that arrive at one site are found: from the callers that the climb followed,
/// when the site climbs, or along the paths from the entry of the site's own function in `code`.
struct Arrival<'a> {
    runner: &'a Runner<'a>,
    code: &'a Code,
    /// The contexts that [`climbed_contexts`] gives a site that climbs.
    climbed: Option<SiteContexts>,
}

impl Arrival<'_> {
    /// The contexts that arrive at `site`, a call that fires or evaluates a scope. A climbed
    /// path's literal comes from a caller, so it is kept only when the name pass at the site
    /// proved a name (`named`): a path label never adds a name.
    fn contexts(self, site: &Site, named: bool) -> SiteContexts {
        match self.climbed {
            Some(found) if named => found,
            Some(found) => without_subject(&found),
            None => site.call.read().map_or_else(SiteContexts::default, |read| {
                self.runner
                    .contexts(self.code, site.function, site.address, read)
            }),
        }
    }
}

/// What `site` establishes, with the name-pass `state` before it and the contexts that `arrival`
/// gives. `inner` holds the checked contexts of each forwarder, as [`verified_forwarders`] gives
/// them. A lookup whose list a firing site uses establishes nothing; the firing site names its
/// list.
fn site_finding(
    input: &CallbacksInput,
    arrival: Arrival<'_>,
    site: &Site,
    state: &State,
    inner: &[Option<SiteContexts>],
    sources: &NameSources<'_>,
) -> Option<SiteFinding> {
    let finding = match site.call {
        SiteCall::Fire {
            name,
            scope: SiteScope::Register(_),
        } => {
            let names = literal_names(input, state, name);
            let found = arrival.contexts(site, names.is_ok());
            SiteFinding::OnActionContexts { names, found }
        }
        SiteCall::Fire {
            name,
            scope: SiteScope::BuiltByCallee,
        } => SiteFinding::OnActionNames {
            names: literal_names(input, state, name),
            reason: "scope-built-by-callee",
        },
        SiteCall::Lookup { name } => {
            if sources.lookup_uses.contains(&site.address) {
                return None;
            }
            let names = literal_names(input, state, name);
            SiteFinding::OnActionNames {
                names,
                reason: "looked-up-only",
            }
        }
        SiteCall::FireList { list, .. } => {
            let names = list_names(input, state, list, sources.pulse, sources.lookup_names);
            let found = arrival.contexts(site, names.is_ok());
            SiteFinding::OnActionContexts { names, found }
        }
        SiteCall::Rule { family, rule, .. } => {
            let name = rule_name(input, site, state, family, rule, sources.rule_names);
            let found = arrival.contexts(site, name.is_ok());
            SiteFinding::Rule {
                family,
                name,
                found,
            }
        }
        SiteCall::Forwarded { forwarder } => {
            let Some(inner) = &inner[forwarder] else {
                return Some(SiteFinding::Unnamed {
                    family: input.forwarders[forwarder].kind.family(),
                    reason: "forwarder-not-verified",
                });
            };
            match input.forwarders[forwarder].kind {
                ForwarderKind::Fire { name, scope } => {
                    let names = literal_names(input, state, name);
                    let found = match scope {
                        Some(scope) => {
                            let read = Read {
                                scope,
                                subject: Subject::String(name),
                            };
                            arrival
                                .runner
                                .contexts(arrival.code, site.function, site.address, read)
                        }
                        None => without_subject(inner),
                    };
                    SiteFinding::OnActionContexts { names, found }
                }
                ForwarderKind::Rule {
                    family,
                    enumeration,
                } => {
                    let name = match constant(state.register(enumeration)) {
                        Some(value) => sources
                            .rule_names
                            .get(&(family, value))
                            .cloned()
                            .ok_or("rule-not-declared"),
                        None => Err("rule-not-constant"),
                    };
                    SiteFinding::Rule {
                        family,
                        name,
                        found: inner.clone(),
                    }
                }
            }
        }
    };

    Some(finding)
}

#[derive(Default)]
struct Assembly {
    result: CallbacksResult,
}

impl Assembly {
    fn record(&mut self, input: &CallbacksInput, finding: SiteFinding) {
        match finding {
            SiteFinding::OnActionContexts { names, found } => {
                self.on_action_site(input, names, found);
            }
            SiteFinding::OnActionNames { names, reason } => {
                self.on_action_names(names, Some(reason));
            }
            SiteFinding::Rule {
                family,
                name,
                found,
            } => self.rule_site(family, name, found),
            SiteFinding::Unnamed { family, reason } => self.unnamed(family, reason),
        }
    }

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
        let (mut names, named) = match names {
            Ok(names) => (names, true),
            Err(reason) => {
                self.unnamed(Family::OnAction, reason);
                (BTreeSet::new(), false)
            }
        };
        // A path that proves no name belongs to the site's name only when the name pass found
        // exactly one.
        let only = match names.len() {
            1 if named => names.first().cloned(),
            _ => None,
        };
        let mut attributed: BTreeMap<String, BTreeSet<Context>> = BTreeMap::new();
        let mut unattributed = false;
        for (literal, context) in found.reached {
            // A path's literal counts only when the name pass did not prove other names: a
            // string label can outlive a change to the string's text.
            let proved = literal
                .and_then(|literal| input.data.string(literal))
                .filter(|name| !named || names.contains(name));
            if let Some(name) = &proved {
                names.insert(name.clone());
            }
            match proved.or_else(|| only.clone()) {
                Some(name) => {
                    attributed.entry(name).or_default().insert(context);
                }
                None => unattributed = true,
            }
        }

        let reached_none = attributed.is_empty() && !unattributed;
        if names.is_empty() && named {
            self.unnamed(Family::OnAction, "name-not-proved");
        }
        for name in names {
            let findings = self.result.on_actions.entry(name.clone()).or_default();
            findings
                .contexts
                .extend(attributed.remove(&name).unwrap_or_default());
            findings.unresolved.extend(found.unresolved.iter().copied());
            if unattributed {
                findings.unresolved.insert("context-not-attributed");
            }
            if reached_none && found.unresolved.is_empty() {
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
        let findings = self.result.rules.entry((name, family)).or_default();
        if found.reached.is_empty() && found.unresolved.is_empty() {
            findings.unresolved.insert("site-not-reached");
        }
        findings
            .contexts
            .extend(found.reached.into_iter().map(|(_, context)| context));
        findings.unresolved.extend(found.unresolved);
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
    let Some(Fact::Argument(0, offset)) = names::sole_fact(state.register(rule)) else {
        return Err("rule-unknown");
    };
    let enumeration = enumeration(input.layout, family, offset).ok_or("rule-offset")?;
    names
        .get(&(family, enumeration))
        .cloned()
        .ok_or("rule-not-declared")
}

fn enumeration(layout: CallbackLayout, family: RuleFamily, offset: i64) -> Option<u64> {
    let array = layout.rule_array(family);
    let relative = u64::try_from(offset).ok()?.checked_sub(array.base)?;
    relative
        .is_multiple_of(array.stride)
        .then(|| relative / array.stride)
}

fn constant(value: &names::Value) -> Option<u64> {
    match names::sole_fact(value)? {
        Fact::Constant(value) => Some(value),
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
        names::sole_fact(state.register(register)) == Some(Fact::Argument(argument, 0))
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
                None => {
                    let read = Read {
                        scope: inner_scope,
                        subject: Subject::None,
                    };
                    Some(runner.contexts(code, forwarder.function, site.address, read))
                }
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
            let array = input.layout.rule_array(family);
            let (base, passed) =
                runner.probe(code, forwarder.function, site.address, enumeration, PROBE);
            let expected = base + array.base + PROBE * array.stride;
            if passed.is_empty() || passed.iter().any(|rule| *rule != Some(expected)) {
                return None;
            }
            let read = Read {
                scope,
                subject: Subject::None,
            };
            Some(runner.contexts(code, forwarder.function, site.address, read))
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
        unresolved: inner.unresolved.clone(),
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
    names::each_state(rows, &input.strings, NO_GETTERS, |row, state| {
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
    if general_register(register) != Some(0) {
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
    let base = general_register(parts.next()?)?;
    let displacement = match parts.next() {
        None => 0,
        Some(text) => names::immediate(text)?,
    };
    match names::sole_fact(state.register(base))? {
        Fact::Argument(0, offset) => Some(offset + displacement),
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
