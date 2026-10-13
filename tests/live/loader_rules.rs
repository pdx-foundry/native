//! Fixture controls of the rules that `Native::loader_rules` states (SDK-795).
//!
//! Each control reads the static answer, builds a fixture and predicts its observation from the
//! stated rule, and compares the fixture observation with the prediction. The expected outcome
//! comes from the stated rule, never from the registry.
use super::*;
use pdx_native::{
    DefinitionForm, DefinitionName, DefinitionStatement, DiagnosticJoin, DuplicateRule,
    FixtureFieldQuestion, FixtureObservation, FixtureRequest, FixtureStorage, FixtureValue,
    GrammarProperty, LoaderRules,
};

/// A template registry whose stated duplicate rule is `Merges`.
const AUTHORITIES: &str = "common/governments/authorities";
/// A custom loader that names one root key by a field, and has an inline fixture binding.
const SPECIAL_PROJECTS: &str = "common/special_projects";

const KEY: &str = "native_rule_definition";
const FIRST_KEPT: &str = "first_kept";
const FIRST_REPEATED: &str = "first_repeated";
const SECOND_REPEATED: &str = "second_repeated";

/// One rule shape that a fixture reaches: the registry and the form of its fixture.
#[derive(Clone, Copy)]
pub struct Shape {
    registry: &'static str,
    fixture: Fixture,
}

/// The form of a control's fixture.
#[derive(Clone, Copy)]
enum Fixture {
    /// One key defined twice, in one file or across two.
    RepeatedKey {
        fields: RepeatedFields,
        two_files: bool,
    },
    /// One definition in the stated `RootKey(k)` / `Field(f)` form.
    NamedByField,
}

const TRADITION_FIELDS: RepeatedFields = RepeatedFields {
    kept: "custom_tooltip",
    repeated: "unlocks_agenda",
};

impl Shape {
    /// `common/traditions` states `LastWins`; its key is repeated in one file.
    pub const LAST_WINS: Self = Self {
        registry: TRADITIONS,
        fixture: Fixture::RepeatedKey {
            fields: TRADITION_FIELDS,
            two_files: false,
        },
    };
    /// `common/governments/authorities` states `Merges`; its key is repeated in one file.
    pub const MERGES: Self = Self {
        registry: AUTHORITIES,
        fixture: Fixture::RepeatedKey {
            fields: RepeatedFields {
                kept: "icon",
                repeated: "localization_postfix",
            },
            two_files: false,
        },
    };
    /// The build-wide file order, with the key of `common/traditions` repeated across two files.
    pub const FILE_ORDER: Self = Self {
        registry: TRADITIONS,
        fixture: Fixture::RepeatedKey {
            fields: TRADITION_FIELDS,
            two_files: true,
        },
    };
    /// `common/special_projects` names its one root key by a field.
    pub const FIELD_NAME: Self = Self {
        registry: SPECIAL_PROJECTS,
        fixture: Fixture::NamedByField,
    };
}

/// The two string fields of a repeated-key fixture: one that only the first definition sets and
/// one that both set.
#[derive(Clone, Copy)]
struct RepeatedFields {
    kept: &'static str,
    repeated: &'static str,
}

/// A source location: fixture file and one-based line.
type Location = (String, u64);

/// What the stated rules predict for a fixture.
enum Prediction {
    /// Two definitions of one key.
    Repeat {
        fields: RepeatedFields,
        loaded_files: Vec<String>,
        constructions: Vec<Location>,
        kept: &'static str,
        already_exists: Vec<Location>,
    },
    /// One definition that the requested name reaches only through the stated field.
    NamedByField { construction: Location },
}

pub async fn control(shape: Shape) -> Outcome {
    let registry = shape.registry;
    let recorded = tempfile::tempdir()?;
    let native = Native::open(std::env::var_os("STELLARIS_PATH").unwrap())?
        .record_answers_to(recorded.path());
    let rules = native.loader_rules(registry)?;
    assert_recorded_rules(recorded.path(), registry, &rules)?;
    let (request, prediction) = match shape.fixture {
        Fixture::RepeatedKey { fields, two_files } => {
            repeated_key(registry, fields, two_files, &rules.value)?
        }
        Fixture::NamedByField => named_by_field(registry, &rules.value)?,
    };

    let mut game = native
        .start_game(options().fixture(request.clone()))
        .await?;
    let mut result = async {
        let answer = game.observe_fixture().await?;
        if answer.completeness != Completeness::Complete {
            return Err(format!("loader rule fixture was incomplete: {answer:?}").into());
        }
        compare(&answer.value, &prediction)?;
        assert_recorded_fixture(recorded.path(), request, &answer).await
    }
    .await;
    and_close(&mut result, &mut game).await;
    result
}

/// A recorded run reads back the same static answer apart from `Basis`.
fn assert_recorded_rules(
    directory: &std::path::Path,
    registry: &str,
    answer: &Answer<LoaderRules>,
) -> Outcome {
    let mut expected = answer.clone();
    expected.source.basis = Basis::Recorded;
    if Native::from_recorded_answers(directory)?.loader_rules(registry)? != expected {
        return Err(format!("recorded loader rules differ for {registry}").into());
    }

    Ok(())
}

/// The stated definition-name forms, whether or not every definition has one.
fn stated_forms(rules: &LoaderRules) -> &[DefinitionForm] {
    match &rules.definition_names {
        GrammarProperty::Known(forms) | GrammarProperty::Partial(forms) => forms,
        GrammarProperty::Unresolved => &[],
    }
}

/// A key defined twice, and what the stated duplicate rule and file order predict. The first
/// definition sets both fields; the second sets only the repeated one. The registry must name a
/// root definition by its key, which the observed constructions then check.
fn repeated_key(
    registry: &str,
    fields: RepeatedFields,
    two_files: bool,
    rules: &LoaderRules,
) -> Result<(FixtureRequest, Prediction), Box<dyn std::error::Error>> {
    let root_key = DefinitionForm {
        statement: DefinitionStatement::Root,
        name: DefinitionName::Key,
    };
    if !stated_forms(rules).contains(&root_key) {
        return Err(format!("{registry} does not name root definitions by key: {rules:?}").into());
    }
    let GrammarProperty::Known(rule) = rules.duplicate_rule else {
        return Err(format!("{registry} states no duplicate rule: {rules:?}").into());
    };

    let first = format!(
        "{KEY} = {{\n {} = \"{FIRST_KEPT}\"\n {} = \"{FIRST_REPEATED}\"\n}}\n",
        fields.kept, fields.repeated
    );
    let second = format!(
        "{KEY} = {{\n {} = \"{SECOND_REPEATED}\"\n}}\n",
        fields.repeated
    );
    let questions =
        [fields.kept, fields.repeated].map(|field| FixtureFieldQuestion::new(registry, KEY, field));

    let (request, statements) = if two_files {
        // The stated order is byte order: `B` (0x42) before `a` (0x61). An order that ignores
        // case would load `a` first.
        let mut files =
            ["native_rule_a", "native_rule_B"].map(|stem| format!("{registry}/{stem}.txt"));
        files.sort();
        let [first_file, second_file] = files;
        let request = FixtureRequest::field_outcomes(first_file.clone(), first, questions)
            .with_file(second_file.clone(), second);
        (request, [(first_file, 1), (second_file, 1)])
    } else {
        let file = format!("{registry}/native_rule.txt");
        let request =
            FixtureRequest::field_outcomes(file.clone(), format!("{first}{second}"), questions);
        (request, [(file.clone(), 1), (file, 5)])
    };

    Ok((request, predict_repeat(rule, fields, statements)))
}

/// The observation that the duplicate rule predicts for two definitions of one key, in load
/// order.
fn predict_repeat(
    rule: DuplicateRule,
    fields: RepeatedFields,
    statements: [Location; 2],
) -> Prediction {
    let mut loaded_files: Vec<String> = statements.iter().map(|(file, _)| file.clone()).collect();
    loaded_files.dedup();
    let [first, second] = statements;

    match rule {
        DuplicateRule::LastWins => Prediction::Repeat {
            fields,
            loaded_files,
            constructions: vec![first, second.clone()],
            kept: "",
            already_exists: vec![second],
        },
        DuplicateRule::Merges => Prediction::Repeat {
            fields,
            loaded_files,
            constructions: vec![first],
            kept: FIRST_KEPT,
            already_exists: Vec::new(),
        },
    }
}

/// One definition written in the stated `RootKey(k)` / `Field(f)` form, named only by `f`. The
/// question's field is the one nested field of the inline fixture binding.
fn named_by_field(
    registry: &str,
    rules: &LoaderRules,
) -> Result<(FixtureRequest, Prediction), Box<dyn std::error::Error>> {
    let Some((root, field)) = stated_forms(rules).iter().find_map(|form| match form {
        DefinitionForm {
            statement: DefinitionStatement::RootKey(root),
            name: DefinitionName::Field(field),
        } => Some((root, field)),
        _ => None,
    }) else {
        return Err(format!("{registry} states no field-named root key: {rules:?}").into());
    };

    let file = format!("{registry}/native_rule.txt");
    let text =
        format!("{root} = {{\n requirements = {{\n  fleet_power = 1\n }}\n {field} = {KEY}\n}}\n");
    let question =
        FixtureFieldQuestion::new(registry, KEY, "fleet_power").with_parent_field("requirements");
    let request = FixtureRequest::field_outcomes(file.clone(), text, [question]);

    Ok((
        request,
        Prediction::NamedByField {
            construction: (file, 1),
        },
    ))
}

fn compare(observed: &FixtureObservation, prediction: &Prediction) -> Outcome {
    match prediction {
        Prediction::Repeat {
            fields,
            loaded_files,
            constructions,
            kept,
            already_exists,
        } => {
            if observed.loaded_files != *loaded_files {
                return Err(format!("file order: {observed:?}").into());
            }

            for (field, final_value) in [(fields.kept, *kept), (fields.repeated, SECOND_REPEATED)] {
                compare_field(observed, field, constructions, final_value)?;
            }

            if repeated_key_diagnostics(observed) != *already_exists {
                return Err(format!("repeated-key diagnostics: {observed:?}").into());
            }

            Ok(())
        }
        Prediction::NamedByField { construction } => {
            let [outcome] = observed.field_outcomes.as_slice() else {
                return Err(format!("field-named outcomes: {observed:?}").into());
            };
            if constructions(outcome) != [construction.clone()] {
                return Err(format!("field-named construction: {observed:?}").into());
            }

            Ok(())
        }
    }
}

/// Checks one field of the repeated key: its constructions, at one owner, and its final value.
fn compare_field(
    observed: &FixtureObservation,
    field: &str,
    expected_constructions: &[Location],
    expected_final: &str,
) -> Outcome {
    let outcome = observed
        .field_outcomes
        .iter()
        .find(|outcome| outcome.question.field == field)
        .ok_or_else(|| format!("no outcome for {field}: {observed:?}"))?;
    let one_owner = outcome
        .definitions
        .iter()
        .all(|definition| definition.owner == outcome.definitions[0].owner);
    if constructions(outcome) != expected_constructions || !one_owner {
        return Err(format!("constructions of {field}: {observed:?}").into());
    }

    let FixtureStorage::Observed {
        final_value: Some(FixtureValue::String(stored)),
        ..
    } = &outcome.storage
    else {
        return Err(format!("storage of {field}: {observed:?}").into());
    };
    if stored != expected_final {
        return Err(format!("final value of {field}: {observed:?}").into());
    }

    Ok(())
}

fn constructions(outcome: &pdx_native::FixtureFieldOutcome) -> Vec<Location> {
    outcome
        .definitions
        .iter()
        .map(|definition| (definition.file.clone(), definition.line))
        .collect()
}

/// The source of each `Object with key: … already exists` diagnostic for the fixture's key.
fn repeated_key_diagnostics(observed: &FixtureObservation) -> Vec<Location> {
    let prefix = format!("Object with key: {KEY} already exists");

    observed
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.text.starts_with(&prefix))
        .map(|diagnostic| match &diagnostic.join {
            DiagnosticJoin::Source { file, line, .. } => (file.clone(), *line),
            DiagnosticJoin::Unavailable(_) => (String::new(), 0),
        })
        .collect()
}
