//! Read the inputs of the derived-name method from executable text.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::{
    discovery::Symbol,
    evaluate::ReadOnlyData,
    families::{Receiver, Root as FamilyRoot, StringFunctions},
    fields::has_owner_receiver,
    names::{
        LogArguments, Miss, NameArgument, NameInput, RegistryInput, ResultView, Role, Root, Sink,
        Stage, Target,
    },
};

use super::declarations::{Text, addresses};
use super::families::{Call, NamedRegistry, Names, calls, constructors, decoded, entered, loading};
use super::grammar::ERROR_LOGS;

/// The functions that check or look up a name. An unchecked localisation lookup states its miss:
/// `LocalizeString` finds no key and returns null, and `PdxLocalizeAndReplaceView` then uses the
/// key's own text and writes no diagnostic. `docs/native/derived-names.md` records the three
/// hand-read checks of this rule.
const SINKS: [(&str, Target, Role, NameArgument, Option<Miss>); 7] = [
    (
        LOCALIZE,
        Target::Localization,
        Role::Lookup,
        NameArgument::View { text: 0, length: 1 },
        Some(Miss::ShowsKey),
    ),
    (
        "HasLocalizeKey(CPdxStringView)",
        Target::Localization,
        Role::Check,
        NameArgument::View { text: 0, length: 1 },
        None,
    ),
    (
        "PdxHasLocalizeKey(CPdxStringView)",
        Target::Localization,
        Role::Check,
        NameArgument::View { text: 0, length: 1 },
        None,
    ),
    (
        "CGuiGraphics::SpriteExists(CString) const",
        Target::Sprite,
        Role::Check,
        NameArgument::Object(1),
        None,
    ),
    (
        "CGuiGraphics::GetSpriteType(CString const&) const",
        Target::Sprite,
        Role::Lookup,
        NameArgument::Object(1),
        None,
    ),
    (
        "VFSExists(CString const&)",
        Target::File,
        Role::Check,
        NameArgument::Object(0),
        None,
    ),
    (
        "VFSExists(char const*)",
        Target::File,
        Role::Check,
        NameArgument::Text(0),
        None,
    ),
];

/// The localisation lookup. It returns a `CPdxTemporaryLocalizationStringView` through `x8`: the
/// text pointer, the 32-bit length at 8 and a flag byte at 12.
const LOCALIZE: &str =
    "PdxLocalizeAndReplaceView(CPdxStringView, CPdxLocalizeKeyValuePair const*, int)";

/// The size of the view that the localisation lookup returns.
const LOCALIZED_VIEW: ResultView = ResultView { size: 0x10 };

/// The constructor that starts a file-and-line diagnostic. It receives no part of the message, and
/// a run must not enter it: it clears a large buffer byte by byte.
const LOG_START: &str =
    "CPdxLogFileAndLine::CPdxLogFileAndLine(char const*, unsigned int, unsigned int)";

/// The post-read functions of an item: the engine runs them after it reads the item.
const POST_READ: [&str; 2] = ["::PostReadInit()", "::InitPostRead("];

/// The sinks and diagnostic functions of the executable.
pub(in crate::binding) fn input(symbols: &[Symbol]) -> NameInput {
    let mut sinks = BTreeMap::new();
    for (name, target, role, argument, unchecked_miss) in SINKS {
        for address in addresses(symbols, name) {
            let sink = Sink {
                target,
                role,
                argument,
                unchecked_miss: unchecked_miss.clone(),
                result: (name == LOCALIZE).then_some(LOCALIZED_VIEW),
            };
            sinks.insert(address, sink);
        }
    }

    let mut logs = BTreeMap::new();
    for name in ERROR_LOGS.iter().chain([&LOG_START]) {
        for address in addresses(symbols, name) {
            logs.insert(address, log_arguments(name));
        }
    }

    NameInput { sinks, logs }
}

/// Where the diagnostic function `name` receives its text.
fn log_arguments(name: &str) -> LogArguments {
    if name.contains("(char const*, ...)") {
        LogArguments::Formatted
    } else if name.contains("(CString const&)") {
        LogArguments::Object(1)
    } else if name.contains("CLogStream::") {
        LogArguments::Text(1)
    } else {
        LogArguments::NoText
    }
}

/// The roots of one registry's item class that reach a sink, the members and composers that
/// their runs enter, and the item constructors and loading code.
pub(in crate::binding) fn registry(
    bytes: &[u8],
    symbols: &[Symbol],
    data: &ReadOnlyData,
    strings: &StringFunctions,
    input: &NameInput,
    registry: &NamedRegistry,
) -> Result<RegistryInput, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let names = Names::new(symbols);
    let owner = registry.owner;
    let members: BTreeSet<u64> = symbols
        .iter()
        .filter(|symbol| is_member(&symbol.name, owner))
        .map(|symbol| symbol.address)
        .collect();
    let sinks: BTreeSet<u64> = input.sinks.keys().copied().collect();
    let reach = Reach::find(&text, &members, &sinks)?;

    let mut roots: BTreeMap<u64, Root> = BTreeMap::new();
    for symbol in symbols {
        let Some(stage) = root_stage(&symbol.name, owner) else {
            continue;
        };
        let sites = reach.sites(symbol.address);
        if !sites.is_empty() {
            roots.entry(symbol.address).or_insert_with(|| Root {
                function: symbol.address,
                name: symbol.name.clone(),
                stage,
                sites,
            });
        }
    }
    let roots: Vec<Root> = roots.into_values().collect();

    let root_functions: BTreeSet<u64> = roots.iter().map(|root| root.function).collect();
    let path = reach.path(&root_functions);
    let recorded: BTreeSet<u64> = sinks.iter().chain(input.logs.keys()).copied().collect();
    let entered = entered(
        &text,
        &names,
        strings,
        root_functions.clone(),
        path,
        &recorded,
    )?;
    let constructors = constructors(symbols, owner);

    let initialization: Vec<FamilyRoot> = roots
        .iter()
        .filter(|root| root.stage == Stage::OwnerInitialization)
        .map(|root| FamilyRoot {
            function: root.function,
            receiver: Receiver::Item,
            sites: BTreeSet::new(),
        })
        .collect();
    let loading = (!initialization.is_empty())
        .then(|| loading(&text, symbols, &names, data, registry, &initialization));

    let functions: Vec<u64> = root_functions
        .iter()
        .chain(&entered)
        .chain(&constructors)
        .copied()
        .collect();
    Ok(RegistryInput {
        roots,
        entered,
        constructors,
        loading,
        code: decoded(&text, &functions),
    })
}

/// When the root `name` of the item class `owner` runs, when it is one: a direct `const` member
/// when the engine uses the item, a post-read function after reading. Thunks and out-of-line
/// parts are not roots.
fn root_stage(name: &str, owner: &str) -> Option<Stage> {
    if name.contains(".cold.") || name.contains("thunk") {
        return None;
    }
    if has_owner_receiver(name, owner) {
        return Some(Stage::WhenUsed);
    }
    let member = name.strip_prefix(owner)?;
    POST_READ
        .iter()
        .any(|suffix| member.starts_with(suffix))
        .then_some(Stage::OwnerInitialization)
}

/// Whether `name` is a direct member function of `owner`, other than an out-of-line part.
fn is_member(name: &str, owner: &str) -> bool {
    let Some(member) = name
        .strip_prefix(owner)
        .and_then(|rest| rest.strip_prefix("::"))
    else {
        return false;
    };
    let method = member.split('(').next().unwrap_or_default();
    member.contains('(') && !method.contains("::") && !name.contains(".cold.")
}

/// The sink calls of each member function, directly or through other members. The member call
/// graph is finite, so the search ends; a run's own path and step bounds turn a deep chain that
/// it cannot follow into a gap.
struct Reach {
    /// The sink calls in each member's own code.
    own: BTreeMap<u64, BTreeSet<u64>>,
    /// The members that each member calls directly.
    callees: BTreeMap<u64, BTreeSet<u64>>,
}

impl Reach {
    fn find(
        text: &Text,
        members: &BTreeSet<u64>,
        sinks: &BTreeSet<u64>,
    ) -> Result<Self, AnalysisError> {
        let mut own = BTreeMap::new();
        let mut callees = BTreeMap::new();
        for &member in members {
            let Ok(calls) = calls(text, member) else {
                continue;
            };
            let direct = calls.into_iter().filter_map(|(at, call)| match call {
                Call::Direct(target) => Some((at, target)),
                Call::Indirect => None,
            });
            let (to_sinks, to_others): (Vec<_>, Vec<_>) =
                direct.partition(|(_, target)| sinks.contains(target));
            own.insert(member, to_sinks.into_iter().map(|(at, _)| at).collect());
            callees.insert(
                member,
                to_others
                    .into_iter()
                    .map(|(_, target)| target)
                    .filter(|target| members.contains(target))
                    .collect(),
            );
        }
        Ok(Self { own, callees })
    }

    /// The sink calls that `function` reaches through any number of member calls.
    fn sites(&self, function: u64) -> BTreeSet<u64> {
        let mut sites = BTreeSet::new();
        let mut pending = vec![function];
        let mut seen = BTreeSet::from([function]);
        while let Some(member) = pending.pop() {
            sites.extend(self.own.get(&member).into_iter().flatten());
            for &callee in self.callees.get(&member).into_iter().flatten() {
                if seen.insert(callee) {
                    pending.push(callee);
                }
            }
        }
        sites
    }

    /// The members that a root calls, through any number of member calls, and that reach a sink.
    /// A root that another root calls is one of them.
    fn path(&self, roots: &BTreeSet<u64>) -> BTreeSet<u64> {
        let mut path = BTreeSet::new();
        let mut pending: Vec<u64> = roots.iter().copied().collect();
        let mut seen = BTreeSet::new();
        while let Some(member) = pending.pop() {
            for &callee in self.callees.get(&member).into_iter().flatten() {
                if !seen.insert(callee) {
                    continue;
                }
                pending.push(callee);
                if !self.sites(callee).is_empty() {
                    path.insert(callee);
                }
            }
        }
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(address: u64, name: &str) -> Symbol {
        Symbol {
            address,
            name: name.into(),
        }
    }

    #[test]
    fn roots_are_const_members_and_post_read_functions_of_the_item_class() {
        let owner = "CItem";
        assert_eq!(
            root_stage("CItem::GetName(CCountry const*) const", owner),
            Some(Stage::WhenUsed)
        );
        assert_eq!(
            root_stage("CItem::PostReadInit()", owner),
            Some(Stage::OwnerInitialization)
        );
        assert_eq!(
            root_stage("CItem::InitPostRead(CString const&, int, int)", owner),
            Some(Stage::OwnerInitialization)
        );
        assert_eq!(root_stage("CItem::SetName(CString const&)", owner), None);
        assert_eq!(root_stage("CItem::Swap::GetName() const", owner), None);
        assert_eq!(
            root_stage("CItem::GetName() const [clone .cold.1]", owner),
            None
        );
        assert_eq!(
            root_stage("non-virtual thunk to CItem::PostReadInit()", owner),
            None
        );
    }

    #[test]
    fn a_root_that_reaches_a_sink_through_five_member_calls_is_a_root() {
        const SINK_CALL: u64 = 0x7000;
        let chain = [0x100, 0x200, 0x300, 0x400, 0x500, 0x600];
        let reach = Reach {
            own: chain
                .iter()
                .map(|&member| (member, BTreeSet::new()))
                .chain([(0x600, BTreeSet::from([SINK_CALL]))])
                .collect(),
            callees: chain
                .windows(2)
                .map(|pair| (pair[0], BTreeSet::from([pair[1]])))
                .collect(),
        };

        assert_eq!(reach.sites(0x100), BTreeSet::from([SINK_CALL]));
        assert_eq!(
            reach.path(&BTreeSet::from([0x100])),
            chain[1..].iter().copied().collect()
        );
        let both_roots = BTreeSet::from([0x100, 0x300]);
        assert!(reach.path(&both_roots).contains(&0x300));
    }

    #[test]
    fn each_sink_and_diagnostic_function_is_read_by_its_symbol() {
        let symbols = [
            symbol(0x100, LOCALIZE),
            symbol(0x200, "HasLocalizeKey(CPdxStringView)"),
            symbol(0x300, "CPdxLogFileAndLine::operator()(char const*, ...)"),
            symbol(0x400, "CLogStream::operator<<(CString const&)"),
            symbol(0x500, "CLogger::Log(char const*, unsigned int, int)"),
        ];
        let input = input(&symbols);

        assert_eq!(input.sinks[&0x100].role, Role::Lookup);
        assert_eq!(input.sinks[&0x100].unchecked_miss, Some(Miss::ShowsKey));
        assert_eq!(input.sinks[&0x200].role, Role::Check);
        assert_eq!(input.logs[&0x300], LogArguments::Formatted);
        assert_eq!(input.logs[&0x400], LogArguments::Object(1));
        assert_eq!(input.logs[&0x500], LogArguments::NoText);
    }
}
