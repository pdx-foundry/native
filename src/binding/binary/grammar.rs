use super::super::targets::DeclarationRecipe;
use crate::AnalysisError;
use crate::engine::analysis::{
    InputError,
    declarations::{DeclarationInput, DeclarationResult, tail_callees},
    decode::decode_arm64,
    discovery::Symbol,
    fields,
    grammar::{AccessorNullObject, CommandBindings, GrammarInput},
};
use std::collections::{BTreeMap, BTreeSet};

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
    for (&address, body) in &mut declarations.functions {
        let length = text.function_length(address).min(65_536);
        if length > body.code.len() as u64 {
            body.code = text.bytes(address, length)?.to_vec();
        }
    }
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
    declarations.constructors = super::receivers::constructors(
        bytes,
        symbols,
        &declarations.pointers,
        bound_slots,
        &roots,
    )?;

    declarations.constructor_calls = super::receivers::constructor_calls(bytes, symbols)?;

    let token_start = super::declarations::unique(symbols, "GetTokenArray()")?;
    let (_, token_code) = text.function(token_start)?;
    let rows = decode_arm64(token_code, token_start).map_err(|_| AnalysisError::InvalidRange)?;
    let (tokens, _) = fields::recover_decoded(&rows, symbols, strings);
    let families = recipe
        .child_families
        .iter()
        .map(|&(name, family)| (name.into(), family))
        .collect();
    let numeric_decoder = super::declarations::unique(symbols, recipe.numeric_key_reader)?;
    let command_bindings = command_bindings(symbols, &tokens, recipe, kind)?;
    let mut form_input = crate::engine::analysis::grammar::forms::Input {
        reader_value_token_offset: Some(recipe.reader_value_token_offset),
        token_text_offset: recipe.token_text_offset,
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
        if text.starts.contains(&symbol.address) && text.function_length(symbol.address) > 65_536 {
            form_input.cut_bodies.insert(symbol.address);
        }
    }
    let targets = target_input(symbols, recipe, &command_bindings)?;
    let durations = super::durations::input(&text, symbols, names, form_input.role_slot);
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
        data: super::fields::read_only_data(bytes)?,
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
            .filter(|(_, token)| token.name == name && !token.ambiguous);
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
    for name in [
        "CLogger::Log(char const*, unsigned int, int)",
        "CLogStream::operator<<(char*)",
        "CLogStream::operator<<(char const*)",
        "CLogStream::operator<<(CString const&)",
        "CPdxLogFileAndLine::operator()(char const*, ...)",
    ] {
        error_logs.insert(unique(symbols, name)?);
    }
    Ok(CommandBindings {
        reader_value_token_offset: recipe.reader_value_token_offset,
        assign_slot: slots.assign,
        validation_slot,
        target_getter_slot,
        boolean_tokens: [true_token?, false_token?],
        token_copy: required_signature(symbols, "CToken::CToken(CToken const&)")?,
        target_from_token: required_signature(
            symbols,
            "CEventTarget::CEventTarget(CToken, EScopeType, CString const&)",
        )?,
        target_from_id: required_signature(symbols, "CEventTarget::CEventTarget(int)")?,
        target_create_from_token: unique(symbols, "CEventTarget::CreateFromToken(int)")?,
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
                "DlcRecommendation" => null_object(symbols, "SDlcRecommendationScriptData")?,
                "Design" => null_object(symbols, "CShipDesign")?,
                kind => null_object(symbols, &format!("C{kind}"))?,
            }
        } else if let Some((_, kind)) = name.split_once(" const* CScopeObjectReference::GetObject<")
        {
            let Some(kind) = kind.strip_suffix(">() const") else {
                continue;
            };
            if kind == "CGalacticCommunity" {
                AccessorNullObject::NoNullObject
            } else {
                null_object(symbols, kind)?
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

fn null_object(symbols: &[Symbol], kind: &str) -> Result<AccessorNullObject, AnalysisError> {
    let name = format!("TPdxNullObject<{kind}>::_pInstance");
    super::declarations::unique(symbols, &name).map(AccessorNullObject::Global)
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
            initialization::{self, Execution, Initialization},
        },
    };
    let facts = references::analyze(input);
    for (name, fact) in &facts.readers {
        if fact.lookup.is_ok()
            && references::reader(name)
                .is_some_and(|reader| reader.form == references::ReaderForm::Deferred)
        {
            let null = references::deferred_null(input, name)
                .ok_or_else(|| InputError(format!("deferred null object missing for {name}")))?;
            let null = super::declarations::unique(symbols, &null)?;
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
            Initialization::Lookup(lookup) => match initialization::execution(input, &name) {
                Some(Execution::Inline { always, null }) => Initializer::Lookup {
                    lookup,
                    execution: LookupExecution::Inline {
                        always,
                        null: super::declarations::unique(symbols, &null)?,
                    },
                },
                Some(Execution::Getter {
                    database,
                    getter,
                    null,
                }) => Initializer::Lookup {
                    lookup,
                    execution: LookupExecution::Getter {
                        database: super::declarations::unique(symbols, &database)?,
                        getter: super::declarations::unique(symbols, &getter)?,
                        null: super::declarations::unique(symbols, &null)?,
                    },
                },
                None => Initializer::Unresolved,
            },
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
    for symbol in symbols {
        if bindings.target_getters.contains(&symbol.address) {
            let suffix = symbol.name.strip_prefix("CEventTarget::").unwrap();
            let kind = suffix
                .strip_prefix("AccessTarget")
                .or_else(|| suffix.strip_prefix("GetTarget"))
                .or_else(|| suffix.strip_prefix("GetScope"))
                .unwrap();
            let kind = kind
                .split(['(', '<'])
                .next()
                .unwrap()
                .trim_end_matches("WithErrorLogging");
            let kind = match kind {
                "System" => "GalacticObject",
                kind => kind,
            };
            let accessor = format!("CScopeObjectReference::Get{kind}() const");
            let null = symbols
                .iter()
                .find(|entry| entry.name == accessor)
                .and_then(|entry| bindings.scope_accessors.get(&entry.address))
                .copied()
                .ok_or_else(|| {
                    InputError(format!(
                        "target getter has no accessor binding: {}",
                        symbol.name
                    ))
                })?;
            input.nulls.insert(symbol.address, null);
        }
        if symbol.name.starts_with("CEventScope::GetTarget")
            || symbol.name.starts_with("CEventScope::GetScope")
            || symbol.name.starts_with("CEventScope::AccessTarget")
        {
            input.helpers.insert(symbol.address);
        }
    }
    Ok(input)
}
