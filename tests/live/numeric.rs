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
    ("integer_below_max", &["2147483646"]),
    ("integer_above_min", &["-2147483647"]),
    ("fixed_below_max", &["92233720368547.75806"]),
    ("fixed_max", &["92233720368547.75807"]),
    ("fixed_overflow", &["92233720368547.75808"]),
    ("fixed_above_min", &["-92233720368547.75807"]),
    ("fixed_min", &["-92233720368547.75808"]),
    ("fixed_underflow", &["-92233720368547.75809"]),
    ("template_below_max", &["281474976710655.99993896484375"]),
    ("template_max", &["281474976710655.999969482421875"]),
    ("template_above_min", &["-281474976710655.999969482421875"]),
    ("template_min", &["-281474976710656.0"]),
    ("template_underflow", &["-281474976710656.000030517578125"]),
    ("template_overflow", &["281474976710656.0"]),
    ("quoted_space", &["\"12 34\""]),
    ("fractional_trailing_text", &["1.25tail"]),
    ("malformed", &["7", "not_a_number"]),
];

const FLOAT_INPUTS: &[(&str, &[&str])] = &[
    ("float_boundary", &["3.4028234663852886e38"]),
    (
        "float_below_max",
        &["340282326356119256160033759537265639424"],
    ),
    ("float_max", &["340282346638528859811704183484516925440"]),
    (
        "float_overflow",
        &["340282366920938463463374607431768211456"],
    ),
    (
        "float_above_min",
        &["-340282326356119256160033759537265639424"],
    ),
    ("float_min", &["-340282346638528859811704183484516925440"]),
    (
        "float_underflow",
        &["-340282366920938463463374607431768211456"],
    ),
    ("float_normal_min", &["1.1754943508222875e-38"]),
    ("float_subnormal_max", &["1.1754942106924411e-38"]),
    ("float_subnormal_min", &["1.401298464324817e-45"]),
    ("float_below_subnormal", &["7.006492321624085e-46"]),
    ("float_zero", &["0"]),
    ("float_negative_zero", &["-0"]),
    ("trailing_text", &["12tail"]),
    ("quoted_space", &["\"12 34\""]),
    ("fractional", &["1.23456789"]),
    ("malformed", &["7", "not_a_number"]),
];

const SHORT_INPUTS: &[(&str, &[&str])] = &[
    ("short_boundary", &["32767"]),
    ("short_below_max", &["32766"]),
    ("short_overflow", &["32768"]),
    ("short_above_min", &["-32767"]),
    ("short_min", &["-32768"]),
    ("short_underflow", &["-32769"]),
    ("unsigned_short_below_max", &["65534"]),
    ("unsigned_short_max", &["65535"]),
    ("unsigned_short_overflow", &["65536"]),
    ("short_zero", &["0"]),
    ("short_above_zero", &["1"]),
    ("short_below_zero", &["-1"]),
    ("scanner_max", &["2147483647"]),
    ("scanner_overflow", &["2147483648"]),
    ("scanner_min", &["-2147483648"]),
    ("scanner_underflow", &["-2147483649"]),
    ("trailing_text", &["12tail"]),
    ("quoted_space", &["\"12 34\""]),
    ("fractional", &["1.23456789"]),
    ("malformed", &["7", "not_a_number"]),
];

const FLOAT_STORAGE_INPUTS: &[(&str, &[&str])] = &[
    ("float_boundary", &["3.4028234663852886e38"]),
    ("fractional", &["1.23456789"]),
    ("malformed", &["7", "not_a_number"]),
];

const SHORT_STORAGE_INPUTS: &[(&str, &[&str])] = &[
    ("short_boundary", &["32767"]),
    ("fractional", &["1.23456789"]),
    ("malformed", &["7", "not_a_number"]),
];

type Observations = std::collections::BTreeMap<String, serde_json::Value>;

pub(super) async fn matrix() -> Outcome {
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?;
    let facts = pdx_native::internals::numeric_readers::run(&native)?;
    let mut report = Observations::new();
    for (registry, fields, nested, callee, inputs) in [
        (
            "common/megastructures",
            &["sensor_range"][..],
            false,
            "CReader::Read(int&)",
            INPUTS,
        ),
        (
            "common/megastructures",
            &["build_time"][..],
            false,
            "CReader::Read(CFixedPoint&)",
            INPUTS,
        ),
        (
            "common/armies",
            &["war_exhaustion"][..],
            false,
            "CReader::Read(CFixedPoint&)",
            INPUTS,
        ),
        (
            "common/special_projects",
            &["fleet_power"][..],
            true,
            "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)",
            INPUTS,
        ),
        (
            "common/star_classes",
            &["icon_scale"][..],
            false,
            "CReader::Read(float&)",
            FLOAT_INPUTS,
        ),
        (
            "common/storm_types",
            &[
                "cosmic_storm_galaxy_lightning_time",
                "cosmic_storm_galaxy_max_opacity",
            ][..],
            false,
            "CReader::Read(float&)",
            FLOAT_STORAGE_INPUTS,
        ),
        (
            "common/astral_actions",
            &["unlock_threshold"][..],
            false,
            "CReader::Read(short&)",
            SHORT_INPUTS,
        ),
        (
            "common/astral_actions",
            &["usages"][..],
            false,
            "CReader::Read(short&)",
            SHORT_STORAGE_INPUTS,
        ),
        (
            "common/sector_types",
            &[
                "max_systems",
                "min_systems",
                "min_colonies",
                "max_colonies",
                "max_jumps",
            ][..],
            false,
            "CReader::Read(short&)",
            SHORT_STORAGE_INPUTS,
        ),
    ] {
        let request = fixture(registry, fields, nested, inputs)?;
        let mut game = native.start_game(options().fixture(request)).await?;
        let mut result = async {
            let answer = game.observe_fixture().await?;
            let observations = checked_observations(
                &answer,
                registry,
                fields.len(),
                inputs,
                &facts.readers[callee].conversion,
            )?;
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
            .join(".local/sdk-655/numeric-conversion-live.json");
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

fn fixture(
    registry: &str,
    fields: &[&str],
    nested: bool,
    inputs: &[(&str, &[&str])],
) -> Result<FixtureRequest, std::fmt::Error> {
    let mut text = String::new();
    let mut questions = Vec::new();
    for (case, inputs) in inputs {
        if nested {
            writeln!(text, "special_project = {{\n requirements = {{")?;
        } else {
            writeln!(text, "{case} = {{")?;
        }
        for field in fields {
            for input in *inputs {
                writeln!(text, " {field} = {input}")?;
            }
            let mut question = FixtureFieldQuestion::new(registry, *case, *field).with_parsing();
            if nested {
                question = question.with_parent_field("requirements");
            }
            questions.push(question);
        }
        if nested {
            writeln!(text, " }}\n key = {case}")?;
        }
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
    field_count: usize,
    case_inputs: &[(&str, &[&str])],
    conversion: &GrammarProperty<Option<pdx_native::NumericConversion>>,
) -> Result<Observations, Box<dyn std::error::Error>> {
    if answer.completeness != Completeness::Complete
        || !answer.gaps.is_empty()
        || answer.value.diagnostic_coverage
            != (DiagnosticCoverage::Complete {
                window: DiagnosticWindow::FixtureFileLoad,
            })
        || answer.value.field_outcomes.len() != field_count * case_inputs.len()
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
        let inputs = case_inputs
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
        let mut row = serde_json::json!({"inputs": inputs, "stored": occurrences.iter().map(|item| &item.value).collect::<Vec<_>>(), "final": final_value, "diagnostics": diagnostics});
        if let Some(reading) = final_value.as_ref().and_then(review_value) {
            row["readings"] = serde_json::json!(
                occurrences
                    .iter()
                    .filter_map(|item| review_value(&item.value))
                    .collect::<Vec<_>>()
            );
            row["final_reading"] = reading;
        }
        report.insert(
            format!(
                "{registry}/{}/{}",
                outcome.question.field, outcome.question.definition
            ),
            row,
        );
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
    let (representation, width, scale, signedness) = match stored {
        FixtureValue::Integer(_) => (
            NumericRepresentation::Integer,
            32,
            Some(1),
            Some(pdx_native::NumericSignedness::Signed),
        ),
        FixtureValue::FixedPoint { scale, .. } => (
            NumericRepresentation::Integer,
            64,
            Some(*scale),
            Some(pdx_native::NumericSignedness::Signed),
        ),
        FixtureValue::Float { .. } => (NumericRepresentation::BinaryFloat, 32, None, None),
        FixtureValue::Integer16 { .. } => (NumericRepresentation::Integer, 16, Some(1), None),
        _ => return Err("unexpected numeric storage type".into()),
    };
    if conversion.representation != GrammarProperty::Known(representation)
        || signedness.is_some_and(|sign| conversion.signedness != GrammarProperty::Known(sign))
        || conversion.width_bits != GrammarProperty::Known(width)
        || conversion.scale != GrammarProperty::Known(scale)
    {
        return Err(format!("static/live conversion conflict: {conversion:?}; {stored:?}").into());
    }
    Ok(())
}

fn review_value(value: &FixtureValue) -> Option<serde_json::Value> {
    match value {
        FixtureValue::Float { bits } => {
            Some(serde_json::json!({"decimal": format!("{:?}", f64::from(f32::from_bits(*bits)))}))
        }
        FixtureValue::Integer16 { bits } => {
            Some(serde_json::json!({"signed": *bits as i16, "unsigned": bits}))
        }
        _ => None,
    }
}
