//! Read the inputs of the derived-name method from executable text.
//!
//! `LOCALIZED_VIEW` keeps the size of the localisation lookup's result view here. It is a
//! per-build layout fact that stays with its reader, not a recipe field. The build is in
//! `docs/native/targets.md` and its facts are on `docs/native/derived-names.md`.
use std::collections::{BTreeMap, BTreeSet};

use crate::engine::analysis::{
    decode::{Instruction, decode_arm64, general_register, is_branch, number, written_registers},
    discovery::Symbol,
    evaluate::{Code, ReadOnlyData},
    families::{Receiver, Root as FamilyRoot},
    fields::has_owner_receiver,
    names::{
        Getter, GetterCall, LogArguments, Miss, NameArgument, NameInput, RegistryInput, ResultView,
        Role, Root, Sink, Stage, UseCall, UseSite,
    },
    strings::StringFunctions,
};
use crate::{AnalysisError, NameLookup};

use super::declarations::{Text, addresses};
use super::families::{
    Call, NamedRegistry, Names, POST_READ, calls, constructors, decoded, entered, loading,
    vtable_group,
};
use super::grammar::{ERROR_LOGS, LOG_FILE_AND_LINE};

/// The functions that check or look up a name. An unchecked localisation lookup states its miss:
/// `LocalizeString` finds no key and returns null, and `PdxLocalizeAndReplaceView` then uses the
/// key's own text and writes no diagnostic. `docs/native/derived-names.md` records the three
/// hand-read checks of this rule.
const SINKS: [(&str, NameLookup, Role, NameArgument, Option<Miss>); 7] = [
    (
        LOCALIZE,
        NameLookup::Localization,
        Role::Lookup,
        NameArgument::View { text: 0, length: 1 },
        Some(Miss::ShowsKey),
    ),
    (
        "HasLocalizeKey(CPdxStringView)",
        NameLookup::Localization,
        Role::Check,
        NameArgument::View { text: 0, length: 1 },
        None,
    ),
    (
        "PdxHasLocalizeKey(CPdxStringView)",
        NameLookup::Localization,
        Role::Check,
        NameArgument::View { text: 0, length: 1 },
        None,
    ),
    (
        "CGuiGraphics::SpriteExists(CString) const",
        NameLookup::Sprite,
        Role::Check,
        NameArgument::Object(1),
        None,
    ),
    (
        "CGuiGraphics::GetSpriteType(CString const&) const",
        NameLookup::Sprite,
        Role::Lookup,
        NameArgument::Object(1),
        None,
    ),
    (
        "VFSExists(CString const&)",
        NameLookup::File,
        Role::Check,
        NameArgument::Object(0),
        None,
    ),
    (
        "VFSExists(char const*)",
        NameLookup::File,
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
    for name in ERROR_LOGS.iter().chain([&LOG_FILE_AND_LINE]) {
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
    let elements = Elements::read(&text, symbols, data, &names);
    let getter_candidates: BTreeSet<u64> = symbols
        .iter()
        .filter(|symbol| root_stage(&symbol.name, owner) == Some(Stage::WhenUsed))
        .filter(|symbol| !root_functions.contains(&symbol.address))
        .map(|symbol| symbol.address)
        .collect();
    let callers = Callers::find(
        &text,
        &members,
        &getter_candidates,
        &sinks,
        elements.as_ref(),
    );

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
        .copied()
        .chain(constructors.iter().map(|constructor| constructor.address))
        .chain(callers.getters.iter().map(|getter| getter.function))
        .chain(callers.functions.iter().copied())
        .collect();
    let getters = callers
        .getters
        .into_iter()
        .map(|mut getter| {
            getter.name = names
                .of(getter.function)
                .next()
                .unwrap_or_default()
                .to_owned();
            getter
        })
        .collect();
    Ok(RegistryInput {
        roots,
        entered,
        constructors,
        loading,
        getters,
        element_calls: callers.element_calls,
        sprite_slot: elements.map(|elements| elements.sprite_slot),
        code: decoded(&text, &functions),
    })
}

/// The GUI element classes whose sprite a caller changes by name, and how callers fetch them.
///
/// An element class `T` has a getter `T* CContainerWindow::GetGuiObject<T>(CString const&)
/// const`, which searches the container's array of `T*` by name, and its primary vtable holds a
/// `ChangeSprite(CString const&)` that calls `CGuiGraphics::GetSpriteType` directly. Callers reach
/// an element through that getter, a function whose whole body branches to it (such as
/// `CContainerWindow::GetButton`), or a call through the displacement where `CContainerWindow`'s
/// vtable holds one. The last assumes that the receiver of that call is a container:
/// `docs/native/derived-names.md` states the assumption and its checks.
struct Elements {
    /// The displacement of every element class's `ChangeSprite(CString const&)`.
    sprite_slot: u64,
    /// The element getters and the functions that branch to one.
    getters: BTreeSet<u64>,
    /// The displacements at which `CContainerWindow`'s vtable holds one of `getters`.
    displacements: BTreeSet<u64>,
}

impl Elements {
    /// The element classes of the executable, or `None` when they do not share one sprite slot.
    fn read(text: &Text, symbols: &[Symbol], data: &ReadOnlyData, names: &Names) -> Option<Self> {
        let sprite_lookups =
            addresses(symbols, "CGuiGraphics::GetSpriteType(CString const&) const");
        let looks_up_sprite = |function: u64| {
            calls(text, function).is_ok_and(|calls| {
                calls
                    .iter()
                    .any(|(_, call)| matches!(call, Call::Direct(target) if sprite_lookups.contains(target)))
            })
        };

        let mut slots = BTreeSet::new();
        let mut typed = BTreeSet::new();
        for symbol in symbols {
            let Some(class) = element_class(&symbol.name) else {
                continue;
            };
            let Some(group) = vtable_group(symbols, data, class) else {
                continue;
            };
            let sprite_slot = group.primary.iter().find(|(_, function)| {
                names
                    .of(**function)
                    .any(|name| name.ends_with("::ChangeSprite(CString const&)"))
                    && looks_up_sprite(**function)
            });
            if let Some((&slot, _)) = sprite_slot {
                slots.insert(slot);
                typed.insert(symbol.address);
            }
        }
        let [sprite_slot] = slots.into_iter().collect::<Vec<_>>()[..] else {
            return None;
        };

        let container = vtable_group(symbols, data, "CContainerWindow")?;
        let mut getters = typed.clone();
        let mut displacements = BTreeSet::new();
        for (&displacement, &function) in &container.primary {
            let branches_to_typed = text
                .function_within(function, 4)
                .and_then(|code| decode_arm64(code, function).ok())
                .is_some_and(|rows| {
                    rows.first().is_some_and(|row| {
                        row.operation == "b"
                            && number(&row.operands).is_some_and(|target| typed.contains(&target))
                    })
                });
            if typed.contains(&function) || branches_to_typed {
                getters.insert(function);
                displacements.insert(displacement);
            }
        }

        Some(Self {
            sprite_slot,
            getters,
            displacements,
        })
    }
}

/// The class `T` of a symbol `T* CContainerWindow::GetGuiObject<T>(CString const&) const`.
fn element_class(name: &str) -> Option<&str> {
    let (returned, rest) = name.split_once("* CContainerWindow::GetGuiObject<")?;
    let class = rest.strip_suffix(">(CString const&) const")?;
    (returned == class).then_some(class)
}

/// The getters of the item class that functions outside it call before a use site, with those
/// calls, and what a run of the callers needs.
struct Callers {
    /// Each getter with its calls that reach a use site. The name is set by the caller.
    getters: Vec<Getter>,
    /// The functions that hold those calls.
    functions: BTreeSet<u64>,
    /// The element calls of those functions.
    element_calls: BTreeSet<u64>,
}

impl Callers {
    /// Find the direct calls (not tail branches) of `candidates` from functions outside the item
    /// class, and for each the sink calls and the candidate sprite calls that its return reaches.
    fn find(
        text: &Text,
        members: &BTreeSet<u64>,
        candidates: &BTreeSet<u64>,
        sinks: &BTreeSet<u64>,
        elements: Option<&Elements>,
    ) -> Self {
        let function_of = |address: u64| text.starts.range(..=address).next_back().copied();
        let mut by_function: BTreeMap<u64, Vec<(u64, u64)>> = BTreeMap::new();
        for (address, target) in text.calls_into(candidates) {
            let Some(function) = function_of(address) else {
                continue;
            };
            if !members.contains(&function) && is_call(text, address) {
                by_function
                    .entry(function)
                    .or_default()
                    .push((address, target));
            }
        }

        let mut getters: BTreeMap<u64, Vec<GetterCall>> = BTreeMap::new();
        let mut functions = BTreeSet::new();
        let mut element_calls = BTreeSet::new();
        for (function, calls) in by_function {
            let Some(caller) = Caller::read(text, function, sinks, elements) else {
                continue;
            };
            let mut used = false;
            for (address, getter) in calls {
                let uses = caller.uses(address);
                if !uses.is_empty() {
                    used = true;
                    getters.entry(getter).or_default().push(GetterCall {
                        address,
                        result: caller.result_offset(address),
                        uses,
                    });
                }
            }
            if used {
                functions.insert(function);
                element_calls.extend(caller.elements);
            }
        }

        Self {
            getters: getters
                .into_iter()
                .map(|(function, calls)| Getter {
                    function,
                    name: String::new(),
                    calls,
                })
                .collect(),
            functions,
            element_calls,
        }
    }
}

/// One caller of a getter: its code, and its sink calls, candidate sprite calls and element
/// calls.
struct Caller {
    rows: Vec<Instruction>,
    /// The targets of the direct branches of `rows`.
    targets: BTreeSet<u64>,
    code: Code,
    sites: Vec<(u64, UseCall)>,
    elements: BTreeSet<u64>,
}

impl Caller {
    fn read(
        text: &Text,
        function: u64,
        sinks: &BTreeSet<u64>,
        elements: Option<&Elements>,
    ) -> Option<Self> {
        let (address, code) = text.function(function).ok()?;
        let rows = decode_arm64(code, address).ok()?;
        let targets: BTreeSet<u64> = rows
            .iter()
            .filter(|row| is_branch(&row.operation))
            .filter_map(|row| number(row.operands.rsplit(',').next()?))
            .collect();

        let mut sites = Vec::new();
        let mut element_calls = BTreeSet::new();
        for (index, row) in rows.iter().enumerate() {
            match row.operation.as_str() {
                "bl" => {
                    let Some(target) = number(&row.operands) else {
                        continue;
                    };
                    if sinks.contains(&target) {
                        sites.push((row.address, UseCall::Sink(target)));
                    }
                    if elements.is_some_and(|elements| elements.getters.contains(&target)) {
                        element_calls.insert(row.address);
                    }
                }
                "blr" => {
                    let (Some(elements), Some(register)) =
                        (elements, general_register(&row.operands))
                    else {
                        continue;
                    };
                    match slot_displacement(&rows[..index], register, &targets, row.address) {
                        Some(slot) if slot == elements.sprite_slot => {
                            sites.push((row.address, UseCall::Register(register)));
                        }
                        Some(slot) if elements.displacements.contains(&slot) => {
                            element_calls.insert(row.address);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        Some(Self {
            code: Code::from_rows(rows.clone()),
            rows,
            targets,
            sites,
            elements: element_calls,
        })
    }

    /// The stack offset of the object that the call at `call` receives in `x8`, when it is one.
    fn result_offset(&self, call: u64) -> Option<u64> {
        let index = self.rows.partition_point(|row| row.address < call);
        result_offset(&self.rows[..index], &self.targets, call)
    }

    /// The sites that the return of the getter call at `call` reaches, each with the element calls
    /// from whose return both the getter call and a sprite call are reachable.
    fn uses(&self, call: u64) -> Vec<UseSite> {
        let reaching_call = self.code.reaching(call);
        self.sites
            .iter()
            .filter_map(|&(address, use_call)| {
                let reaching = self.code.reaching(address);
                if !reaching.contains(&(call + 4)) {
                    return None;
                }
                let element_starts = match use_call {
                    UseCall::Sink(_) => Vec::new(),
                    UseCall::Register(_) => self
                        .elements
                        .iter()
                        .copied()
                        .filter(|element| {
                            reaching.contains(&(element + 4))
                                && reaching_call.contains(&(element + 4))
                        })
                        .collect(),
                };
                Some(UseSite {
                    address,
                    call: use_call,
                    element_starts,
                })
            })
            .collect()
    }
}

/// Whether the instruction at `address` is a `bl`, not a tail branch.
fn is_call(text: &Text, address: u64) -> bool {
    text.bytes(address, 4)
        .ok()
        .and_then(|bytes| decode_arm64(bytes, address).ok())
        .is_some_and(|rows| rows.first().is_some_and(|row| row.operation == "bl"))
}

/// The vtable displacement that a call through `register` at `call` loads its target from: the
/// register's reaching write is `ldr register, [base, #displacement]`.
fn slot_displacement(
    before: &[Instruction],
    register: usize,
    targets: &BTreeSet<u64>,
    call: u64,
) -> Option<u64> {
    let write = reaching_write(before, register, targets, call)?;
    let (destination, memory) = write.operands.split_once(",[")?;
    if write.operation != "ldr" || !destination.starts_with('x') {
        return None;
    }
    let memory = memory.strip_suffix(']')?;
    match memory.split_once(',') {
        Some((_, displacement)) => number(displacement),
        None => Some(0),
    }
}

/// The stack offset that `x8` holds at the call at `call`, where a callee returns an object by
/// value: its reaching write is `add x8, sp, #offset` or `mov x8, sp`.
fn result_offset(before: &[Instruction], targets: &BTreeSet<u64>, call: u64) -> Option<u64> {
    let write = reaching_write(before, 8, targets, call)?;
    match (write.operation.as_str(), write.operands.as_str()) {
        ("mov", "x8,sp") => Some(0),
        ("add", operands) => number(operands.strip_prefix("x8,sp,")?),
        _ => None,
    }
}

/// The last write of `register` in the straight-line code before the instruction at `call`, the
/// end of `before`. A branch, or a branch target between that write and the call, gives `None`.
fn reaching_write<'a>(
    before: &'a [Instruction],
    register: usize,
    targets: &BTreeSet<u64>,
    call: u64,
) -> Option<&'a Instruction> {
    for row in before.iter().rev() {
        let joined = targets.range(row.address + 4..=call).next().is_some();
        if joined || is_branch(&row.operation) {
            return None;
        }
        if written_registers(&row.operation, &row.operands).contains(&register) {
            return Some(row);
        }
    }
    None
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

    fn rows(code: Vec<u8>, start: u64) -> Vec<Instruction> {
        decode_arm64(&code, start).unwrap()
    }

    #[test]
    fn a_register_call_loads_its_target_from_the_displacement_of_its_last_write() {
        let code = crate::engine::analysis::assembler::arm64!(at 0x1000;
            ldr x8, [x26];
            ldr x8, [x8, #0x2e8];
            add x1, sp, #0x18;
            mov x0, x26;
            blr x8
        );
        let rows = rows(code, 0x1000);

        assert_eq!(
            slot_displacement(&rows[..4], 8, &BTreeSet::new(), 0x1010),
            Some(0x2e8)
        );
    }

    #[test]
    fn a_clobber_or_a_join_after_the_load_gives_no_displacement() {
        let clobbered = rows(
            crate::engine::analysis::assembler::arm64!(at 0x1000;
                ldr x8, [x8, #0x2e8];
                mov x8, x9;
                blr x8
            ),
            0x1000,
        );
        let called = rows(
            crate::engine::analysis::assembler::arm64!(at 0x1000;
                ldr x8, [x8, #0x2e8];
                bl extern 0x2000;
                blr x8
            ),
            0x1000,
        );
        let joined = rows(
            crate::engine::analysis::assembler::arm64!(at 0x1000;
                ldr x8, [x8, #0x2e8];
                mov x0, x26;
                blr x8
            ),
            0x1000,
        );

        assert_eq!(
            slot_displacement(&clobbered[..2], 8, &BTreeSet::new(), 0x1008),
            None
        );
        assert_eq!(
            slot_displacement(&called[..2], 8, &BTreeSet::new(), 0x1008),
            None
        );
        assert_eq!(
            slot_displacement(&joined[..2], 8, &BTreeSet::from([0x1004]), 0x1008),
            None
        );
    }

    #[test]
    fn an_element_class_is_the_class_that_its_container_getter_returns() {
        assert_eq!(
            element_class(
                "CButtonStandard* CContainerWindow::GetGuiObject<CButtonStandard>(CString const&) const"
            ),
            Some("CButtonStandard")
        );
        assert_eq!(
            element_class(
                "CIcon* CContainerWindow::GetGuiObject<CButtonStandard>(CString const&) const"
            ),
            None
        );
        assert_eq!(
            element_class("CContainerWindow::GetButton(CString const&) const"),
            None
        );
    }

    #[test]
    fn a_result_object_on_the_stack_is_the_offset_that_x8_holds_at_the_call() {
        let rows = rows(
            crate::engine::analysis::assembler::arm64!(at 0x1000;
                add x8, sp, #0x30;
                mov x0, x19;
                bl extern 0x2000;
                mov x8, x25;
                bl extern 0x2000
            ),
            0x1000,
        );

        assert_eq!(
            result_offset(&rows[..2], &BTreeSet::new(), 0x1008),
            Some(0x30)
        );
        assert_eq!(result_offset(&rows[..4], &BTreeSet::new(), 0x1010), None);
    }
}
