//! Script expansion: where script can use each text-reuse mechanism, where its names are defined,
//! when the engine expands a use, and the diagnostics that it logs for one.
//!
//! The binding names the engine functions of each mechanism and classifies every direct caller
//! of the function that expands a use, or that constructs a use's placeholder. The method then
//! decides each property from that evidence:
//!
//! - **Hosts.** Each classified caller gives a host. A placeholder counts only when its
//!   constructor receives the object that `operator new` returned just before, so that a call on
//!   another object is not a use. A caller that the binding cannot join to script content keeps
//!   the hosts partial.
//! - **Stage.** A stage holds when each link of its call chain is reached from an expected
//!   function and from no unclassified one. For a placeholder, the chain is the generation call
//!   that the placeholder's post-initialization makes after all content loads.
//! - **Checks.** A check holds when its message literal is the message argument (`x1`) of the
//!   next call, and that call is a bound message function. A literal that is only referenced is
//!   not a check.
//! - **Definitions.** Each directory comes from the directory scan; more than one source keeps
//!   the lookup order unestablished.
//!
//! Call forms, parameter forms, and an inline script's absent parameter are stated per build:
//! they are character and token-kind tests inside one scanner or reader loop, which no method
//! reads. The binding supplies them only when the functions that hold the tests are bound.
//!
//! Outside the method: which duplicate definition wins, cycle behavior, whether a use resolves a
//! definition that loads after it, and runtime evaluation.
use std::collections::BTreeSet;

use super::declarations::number;
use super::decode::{Instruction, is_control_transfer};
use super::directories::Directory;
use super::stop::Unresolved;
use crate::{
    CallForm, ExpansionCheck, ExpansionDefinitions, ExpansionHost, ExpansionMechanism,
    ExpansionStage, GrammarProperty, MissingParameter, ParameterForm, ScriptExpansion,
};

#[cfg(test)]
mod tests;

/// Name and revision of the script expansion method.
pub const METHOD: &str = "script-expansions/v1";

/// Executable-derived input of the script expansion method.
pub struct ExpansionInput {
    pub mechanisms: Vec<MechanismInput>,
    /// Entries of the functions that log a message or build its text from a literal argument.
    pub message_functions: BTreeSet<u64>,
    /// Entries of `operator new`.
    pub allocations: BTreeSet<u64>,
}

/// The evidence for one mechanism.
pub struct MechanismInput {
    pub mechanism: ExpansionMechanism,
    pub definitions: Vec<DefinitionInput>,
    /// Each direct call of the function that expands a use or constructs its placeholder.
    pub uses: Vec<UseCall>,
    /// Whether a use constructs a placeholder object that a later stage expands.
    pub placeholder: bool,
    pub stage: StageInput,
    pub checks: Vec<(ExpansionCheck, MessageSite)>,
    pub missing_parameter: MissingInput,
    /// The stated forms, or why the functions that hold them are not bound.
    pub stated: Result<StatedForms, &'static str>,
}

/// One source of a mechanism's names.
pub enum DefinitionInput {
    Directory(Directory),
    /// Whether the shared statement reader registers a definition in the reading file's own list.
    SameFile(bool),
}

/// One direct call of a mechanism's expanding or constructing function.
pub struct UseCall {
    pub role: UseRole,
    /// The caller's rows from its entry through the call.
    pub rows: Vec<Instruction>,
}

/// What the binding knows of a caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseRole {
    /// A reader of script content at this host.
    Host(ExpansionHost),
    /// A reader of script content whose content is not named.
    Unjoined,
    /// Not a reader of script content, such as a console command or a save-game reader.
    Excluded,
}

/// The call chain that places a mechanism's expansion at a stage.
pub struct StageInput {
    pub stage: ExpansionStage,
    /// Each link: the classified direct callers of one function of the chain. Empty when a
    /// function of the chain is not bound.
    pub links: Vec<Vec<LinkCaller>>,
}

/// One direct caller of a function in a stage chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkCaller {
    /// The function that the stage requires.
    Expected,
    /// A known caller that does not change the stage, such as an error path.
    Allowed,
    /// A caller that the binding does not classify.
    Other,
}

/// Where a function forms a message literal.
pub struct MessageSite {
    /// The function's rows; empty when the function is not bound.
    pub rows: Vec<Instruction>,
    /// Address of the message literal.
    pub literal: u64,
}

/// What a use that does not give a parameter yields.
pub enum MissingInput {
    NoParameters,
    /// The engine logs this message for the use.
    Logged(MessageSite),
    /// Stated per build, under the same condition as the stated forms.
    Stated(MissingParameter),
}

/// Forms that the build states, because they are character tests that no method reads.
#[derive(Debug, Clone)]
pub struct StatedForms {
    pub call_forms: Vec<CallForm>,
    pub parameter_forms: Vec<ParameterForm>,
}

/// The expansions, and why a property is not established.
pub struct ExpansionResult {
    pub expansions: Vec<ScriptExpansion>,
    pub gaps: Vec<ExpansionGap>,
}

/// A property of one mechanism that the method could not establish, or did not establish
/// completely.
#[derive(Debug)]
pub struct ExpansionGap {
    pub mechanism: ExpansionMechanism,
    pub property: Property,
    pub cause: Unresolved,
}

/// A property of [`ScriptExpansion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Property {
    Definitions,
    Hosts,
    Stage,
    Forms,
    MissingParameter,
    Checks,
}

/// Decide every property of every mechanism.
pub fn analyze(input: &ExpansionInput) -> ExpansionResult {
    let mut gaps = Vec::new();
    let expansions = input
        .mechanisms
        .iter()
        .map(|mechanism| {
            let mut gap = |property, cause| {
                gaps.push(ExpansionGap {
                    mechanism: mechanism.mechanism,
                    property,
                    cause,
                })
            };
            expansion(input, mechanism, &mut gap)
        })
        .collect();

    ExpansionResult { expansions, gaps }
}

fn expansion(
    input: &ExpansionInput,
    mechanism: &MechanismInput,
    gap: &mut impl FnMut(Property, Unresolved),
) -> ScriptExpansion {
    let (call_forms, parameter_forms) = match &mechanism.stated {
        Ok(stated) => (
            GrammarProperty::Known(stated.call_forms.clone()),
            GrammarProperty::Known(stated.parameter_forms.clone()),
        ),
        Err(reason) => {
            gap(Property::Forms, Unresolved::new(reason));
            (GrammarProperty::Unresolved, GrammarProperty::Unresolved)
        }
    };

    ScriptExpansion {
        mechanism: mechanism.mechanism,
        definitions: definitions(&mechanism.definitions, gap),
        hosts: hosts(mechanism, &input.allocations, gap),
        call_forms,
        stage: stage(&mechanism.stage, gap),
        parameter_forms,
        missing_parameter: missing_parameter(mechanism, &input.message_functions, gap),
        checks: checks(&mechanism.checks, &input.message_functions, gap),
    }
}

fn definitions(
    sources: &[DefinitionInput],
    gap: &mut impl FnMut(Property, Unresolved),
) -> GrammarProperty<Vec<ExpansionDefinitions>> {
    let mut found = Vec::new();

    for source in sources {
        match source {
            DefinitionInput::Directory(Directory::Named(directory)) => {
                found.push(ExpansionDefinitions::Directory {
                    directory: directory.clone(),
                });
            }
            DefinitionInput::SameFile(true) => found.push(ExpansionDefinitions::SameFile),
            DefinitionInput::Directory(_) => {
                gap(
                    Property::Definitions,
                    Unresolved::new("definition-directory"),
                );
            }
            DefinitionInput::SameFile(false) => {
                gap(Property::Definitions, Unresolved::new("file-definitions"));
            }
        }
    }

    if found.is_empty() {
        return GrammarProperty::Unresolved;
    }
    if found.len() < sources.len() {
        return GrammarProperty::Partial(found);
    }
    if found.len() > 1 {
        gap(Property::Definitions, Unresolved::new("lookup-order"));
        return GrammarProperty::Partial(found);
    }

    GrammarProperty::Known(found)
}

fn hosts(
    mechanism: &MechanismInput,
    allocations: &BTreeSet<u64>,
    gap: &mut impl FnMut(Property, Unresolved),
) -> GrammarProperty<Vec<ExpansionHost>> {
    let mut found = BTreeSet::new();
    let mut unproved = false;
    let mut unjoined = false;

    for use_call in &mechanism.uses {
        let host = match &use_call.role {
            UseRole::Excluded => continue,
            UseRole::Unjoined => None,
            UseRole::Host(host) => Some(host),
        };
        if mechanism.placeholder && !constructs_new_object(&use_call.rows, allocations) {
            unproved = true;
            continue;
        }

        match host {
            Some(host) => {
                found.insert(host.clone());
            }
            None => unjoined = true,
        }
    }

    if unproved {
        gap(Property::Hosts, Unresolved::new("placeholder-object"));
    }
    if unjoined {
        gap(Property::Hosts, Unresolved::new("unjoined-reader"));
    }
    if found.is_empty() {
        gap(Property::Hosts, Unresolved::new("no-host"));
        return GrammarProperty::Unresolved;
    }

    let found = found.into_iter().collect();
    if unproved || unjoined {
        GrammarProperty::Partial(found)
    } else {
        GrammarProperty::Known(found)
    }
}

/// Whether the last call in `rows`, a constructor, receives in `x0` the object that the nearest
/// earlier call of `operator new` returned. The scan follows register moves; any other call
/// clears the caller-saved registers, and any other write clears its destination.
fn constructs_new_object(rows: &[Instruction], allocations: &BTreeSet<u64>) -> bool {
    let Some((constructor, before)) = rows.split_last() else {
        return false;
    };
    if constructor.operation != "bl" {
        return false;
    }

    let mut holding: BTreeSet<String> = BTreeSet::new();
    for row in before {
        let operands: Vec<String> = row.operands.split(',').map(register).collect();

        match row.operation.as_str() {
            "bl" if number(&row.operands).is_some_and(|to| allocations.contains(&to)) => {
                holding = BTreeSet::from(["x0".to_owned()]);
            }
            "bl" | "blr" => holding.retain(|name| callee_saved(name)),
            "mov" if operands.len() == 2 && holding.contains(&operands[1]) => {
                holding.insert(operands[0].clone());
            }
            operation if writes_first_operand(operation) => {
                if let Some(written) = operands.first() {
                    holding.remove(written);
                }
            }
            _ => {}
        }
    }

    holding.contains("x0")
}

/// The 64-bit name of a general register operand, so that `w0` and `x0` are one register.
fn register(operand: &str) -> String {
    match operand.strip_prefix('w') {
        Some(number) if number.parse::<u32>().is_ok() => format!("x{number}"),
        _ => operand.to_owned(),
    }
}

/// Whether an instruction writes its first operand. Stores, compares, tests and branches only
/// read it.
fn writes_first_operand(operation: &str) -> bool {
    !operation.starts_with("st")
        && !is_control_transfer(operation)
        && !matches!(operation, "cmp" | "cmn" | "tst" | "ccmp" | "ccmn")
}

/// Whether a call keeps `register`: `x19` to `x28`.
fn callee_saved(register: &str) -> bool {
    register
        .strip_prefix('x')
        .and_then(|number| number.parse::<u32>().ok())
        .is_some_and(|number| (19..=28).contains(&number))
}

fn stage(
    input: &StageInput,
    gap: &mut impl FnMut(Property, Unresolved),
) -> GrammarProperty<ExpansionStage> {
    let holds = !input.links.is_empty()
        && input.links.iter().all(|callers| {
            callers.contains(&LinkCaller::Expected) && !callers.contains(&LinkCaller::Other)
        });
    if !holds {
        gap(Property::Stage, Unresolved::new("stage-chain"));
        return GrammarProperty::Unresolved;
    }

    GrammarProperty::Known(input.stage)
}

fn missing_parameter(
    mechanism: &MechanismInput,
    message_functions: &BTreeSet<u64>,
    gap: &mut impl FnMut(Property, Unresolved),
) -> GrammarProperty<Option<MissingParameter>> {
    match &mechanism.missing_parameter {
        MissingInput::NoParameters => GrammarProperty::Known(None),
        MissingInput::Logged(site) if logs(site, message_functions) => {
            GrammarProperty::Known(Some(MissingParameter::Diagnostic))
        }
        MissingInput::Logged(_) => {
            gap(
                Property::MissingParameter,
                Unresolved::new("message-argument"),
            );
            GrammarProperty::Unresolved
        }
        // An unbound stated rule already gives its gap with the forms.
        MissingInput::Stated(missing) if mechanism.stated.is_ok() => {
            GrammarProperty::Known(Some(*missing))
        }
        MissingInput::Stated(_) => GrammarProperty::Unresolved,
    }
}

fn checks(
    sites: &[(ExpansionCheck, MessageSite)],
    message_functions: &BTreeSet<u64>,
    gap: &mut impl FnMut(Property, Unresolved),
) -> GrammarProperty<Vec<ExpansionCheck>> {
    let mut found = BTreeSet::new();

    for (check, site) in sites {
        if logs(site, message_functions) {
            found.insert(*check);
        } else {
            gap(Property::Checks, Unresolved::new("message-argument"));
        }
    }

    if found.is_empty() {
        GrammarProperty::Unresolved
    } else {
        GrammarProperty::Partial(found.into_iter().collect())
    }
}

/// Whether `site` forms its literal in `x1` with `adrp` and `add`, and the next call, with no
/// write to `x1` before it, is a message function.
fn logs(site: &MessageSite, message_functions: &BTreeSet<u64>) -> bool {
    let formed = site.rows.windows(2).position(|pair| {
        let page = pair[0].operation == "adrp"
            && pair[0]
                .operands
                .strip_prefix("x1,")
                .and_then(number)
                .is_some_and(|page| page == site.literal & !0xfff);
        let offset = pair[1].operation == "add"
            && pair[1]
                .operands
                .strip_prefix("x1,x1,")
                .and_then(number)
                .is_some_and(|offset| offset == site.literal & 0xfff);
        page && offset
    });
    let Some(formed) = formed else {
        return false;
    };

    for row in &site.rows[formed + 2..] {
        if row.operation == "bl" {
            return number(&row.operands).is_some_and(|to| message_functions.contains(&to));
        }
        let first = row.operands.split(',').next().map(register);
        let writes_x1 = writes_first_operand(&row.operation) && first.as_deref() == Some("x1");
        if writes_x1 || is_control_transfer(&row.operation) {
            return false;
        }
    }

    false
}
