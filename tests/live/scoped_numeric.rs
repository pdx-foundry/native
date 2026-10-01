//! SDK-493 parser cases adapted to proven root destinations; no evaluation is inferred.
use super::*;
use pdx_native::{
    FixtureFieldQuestion, FixtureParsing, FixtureRequest, FixtureStorage, FixtureValue,
    GrammarProperty, ScopedNumericLiteral,
};

#[derive(serde::Deserialize)]
struct CaseInput {
    id: String,
    original_kind: String,
    registry: String,
    field: String,
    body: String,
}

pub(super) async fn matrix() -> Outcome {
    let cases: Vec<CaseInput> =
        serde_json::from_str(include_str!("../expected/scoped-numeric-m45/cases.json"))?;
    assert_eq!(
        cases
            .iter()
            .filter(|case| case.original_kind != "new")
            .count(),
        62
    );
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?;
    let mut report = std::collections::BTreeMap::new();
    for registry in ["common/council_agendas", "common/megastructures"] {
        let fields = native.registry_fields(registry)?;
        let selected: Vec<_> = cases
            .iter()
            .filter(|case| case.registry == registry)
            .collect();
        // A block supplied to this scalar reader can consume later definitions.
        let (structural, ordinary): (Vec<_>, Vec<_>) = selected
            .into_iter()
            .partition(|case| case.body.contains("= {"));
        let batches: Vec<_> = ordinary.chunks(24).chain(structural.chunks(1)).collect();
        for batch in batches {
            observe_batch(&native, registry, &fields.value, batch, &mut report).await?;
        }
    }
    check_report(&native, &report)
}

async fn observe_batch(
    native: &Native,
    registry: &str,
    fields: &[pdx_native::Field],
    batch: &[&CaseInput],
    report: &mut std::collections::BTreeMap<String, serde_json::Value>,
) -> Outcome {
    let mut text = String::from("@atlas_number = 7\n");
    let mut questions = Vec::new();
    for case in batch {
        writeln!(text, "{} = {{\n{}\n}}", case.id, case.body)?;
        questions.push(FixtureFieldQuestion::new(registry, &case.id, &case.field).with_parsing());
    }
    let request =
        FixtureRequest::field_outcomes(format!("{registry}/native_scoped.txt"), text, questions);
    let mut game = native
        .start_game(options().registries([registry]).fixture(request))
        .await?;
    let mut result = match game.observe_fixture().await {
        Ok(answer) => record_batch(registry, fields, batch, &answer, report),
        Err(error) => Err(error.into()),
    };
    and_close(&mut result, &mut game).await;
    result
}

fn record_batch(
    registry: &str,
    fields: &[pdx_native::Field],
    batch: &[&CaseInput],
    answer: &Answer<pdx_native::FixtureObservation>,
    report: &mut std::collections::BTreeMap<String, serde_json::Value>,
) -> Outcome {
    let structural_case = batch.len() == 1 && batch[0].body.contains("= {");
    if (!structural_case
        && (answer.completeness != Completeness::Complete || !answer.gaps.is_empty()))
        || answer.value.field_outcomes.len() != batch.len()
    {
        return Err(format!("scoped fixture incomplete: {answer:?}").into());
    }
    for outcome in &answer.value.field_outcomes {
        let case = batch
            .iter()
            .find(|case| case.id == outcome.question.definition)
            .unwrap();
        let FixtureStorage::Observed {
            occurrences,
            final_value: Some(final_value),
            completeness: Completeness::Complete,
        } = &outcome.storage
        else {
            return Err(format!("scoped storage unavailable: {outcome:?}").into());
        };
        let FixtureParsing::Observed {
            occurrences: parsed,
            completeness: Completeness::Complete,
        } = &outcome.parsing
        else {
            return Err(format!("scoped parser unavailable: {outcome:?}").into());
        };
        if outcome.owner.is_none()
            || occurrences.len() != case.body.matches(&format!("{} =", case.field)).count()
            || parsed.len() != occurrences.len()
            || parsed.iter().zip(occurrences).any(|(parser, stored)| {
                parser.line != stored.line
                    || parser.return_line.is_none()
                    || parser.occurrence != stored.occurrence
            })
        {
            return Err(format!("scoped source joins differ: {outcome:?}").into());
        }
        let field = fields
            .iter()
            .find(|field| field.name == case.field)
            .unwrap();
        for value in occurrences
            .iter()
            .map(|item| &item.value)
            .chain([final_value])
        {
            check_storage(&field.reader, value)?;
        }
        let diagnostics: Vec<_> = outcome
            .diagnostics
            .iter()
            .map(|index| {
                let diagnostic = &answer.value.diagnostics[*index];
                serde_json::json!({"stage":diagnostic.stage,"text":diagnostic.text})
            })
            .collect();
        report.insert(
            case.id.clone(),
            serde_json::json!({
                "registry":registry, "field":case.field,
                "stored":occurrences.iter().map(|item| &item.value).collect::<Vec<_>>(),
                "final":final_value,"diagnostics":diagnostics,
                "coverage":answer.completeness, "gaps":answer.gaps
            }),
        );
    }
    Ok(())
}

fn check_report(
    native: &Native,
    report: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Outcome {
    let actual = serde_json::json!({"build":native.build(),"cases":report});
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".local/sdk-645/scoped-live.json");
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_string_pretty(&actual)?)?;
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../expected/scoped-numeric-m45/live.json"))?;
    if actual != expected {
        return Err(format!("scoped observations differ; inspect {}", path.display()).into());
    }
    Ok(())
}

fn check_storage(reader: &pdx_native::Reader, value: &FixtureValue) -> Outcome {
    let (GrammarProperty::Known(Some(conversion)) | GrammarProperty::Partial(Some(conversion))) =
        &reader.numeric
    else {
        return Err("missing numeric conversion".into());
    };
    let (width, scale) = match value {
        FixtureValue::Integer(_) => (32, 1),
        FixtureValue::ScopedNumeric(value) => match value.literal {
            ScopedNumericLiteral::Integer(_) => (32, 1),
            ScopedNumericLiteral::FixedPoint { scale, .. } => (64, scale),
        },
        _ => return Err("unexpected scoped literal representation".into()),
    };
    if conversion.width_bits != GrammarProperty::Known(width)
        || conversion.scale != GrammarProperty::Known(Some(scale))
    {
        return Err(format!("static/live storage conflict: {reader:?}; {value:?}").into());
    }
    let (minimum, maximum) = if width == 32 {
        (
            pdx_native::NumericBound::Signed(-2147483648),
            pdx_native::NumericBound::Signed(2147483647),
        )
    } else {
        (
            pdx_native::NumericBound::Rational {
                numerator: i64::MIN,
                denominator: scale,
            },
            pdx_native::NumericBound::Rational {
                numerator: i64::MAX,
                denominator: scale,
            },
        )
    };
    let expected = GrammarProperty::Known(Box::new(pdx_native::NumericRange {
        minimum: GrammarProperty::Known(minimum),
        maximum: GrammarProperty::Known(maximum),
    }));
    if conversion.accepted_range != expected {
        return Err(format!("scoped literal did not inherit its shared range: {reader:?}").into());
    }
    Ok(())
}

pub(super) async fn worker_loss() -> Outcome {
    let registry = "common/council_agendas";
    let request = FixtureRequest::field_outcomes(
        format!("{registry}/native_scoped.txt"),
        "sample = { agenda_cost = value:missing agenda_cost = 9 }",
        [FixtureFieldQuestion::new(registry, "sample", "agenda_cost").with_parsing()],
    );
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?;
    match native
        .start_game(
            options()
                .registries([registry])
                .fixture(request)
                .fault(ObservationTarget::Fixture, Fault::WorkerLoss),
        )
        .await
    {
        Err(Error::Startup {
            disposal: Disposal::Confirmed,
            reason,
        }) if reason.contains("WorkerLost") => Ok(()),
        Ok(mut game) => {
            let _ = game.close().await;
            Err("scoped fixture worker loss unexpectedly started a session".into())
        }
        Err(error) => Err(format!("scoped fixture worker loss: {error:?}").into()),
    }
}
