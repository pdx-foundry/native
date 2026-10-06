//! Fixture checks of the static script expansion answer: the stated forms, the stage of each
//! mechanism, inline scripts at two hosts, and the scope of scripted variables.
use super::*;
use pdx_native::{
    CallForm, DiagnosticCoverage, DiagnosticJoin, DiagnosticWindow, ExpansionDefinitions,
    ExpansionHost, ExpansionMechanism, ExpansionStage, FixtureDiagnostic, FixtureFieldQuestion,
    FixtureRequest, FixtureStorage, FixtureValue, GrammarProperty, MissingParameter, ParameterForm,
    ScriptExpansion,
};

/// Each row is one line, so that its diagnostics join to it. The vanilla definitions:
/// `paragon/tradition_swap_desc_hive` writes `name = $tradition$_hive`;
/// `shroud/add_percentage_to_resource` writes `$RESOURCE$ = 1` in an effect block;
/// `pop_group_add_ethic_effect` writes `$POP_GROUP$` only inside `[[POP_GROUP]`;
/// `pop_group_transfer_effect` writes `$OLD_ETHOS$` inside `[[!POP_GROUP]` and inside branches
/// that need `AMOUNT` or `PERCENTAGE`; `store_galactic_community_leader_backup_data` writes
/// `flag = $FLAG|no$` on line 10 and `room = $ROOM|no$` on line 11 of its generated source.
const TEMPLATES: &str = "\
native_inline_statements = {
 inline_script = { script = paragon/tradition_swap_desc_hive tradition = \"native_x inherit_icon = native_bad\" }
}
native_inline_control = {
 inline_script = { script = paragon/tradition_swap_desc_hive tradition = native_ok }
}
native_expansions = {
 on_enabled = {
  inline_script = { script = shroud/add_percentage_to_resource RESOURCE = native_bad_resource PERCENTAGE = 10 }
  inline_script = { script = shroud/add_percentage_to_resource RESOURCE = energy PERCENTAGE = 10 }
  inline_script = { script = shroud/add_percentage_to_resource PERCENTAGE = 10 }
  native_missing_effect = yes
  pop_group_add_ethic_effect = { POP_GROUP = native_bad_target }
  pop_group_add_ethic_effect = { RANDOM = 0 }
  pop_group_transfer_effect = { OLD_ETHOS = native_bad_old_ethic }
  pop_group_transfer_effect = { POP_GROUP = this OLD_ETHOS = native_bad_old_ethic }
  store_galactic_community_leader_backup_data = { ROOM = native_bad_room }
  store_galactic_community_leader_backup_data = { ROOM = native_bad_room FLAG = native_flag }
  ruler = { add_age = value:skill_scaled_age_increase|SKILL|1| }
  ruler = { add_age = value:skill_scaled_age_increase|SKILL| }
  ruler = { add_age = value:native_missing_value|K|v| }
 }
}
";

/// `@resettlement_unity` is 10 and `@discovery_weight` is 3 in `common/scripted_variables`.
const VARIABLES: &str = "\
@native_local = 7
@resettlement_unity = 11
special_project = {
 requirements = { fleet_power = @native_local }
 key = native_variable_local
}
special_project = {
 requirements = { fleet_power = @discovery_weight }
 key = native_variable_global
}
special_project = {
 requirements = { fleet_power = @resettlement_unity }
 key = native_variable_shadowed
}
special_project = {
 requirements = { fleet_power = @[ native_local + discovery_weight ] }
 key = native_variable_arithmetic
}
";

pub(super) async fn templates_and_inline_scripts(native: &Native) -> Outcome {
    let expansions = native.script_expansions()?.value;
    let inline = expansion(&expansions, ExpansionMechanism::InlineScript)?;
    let effect = expansion(&expansions, ExpansionMechanism::ScriptedEffect)?;
    let value = expansion(&expansions, ExpansionMechanism::ScriptValue)?;
    let every_form = [
        ParameterForm::Substitution,
        ParameterForm::Default,
        ParameterForm::Conditional,
        ParameterForm::NegatedConditional,
    ];
    let stated = inline.stage == GrammarProperty::Known(ExpansionStage::Read)
        && known_contains(&inline.parameter_forms, &[ParameterForm::Substitution])
        && inline.missing_parameter == GrammarProperty::Known(Some(MissingParameter::KeptAsText))
        && partial_or_known_contains(
            &inline.hosts,
            &[
                ExpansionHost::ObjectBlock,
                ExpansionHost::Commands(pdx_native::BlockFamily::Effect),
            ],
        )
        && known_contains(
            &inline.call_forms,
            &[CallForm::Block {
                name_key: Some("script".into()),
            }],
        )
        && effect.stage == GrammarProperty::Known(ExpansionStage::Compile)
        && known_contains(&effect.parameter_forms, &every_form)
        && known_contains(
            &effect.call_forms,
            &[CallForm::Value, CallForm::Block { name_key: None }],
        )
        && value.stage == GrammarProperty::Known(ExpansionStage::Compile)
        && known_contains(&value.call_forms, &[CallForm::Pipe]);
    if !stated {
        return Err(format!("static expansions: {inline:?}\n{effect:?}\n{value:?}").into());
    }

    let file = "common/traditions/native_expansions.txt";
    let request = FixtureRequest::field_outcomes(
        file,
        TEMPLATES,
        [FixtureFieldQuestion::new(
            TRADITIONS,
            "native_expansions",
            "on_enabled",
        )],
    )
    .through_validation();
    let mut game = native.start_game(options().fixture(request)).await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        if answer.value.diagnostic_coverage
            != (DiagnosticCoverage::Complete {
                window: DiagnosticWindow::FixtureFileLoadAndValidation,
            })
        {
            return Err(format!("expansion diagnostic coverage: {:?}", answer.gaps).into());
        }
        let rows = Rows {
            text: TEMPLATES,
            diagnostics: &answer.value.diagnostics,
        };

        rows.reports("native_x inherit_icon", "reader-malformed-report", "(inline_script) common/inline_scripts/paragon/tradition_swap_desc_hive.txt near line 3")?;
        rows.quiet("tradition = native_ok")?;
        rows.reports("RESOURCE = native_bad_resource", "engine-validation-log", "native_bad_resource")?;
        rows.quiet("RESOURCE = energy")?;
        rows.reports("script = shroud/add_percentage_to_resource PERCENTAGE", "engine-validation-log", "$RESOURCE$")?;
        rows.reports("native_missing_effect", "engine-validation-log", "Invalid scripted effect: native_missing_effect")?;
        rows.reports("POP_GROUP = native_bad_target", "engine-validation-log", "native_bad_target")?;
        rows.quiet("RANDOM = 0")?;
        rows.reports("{ OLD_ETHOS = native_bad_old_ethic", "engine-validation-log", "native_bad_old_ethic")?;
        rows.quiet("POP_GROUP = this")?;
        rows.generated_lines("ROOM = native_bad_room }", &[11])?;
        rows.generated_lines("FLAG = native_flag", &[10, 11])?;
        rows.quiet("SKILL|1|")?;
        rows.reports("SKILL| }", "engine-parser-log", "Uneven number of parameters")?;
        rows.reports("native_missing_value", "engine-validation-log", "Invalid script value: native_missing_value")?;
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

pub(super) async fn scripted_variables(native: &Native) -> Outcome {
    let expansions = native.script_expansions()?.value;
    let variable = expansion(&expansions, ExpansionMechanism::ScriptedVariable)?;
    let stated = variable.stage == GrammarProperty::Known(ExpansionStage::Lex)
        && known_contains(
            &variable.call_forms,
            &[CallForm::Variable, CallForm::Arithmetic],
        )
        && variable.definitions
            == GrammarProperty::Partial(vec![
                ExpansionDefinitions::SameFile,
                ExpansionDefinitions::Directory {
                    directory: "common/scripted_variables".into(),
                },
            ]);
    if !stated {
        return Err(format!("static variables: {variable:?}").into());
    }

    let registry = "common/special_projects";
    let expected = [
        ("native_variable_local", 7),
        ("native_variable_global", 3),
        ("native_variable_shadowed", 11),
        ("native_variable_arithmetic", 10),
    ];
    let questions = expected.map(|(key, _)| {
        FixtureFieldQuestion::new(registry, key, "fleet_power").with_parent_field("requirements")
    });
    let request = FixtureRequest::field_outcomes(
        format!("{registry}/native_variables.txt"),
        VARIABLES,
        questions,
    );
    let mut game = native.start_game(options().fixture(request)).await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        for (key, whole) in expected {
            let outcome = answer
                .value
                .field_outcomes
                .iter()
                .find(|outcome| outcome.question.definition == key)
                .ok_or(format!("{key}: no outcome"))?;
            let stored = match &outcome.storage {
                FixtureStorage::Observed {
                    final_value: Some(FixtureValue::FixedPoint { raw, scale }),
                    ..
                } => *raw == whole * *scale as i64,
                _ => false,
            };
            if !stored {
                return Err(
                    format!("{key}: expected {whole}, stored {:?}", outcome.storage).into(),
                );
            }
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

fn expansion(
    expansions: &[ScriptExpansion],
    mechanism: ExpansionMechanism,
) -> Result<&ScriptExpansion, String> {
    expansions
        .iter()
        .find(|expansion| expansion.mechanism == mechanism)
        .ok_or(format!("{mechanism:?} missing"))
}

fn known_contains<T: PartialEq>(property: &GrammarProperty<Vec<T>>, wanted: &[T]) -> bool {
    matches!(property, GrammarProperty::Known(values) if wanted.iter().all(|value| values.contains(value)))
}

fn partial_or_known_contains<T: PartialEq>(
    property: &GrammarProperty<Vec<T>>,
    wanted: &[T],
) -> bool {
    matches!(property, GrammarProperty::Known(values) | GrammarProperty::Partial(values)
        if wanted.iter().all(|value| values.contains(value)))
}

/// The fixture's diagnostics, by the row that each joins to.
struct Rows<'a> {
    text: &'a str,
    diagnostics: &'a [FixtureDiagnostic],
}

impl Rows<'_> {
    /// The one-based line of the one row that holds `needle`.
    fn line(&self, needle: &str) -> Result<u64, String> {
        let mut lines = self
            .text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(needle));
        match (lines.next(), lines.next()) {
            (Some((index, _)), None) => Ok(index as u64 + 1),
            _ => Err(format!("no single row holds {needle:?}")),
        }
    }

    fn at(&self, needle: &str) -> Result<Vec<&FixtureDiagnostic>, String> {
        let line = self.line(needle)?;
        Ok(self
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(&diagnostic.join, DiagnosticJoin::Source { line: joined, .. } if *joined == line)
            })
            .collect())
    }

    fn reports(&self, needle: &str, stage: &str, text: &str) -> Outcome {
        let found = self.at(needle)?;
        if found
            .iter()
            .any(|diagnostic| diagnostic.stage == stage && diagnostic.text.contains(text))
        {
            return Ok(());
        }
        Err(format!("{needle}: no {stage} diagnostic with {text:?}: {found:?}").into())
    }

    fn quiet(&self, needle: &str) -> Outcome {
        match self.at(needle)?.as_slice() {
            [] => Ok(()),
            found => Err(format!("{needle}: unexpected diagnostics: {found:?}").into()),
        }
    }

    /// The generated-source lines of the reader reports that join to the row.
    fn generated_lines(&self, needle: &str, lines: &[u64]) -> Outcome {
        let found = self.at(needle)?;
        let reported: Vec<u64> = found
            .iter()
            .filter(|diagnostic| diagnostic.stage == "reader-malformed-report")
            .filter_map(|diagnostic| diagnostic.text.rsplit_once("near line ")?.1.parse().ok())
            .collect();
        if reported == lines {
            return Ok(());
        }
        Err(format!("{needle}: generated lines {reported:?}, expected {lines:?}: {found:?}").into())
    }
}
