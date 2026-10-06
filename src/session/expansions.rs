//! Static script expansion rules: inline scripts, scripted effects and triggers, script values and
//! scripted variables.
use super::Native;
use super::language::gap;
use super::questions::error;
use crate::answer::{Answer, Basis, BuildId, Completeness, Error, Gap, GapKind, Operation, Source};
use crate::engine::analysis::expansions::{self, ExpansionGap, ExpansionResult, Property};
use crate::{ExpansionMechanism, ScriptExpansion};

/// Why the cycle behavior of a template mechanism is not established.
const CYCLES: &str = "What a cycle of definitions yields is not established. The depth check is reached by deep nesting; other ways that a cycle can end were not traced, and a fixture of a cycle needs its own definitions.";
/// Why the definition that a duplicated name selects is not established.
const DUPLICATE_DEFINITIONS: &str = "Which definition a name selects when files of the directory define it twice is not established.";
/// Why a use of a definition that loads after it is not established.
const FORWARD_DEFINITIONS: &str = "Whether a use resolves a definition that loads after it is not established. A use is a placeholder until all content loads, but the lookup's result was not traced.";

impl Native {
    /// Read how script reuses text: for each mechanism, where script can use it, where its names
    /// are defined, when the engine expands a use, its call forms and parameter forms, what an
    /// absent parameter yields, and the diagnostics that the engine logs for a use as it loads.
    ///
    /// The stage tells a tool whether to check the written text or the expanded text. Scripted
    /// modifiers are modifier names, not expansions: [`Native::modifier_families`] gives them.
    /// Which duplicate definition wins, cycles, and runtime evaluation are outside this method.
    pub fn script_expansions(&self) -> Result<Answer<Vec<ScriptExpansion>>, Error> {
        self.answer("script_expansions", None, || {
            let operation = Operation::ScriptExpansions;
            let input = self
                .declaration_analysis(operation)?
                .expansion_input()
                .map_err(|failure| error(operation, failure))?;
            Ok(normalize(expansions::analyze(&input), self.build()))
        })
    }
}

fn normalize(result: ExpansionResult, build: BuildId) -> Answer<Vec<ScriptExpansion>> {
    let mut gaps: Vec<Gap> = result.gaps.iter().map(property_gap).collect();

    for expansion in &result.expansions {
        let subject = mechanism_name(expansion.mechanism);
        if has_directory(&expansion.definitions) {
            gaps.push(gap(
                GapKind::OutsideMethod,
                Some(subject),
                DUPLICATE_DEFINITIONS,
            ));
        }
        if expansion.stage == crate::GrammarProperty::Known(crate::ExpansionStage::Compile) {
            gaps.push(gap(GapKind::OutsideMethod, Some(subject), CYCLES));
            gaps.push(gap(
                GapKind::OutsideMethod,
                Some(subject),
                FORWARD_DEFINITIONS,
            ));
        }
    }
    gaps.dedup();

    Answer {
        value: result.expansions,
        completeness: Completeness::from_gaps(&gaps),
        gaps,
        source: Source::new(build, expansions::METHOD, Basis::StaticAnalysis),
    }
}

/// Whether a content directory defines some of the mechanism's names.
fn has_directory(definitions: &crate::GrammarProperty<Vec<crate::ExpansionDefinitions>>) -> bool {
    let (crate::GrammarProperty::Known(sources) | crate::GrammarProperty::Partial(sources)) =
        definitions
    else {
        return false;
    };

    sources
        .iter()
        .any(|source| matches!(source, crate::ExpansionDefinitions::Directory { .. }))
}

fn property_gap(found: &ExpansionGap) -> Gap {
    let detail = match (found.property, found.cause.reason) {
        (Property::Definitions, "lookup-order") => {
            "the order in which a use looks up the file's own definitions and the directory's is not established"
        }
        (Property::Definitions, _) => {
            "a source of the definitions is not established: no single directory, or no registration in the reading file's own list"
        }
        (Property::Hosts, "placeholder-object") => {
            "a call of the placeholder constructor does not receive a newly allocated object, so it is not counted as a use"
        }
        (Property::Hosts, "unjoined-reader") => {
            "a reader that expands the mechanism is not joined to a content directory or a shared reader"
        }
        (Property::Hosts, _) => "no reader of script content is joined to the mechanism",
        (Property::Stage, _) => {
            "the call chain that places the expansion at its stage is not established, or has an unclassified caller"
        }
        (Property::Forms, _) | (Property::MissingParameter, "stated-form-functions") => {
            "the engine functions that hold the stated forms are not bound on this build"
        }
        (Property::MissingParameter, _) => {
            "the missing-parameter message is not established as the argument of a message call"
        }
        (Property::Checks, _) => {
            "a check's message literal is not established as the argument of a message call"
        }
    };

    gap(
        GapKind::UnresolvedPath,
        Some(mechanism_name(found.mechanism)),
        format!("{detail} ({})", found.cause.reason),
    )
}

fn mechanism_name(mechanism: ExpansionMechanism) -> &'static str {
    match mechanism {
        ExpansionMechanism::InlineScript => "inline script",
        ExpansionMechanism::ScriptedEffect => "scripted effect",
        ExpansionMechanism::ScriptedTrigger => "scripted trigger",
        ExpansionMechanism::ScriptValue => "script value",
        ExpansionMechanism::ScriptedVariable => "scripted variable",
    }
}
