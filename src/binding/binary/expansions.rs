//! The executable evidence of the script expansion method: the engine functions of each
//! mechanism, the classified callers of each, the call chains of its stage and the message sites
//! of its checks.
//!
//! **Recorded manual exception (stated forms).** Claim: the call forms and parameter forms in
//! [`stated_forms`], and an inline script's absent parameter kept as text. Conditions: the
//! functions that hold the tests (`STATED_CONDITIONS`) are bound on the build; otherwise the
//! properties are unresolved. Obstacle: the forms are character and token-kind tests inside one
//! scanner or reader loop, which no method reads. Removal route: a scanner-shape reader.
//! `docs/native/script-expansion.md` records the fixture rows that check each form.
use std::collections::{BTreeMap, BTreeSet};

use super::declarations::{Text, addresses};
use crate::AnalysisError;
use crate::binding::analysis::NamedCandidate;
use crate::engine::analysis::decode::{Instruction, decode_arm64};
use crate::engine::analysis::directories::{self, Anchors, Constructor, Directory};
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::expansions::{
    DefinitionInput, ExpansionInput, LinkCaller, MechanismInput, MessageSite, MissingInput,
    StageInput, StatedForms, UseCall, UseRole,
};
use crate::{
    BlockFamily, CallForm, ExpansionCheck, ExpansionHost, ExpansionMechanism, ExpansionStage,
    MissingParameter, ParameterForm,
};

const INLINE_READER: &str = "CreateInlineScriptReader(CReader&)";
const INLINE_DATABASE_INIT: &str = "CInlineScriptDatabase::Init()";
const INLINE_PARAMETERS: &str = "CInlineScriptParameters::ReadMember(CReader&, int)";

const GENERATE_SOURCE: &str = "CMetaScriptTemplate::GenerateSource(CPdxArray<std::__1::pair<CString, CString>, int> const&, CString&) const";
const VALIDATE_ARGUMENTS: &str = "CMetaScriptTemplate::ValidateArguments(CPdxArray<std::__1::pair<CString, CString>, int> const&) const";
const PARSE_ARGUMENTS: &str = "CMetaScriptTemplate::ParseForArguments(CString const&, CPdxArray<CMetaScriptTemplate::SArgumentData, int>&, int, bool) const";
const PROCESS_MACROS: &str = "CMetaScriptTemplate::ProcessSourceForMacros(CString&, CPdxArray<std::__1::pair<CString, CString>, int> const&) const";
const GENERATION_ERROR: &str = "CScriptedEffect::OnError(CString const&) const";
const CONSOLE_SCRIPT_VALUE: &str =
    "OnExecute_EvaluateScriptedValue(CPdxArray<CString, int> const&)";

const STATEMENT_READER: &str = "CReader::ReadSimpleStatement()";
const REGISTER_VARIABLE: &str =
    "CReader::RegisterVariableInList(CPdxArray<CReader::SReaderVariable, int>&)";
const VARIABLE_EXPRESSION: &str =
    "CReader::ParseAdvancedStatementWithVariables(CPdxArray<CReader::SReaderVariable, int> const&)";
/// The class of the shared reader: every function of it reads script statements.
const SHARED_READER: &str = "CReader::";
const GLOBAL_VARIABLES: &str =
    "CGlobalScriptedVariablesDatabase::InitializeWithDirectory(CString const&)";
const GAME_INIT: &str = "CGameApplication::InitGame()";

const LOAD_FROM_READER: &str = "::LoadFromReader(CReader&, bool)";
const LOAD_FILE: &str = "::LoadFile(char const*, bool)";
const OPERATOR_NEW: &str = "operator new(unsigned long)";

/// Functions that log a message, or build its text, from a literal in `x1`.
const MESSAGE_FUNCTIONS: [&str; 5] = [
    "CPdxLogFileAndLine::operator()(char const*, ...)",
    "CLogStream::operator<<(char const*)",
    "PdxStrFmt<512>::PdxStrFmt(char const*, ...)",
    "CString::CString(char const*)",
    "CString::operator+=(char const*)",
];

/// Functions whose own text holds the stated forms.
const STATED_CONDITIONS: [&str; 4] = [
    PARSE_ARGUMENTS,
    PROCESS_MACROS,
    INLINE_PARAMETERS,
    STATEMENT_READER,
];

/// Readers of script content, by the host that they read.
const HOST_READERS: [(&str, ExpansionHost); 8] = [
    (
        "CEffect::Read(CReader&, EScopeType)",
        ExpansionHost::Commands(BlockFamily::Effect),
    ),
    (
        "CTrigger::Read(CReader&, EScopeType)",
        ExpansionHost::Commands(BlockFamily::Trigger),
    ),
    (
        "CEffect::ReadMember(CReader&, int, EScopeType)",
        ExpansionHost::Commands(BlockFamily::Effect),
    ),
    (
        "CTriggerCollectionBase::ReadMember(CReader&, int, EScopeType)",
        ExpansionHost::Commands(BlockFamily::Trigger),
    ),
    (
        "CPersistent::ReadWithoutInitPostRead(CReader&)",
        ExpansionHost::ObjectBlock,
    ),
    (
        "CVariableValue::ReadTriggerModifierOrScriptValue(CString&, EScopeType)",
        ExpansionHost::ScopedOperand,
    ),
    (
        "CComplexTriggerMTTHModifier::ReadMember(CReader&, int)",
        ExpansionHost::TriggerReference,
    ),
    (
        "CExportTriggerValueToVariableEffect::ReadMember(CReader&, int, EScopeType)",
        ExpansionHost::TriggerReference,
    ),
];

/// How many helpers a use can pass through before its reader.
const HELPER_DEPTH: usize = 2;

/// Functions that pass a use on for their own callers: each caller is the reader.
const HELPERS: [&str; 2] = [
    "CTriggerDatabase::CreateTriggerOrScriptedPlaceholder(int, CString const&, CString const&) const",
    "CTriggerDatabase::CreateTriggerOrScriptedPlaceholder(CToken const&, CString const&) const",
];

/// Callers that read no script content: save-game instance readers and a console command.
const NOT_SCRIPT: [&str; 3] = [
    "CScriptedInstanceBase<CSharedScriptedEffect>::CSerializer::Read(CReader&)",
    "CScriptedInstanceBase<CSharedScriptedTrigger>::CSerializer::Read(CReader&)",
    CONSOLE_SCRIPT_VALUE,
];

/// The engine functions of one template mechanism.
struct Template {
    mechanism: ExpansionMechanism,
    database: &'static str,
    placeholders: &'static [&'static str],
    build: &'static str,
    post_init: &'static str,
    unknown_name: (&'static str, &'static str),
    depth_limit: (&'static str, &'static str),
}

const TEMPLATES: [Template; 3] = [
    Template {
        mechanism: ExpansionMechanism::ScriptedEffect,
        database: "CScriptedEffectTemplateDatabase",
        placeholders: &["CScriptedEffect::CScriptedEffect(CString const&)"],
        build: "CScriptedEffect::BuildFromSource()",
        post_init: "CScriptedEffect::PostInit()",
        unknown_name: (
            "CScriptedEffect::PostValidate() const",
            "Script Error: Invalid scripted effect: ",
        ),
        depth_limit: (
            "CEffectDatabase::PostInit()",
            "CRITICAL: Max effects post init recursive depth",
        ),
    },
    Template {
        mechanism: ExpansionMechanism::ScriptedTrigger,
        database: "CScriptedTriggerTemplateDatabase",
        placeholders: &["CScriptedTrigger::CScriptedTrigger(CString const&)"],
        build: "CScriptedTrigger::BuildFromSource()",
        post_init: "CScriptedTrigger::PostInit()",
        unknown_name: (
            "CScriptedTrigger::PostValidate() const",
            "[%s]: Error in scripted trigger, cannot find: %s\n",
        ),
        depth_limit: (
            "CTriggerDatabase::PostInit()",
            "CRITICAL: Max triggers post init recursive depth",
        ),
    },
    Template {
        mechanism: ExpansionMechanism::ScriptValue,
        database: "CScriptValueTemplateDatabase",
        placeholders: &[
            "CScriptableLookupValue::CScriptableLookupValue(CString const&, CPdxArray<std::__1::pair<CString, CString>, int> const&, EScopeType)",
            "CScriptableLookupValue::CScriptableLookupValue(CString const&, EScopeType)",
        ],
        build: "CScriptableLookupValue::BuildFromSource()",
        post_init: "CScriptableLookupValue::PostInit()",
        unknown_name: (
            "CScriptableLookupValue::PostInit()",
            "Script Error: Invalid script value: %s at %s",
        ),
        depth_limit: (
            "CScriptableLookupValueDatabase::PostInit()",
            "CRITICAL: Max script values post init recursive depth",
        ),
    },
];

/// The forms that the build states for each mechanism.
fn stated_forms(mechanism: ExpansionMechanism) -> StatedForms {
    use ParameterForm::*;

    let template = vec![Substitution, Default, Conditional, NegatedConditional];
    let (call_forms, parameter_forms) = match mechanism {
        ExpansionMechanism::InlineScript => (
            vec![CallForm::Block {
                name_key: Some("script".into()),
            }],
            vec![Substitution],
        ),
        ExpansionMechanism::ScriptValue => (vec![CallForm::Pipe], template),
        ExpansionMechanism::ScriptedVariable => {
            (vec![CallForm::Variable, CallForm::Arithmetic], vec![])
        }
        _ => (
            vec![CallForm::Value, CallForm::Block { name_key: None }],
            template,
        ),
    };

    StatedForms {
        call_forms,
        parameter_forms,
    }
}

/// Read the evidence of every mechanism.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    strings: &BTreeMap<u64, String>,
    candidates: &[NamedCandidate],
) -> Result<ExpansionInput, AnalysisError> {
    let image = Image::read(bytes, symbols, strings, candidates)?;
    let bound = STATED_CONDITIONS
        .iter()
        .all(|name| !image.addresses(name).is_empty());
    let stated = |mechanism| {
        bound
            .then(|| stated_forms(mechanism))
            .ok_or("stated-form-functions")
    };

    let mut mechanisms = vec![inline_script(
        &image,
        stated(ExpansionMechanism::InlineScript),
    )?];
    for template in &TEMPLATES {
        mechanisms.push(template_mechanism(
            &image,
            template,
            stated(template.mechanism),
        )?);
    }
    mechanisms.push(scripted_variable(
        &image,
        stated(ExpansionMechanism::ScriptedVariable),
    )?);

    Ok(ExpansionInput {
        mechanisms,
        message_functions: MESSAGE_FUNCTIONS
            .iter()
            .flat_map(|name| image.addresses(name))
            .collect(),
        allocations: image.addresses(OPERATOR_NEW),
    })
}

fn inline_script(
    image: &Image,
    stated: Result<StatedForms, &'static str>,
) -> Result<MechanismInput, AnalysisError> {
    let uses = image.uses(&[INLINE_READER])?;
    let read_link = uses
        .iter()
        .map(|use_call| match use_call.role {
            UseRole::Host(_) => LinkCaller::Expected,
            _ => LinkCaller::Allowed,
        })
        .collect();

    Ok(MechanismInput {
        mechanism: ExpansionMechanism::InlineScript,
        definitions: vec![DefinitionInput::Directory(
            image.enumerated_directory(INLINE_DATABASE_INIT)?,
        )],
        uses,
        placeholder: false,
        stage: StageInput {
            stage: ExpansionStage::Read,
            links: vec![read_link],
        },
        checks: vec![(
            ExpansionCheck::UnknownName,
            image.message(INLINE_READER, "Unknown inline_script \"")?,
        )],
        missing_parameter: MissingInput::Stated(MissingParameter::KeptAsText),
        stated,
    })
}

fn template_mechanism(
    image: &Image,
    template: &Template,
    stated: Result<StatedForms, &'static str>,
) -> Result<MechanismInput, AnalysisError> {
    let other_builds = TEMPLATES
        .iter()
        .map(|other| other.build)
        .filter(|build| *build != template.build)
        .chain([GENERATION_ERROR]);
    let generation = image.link(
        GENERATE_SOURCE,
        &[template.build],
        &other_builds.collect::<Vec<_>>(),
    );
    let build = image.link(
        template.build,
        &[template.post_init],
        &[CONSOLE_SCRIPT_VALUE],
    );

    Ok(MechanismInput {
        mechanism: template.mechanism,
        definitions: vec![DefinitionInput::Directory(
            image.registry_directory(template.database),
        )],
        uses: image.uses(template.placeholders)?,
        placeholder: true,
        stage: StageInput {
            stage: ExpansionStage::Compile,
            links: vec![generation, build],
        },
        checks: vec![
            (
                ExpansionCheck::UnknownName,
                image.message(template.unknown_name.0, template.unknown_name.1)?,
            ),
            (
                ExpansionCheck::DepthLimit,
                image.message(template.depth_limit.0, template.depth_limit.1)?,
            ),
        ],
        missing_parameter: MissingInput::Logged(
            image.message(VALIDATE_ARGUMENTS, " failed for missing args: ")?,
        ),
        stated,
    })
}

fn scripted_variable(
    image: &Image,
    stated: Result<StatedForms, &'static str>,
) -> Result<MechanismInput, AnalysisError> {
    let registration = image.reader_link(REGISTER_VARIABLE);
    let expression = image.reader_link(VARIABLE_EXPRESSION);
    let same_file = registration.contains(&LinkCaller::Expected);
    let uses = image
        .calls(VARIABLE_EXPRESSION)
        .into_iter()
        .map(|caller| UseCall {
            role: if image.in_shared_reader(caller.function) {
                UseRole::Host(ExpansionHost::Statement)
            } else {
                UseRole::Unjoined
            },
            rows: Vec::new(),
        })
        .collect();

    Ok(MechanismInput {
        mechanism: ExpansionMechanism::ScriptedVariable,
        definitions: vec![
            DefinitionInput::SameFile(same_file),
            DefinitionInput::Directory(image.argument_directory(GAME_INIT, GLOBAL_VARIABLES)?),
        ],
        uses,
        placeholder: false,
        stage: StageInput {
            stage: ExpansionStage::Lex,
            links: vec![registration, expression],
        },
        checks: vec![
            (
                ExpansionCheck::InvalidVariableName,
                image.message(REGISTER_VARIABLE, "Invalid variable name [%s]. Variable names must start with a letter and only contain letters, numbers, or under scores. %s")?,
            ),
            (
                ExpansionCheck::DuplicateVariable,
                image.message(REGISTER_VARIABLE, "Variable name %s is already taken. %s")?,
            ),
        ],
        missing_parameter: MissingInput::NoParameters,
        stated,
    })
}

/// One direct call or tail call, and the function that holds it.
struct Caller {
    site: u64,
    function: u64,
}

/// The executable text with its symbols and the registries' readers.
struct Image<'a> {
    text: Text<'a>,
    symbols: &'a [Symbol],
    strings: &'a BTreeMap<u64, String>,
    names: BTreeMap<u64, Vec<&'a str>>,
    /// The content directory of each named registry, by its `LoadFromReader` symbol.
    registry_readers: BTreeMap<String, String>,
    /// The content directory of each named registry, by its database class.
    registry_databases: BTreeMap<&'a str, Directory>,
}

impl<'a> Image<'a> {
    fn read(
        bytes: &'a [u8],
        symbols: &'a [Symbol],
        strings: &'a BTreeMap<u64, String>,
        candidates: &'a [NamedCandidate],
    ) -> Result<Self, AnalysisError> {
        let mut names: BTreeMap<u64, Vec<&str>> = BTreeMap::new();
        for symbol in symbols {
            names.entry(symbol.address).or_default().push(&symbol.name);
        }

        let registry_readers = candidates
            .iter()
            .filter_map(|candidate| {
                let Directory::Named(directory) = &candidate.directory else {
                    return None;
                };
                let database = candidate.record.loader.strip_suffix(LOAD_FILE)?;
                Some((format!("{database}{LOAD_FROM_READER}"), directory.clone()))
            })
            .collect();
        let registry_databases = candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.record.database.as_str(),
                    candidate.directory.clone(),
                )
            })
            .collect();

        Ok(Self {
            text: Text::read(bytes, symbols)?,
            symbols,
            strings,
            names,
            registry_readers,
            registry_databases,
        })
    }

    fn addresses(&self, name: &str) -> BTreeSet<u64> {
        addresses(self.symbols, name)
    }

    /// Whether the function at `function` has one of `names`.
    fn named(&self, function: u64, names: &[&str]) -> bool {
        self.names
            .get(&function)
            .is_some_and(|aliases| aliases.iter().any(|alias| names.contains(alias)))
    }

    /// Every direct call or tail call of a function named `name`.
    fn callers(&self, name: &str) -> Vec<Caller> {
        let sites = self.text.calls_into(&self.addresses(name));

        self.holding(sites.into_iter().map(|(site, _)| site))
    }

    /// Every direct call of a function named `name`. A tail call, such as one constructor of a
    /// class that continues in another, is not a use.
    fn calls(&self, name: &str) -> Vec<Caller> {
        let sites = self
            .addresses(name)
            .into_iter()
            .flat_map(|target| self.text.direct_calls(target));

        self.holding(sites)
    }

    /// Each site with the function that holds it: the nearest symbol at or before the site.
    fn holding(&self, sites: impl Iterator<Item = u64>) -> Vec<Caller> {
        sites
            .filter_map(|site| {
                let function = *self.text.starts.range(..=site).next_back()?;
                Some(Caller { site, function })
            })
            .collect()
    }

    fn rows(&self, function: u64) -> Result<Vec<Instruction>, AnalysisError> {
        let (start, code) = self.text.function(function)?;
        decode_arm64(code, start).map_err(|_| AnalysisError::InvalidRange)
    }

    /// Each use of the functions named `targets`, with the role of the reader behind each call.
    fn uses(&self, targets: &[&str]) -> Result<Vec<UseCall>, AnalysisError> {
        let mut uses = Vec::new();
        for target in targets {
            for caller in self.calls(target) {
                let rows: Vec<_> = self
                    .rows(caller.function)?
                    .into_iter()
                    .take_while(|row| row.address <= caller.site)
                    .collect();
                for role in self.roles(caller.function, 0) {
                    uses.push(UseCall {
                        role,
                        rows: rows.clone(),
                    });
                }
            }
        }

        Ok(uses)
    }

    /// The role of the reader at `function`; a helper gives the roles of its callers.
    fn roles(&self, function: u64, depth: usize) -> Vec<UseRole> {
        if self.named(function, &HELPERS) && depth < HELPER_DEPTH {
            let names = self.names.get(&function).cloned().unwrap_or_default();
            return names
                .iter()
                .flat_map(|name| self.calls(name))
                .flat_map(|caller| self.roles(caller.function, depth + 1))
                .collect();
        }

        vec![self.role(function)]
    }

    fn role(&self, function: u64) -> UseRole {
        let aliases = self.names.get(&function).cloned().unwrap_or_default();
        for alias in aliases {
            if let Some((_, host)) = HOST_READERS.iter().find(|(name, _)| *name == alias) {
                return UseRole::Host(host.clone());
            }
            if let Some(registry) = self.registry_readers.get(alias) {
                return UseRole::Host(ExpansionHost::RegistryRoot {
                    registry: registry.clone(),
                });
            }
            if NOT_SCRIPT.contains(&alias) {
                return UseRole::Excluded;
            }
        }

        UseRole::Unjoined
    }

    /// The classified direct callers of a function named `name`. Empty when it is not bound.
    fn link(&self, name: &str, expected: &[&str], allowed: &[&str]) -> Vec<LinkCaller> {
        self.callers(name)
            .into_iter()
            .map(|caller| {
                if self.named(caller.function, expected) {
                    LinkCaller::Expected
                } else if self.named(caller.function, allowed) {
                    LinkCaller::Allowed
                } else {
                    LinkCaller::Other
                }
            })
            .collect()
    }

    /// Whether the function at `function` belongs to the shared reader class.
    fn in_shared_reader(&self, function: u64) -> bool {
        self.names
            .get(&function)
            .is_some_and(|aliases| aliases.iter().any(|alias| alias.contains(SHARED_READER)))
    }

    /// The direct callers of a function named `name` in a lexing chain: the statement reader is
    /// expected, another function of the shared reader is allowed, and any other is unclassified.
    fn reader_link(&self, name: &str) -> Vec<LinkCaller> {
        self.callers(name)
            .into_iter()
            .map(|caller| {
                if self.named(caller.function, &[STATEMENT_READER]) {
                    LinkCaller::Expected
                } else if self.in_shared_reader(caller.function) {
                    LinkCaller::Allowed
                } else {
                    LinkCaller::Other
                }
            })
            .collect()
    }

    /// Where a function named `function` forms the one literal that starts with `message`. A
    /// missing function, or no single such literal, gives a site that proves nothing.
    fn message(&self, function: &str, message: &str) -> Result<MessageSite, AnalysisError> {
        let mut literals = self
            .strings
            .iter()
            .filter(|(_, text)| text.starts_with(message))
            .map(|(address, _)| *address);
        let literal = match (literals.next(), literals.next()) {
            (Some(address), None) => address,
            _ => 0,
        };
        let rows = match self.addresses(function).first() {
            Some(entry) => self.rows(*entry)?,
            None => Vec::new(),
        };

        Ok(MessageSite { rows, literal })
    }

    fn registry_directory(&self, database: &str) -> Directory {
        self.registry_databases
            .get(database)
            .cloned()
            .unwrap_or(Directory::Missing)
    }

    fn body(&self, name: &str) -> Result<Vec<Constructor>, AnalysisError> {
        self.addresses(name)
            .into_iter()
            .map(|entry| {
                let (address, code) = self.text.function(entry)?;
                Ok(Constructor {
                    address,
                    code: code.to_vec(),
                })
            })
            .collect()
    }

    /// The directory that the function named `loader` enumerates.
    fn enumerated_directory(&self, loader: &str) -> Result<Directory, AnalysisError> {
        let anchors = Anchors {
            file_enumerations: self.addresses(directories::FILE_ENUMERATION),
            ..Anchors::default()
        };

        Ok(directories::loader_directory(
            &self.body(loader)?,
            &anchors,
            self.strings,
        ))
    }

    /// The directory that the function named `caller` passes to the function named `callee` as
    /// a `CString` built from a literal.
    fn argument_directory(&self, caller: &str, callee: &str) -> Result<Directory, AnalysisError> {
        let anchors = Anchors {
            base_constructors: self.addresses(callee),
            string_constructors: self.addresses(directories::STRING_CONSTRUCTOR),
            file_enumerations: BTreeSet::new(),
        };
        let arguments: Vec<_> = self
            .body(caller)?
            .iter()
            .flat_map(|body| directories::arguments(body, &anchors, self.strings))
            .collect();

        Ok(directories::directory(&arguments, &BTreeMap::new()))
    }
}
