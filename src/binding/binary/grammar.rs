//! Bind the command grammar method to executable symbols and the selected build recipe.
//!
//! Stated per-build facts. The knowledge page is `docs/native/command-grammar.md`, F4.
//!
//! - Claim: `CScopeObjectReference::Get{LocalPointer, ColonyCarrierRef, OpenerID, ObjectName}()`
//!   are not typed scope accessors, so the binding skips them. Conditions: M45-release names.
//!   Obstacle: their symbol shape is the shape of a typed accessor. Removal route: derive the
//!   accessor set from the scope-type compare in each body.
//! - Claim: `GetGrowthStage()`, `GetGalacticCommunity()` and `GetObject<CGalacticCommunity>()` have
//!   no rejection null object; `GetDesign()` and `GetDlcRecommendation()` return a null object
//!   that the suffix does not name. Removal route: read the null object from the accessor body.
//! - Claim: the `System` target getters use the `GalacticObject` accessor. Conditions: M45-release
//!   names. Removal route: follow the call from the getter to its accessor.
//!
//! An accessor with no single `TPdxNullObject` and a getter with no accessor are bound
//! `NoNullObject`. The getter analysis keeps them unresolved.
use super::super::targets::DeclarationRecipe;
use crate::engine::analysis::{
    InputError,
    declarations::{DeclarationInput, DeclarationResult, tail_callees},
    discovery::Symbol,
    fields,
    grammar::{AccessorNullObject, CommandBindings, GrammarInput},
};
use crate::{AnalysisError, BlockFamily};
use std::collections::{BTreeMap, BTreeSet};

/// The functions that write an engine error message, or a part of one.
pub(super) const ERROR_LOGS: [&str; 5] = [
    "CLogger::Log(char const*, unsigned int, int)",
    "CLogStream::operator<<(char*)",
    "CLogStream::operator<<(char const*)",
    "CLogStream::operator<<(CString const&)",
    "CPdxLogFileAndLine::operator()(char const*, ...)",
];

/// The constructor that starts a file-and-line diagnostic. It receives no part of the message, and
/// a run must not enter it: it clears a large buffer byte by byte.
pub(super) const LOG_FILE_AND_LINE: &str =
    "CPdxLogFileAndLine::CPdxLogFileAndLine(char const*, unsigned int, unsigned int)";

/// The reader of a block of effects, member by member.
pub(super) const EFFECT_MEMBER_READER: &str = "CEffect::ReadMember(CReader&, int, EScopeType)";

/// The reader of a block of triggers, member by member.
pub(super) const TRIGGER_MEMBER_READER: &str =
    "CTriggerCollectionBase::ReadMember(CReader&, int, EScopeType)";

/// The member readers of the command blocks, with the family of the commands that each reads.
pub(super) const CHILD_FAMILIES: [(&str, BlockFamily); 2] = [
    (TRIGGER_MEMBER_READER, BlockFamily::Trigger),
    (EFFECT_MEMBER_READER, BlockFamily::Effect),
];

#[allow(clippy::too_many_arguments)] // Independent executable inputs and the selected build recipe.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    bound_slots: &BTreeSet<u64>,
    mut declarations: DeclarationInput,
    inventory: &DeclarationResult,
    recipe: &DeclarationRecipe,
    kind: crate::DeclarationKind,
    names: BTreeMap<u64, String>,
) -> Result<GrammarInput, AnalysisError> {
    let text = super::declarations::Text::read(bytes, symbols)?;
    let mut roots: Vec<_> = inventory
        .sites
        .iter()
        .filter_map(|(_, site)| {
            let crate::engine::analysis::declarations::Site::Declared { factory, .. } = site else {
                return None;
            };
            let entry = declarations
                .pointers
                .get(&(factory + declarations.slots.create))?;
            declarations.functions.get(entry)
        })
        .collect();
    let out_of_line: Vec<_> = roots
        .iter()
        .flat_map(|create| tail_callees(&declarations.functions, create))
        .collect();
    roots.extend(out_of_line);
    roots.extend(
        symbols
            .iter()
            .filter(|symbol| {
                symbol
                    .name
                    .ends_with("::ReadMember(CReader&, int, EScopeType)")
            })
            .filter_map(|symbol| declarations.functions.get(&symbol.address)),
    );
    let constants = super::language::constant_data(bytes, &declarations.pointers, bound_slots)?;
    declarations.constructors = super::receivers::constructors(&text, symbols, &constants, &roots)?;

    declarations.constructor_calls = super::receivers::constructor_calls(&text, symbols)?;

    let tokens = text.token_table(symbols, strings)?;
    let families = CHILD_FAMILIES
        .iter()
        .map(|&(name, family)| (name.into(), family))
        .collect();
    let numeric_decoder = super::declarations::unique(symbols, super::numeric::READ_INT_VALUE)?;
    let command_bindings = command_bindings(symbols, &tokens, recipe, kind)?;
    let mut form_input = crate::engine::analysis::grammar::forms::Input {
        reader_value_token_offset: Some(recipe.persistent.value_token),
        token_text_offset: recipe.persistent.token_text,
        target_size: recipe.event_target_size,
        string_size: recipe.string_object_size,
        role_slot: match kind {
            crate::DeclarationKind::Effect => recipe.effect_names.role,
            crate::DeclarationKind::Trigger => recipe.trigger_names.role,
        },
        ..Default::default()
    };
    let flags = super::dynamic_names::flag_functions(symbols)?;
    form_input.dynamic_name = flags.name_reader;
    form_input.interner = flags.interner;
    for symbol in symbols {
        if crate::engine::analysis::readers::call_arguments(&symbol.name).is_some() {
            form_input
                .shared
                .insert(symbol.address, symbol.name.clone());
        }
        if symbol.name.ends_with("::Assign(CToken const&, EScopeType)") {
            form_input.assignments.insert(symbol.address);
        }
        if matches!(
            symbol.name.as_str(),
            "CString::operator=(CString const&)" | "CString::CString(CString const&)"
        ) {
            form_input.string_copies.insert(symbol.address);
        }
        if symbol.name == "CString::CString(char const*)" {
            form_input.strings_from_text.insert(symbol.address);
        }
        if matches!(
            symbol.name.as_str(),
            "CEventTarget::~CEventTarget()"
                | "CString::~CString()"
                | "operator delete[](void*)"
                | "operator delete(void*)"
        ) {
            form_input.harmless.insert(symbol.address);
        }
        if text.starts.contains(&symbol.address)
            && text.function_length(symbol.address) > super::declarations::FUNCTION_BODY_LIMIT
        {
            form_input.cut_bodies.insert(symbol.address);
        }
    }
    let targets = target_input(symbols, recipe, &command_bindings)?;
    let durations = crate::engine::analysis::durations::Input {
        execute_slot: form_input.role_slot,
        names,
        scoped_storage: BTreeMap::new(),
    };
    Ok(GrammarInput {
        durations,
        targets,
        key_readers: super::fields::key_readers(symbols, &recipe.persistent)?,
        persistent_slots: [recipe.persistent.read_slot, recipe.persistent.member_slot],
        forms: form_input,
        command_bindings,
        child_layout: recipe.command_children,
        numeric_decoder,
        reader_token_offset: recipe.reader_token_offset,
        declarations,
        symbols: symbols.to_vec(),
        data: super::declarations::read_only_sections(bytes)?,
        tokens,
        families,
    })
}

/// Resolve every required signature before a later stage consumes these bindings.
fn command_bindings(
    symbols: &[Symbol],
    tokens: &BTreeMap<i64, fields::Token>,
    recipe: &DeclarationRecipe,
    kind: crate::DeclarationKind,
) -> Result<CommandBindings, AnalysisError> {
    use super::declarations::unique;

    let (slots, validation_slot, target_getter_slot) = match kind {
        crate::DeclarationKind::Effect => (
            recipe.effect_names,
            recipe.effect_validation_slot,
            recipe.effect_target_getter_slot,
        ),
        crate::DeclarationKind::Trigger => (
            recipe.trigger_names,
            recipe.trigger_validation_slot,
            recipe.trigger_target_getter_slot,
        ),
    };
    let boolean_tokens: [Result<i64, AnalysisError>; 2] = recipe.boolean_tokens.map(|name| {
        let mut matches = tokens
            .iter()
            .filter(|(_, token)| token.name.as_deref() == Some(name));
        match (matches.next(), matches.next()) {
            (Some((&id, _)), None) => Ok(id),
            _ => Err(InputError(format!("Boolean token {name} is missing or ambiguous")).into()),
        }
    });
    let [true_token, false_token] = boolean_tokens;
    let mut target_getters = required_matching(symbols, "typed target getters", |name| {
        name.starts_with("CEventTarget::GetScope") && name.ends_with("(CEventScope&) const")
    })?;
    for prefix in ["CEventTarget::GetTarget", "CEventTarget::AccessTarget"] {
        target_getters.extend(required_matching(symbols, prefix, |name| {
            name.starts_with(prefix)
                && name
                    .ends_with("WithErrorLogging(CEventScope&, CString const&, char const*) const")
        })?);
    }
    let scope_accessors = scope_accessors(symbols)?;
    let mut error_logs = BTreeSet::new();
    for name in ERROR_LOGS {
        error_logs.insert(unique(symbols, name)?);
    }
    Ok(CommandBindings {
        reader_value_token_offset: recipe.persistent.value_token,
        assign_slot: slots.assign,
        validation_slot,
        target_getter_slot,
        boolean_tokens: [true_token?, false_token?],
        token_copy: required_signature(symbols, "CToken::CToken(CToken const&)")?,
        target_from_token: required_signature(
            symbols,
            "CEventTarget::CEventTarget(CToken, EScopeType, CString const&)",
        )?,
        target_move: unique(symbols, "CEventTarget::operator=(CEventTarget&&)")?,
        target_resolver: unique(
            symbols,
            "CEventTarget::GetScope(CEventScope&, char const*) const",
        )?,
        target_getters,
        scope_accessors,
        target_scope_type: unique(symbols, "CEventTarget::GetScopeType() const")?,
        operator_readers: [
            unique(symbols, "CAssignOperator::Read(CReader&)")?,
            unique(symbols, "CCompareOperator::Read(CReader&)")?,
        ],
        variable_assign: unique(
            symbols,
            "CVariableValue::Assign(CToken const&, EScopeType, CString const&)",
        )?,
        error_logs,
    })
}

/// Bind the typed accessors and the null-object pointer slot of their return type.
fn scope_accessors(symbols: &[Symbol]) -> Result<BTreeMap<u64, AccessorNullObject>, AnalysisError> {
    let mut accessors = BTreeMap::new();
    for symbol in symbols {
        let name = symbol.name.as_str();
        let null_object = if let Some(suffix) = name
            .strip_prefix("CScopeObjectReference::Get")
            .and_then(|suffix| suffix.strip_suffix("() const"))
        {
            match suffix {
                "LocalPointer" | "ColonyCarrierRef" | "OpenerID" | "ObjectName" => continue,
                "GrowthStage" | "GalacticCommunity" => AccessorNullObject::NoNullObject,
                "DlcRecommendation" => null_object(symbols, "SDlcRecommendationScriptData"),
                "Design" => null_object(symbols, "CShipDesign"),
                kind => null_object(symbols, &format!("C{kind}")),
            }
        } else if let Some((_, kind)) = name.split_once(" const* CScopeObjectReference::GetObject<")
        {
            let Some(kind) = kind.strip_suffix(">() const") else {
                continue;
            };
            if kind == "CGalacticCommunity" {
                AccessorNullObject::NoNullObject
            } else {
                null_object(symbols, kind)
            }
        } else {
            continue;
        };
        accessors.insert(symbol.address, null_object);
    }
    if accessors.is_empty() {
        return Err(InputError("no typed scope accessors".into()).into());
    }
    Ok(accessors)
}

/// The null object that an accessor of `kind` returns on a type mismatch, or `NoNullObject` when
/// the image has no single one.
fn null_object(symbols: &[Symbol], kind: &str) -> AccessorNullObject {
    let name = format!("TPdxNullObject<{kind}>::_pInstance");

    super::declarations::unique(symbols, &name)
        .map_or(AccessorNullObject::NoNullObject, AccessorNullObject::Global)
}

/// Constructors can have distinct complete-object and base-object entry points.
fn required_signature(symbols: &[Symbol], name: &str) -> Result<BTreeSet<u64>, AnalysisError> {
    required_matching(symbols, name, |candidate| candidate == name)
}

fn required_matching(
    symbols: &[Symbol],
    description: &str,
    matches: impl Fn(&str) -> bool,
) -> Result<BTreeSet<u64>, AnalysisError> {
    let addresses: BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| matches(&symbol.name))
        .map(|symbol| symbol.address)
        .collect();
    if addresses.is_empty() {
        return Err(InputError(format!("no symbol for {description}")).into());
    }
    Ok(addresses)
}

/// Preserve the matched initializer operations without changing reference answers.
pub(in crate::binding) fn initializers(
    input: &crate::engine::analysis::references::ReferenceInput,
    symbols: &[Symbol],
    qualified_references: &mut BTreeMap<u64, u64>,
) -> Result<BTreeMap<u64, crate::engine::analysis::grammar::forms::Initializer>, AnalysisError> {
    use crate::engine::analysis::{
        grammar::forms::{Initializer, LookupExecution},
        references::{
            self,
            initialization::{Execution, Initialization},
        },
    };
    let facts = references::analyze(input);
    for (name, fact) in &facts.readers {
        if fact.lookup.is_ok()
            && references::reader(name)
                .is_some_and(|reader| reader.form == references::ReaderForm::Deferred)
        {
            let null = fact
                .null_object
                .as_deref()
                .ok_or_else(|| InputError(format!("deferred null object missing for {name}")))?;
            let null = super::declarations::unique(symbols, null)?;
            qualified_references.extend(
                symbols
                    .iter()
                    .filter(|symbol| &symbol.name == name)
                    .map(|symbol| (symbol.address, null)),
            );
        }
    }
    let mut bound = BTreeMap::new();
    for (name, fact) in facts.initializers {
        let addresses: Vec<_> = symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.address)
            .collect();
        let initializer = match fact {
            Initialization::NoLookup => Initializer::NoLookup,
            Initialization::Lookup(lookup) => {
                let execution = match &lookup.execution {
                    Execution::Inline { null } => LookupExecution::Inline {
                        always: lookup.lookup.empty_key_looked_up,
                        null: super::declarations::unique(symbols, null)?,
                    },
                    Execution::Getter {
                        database,
                        getter,
                        null,
                    } => LookupExecution::Getter {
                        database: super::declarations::unique(symbols, database)?,
                        getter: super::declarations::unique(symbols, getter)?,
                        null: super::declarations::unique(symbols, null)?,
                    },
                };

                Initializer::Lookup { lookup, execution }
            }
            Initialization::Unresolved(_) => Initializer::Unresolved,
        };
        for address in addresses {
            bound.insert(address, initializer.clone());
        }
    }
    Ok(bound)
}

/// Bind executable routes, not an assumed input type set based on the getter name.
fn target_input(
    symbols: &[Symbol],
    recipe: &DeclarationRecipe,
    bindings: &CommandBindings,
) -> Result<crate::engine::analysis::grammar::targets::Input, AnalysisError> {
    let mut input = crate::engine::analysis::grammar::targets::Input {
        scope_type_offset: recipe.callbacks.scope_type_offset,
        scope_object_offset: recipe.scope_object_offset,
        ..Default::default()
    };
    input.nulls.extend(bindings.scope_accessors.clone());
    input.nulls.extend(getter_nulls(
        symbols,
        &bindings.target_getters,
        &bindings.scope_accessors,
    ));
    for symbol in symbols {
        if symbol.name.starts_with("CEventScope::GetTarget")
            || symbol.name.starts_with("CEventScope::GetScope")
            || symbol.name.starts_with("CEventScope::AccessTarget")
        {
            input.helpers.insert(symbol.address);
        }
    }
    Ok(input)
}

/// The rejection null object of each target getter: the null object of the accessor that the
/// getter's type names, or `NoNullObject` when no accessor is bound for that type.
fn getter_nulls(
    symbols: &[Symbol],
    target_getters: &BTreeSet<u64>,
    scope_accessors: &BTreeMap<u64, AccessorNullObject>,
) -> BTreeMap<u64, AccessorNullObject> {
    let accessor_null = |accessor: &str| {
        symbols
            .iter()
            .find(|entry| entry.name == accessor)
            .and_then(|entry| scope_accessors.get(&entry.address))
            .copied()
            .unwrap_or(AccessorNullObject::NoNullObject)
    };

    symbols
        .iter()
        .filter(|symbol| target_getters.contains(&symbol.address))
        .map(|symbol| {
            let kind = match getter_kind(&symbol.name) {
                "System" => "GalacticObject",
                kind => kind,
            };
            let accessor = format!("CScopeObjectReference::Get{kind}() const");

            (symbol.address, accessor_null(&accessor))
        })
        .collect()
}

/// The scope type that a target getter's name states, such as `Country` in
/// `CEventTarget::AccessTargetCountryWithErrorLogging(…)`.
fn getter_kind(name: &str) -> &str {
    let suffix = name.strip_prefix("CEventTarget::").unwrap_or(name);
    let kind = suffix
        .strip_prefix("AccessTarget")
        .or_else(|| suffix.strip_prefix("GetTarget"))
        .or_else(|| suffix.strip_prefix("GetScope"))
        .unwrap_or(suffix);
    let kind = kind.split(['(', '<']).next().unwrap_or(kind);

    kind.trim_end_matches("WithErrorLogging")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(name: &str, address: u64) -> Symbol {
        Symbol {
            name: name.into(),
            address,
        }
    }

    #[test]
    fn an_unfamiliar_accessor_is_a_typed_gap() {
        let symbols = [
            symbol("CScopeObjectReference::GetCountry() const", 0x1000),
            symbol("TPdxNullObject<CCountry>::_pInstance", 0x9000),
            symbol("CScopeObjectReference::GetNewThing() const", 0x1100),
            symbol("CEventTarget::GetScopeCountry(CEventScope&) const", 0x2000),
            symbol("CEventTarget::GetScopeNewThing(CEventScope&) const", 0x2100),
            symbol("CEventTarget::GetScopeUnbound(CEventScope&) const", 0x2200),
        ];

        let accessors = scope_accessors(&symbols).unwrap();
        let getters = BTreeSet::from([0x2000, 0x2100, 0x2200]);
        let nulls = getter_nulls(&symbols, &getters, &accessors);

        assert_eq!(accessors[&0x1000], AccessorNullObject::Global(0x9000));
        assert_eq!(accessors[&0x1100], AccessorNullObject::NoNullObject);
        assert_eq!(nulls[&0x2000], AccessorNullObject::Global(0x9000));
        assert_eq!(nulls[&0x2100], AccessorNullObject::NoNullObject);
        assert_eq!(nulls[&0x2200], AccessorNullObject::NoNullObject);
    }
}
