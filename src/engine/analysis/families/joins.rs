//! Which registry's code reaches each call that generates modifier names.
//!
//! A root is a function that the engine runs for the items of one content database: a database
//! function that loops over every item, or an item's post-read function. A context of a
//! generation call is one chain of direct calls from a root down to the call, through at most
//! [`DEPTH`] calls. The search climbs from the function that contains the call through its
//! callers and ends at the first root on each chain; a chain that meets no root is not a context.
//!
//! A context is joined when its root belongs to a named registry and no function on it takes a
//! content object that no named registry owns (the second input of a matrix). A generation call
//! is joined only when every context is joined. Every other call keeps one reason, so each is
//! accounted for.
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
    /// Functions that take a content object that no named registry owns.
    pub unnamed_inputs: BTreeSet<u64>,
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
    /// A context of this registry takes a content object that no named registry owns.
    pub unnamed_input: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteJoin {
    Joined,
    Unjoined(Reason),
}

/// Why a generation call is not joined, in order of precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    /// The call is inside the registration function.
    RegistrationFunction,
    /// A context also takes a content object that no named registry owns.
    UnnamedInput,
    /// A context starts at a content object that is not an item of a named registry.
    UnnamedContent,
    /// No chain of callers reaches a root.
    NoRoot,
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

        let mut reasons = BTreeSet::new();
        let contexts = contexts(graph, site.function);
        if contexts.is_empty() {
            reasons.insert(Reason::NoRoot);
        }

        for context in contexts {
            let RootOf::Registry { registry, receiver } = &graph.roots[&context.root] else {
                reasons.insert(Reason::UnnamedContent);
                continue;
            };
            let registry = joins.registries.entry(registry.clone()).or_default();
            if context
                .path
                .iter()
                .any(|function| graph.unnamed_inputs.contains(function))
            {
                registry.unnamed_input = true;
                reasons.insert(Reason::UnnamedInput);
                continue;
            }

            registry
                .roots
                .entry(context.root)
                .or_insert_with(|| (*receiver, BTreeSet::new()))
                .1
                .insert(site.call);
            registry.path.extend(context.path);
        }

        let join = match reasons.first() {
            Some(reason) => SiteJoin::Unjoined(*reason),
            None => SiteJoin::Joined,
        };
        joins.sites.insert(site.call, join);
    }

    joins
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
            unnamed_inputs: BTreeSet::new(),
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
    fn a_context_through_an_unnamed_input_is_not_followed() {
        let mut graph = graph();
        graph.unnamed_inputs.insert(0x210);
        let joins = join(&graph);

        assert_eq!(
            joins.sites[&0x204],
            SiteJoin::Unjoined(Reason::UnnamedInput)
        );
        let a = &joins.registries["common/a"];
        assert!(a.unnamed_input);
        assert!(!a.path.contains(&0x210));
        assert_eq!(a.roots[&0x1000].1, BTreeSet::from([0x104]));
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
}
