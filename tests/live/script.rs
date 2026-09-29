//! Paused command controls use the same sample authority as the paired file cases.
use super::*;
use pdx_native::{DeclarationKind, ScriptCheck};

pub(super) async fn arguments(native: &Native) -> Outcome {
    let scope = native
        .scopes()?
        .value
        .types
        .into_iter()
        .find(|scope| scope.name == "country")
        .ok_or("country scope missing")?
        .id;
    let cases = argument_cases();
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        for (_, case) in &cases {
            let Case::FixtureArgument {
                field,
                commands,
                samples,
            } = case
            else {
                continue;
            };
            let kind = if *field == "potential" {
                DeclarationKind::Trigger
            } else {
                DeclarationKind::Effect
            };
            for command in commands {
                let grammar = native.command_grammar(kind, command)?;
                if grammar.completeness != Completeness::Complete {
                    return Err(format!("{command}: incomplete grammar: {:?}", grammar.gaps).into());
                }
            }
            for sample in samples {
                check(&mut game, kind, &scope, sample).await?;
                if sample.stage.is_some() {
                    let command = sample.child.split_whitespace().next().unwrap();
                    let correction = cases
                        .iter()
                        .filter_map(|(_, case)| match case {
                            Case::FixtureArgument { samples, .. } => Some(samples),
                            _ => None,
                        })
                        .flatten()
                        .find(|candidate| {
                            candidate.stage.is_none()
                                && candidate.child.split_whitespace().next() == Some(command)
                        })
                        .ok_or("missing corrected contrast")?;
                    check(&mut game, kind, &scope, correction).await?;
                }
            }
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

async fn check(
    game: &mut Game,
    kind: DeclarationKind,
    scope: &pdx_native::ScopeId,
    sample: &ValidationSample,
) -> Outcome {
    let answer = game
        .check_script(&ScriptCheck {
            kind,
            scope: scope.clone(),
            text: sample.child.clone(),
        })
        .await?;
    let observation = &answer.value;
    if !observation.read_returned || !observation.hooks_active || observation.children == 0 {
        return Err(format!("{}: incomplete read: {answer:?}", sample.name).into());
    }
    if sample.stage.is_some() == observation.diagnostics.is_empty() {
        return Err(format!("{}: current diagnostic mismatch: {answer:?}", sample.name).into());
    }
    if observation
        .diagnostics
        .iter()
        .any(|diagnostic| diagnostic.line != Some(1))
    {
        return Err(format!("{}: diagnostic line mismatch: {answer:?}", sample.name).into());
    }
    println!(
        "{}: current={} foreign={} unjoined={} bounded={}",
        sample.name,
        observation.diagnostics.len(),
        observation.foreign.len(),
        observation.unjoined.len(),
        observation.bound_reached
    );
    Ok(())
}

pub(super) async fn paired_file(
    native: &Native,
    field: &str,
    samples: &[ValidationSample],
) -> Outcome {
    use pdx_native::{DiagnosticJoin, FixtureFieldQuestion, FixtureRequest};
    let file = format!("common/traditions/native_paired_{field}.txt");
    let text = samples
        .iter()
        .flat_map(|sample| sample.definition_lines(field))
        .map(|line| line + "\n")
        .collect::<String>();
    let questions = samples.iter().map(|sample| {
        FixtureFieldQuestion::new(TRADITIONS, sample.definition(), field).with_parsing()
    });
    let request = FixtureRequest::field_outcomes(&file, text, questions).through_validation();
    let scope = native
        .scopes()?
        .value
        .types
        .into_iter()
        .find(|scope| scope.name == "country")
        .ok_or("country scope missing")?
        .id;
    let kind = if field == "potential" {
        DeclarationKind::Trigger
    } else {
        DeclarationKind::Effect
    };
    let mut game = native
        .start_game(options().loaded_modifiers().fixture(request))
        .await?;
    let mut result = async {
        let file_answer = game.observe_fixture().await?;
        let failures = validation_failures(field, &file, samples, &file_answer);
        if !failures.is_empty() { return Err(failures.join("; ").into()); }
        game.close().await?;
        game = native.start_game(options().loaded_modifiers()).await?;
        for (index, sample) in samples.iter().enumerate() {
            let answer = game.check_script(&ScriptCheck { kind, scope: scope.clone(), text: sample.child.clone() }).await?;
            let file_messages = file_answer.value.diagnostics.iter().filter(|message|
                matches!(&message.join, DiagnosticJoin::Source { file: source, line, .. }
                    if source == &file && *line == ValidationSample::child_line(index)))
                .map(|message| message_identity(&message.text, &file)).collect::<Vec<_>>();
            let script_messages = answer.value.diagnostics.iter()
                .map(|message| message_identity(&message.text, "<script>")).collect::<Vec<_>>();
            if !same_message_identities(&file_messages, &script_messages) {
                return Err(format!("{}: file/check message identities differ: {file_messages:?} / {script_messages:?}", sample.name).into());
            }
            println!("{}: paired identities {file_messages:?}", sample.name);
        }
        Ok(())
    }.await;
    and_close(&mut result, &mut game).await;
    result
}

fn same_message_identities(file_messages: &[String], script_messages: &[String]) -> bool {
    let every_file_message_observed = file_messages
        .iter()
        .all(|file| script_messages.iter().any(|script| script.contains(file)));
    let every_script_message_observed = script_messages
        .iter()
        .all(|script| file_messages.iter().any(|file| script.contains(file)));
    every_file_message_observed && every_script_message_observed
}

/// File-reader reports are the inner text of the logger message. Only source and line vary.
fn message_identity(text: &str, source: &str) -> String {
    let replaced = text.replace(source, "<script>");
    let mut after_line = false;
    replaced
        .split_whitespace()
        .map(|word| {
            let normalized = if after_line { "<line>" } else { word };
            after_line = word == "line:";
            normalized
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) async fn attribution(native: &Native) -> Outcome {
    let scopes = native.scopes()?.value.types;
    let country = scopes
        .iter()
        .find(|scope| scope.name == "country")
        .ok_or("country missing")?
        .id
        .clone();
    let colony = scopes
        .iter()
        .find(|scope| scope.name == "colony")
        .ok_or("colony missing")?
        .id
        .clone();
    let texts = [
        "always = banana",
        "always = yes",
        "get_councilor_level = { native_unknown_key = yes }",
        "has_technology = native_missing_technology",
    ];
    let mut isolated = Vec::new();
    for text in texts {
        let mut game = native.start_game(options().loaded_modifiers()).await?;
        let answer = game
            .check_script(&ScriptCheck {
                kind: DeclarationKind::Trigger,
                scope: country.clone(),
                text: text.into(),
            })
            .await;
        let mut result = answer
            .map(|answer| isolated.push(answer))
            .map_err(Into::into);
        and_close(&mut result, &mut game).await;
        result?;
    }
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        for index in [0, 1, 0, 1, 2, 0, 0, 2, 1, 3, 1] {
            let answer = game
                .check_script(&ScriptCheck {
                    kind: DeclarationKind::Trigger,
                    scope: country.clone(),
                    text: texts[index].into(),
                })
                .await?;
            let baseline = &isolated[index];
            if answer.value.diagnostics != baseline.value.diagnostics
                || answer.value.unjoined != baseline.value.unjoined
                || answer.completeness != baseline.completeness
            {
                return Err(format!(
                    "order changed current attribution: {answer:?} vs {baseline:?}"
                )
                .into());
            }
            if answer
                .value
                .foreign
                .iter()
                .any(|message| message.check >= answer.value.check)
            {
                return Err("invalid foreign check identity".into());
            }
        }
        let high_scope = game
            .check_script(&ScriptCheck {
                kind: DeclarationKind::Trigger,
                scope: colony,
                text: "always = yes".into(),
            })
            .await?;
        if high_scope.value.children != 1 || !high_scope.value.diagnostics.is_empty() {
            return Err("colony scope control failed".into());
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

pub(super) async fn deep_nesting(native: &Native) -> Outcome {
    let country = native
        .scopes()?
        .value
        .types
        .into_iter()
        .find(|scope| scope.name == "country")
        .ok_or("country missing")?
        .id;
    let mut game = native.start_game(options().loaded_modifiers()).await?;
    let mut result = async {
        let earlier = ScriptCheck {
            kind: DeclarationKind::Trigger,
            scope: country.clone(),
            text: "always = banana".into(),
        };
        let baseline = game.check_script(&earlier).await?;
        if baseline.value.diagnostics.is_empty() {
            return Err("missing baseline diagnostic".into());
        }
        for (kind, opener, leaf) in [
            (DeclarationKind::Trigger, "and={", "always=yes"),
            (
                DeclarationKind::Effect,
                "hidden_effect={",
                "stop_crisis_sound=yes",
            ),
        ] {
            let depth = (4096 - leaf.len()) / (opener.len() + 1);
            let text = opener.repeat(depth) + leaf + &"}".repeat(depth);
            println!("{kind:?}: depth={depth}, bytes={}", text.len());
            let nested = game
                .check_script(&ScriptCheck {
                    kind,
                    scope: country.clone(),
                    text,
                })
                .await?;
            if nested.completeness != Completeness::Complete
                || nested.value.children != 1
                || !nested.value.diagnostics.is_empty()
                || !nested.value.unjoined.is_empty()
            {
                return Err(format!("deep nesting observation: {nested:?}").into());
            }
            let clean = game
                .check_script(&ScriptCheck {
                    kind,
                    scope: country.clone(),
                    text: leaf.into(),
                })
                .await?;
            if clean.completeness != Completeness::Complete
                || clean.value.children != 1
                || !clean.value.diagnostics.is_empty()
            {
                return Err(format!("clean check after deep nesting: {clean:?}").into());
            }
            let repeated = game.check_script(&earlier).await?;
            if repeated.value.diagnostics != baseline.value.diagnostics
                || repeated.completeness != baseline.completeness
            {
                return Err(
                    format!("earlier check changed after deep nesting: {repeated:?}").into(),
                );
            }
        }
        Ok(())
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}
