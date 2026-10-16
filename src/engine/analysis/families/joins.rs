//! Which registry's code reaches each call that generates modifier names.
//!
//! A root is a function that the engine runs for the items of one content database: a database
//! function that loops over every item, or an item's post-read function. A context of a
//! generation call is one chain of direct calls from a root down to the call, through at most
//! [`DEPTH`] calls. The search climbs from the function that contains the call through its
//! callers and ends at the first root on each chain; a chain that meets no root is not a context.
//!
//! A context is joined when its root belongs to a named registry and every function on it that
//! takes a second content object (the second input of a matrix) takes an item of one other named
//! registry. A second content object is one that no template registry owns and that is no item of
//! the root's own registry. A generation call is joined only when every context is joined. Every
//! other call keeps one reason, so each is accounted for.
//!
//! An unjoined call may generate the names of a registry only when that registry's classes are on
//! its chains: the function that contains the call, or one of its callers, is a member of the
//! registry's item or database class or takes one of its items. The call is counted in that
//! registry's join and in no other. The call inside the registration function registers the names
//! that its callers compose, so it counts in no registry.
use std::collections::{BTreeMap, BTreeSet};

/// The most calls between a root and a generation call.
pub const DEPTH: usize = 4;

/// Static facts about the code that generates modifier names.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub sites: Vec<GenerationSite>,
    /// The functions that call or tail-call each function.
    pub callers: BTreeMap<u64, BTreeSet<u64>>,
    /// Where a search for callers ends.
    pub roots: BTreeMap<u64, RootOf>,
    /// Functions that take a content object that no template registry owns, with the one
    /// registry whose items they take, when there is one: for that registry's own contexts the
    /// object is no second input.
    pub second_inputs: BTreeMap<u64, Option<String>>,
    /// The named registries whose classes each function of the graph is a member of or takes.
    pub registries: BTreeMap<u64, BTreeSet<String>>,
}

/// One call that generates a modifier name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationSite {
    pub call: u64,
    /// The function that contains the call.
    pub function: u64,
    /// The call is inside the registration function itself, so every registration passes
    /// through it.
    pub inside_registration: bool,
}

/// The owner of a root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootOf {
    Registry {
        registry: String,
        receiver: Receiver,
    },
    /// A post-read function of a content object that is not an item of a named registry.
    UnnamedContent,
}

/// What a root receives in `x0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Receiver {
    /// The database, whose items the root loops over.
    Database,
    /// One item. That the engine calls the root for every item is not established.
    Item,
}

/// The joins of every generation call.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Joins {
    pub registries: BTreeMap<String, RegistryJoin>,
    /// Each generation call, by address.
    pub sites: BTreeMap<u64, SiteJoin>,
}

/// The code of one registry that generates modifier names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RegistryJoin {
    /// Each root with a joined context, what it receives, and the calls that it reaches.
    pub roots: BTreeMap<u64, (Receiver, BTreeSet<u64>)>,
    /// The functions between a root and a generation call on the joined contexts.
    pub path: BTreeSet<u64>,
    /// Each function on the joined contexts that takes an item of another named registry, by
    /// address.
    pub inputs: BTreeMap<u64, InputJoin>,
    /// A context of this registry takes a second content object that is no item of one named
    /// registry.
    pub second_input: bool,
    /// Each call that is not joined and that a function of this registry's classes contains or
    /// reaches, with the reason that it keeps.
    pub unjoined: BTreeMap<u64, Reason>,
}

/// A function that takes an item of another named registry: the first such function on a joined
/// context, so it receives the root's item and that registry's item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputJoin {
    /// The other registry.
    pub registry: String,
    /// The generation calls that it reaches by direct calls.
    pub sites: BTreeSet<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteJoin {
    Joined,
    Unjoined(Reason),
}

/// Why a generation call is not joined. [`Reason::precedence`] selects the one that a call keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    /// The call is inside the registration function.
    RegistrationFunction,
    /// A context also takes a second content object that is no item of one other named registry.
    SecondInput,
    /// A context starts at a content object that is not an item of a named registry.
    UnnamedContent,
    /// No chain of callers reaches a root.
    NoRoot,
}

impl Reason {
    /// The rank of this reason when a call's contexts give several; the call keeps the reason of
    /// lowest rank.
    fn precedence(self) -> u8 {
        match self {
            Self::RegistrationFunction => 0,
            Self::SecondInput => 1,
            Self::UnnamedContent => 2,
            Self::NoRoot => 3,
        }
    }
}

/// One chain from a root down to a generation call.
struct Context {
    root: u64,
    /// The functions below the root, from the root's callee to the function of the call.
    path: Vec<u64>,
}

/// Join every generation call of `graph`.
pub fn join(graph: &Graph) -> Joins {
    let mut joins = Joins::default();

    for site in &graph.sites {
        if site.inside_registration {
            joins
                .sites
                .insert(site.call, SiteJoin::Unjoined(Reason::RegistrationFunction));
            continue;
        }

        let mut reasons = Vec::new();
        let contexts = contexts(graph, site.function);
        if contexts.is_empty() {
            reasons.push(Reason::NoRoot);
        }

        for context in contexts {
            let RootOf::Registry { registry, receiver } = &graph.roots[&context.root] else {
                reasons.push(Reason::UnnamedContent);
                continue;
            };
            let takes = takes(graph, registry, &context.path);
            let registry = joins.registries.entry(registry.clone()).or_default();
            match takes {
                Takes::RootItems => {
                    registry
                        .roots
                        .entry(context.root)
                        .or_insert_with(|| (*receiver, BTreeSet::new()))
                        .1
                        .insert(site.call);
                }
                Takes::Input {
                    function,
                    registry: other,
                } => {
                    registry
                        .inputs
                        .entry(function)
                        .or_insert_with(|| InputJoin {
                            registry: other,
                            sites: BTreeSet::new(),
                        })
                        .sites
                        .insert(site.call);
                }
                Takes::Unnamed => {
                    registry.second_input = true;
                    reasons.push(Reason::SecondInput);
                    continue;
                }
            }
            registry.path.extend(context.path);
        }

        let Some(reason) = reasons.into_iter().min_by_key(|reason| reason.precedence()) else {
            joins.sites.insert(site.call, SiteJoin::Joined);
            continue;
        };

        joins.sites.insert(site.call, SiteJoin::Unjoined(reason));
        for registry in chain_registries(graph, site.function) {
            joins
                .registries
                .entry(registry.clone())
                .or_default()
                .unjoined
                .insert(site.call, reason);
        }
    }

    joins
}

/// The content objects that one context takes.
enum Takes {
    /// Only the items of the root's registry.
    RootItems,
    /// Also an item of one other named registry, first through `function`.
    Input { function: u64, registry: String },
    /// A second content object of no named registry, or items of more than one other registry.
    Unnamed,
}

/// The content objects that the functions on `path` take, with a root of `registry`.
fn takes(graph: &Graph, registry: &str, path: &[u64]) -> Takes {
    let mut takes = Takes::RootItems;
    for function in path {
        match (graph.second_inputs.get(function), &takes) {
            (None, _) => {}
            (Some(Some(own)), _) if own == registry => {}
            (Some(Some(other)), Takes::RootItems) => {
                takes = Takes::Input {
                    function: *function,
                    registry: other.clone(),
                };
            }
            (Some(Some(other)), Takes::Input { registry, .. }) if other == registry => {}
            (Some(_), _) => return Takes::Unnamed,
        }
    }

    takes
}

/// The registries whose classes `function` or one of its callers is a member of or takes.
fn chain_registries(graph: &Graph, function: u64) -> BTreeSet<&String> {
    let mut seen = BTreeSet::from([function]);
    let mut pending = vec![function];

    while let Some(callee) = pending.pop() {
        for &caller in graph.callers.get(&callee).into_iter().flatten() {
            if seen.insert(caller) {
                pending.push(caller);
            }
        }
    }

    seen.iter()
        .filter_map(|function| graph.registries.get(function))
        .flatten()
        .collect()
}

/// Every chain from a root down to `function`.
fn contexts(graph: &Graph, function: u64) -> Vec<Context> {
    let mut contexts = Vec::new();
    let mut pending = vec![vec![function]];

    while let Some(chain) = pending.pop() {
        let top = chain[0];
        if graph.roots.contains_key(&top) {
            contexts.push(Context {
                root: top,
                path: chain[1..].to_vec(),
            });
            continue;
        }
        if chain.len() > DEPTH {
            continue;
        }

        for &caller in graph.callers.get(&top).into_iter().flatten() {
            if chain.contains(&caller) {
                continue;
            }
            let mut longer = vec![caller];
            longer.extend(&chain);
            pending.push(longer);
        }
    }

    contexts
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGISTRATION: u64 = 0x10;

    fn registry(name: &str, receiver: Receiver) -> RootOf {
        RootOf::Registry {
            registry: name.into(),
            receiver,
        }
    }

    fn site(call: u64, function: u64) -> GenerationSite {
        GenerationSite {
            call,
            function,
            inside_registration: function == REGISTRATION,
        }
    }

    /// Two registries share a helper; one of its callers is a content object with no named
    /// registry.
    fn graph() -> Graph {
        Graph {
            sites: vec![
                site(0x104, 0x100),
                site(0x204, 0x200),
                site(0x304, 0x300),
                site(0x14, REGISTRATION),
                site(0x504, 0x500),
            ],
            callers: BTreeMap::from([
                (0x100, BTreeSet::from([0x1000, 0x2000])),
                (0x200, BTreeSet::from([0x210])),
                (0x210, BTreeSet::from([0x1000])),
                (0x300, BTreeSet::from([0x3000])),
                (0x500, BTreeSet::from([0x510])),
            ]),
            roots: BTreeMap::from([
                (0x1000, registry("common/a", Receiver::Database)),
                (0x2000, registry("common/b", Receiver::Item)),
                (0x3000, RootOf::UnnamedContent),
            ]),
            second_inputs: BTreeMap::new(),
            registries: BTreeMap::new(),
        }
    }

    #[test]
    fn a_call_is_joined_only_when_every_context_is_joined() {
        let joins = join(&graph());

        assert_eq!(joins.sites[&0x104], SiteJoin::Joined);
        assert_eq!(joins.sites[&0x204], SiteJoin::Joined);
        assert_eq!(
            joins.sites[&0x304],
            SiteJoin::Unjoined(Reason::UnnamedContent)
        );
        assert_eq!(
            joins.sites[&0x14],
            SiteJoin::Unjoined(Reason::RegistrationFunction)
        );
        assert_eq!(joins.sites[&0x504], SiteJoin::Unjoined(Reason::NoRoot));

        let a = &joins.registries["common/a"];
        assert_eq!(
            a.roots,
            BTreeMap::from([(0x1000, (Receiver::Database, BTreeSet::from([0x104, 0x204])))])
        );
        assert_eq!(a.path, BTreeSet::from([0x100, 0x200, 0x210]));
        assert_eq!(
            joins.registries["common/b"].roots[&0x2000],
            (Receiver::Item, BTreeSet::from([0x104]))
        );
    }

    #[test]
    fn a_helper_with_a_named_and_an_unnamed_caller_stays_counted() {
        let mut graph = graph();
        graph.callers.get_mut(&0x100).unwrap().insert(0x3000);
        let joins = join(&graph);

        assert_eq!(
            joins.sites[&0x104],
            SiteJoin::Unjoined(Reason::UnnamedContent)
        );
        assert!(
            joins.registries["common/a"].roots[&0x1000]
                .1
                .contains(&0x104)
        );
    }

    #[test]
    fn a_context_through_a_second_input_is_not_followed() {
        let mut graph = graph();
        graph.second_inputs.insert(0x210, None);
        let joins = join(&graph);

        assert_eq!(joins.sites[&0x204], SiteJoin::Unjoined(Reason::SecondInput));
        let a = &joins.registries["common/a"];
        assert!(a.second_input);
        assert!(!a.path.contains(&0x210));
        assert_eq!(a.roots[&0x1000].1, BTreeSet::from([0x104]));
    }

    #[test]
    fn an_item_of_another_named_registry_joins_as_an_input() {
        let mut graph = graph();
        graph.second_inputs.insert(0x100, Some("common/b".into()));
        let joins = join(&graph);

        assert_eq!(joins.sites[&0x104], SiteJoin::Joined);
        let a = &joins.registries["common/a"];
        assert!(!a.second_input);
        let input = InputJoin {
            registry: "common/b".into(),
            sites: BTreeSet::from([0x104]),
        };
        assert_eq!(a.inputs, BTreeMap::from([(0x100, input)]));
        assert_eq!(a.roots[&0x1000].1, BTreeSet::from([0x204]));
        let b = &joins.registries["common/b"];
        assert!(b.inputs.is_empty());
        assert_eq!(b.roots[&0x2000].1, BTreeSet::from([0x104]));
    }

    #[test]
    fn items_of_two_other_registries_on_one_context_stay_unjoined() {
        let mut graph = graph();
        graph.second_inputs.insert(0x200, Some("common/b".into()));
        graph.second_inputs.insert(0x210, Some("common/c".into()));
        let joins = join(&graph);

        assert_eq!(joins.sites[&0x204], SiteJoin::Unjoined(Reason::SecondInput));
        let a = &joins.registries["common/a"];
        assert!(a.second_input);
        assert!(a.inputs.is_empty());
    }

    #[test]
    fn the_search_climbs_at_most_the_depth_and_never_past_a_root() {
        let mut graph = graph();
        graph.sites = vec![site(0x604, 0x600)];
        let chain = [0x600, 0x610, 0x620, 0x630, 0x640];
        for pair in chain.windows(2) {
            graph.callers.insert(pair[0], BTreeSet::from([pair[1]]));
        }
        graph.callers.insert(0x640, BTreeSet::from([0x1000]));
        assert_eq!(
            join(&graph).sites[&0x604],
            SiteJoin::Unjoined(Reason::NoRoot)
        );

        graph.callers.insert(0x630, BTreeSet::from([0x1000]));
        assert_eq!(join(&graph).sites[&0x604], SiteJoin::Joined);

        graph.callers.insert(0x1000, BTreeSet::from([0x3000]));
        assert_eq!(join(&graph).sites[&0x604], SiteJoin::Joined);
    }

    #[test]
    fn an_unjoined_call_counts_only_in_the_registries_of_its_chains() {
        let mut graph = graph();
        // A named class above the root of unnamed content.
        graph.callers.insert(0x3000, BTreeSet::from([0x3100]));
        graph
            .registries
            .insert(0x3100, BTreeSet::from(["common/c".into()]));
        graph
            .registries
            .insert(0x500, BTreeSet::from(["common/d".into()]));
        graph
            .registries
            .insert(REGISTRATION, BTreeSet::from(["common/a".into()]));
        let joins = join(&graph);

        assert_eq!(
            joins.registries["common/c"].unjoined,
            BTreeMap::from([(0x304, Reason::UnnamedContent)])
        );
        assert_eq!(
            joins.registries["common/d"].unjoined,
            BTreeMap::from([(0x504, Reason::NoRoot)])
        );
        assert!(joins.registries["common/a"].unjoined.is_empty());
        assert!(joins.registries["common/b"].unjoined.is_empty());
    }
}
