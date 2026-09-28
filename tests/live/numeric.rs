//! Finite conversion observations, checked independently of general acceptance claims.
use super::*;
use pdx_native::{
    DiagnosticCoverage, DiagnosticWindow, FixtureFieldQuestion, FixtureParsing, FixtureRequest,
    FixtureStorage, FixtureValue, GrammarProperty, NumericRepresentation,
};

const INPUTS: &[(&str, &[&str])] = &[
    ("positive_sign", &["+7"]),
    ("negative", &["-7"]),
    ("negative_zero", &["-0.0"]),
    ("trailing_text", &["12tail"]),
    ("hexadecimal", &["0x10"]),
    ("octal", &["010"]),
    ("exponent", &["1e2"]),
    ("precision", &["1.23456789"]),
    ("negative_precision", &["-1.23456789"]),
    ("tiny", &["0.000005"]),
    ("negative_tiny", &["-0.000005"]),
    ("integer_max", &["2147483647"]),
    ("integer_overflow", &["2147483648"]),
    ("integer_min", &["-2147483648"]),
    ("integer_underflow", &["-2147483649"]),
    ("fixed_max", &["92233720368547.75807"]),
    ("fixed_overflow", &["92233720368547.75808"]),
    ("template_min", &["-281474976710656.0"]),
    ("template_overflow", &["281474976710656.0"]),
    ("malformed", &["7", "not_a_number"]),
];

type Observations = std::collections::BTreeMap<String, serde_json::Value>;

pub(super) async fn matrix() -> Outcome {
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?;
    let facts = pdx_native::internals::numeric_readers::run(&native)?;
    let mut report = Observations::new();
    for (registry, field, nested, callee) in [
        (
            "common/megastructures",
            "sensor_range",
            false,
            "CReader::Read(int&)",
        ),
        (
            "common/megastructures",
            "build_time",
            false,
            "CReader::Read(CFixedPoint&)",
        ),
        (
            "common/armies",
            "war_exhaustion",
            false,
            "CReader::Read(CFixedPoint&)",
        ),
        (
            "common/special_projects",
            "fleet_power",
            true,
            "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)",
        ),
    ] {
        let request = fixture(registry, field, nested)?;
        let mut game = native
            .start_game(
                options()
                    .registries([if nested { TRADITIONS } else { registry }])
                    .fixture(request),
            )
            .await?;
        let mut result = async {
            let answer = game.observe_fixture().await?;
            let observations =
                checked_observations(&answer, registry, &facts.readers[callee].conversion)?;
            report.extend(observations);
            Ok(())
        }
        .await;
        and_close(&mut result, &mut game).await;
        result?;
    }
    let actual = serde_json::json!({"build": native.build(), "cases": report});
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("../expected/numeric-m45/live.json"))?;
    if actual != expected {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".local/sdk-644/numeric-conversion-live.json");
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, serde_json::to_string_pretty(&actual)?)?;
        return Err(format!(
            "numeric conversion observations differ; inspect {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn fixture(registry: &str, field: &str, nested: bool) -> Result<FixtureRequest, std::fmt::Error> {
    let mut text = String::new();
    let mut questions = Vec::new();
    for (case, inputs) in INPUTS {
        if nested {
            writeln!(text, "special_project = {{\n requirements = {{")?;
        } else {
            writeln!(text, "{case} = {{")?;
        }
        for input in *inputs {
            writeln!(text, " {field} = {input}")?;
        }
        let mut question = FixtureFieldQuestion::new(registry, *case, field).with_parsing();
        if nested {
            question = question.with_parent_field("requirements");
            writeln!(text, " }}\n key = {case}")?;
        }
        questions.push(question);
        writeln!(text, "}}")?;
    }
    Ok(FixtureRequest::field_outcomes(
        format!("{registry}/native_conversion.txt"),
        text,
        questions,
    ))
}

fn checked_observations(
    answer: &Answer<pdx_native::FixtureObservation>,
    registry: &str,
    conversion: &GrammarProperty<Option<pdx_native::NumericConversion>>,
) -> Result<Observations, Box<dyn std::error::Error>> {
    if answer.completeness != Completeness::Complete
        || !answer.gaps.is_empty()
        || answer.value.diagnostic_coverage
            != (DiagnosticCoverage::Complete {
                window: DiagnosticWindow::FixtureFileLoad,
            })
        || answer.value.field_outcomes.len() != INPUTS.len()
    {
        return Err(format!("numeric conversion coverage: {answer:?}").into());
    }
    let mut report = Observations::new();
    for outcome in &answer.value.field_outcomes {
        let FixtureStorage::Observed {
            occurrences,
            final_value,
            completeness: Completeness::Complete,
        } = &outcome.storage
        else {
            return Err(format!("numeric conversion storage unavailable: {outcome:?}").into());
        };
        let FixtureParsing::Observed {
            occurrences: parsed,
            completeness: Completeness::Complete,
        } = &outcome.parsing
        else {
            return Err(format!("numeric conversion parsing unavailable: {outcome:?}").into());
        };
        let inputs = INPUTS
            .iter()
            .find(|(name, _)| *name == outcome.question.definition)
            .unwrap()
            .1;
        if outcome.owner.is_none()
            || occurrences.len() != inputs.len()
            || parsed.len() != inputs.len()
            || parsed.iter().zip(occurrences).any(|(parser, stored)| {
                parser.line != stored.line
                    || parser.return_line != Some(stored.line)
                    || parser.occurrence != stored.occurrence
            })
        {
            return Err(format!("numeric conversion joins incomplete: {outcome:?}").into());
        }
        for stored in occurrences {
            check_storage(conversion, &stored.value)?;
        }
        let diagnostics: Vec<_> = outcome
            .diagnostics
            .iter()
            .map(|index| {
                let diagnostic = &answer.value.diagnostics[*index];
                serde_json::json!({"stage": diagnostic.stage, "text": diagnostic.text})
            })
            .collect();
        report.insert(format!("{registry}/{}/{}", outcome.question.field, outcome.question.definition),
            serde_json::json!({"inputs": inputs, "stored": occurrences.iter().map(|item| &item.value).collect::<Vec<_>>(), "final": final_value, "diagnostics": diagnostics}));
    }
    Ok(report)
}

fn check_storage(
    conversion: &GrammarProperty<Option<pdx_native::NumericConversion>>,
    stored: &FixtureValue,
) -> Outcome {
    let GrammarProperty::Partial(Some(conversion)) = conversion else {
        return Err(format!("missing static conversion: {conversion:?}").into());
    };
    let (width, scale) = match stored {
        FixtureValue::Integer(_) => (32, 1),
        FixtureValue::FixedPoint { scale, .. } => (64, *scale),
        _ => return Err("unexpected numeric storage type".into()),
    };
    if conversion.representation != GrammarProperty::Known(NumericRepresentation::Integer)
        || conversion.signedness != GrammarProperty::Known(pdx_native::NumericSignedness::Signed)
        || conversion.width_bits != GrammarProperty::Known(width)
        || conversion.scale != GrammarProperty::Known(Some(scale))
    {
        return Err(format!("static/live conversion conflict: {conversion:?}; {stored:?}").into());
    }
    Ok(())
}
