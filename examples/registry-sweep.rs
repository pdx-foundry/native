//! The automatic rate of the registry sweep (SDK-553), from recorded answers and the public API
//! only. The classification rules are in `docs/native/method-authoring.md#report-the-registry-sweep`.
//!
//! `registry-sweep --record DIRECTORY INSTALLATION` records `registries`, and `registry_fields`,
//! `derived_names` and `modifier_families` for each listed registry, into a new directory.
//!
//! `registry-sweep DIRECTORY` reads that directory through `Native::from_recorded_answers` and
//! writes the report. `registry-sweep --baseline BASELINE DIRECTORY` replaces the baseline
//! directory with the report's comparison inputs, one file per registry.
//!
//! `registry-sweep --diff BEFORE AFTER` compares two reports or baseline directories and writes each
//! registry and relationship whose statuses changed, and each registry whose open or accepted
//! gaps changed.
// The report shares only the baseline format and its reader.
#[allow(dead_code)]
#[path = "support/population.rs"]
mod population;

use pdx_native::{
    AcceptedCategories, Answer, BlockFamily, Completeness, DerivedName, EmptyKey, EntryContext,
    EntryScope, Error, Field, FieldCondition, FieldMembers, FieldReadOutcome, FieldReference, Gap,
    GapKind, GapSubject, GrammarProperty, KeyMatch, LookupStage, MissingResult, ModifierFamily,
    Native, ReaderKind, ReferenceTarget, RepeatBehavior, ValueShape, WeightBlock, WeightOtherKeys,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

const USAGE: &str = "usage: registry-sweep --record DIRECTORY INSTALLATION | registry-sweep [--baseline BASELINE] DIRECTORY | registry-sweep --diff BEFORE AFTER";

const FIELDS: &str = "fields";
const READER_KINDS: &str = "reader_kinds";
const SHAPES_AND_CONDITIONS: &str = "shapes_and_conditions";
const REFERENCES: &str = "references";
const NUMERICS: &str = "numerics";
const WEIGHTS: &str = "weights";
const NAMING_RULES: &str = "naming_rules";
const SCOPE_CONTEXT: &str = "scope_context";
/// A registry row's gaps by shape, open or accepted; not relationships.
const OPEN_GAPS: &str = "open_gaps";
const ACCEPTED_GAPS: &str = "accepted_gaps";

/// The outcome of one unit of one relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Complete,
    Partial,
    Failed,
    Unsupported,
    /// The recording holds no answer; not a method failure.
    Missing,
    /// The method did not establish whether the relationship applies.
    Undetermined,
}

impl Status {
    /// Complete when every part is complete, failed when every part failed, otherwise partial.
    fn of_parts(parts: &[Self]) -> Self {
        if parts.iter().all(|part| *part == Self::Complete) {
            Self::Complete
        } else if parts.iter().all(|part| *part == Self::Failed) {
            Self::Failed
        } else {
            Self::Partial
        }
    }

    fn established(established: bool) -> Self {
        if established {
            Self::Complete
        } else {
            Self::Failed
        }
    }

    fn in_denominator(self) -> bool {
        self != Self::Missing
    }
}

/// What one recorded question gave.
enum Outcome<'a, T> {
    Answer(&'a Answer<T>),
    Failed(String),
    Unsupported(String),
    Missing,
}

impl<'a, T> Outcome<'a, T> {
    /// Refuses an unreadable recording, which is no answer.
    fn of(result: &'a Result<Answer<T>, Error>) -> Result<Self, Error> {
        Ok(match result {
            Ok(answer) => Self::Answer(answer),
            Err(Error::NotRecorded { .. }) => Self::Missing,
            Err(Error::Unsupported { reason, .. }) => Self::Unsupported(reason.clone()),
            Err(Error::Recorded(reason)) => return Err(Error::Recorded(reason.clone())),
            Err(error) => Self::Failed(error.to_string()),
        })
    }

    fn answer(&self) -> Option<&'a Answer<T>> {
        match self {
            Self::Answer(answer) => Some(answer),
            _ => None,
        }
    }
}

/// A registry relationship's status, with the reason of a failed or unsupported one.
struct RegistryStatus {
    status: Status,
    reason: Option<String>,
}

impl RegistryStatus {
    fn new(status: Status) -> Self {
        Self {
            status,
            reason: None,
        }
    }

    /// The baseline form, such as `failed: the method failed: …`.
    fn text(&self) -> String {
        let status = serde_json::to_value(self.status).expect("statuses serialize");
        let status = status.as_str().expect("a status is a string");
        match &self.reason {
            Some(reason) => format!("{status}: {reason}"),
            None => status.to_owned(),
        }
    }
}

/// Complete with an answer and no gap that can hide a field.
fn fields_status(outcome: &Outcome<Vec<Field>>) -> RegistryStatus {
    match outcome {
        Outcome::Answer(answer) => {
            let hides_a_field = answer.gaps.iter().any(|gap| {
                matches!(
                    gap.kind,
                    GapKind::UnresolvedPath
                        | GapKind::UnnamedField
                        | GapKind::UnresolvedReader
                        | GapKind::UnreadableInput
                ) && matches!(gap.subject, None | Some(GapSubject::Registry { .. }))
            });
            RegistryStatus::new(if hides_a_field {
                Status::Partial
            } else {
                Status::Complete
            })
        }
        other => error_status(other),
    }
}

/// Complete when both naming questions answer completely, failed when neither answers.
fn naming_status<A, B>(derived: &Outcome<A>, families: &Outcome<B>) -> RegistryStatus {
    match (derived, families) {
        (Outcome::Missing, _) | (_, Outcome::Missing) => RegistryStatus::new(Status::Missing),
        (Outcome::Answer(derived), Outcome::Answer(families)) => {
            let complete = [derived.completeness, families.completeness]
                .iter()
                .all(|completeness| *completeness == Completeness::Complete);
            RegistryStatus::new(if complete {
                Status::Complete
            } else {
                Status::Partial
            })
        }
        (Outcome::Answer(_), error) => RegistryStatus {
            status: Status::Partial,
            reason: error_status(error).reason,
        },
        (error, Outcome::Answer(_)) => RegistryStatus {
            status: Status::Partial,
            reason: error_status(error).reason,
        },
        (Outcome::Unsupported(_), Outcome::Unsupported(_)) | (Outcome::Failed(_), _) => {
            error_status(derived)
        }
        (_, failed) => error_status(failed),
    }
}

/// The status of a question that gave no answer.
fn error_status<T>(outcome: &Outcome<T>) -> RegistryStatus {
    let (status, reason) = match outcome {
        Outcome::Answer(_) => unreachable!("an answer has no error status"),
        Outcome::Failed(reason) => (Status::Failed, Some(reason.clone())),
        Outcome::Unsupported(reason) => (Status::Unsupported, Some(reason.clone())),
        Outcome::Missing => (Status::Missing, None),
    };
    RegistryStatus { status, reason }
}

/// The field relationship that a gap of this kind leaves partial when it names the field.
fn gap_relationship(kind: GapKind) -> Option<&'static str> {
    match kind {
        GapKind::UnresolvedReader => Some(READER_KINDS),
        GapKind::UnresolvedStorage => Some(SHAPES_AND_CONDITIONS),
        GapKind::ReaderSemantics => Some(REFERENCES),
        GapKind::NumericConversion => Some(NUMERICS),
        GapKind::UnresolvedPath => Some(SCOPE_CONTEXT),
        _ => None,
    }
}

/// A gap that the method could close but Atlas does not need, with the decision that accepts it.
/// The report counts its gaps apart from the open failure shapes, and no rate counts them.
struct AcceptedLimit {
    /// The ticket and the knowledge page that record the decision.
    decision: &'static str,
    kind: GapKind,
    /// The gap details. In a gap whose subject is an answer item, `{name}` stands for the item's
    /// name.
    details: &'static [&'static str],
    subjects: LimitSubjects,
}

/// The gap subjects that an accepted limit covers.
enum LimitSubjects {
    Any,
    /// These fixed keys of a weight block.
    WeightKeys(&'static [&'static str]),
    /// These fields, by registry and field name.
    Fields(&'static [(&'static str, &'static str)]),
    /// These keys, each one level below one of these fields.
    FieldKeys {
        fields: &'static [(&'static str, &'static str)],
        keys: &'static [&'static str],
    },
    /// A gap of these registries' answers whose subject is the registry.
    Registries(&'static [&'static str]),
    /// A gap whose subject is an item of the answer.
    AnswerItems,
}

impl AcceptedLimit {
    /// Whether this limit accepts `gap` of the answer of `registry` whose value is `fields`.
    fn covers(&self, gap: &Gap, registry: &str, fields: &[Field]) -> bool {
        let subject_covered = match (&self.subjects, &gap.subject) {
            (LimitSubjects::Any, _) => true,
            (LimitSubjects::WeightKeys(keys), Some(GapSubject::KeyPath { path })) => {
                path.last().is_some_and(|key| keys.contains(&key.as_str()))
                    && weight_key(fields, path)
            }
            (LimitSubjects::Fields(listed), Some(GapSubject::Field { name })) => listed
                .iter()
                .any(|(listed_registry, field)| *listed_registry == registry && field == name),
            (LimitSubjects::FieldKeys { fields, keys }, Some(GapSubject::KeyPath { path })) => {
                let [field, key] = path.as_slice() else {
                    return false;
                };

                fields.iter().any(|(listed_registry, listed)| {
                    *listed_registry == registry && listed == field
                }) && keys.contains(&key.as_str())
            }
            (LimitSubjects::Registries(listed), Some(GapSubject::Registry { .. })) => {
                listed.contains(&registry)
            }
            (LimitSubjects::AnswerItems, Some(GapSubject::AnswerItem { .. })) => true,
            _ => false,
        };
        let detail_listed = self.details.iter().any(|detail| match &gap.subject {
            Some(GapSubject::AnswerItem { name }) => detail.replace("{name}", name) == gap.detail,
            _ => *detail == gap.detail,
        });

        gap.kind == self.kind && detail_listed && subject_covered
    }
}

/// Whether `path` names a fixed key of a weight block in `fields`.
fn weight_key(fields: &[Field], path: &[String]) -> bool {
    let Some((key, parents)) = path.split_last() else {
        return false;
    };
    let mut members = fields;
    let mut in_weight_block = false;
    for name in parents {
        let Some(parent) = members.iter().find(|field| &field.name == name) else {
            return false;
        };
        in_weight_block = matches!(parent.members, FieldMembers::WeightBlock(_));
        members = match &parent.members {
            FieldMembers::Fields(children) => children,
            FieldMembers::WeightBlock(block) => fixed_keys(&block.fixed_keys),
            FieldMembers::ModifierBlock(block) => fixed_keys(&block.fixed_keys),
            _ => return false,
        };
    }

    in_weight_block && members.iter().any(|field| &field.name == key)
}

/// The established fixed keys of a block, or none.
fn fixed_keys(keys: &GrammarProperty<Vec<Field>>) -> &[Field] {
    match keys {
        GrammarProperty::Known(keys) | GrammarProperty::Partial(keys) => keys,
        GrammarProperty::Unresolved => &[],
    }
}

/// The `NumericConversion` details of `src/session/numeric.rs` that name a range or storage
/// obstacle. A conversion-path gap is never accepted.
const UNKNOWN_RANGE: &str =
    "Numeric conversion is incomplete: the accepted range is not established.";
const UNKNOWN_STORAGE: &str = "Numeric conversion is incomplete: a storage property (representation, width, signedness or scale) is not established.";
const UNREPRESENTABLE_BOUNDS: &str =
    "Exact binary32 range endpoints cannot be represented by NumericBound.";

/// Each accepted limit once. The first that covers a gap accepts it.
const ACCEPTED_LIMITS: [AcceptedLimit; 11] = [
    AcceptedLimit {
        decision: "SDK-739: docs/native/numeric-conversion.md#token-value-methods",
        kind: GapKind::NumericConversion,
        details: &[UNKNOWN_RANGE],
        subjects: LimitSubjects::WeightKeys(&["days", "months", "years", "factor"]),
    },
    AcceptedLimit {
        decision: "SDK-809: docs/native/numeric-conversion.md#accepted-limit-the-sweeps-numeric-gaps-sdk-809",
        kind: GapKind::NumericConversion,
        details: &[UNKNOWN_RANGE, UNKNOWN_STORAGE, UNREPRESENTABLE_BOUNDS],
        subjects: LimitSubjects::Any,
    },
    AcceptedLimit {
        decision: SDK_811,
        kind: GapKind::UnresolvedCondition,
        details: &[UNRESOLVED_LOADER_CONDITION],
        subjects: LimitSubjects::Fields(&LOADER_CONDITIONS),
    },
    AcceptedLimit {
        decision: SDK_811,
        kind: GapKind::UnresolvedCondition,
        details: &[USE_TIME_SELECTIONS, USE_TIME_CONTEXT],
        subjects: LimitSubjects::Any,
    },
    AcceptedLimit {
        decision: SDK_817,
        kind: GapKind::UnresolvedPath,
        details: &[TOOLTIP_EMBEDDED_POINT],
        subjects: LimitSubjects::Fields(&TOOLTIP_CLAUSES),
    },
    AcceptedLimit {
        decision: SDK_817,
        kind: GapKind::UnresolvedReader,
        details: &[TOOLTIP_DELEGATE],
        subjects: LimitSubjects::Fields(&TOOLTIP_CLAUSES),
    },
    AcceptedLimit {
        decision: SDK_817,
        kind: GapKind::UnresolvedStorage,
        details: &[SCOPED_DESTINATION],
        subjects: LimitSubjects::FieldKeys {
            fields: &TOOLTIP_CLAUSES,
            keys: &["mult", "multiplier"],
        },
    },
    AcceptedLimit {
        decision: SDK_812,
        kind: GapKind::UnresolvedPath,
        details: &ENTRY_CONTEXT_STOPS,
        subjects: LimitSubjects::Fields(&ENTRY_CONTEXTS_WITHOUT_CLAIM),
    },
    AcceptedLimit {
        decision: SDK_812,
        kind: GapKind::UnresolvedPath,
        details: &UNNAMED_EVALUATIONS,
        subjects: LimitSubjects::Registries(&UNNAMED_EVALUATION_REGISTRIES),
    },
    AcceptedLimit {
        decision: SDK_810,
        kind: GapKind::UnresolvedCondition,
        details: &NAME_PROPERTIES,
        subjects: LimitSubjects::AnswerItems,
    },
    AcceptedLimit {
        decision: SDK_810,
        kind: GapKind::UnresolvedCondition,
        details: &EVERY_ITEM_INITIALIZATION,
        subjects: LimitSubjects::Registries(&EVERY_ITEM_REGISTRIES),
    },
];

const SDK_810: &str =
    "SDK-810: docs/native/derived-names.md#accepted-limit-conditions-and-missing-names-sdk-810";

/// The `UnresolvedCondition` details of a derived name in `src/session/names.rs`. A name's
/// template is both the gap's answer item and the `{name}` of its detail.
const NAME_PROPERTIES: [&str; 3] = [
    "what a missing {name} gives is not established",
    "the field values under which the engine uses {name} are not established",
    "{name} also depends on field states that the method does not explore",
];

/// The M452 every-item details of `src/session/names.rs`, each with a reason of
/// `src/session/families.rs`.
const EVERY_ITEM_INITIALIZATION: [&str; 3] = [
    "that the engine initializes every item after reading it is not established, so no name of the initialization is always used: a function forms an address of the item's vtables without calling its constructor, which the method does not follow",
    "that the engine initializes every item after reading it is not established, so no name of the initialization is always used: another function constructs an item outside the database's code",
    "that the engine initializes every item after reading it is not established, so no name of the initialization is always used: a path of a database function keeps an item that it constructed without running its post-read code",
];

/// The registries of the M452 every-item gaps. Such a gap of another registry stays open.
const EVERY_ITEM_REGISTRIES: [&str; 4] = [
    "common/colony_automation_categories",
    "common/economic_categories",
    "common/system_types",
    "common/technology",
];

const SDK_812: &str =
    "SDK-812: docs/native/registry-fields.md#block-entry-context-stops-on-m452-sdk-812";

/// The entry-context details of `src/session/field_entries.rs` and `src/session/callbacks.rs`.
const ENTRY_CONTEXT_STOPS: [&str; 9] = [
    "no evaluation that the method attributes evaluates this block",
    "only tooltip calls name this block; the method established no evaluation of it",
    "some entry scopes of a call site could not be established",
    "a call site has more paths than the method follows (path-limit)",
    "a path to a call site goes around a loop more often than the method follows (loop-limit)",
    "a path to a call site could not be followed (instruction)",
    "the scope comes from a caller, and no direct call to the function was found (no-caller)",
    "the scope comes from more callers than the method follows (caller-depth)",
    "a path does not show which of several candidate blocks the call evaluates (selected-block-identity)",
];

/// The M452 registry gaps of direct evaluations whose block the method cannot name. No field, so
/// no `replace_scopes` claim, has them. SDK-824's `CScriptableValue` evaluators add the
/// federation law and opinion modifier gaps.
const UNNAMED_EVALUATIONS: [&str; 2] = [
    "1 direct evaluation calls in the owner's methods evaluate a block that the method cannot name",
    "2 direct evaluation calls in the owner's methods evaluate a block that the method cannot name",
];

/// The registries of the M452 unnamed-evaluation gaps. Such a gap of another registry stays open.
const UNNAMED_EVALUATION_REGISTRIES: [&str; 11] = [
    "common/colony_automation",
    "common/federation_laws",
    "common/governments/civics",
    "common/map_modes",
    "common/on_actions",
    "common/opinion_modifiers",
    "common/ship_sizes",
    "common/special_projects",
    "common/starbase_buildings",
    "common/starbase_modules",
    "events",
];

/// The M452 trigger, effect and weight fields with an entry-context gap and no field-level
/// `replace_scopes` claim in the config. An entry-context gap of another field stays open.
const ENTRY_CONTEXTS_WITHOUT_CLAIM: [(&str, &str); 93] = [
    ("common/agreement_term_values", "resource_max"),
    ("common/agreement_term_values", "resource_min"),
    ("common/ai_budget", "desired_max"),
    ("common/ai_budget", "desired_min"),
    ("common/ai_budget", "potential"),
    ("common/ai_budget", "weight"),
    ("common/armies", "ai_weight"),
    ("common/armies", "show_tech_unlock_if"),
    ("common/ascension_perks", "on_disabled"),
    ("common/bombardment_stances", "kill_pop_chance"),
    ("common/buildings", "custom_storm_ai_weight"),
    ("common/buildings", "on_queued"),
    ("common/buildings", "on_unqueued"),
    ("common/buildings", "potential"),
    ("common/colony_automation", "available"),
    ("common/colony_types", "potential"),
    ("common/colony_types", "weight_modifier"),
    ("common/council_agendas", "ai_weight"),
    ("common/crisis_levels", "on_unlock"),
    ("common/crisis_objectives", "reward"),
    ("common/decisions", "abort_effect"),
    ("common/decisions", "abort_trigger"),
    ("common/decisions", "ai_weight"),
    ("common/decisions", "allow"),
    ("common/decisions", "custom_tooltip"),
    ("common/decisions", "effect"),
    ("common/decisions", "on_queued"),
    ("common/decisions", "on_unqueued"),
    ("common/decisions", "potential"),
    ("common/deposits", "ai_weight"),
    ("common/deposits", "can_be_cleared"),
    ("common/deposits", "can_be_cleared_potential"),
    ("common/deposits", "drop_weight"),
    ("common/deposits", "on_cleared"),
    ("common/diplo_phrases", "trigger"),
    ("common/dynamic_text", "available"),
    ("common/espionage_assets", "possible"),
    ("common/espionage_assets", "potential"),
    ("common/espionage_operation_types", "on_create"),
    ("common/espionage_operation_types", "on_roll_failed"),
    ("common/federation_laws", "required_centralization"),
    ("common/first_contact", "on_abort"),
    ("common/first_contact", "on_create"),
    ("common/first_contact", "on_roll_failed"),
    ("common/governments/councilors", "ai_priority"),
    ("common/greeting_overlay_sounds", "possible"),
    ("common/greeting_overlay_sounds", "weight"),
    ("common/lawsuits", "effect"),
    ("common/missions/missions", "on_accept"),
    ("common/missions/missions", "on_fail"),
    ("common/missions/missions", "on_issue"),
    ("common/observation_station_missions", "ai_weight"),
    ("common/personalities", "weight_modifier"),
    ("common/pop_categories", "weight"),
    ("common/pop_faction_types", "attraction"),
    ("common/pop_faction_types", "is_potential"),
    ("common/pop_faction_types", "leader"),
    ("common/pop_faction_types", "on_create"),
    ("common/pop_faction_types", "on_set_leader"),
    ("common/pop_faction_types", "valid"),
    ("common/pop_jobs", "possible"),
    ("common/pop_jobs", "weight"),
    ("common/resolutions", "ai_weight"),
    ("common/situations", "monthly_progress"),
    ("common/situations", "on_abort"),
    ("common/situations", "total_progress"),
    ("common/special_projects", "AI_wait_days"),
    ("common/special_projects", "abort_trigger"),
    ("common/special_projects", "fail_trigger"),
    ("common/special_projects", "on_cancel"),
    ("common/special_projects", "on_fail"),
    ("common/strategic_resources", "ai_wants"),
    ("common/strategic_resources", "ai_weight"),
    ("common/strategic_resources", "dynamic_capacity"),
    ("common/system_tooltips", "custom_tooltip"),
    ("common/technology", "cost"),
    ("common/tradable_actions", "ai_acceptance"),
    ("common/traditions", "on_disabled"),
    ("common/traits", "allowed_planet_classes_override"),
    ("common/traits", "archetype_override"),
    ("common/traits", "assembly_score"),
    ("common/traits", "leader_potential_add"),
    ("common/traits", "on_gained_effect"),
    ("common/traits", "selectable_weight"),
    ("common/traits", "species_class_override"),
    ("common/traits", "species_possible_add"),
    ("common/traits", "species_possible_merge_add"),
    ("common/traits", "species_possible_merge_remove"),
    ("common/traits", "species_possible_remove"),
    ("common/traits", "species_potential_add"),
    ("common/traits", "trade_acceptance_weight"),
    ("gfx/portraits/sprite_configurations", "trigger"),
    ("sound/advisor_voice_types", "weight"),
];

const SDK_817: &str =
    "SDK-817: docs/native/triggered-modifiers.md#accepted-limit-tooltip-clauses-sdk-817";

/// The triggered modifier fields of the Tooltip clause variant on M452.
const TOOLTIP_CLAUSES: [(&str, &str); 5] = [
    ("common/pop_jobs", "triggered_country_modifier"),
    ("common/pop_jobs", "triggered_planet_modifier"),
    (
        "common/pop_jobs",
        "triggered_planet_pop_group_modifier_for_all",
    ),
    (
        "common/pop_jobs",
        "triggered_planet_pop_group_modifier_for_species",
    ),
    ("common/pop_jobs", "triggered_system_modifier"),
];

/// The Tooltip clause details of `src/session/triggered_modifiers.rs` and
/// `src/session/scoped_numeric.rs`.
const TOOLTIP_EMBEDDED_POINT: &str =
    "The triggered modifier analysis stopped at triggered-embedded-point.";
const TOOLTIP_DELEGATE: &str = "The reader of keys that the triggered modifier does not name is not established: triggered-delegate.";
const SCOPED_DESTINATION: &str = "Scoped destination vtable is not established.";

const SDK_811: &str =
    "SDK-811: docs/native/registry-fields.md#unresolved-loader-conditions-on-m452-sdk-811";

/// The `UnresolvedCondition` details of `src/session/questions.rs`.
const UNRESOLVED_LOADER_CONDITION: &str =
    "A loader condition remains unresolved; its outcome is retained separately.";
const USE_TIME_SELECTIONS: &str = "Use-time analysis covers local Boolean selections only; enclosing context, other callers and bounded or unsupported paths remain unresolved.";
const USE_TIME_CONTEXT: &str =
    "The local use-time flag test is established; the enclosing selection context is unresolved.";

/// The fields of the M452 loader-condition gaps that SDK-811 accepts, each with its own reopen
/// condition. A loader-condition gap of another field stays open.
const LOADER_CONDITIONS: [(&str, &str); 207] = [
    ("common/buildings", "is_colony"),
    ("common/colony_automation", "buildings"),
    ("common/colony_automation", "job_changes"),
    ("common/colony_types", "ai_building_set_affinity"),
    ("common/country_focus/focus_rewards", "mode"),
    ("common/country_types", "modules"),
    ("common/edicts", "edict_cap_usage"),
    ("common/espionage_operation_types", "target_allow"),
    ("common/espionage_operation_types", "target_potential"),
    ("common/ethics", "leader_background_job_weight"),
    ("common/federation_perks", "triggered_modifier"),
    ("common/federation_types", "levels"),
    ("common/global_ship_designs", "behaviour"),
    ("common/global_ship_designs", "custom_military_power"),
    ("common/global_ship_designs", "custom_trade_weight"),
    ("common/global_ship_designs", "hide_size"),
    ("common/global_ship_designs", "is_boss"),
    ("common/global_ship_designs", "owned_upgrade"),
    ("common/global_ship_designs", "parent"),
    ("common/global_ship_designs", "required_component"),
    ("common/global_ship_designs", "resource_restriction"),
    ("common/global_ship_designs", "section"),
    ("common/global_ship_designs", "ship_size"),
    ("common/global_ship_designs", "upgrades_to"),
    ("common/governments/authorities", "possible"),
    ("common/governments/authorities", "potential"),
    ("common/governments/civics", "has_secondary_species"),
    ("common/governments/civics", "leader_background_job_weight"),
    ("common/governments/civics", "modification"),
    ("common/governments/civics", "possible"),
    ("common/governments/civics", "potential"),
    ("common/leader_classes", "ai_weight_for_planet_assignment"),
    ("common/leader_classes", "default_category"),
    ("common/leader_classes", "leader_background_job_weight"),
    ("common/megastructures", "upgrade_desc"),
    ("common/mutations", "ship_components"),
    ("common/personalities", "ship_roles"),
    ("common/policies", "category"),
    ("common/pop_categories", "can_change_category"),
    ("common/pop_faction_types", "actions"),
    ("common/ship_sizes", "construction_type"),
    ("common/ship_sizes", "graphical_culture"),
    ("common/ship_sizes", "hero_ship"),
    ("common/situations", "picture"),
    ("common/situations", "progress_direction"),
    ("common/situations", "stages"),
    ("common/special_projects", "cost"),
    ("common/special_projects", "on_progress_25"),
    ("common/special_projects", "on_progress_50"),
    ("common/special_projects", "on_progress_75"),
    ("common/special_projects", "on_start"),
    ("common/special_projects", "on_success"),
    ("common/special_projects", "tech_department"),
    ("common/species_classes", "possible"),
    ("common/species_classes", "possible_secondary"),
    ("common/species_classes", "randomized"),
    ("common/starbase_levels", "picture"),
    ("common/starbase_modules", "collected_colony_resources"),
    ("common/starbase_modules", "collected_system_resources"),
    ("common/start_screen_messages", "trigger"),
    ("common/technology", "mod_weight_if_group_picked"),
    ("common/traits", "forced_happiness"),
    ("common/traits", "icon"),
    ("common/traits", "leader_class"),
    (
        "common/traits",
        "triggered_background_planet_pop_group_modifier",
    ),
    ("common/war_goals", "forbidden_peace_offers"),
    ("events", "agreement_event.desc"),
    ("events", "agreement_event.desc_clear"),
    ("events", "agreement_event.mean_time_to_happen"),
    ("events", "agreement_event.picture"),
    ("events", "agreement_event.show_sound"),
    ("events", "agreement_event.weight_multiplier"),
    ("events", "astral_rift_event.desc"),
    ("events", "astral_rift_event.desc_clear"),
    ("events", "astral_rift_event.mean_time_to_happen"),
    ("events", "astral_rift_event.picture"),
    ("events", "astral_rift_event.show_sound"),
    ("events", "astral_rift_event.weight_multiplier"),
    ("events", "bypass_event.desc"),
    ("events", "bypass_event.desc_clear"),
    ("events", "bypass_event.mean_time_to_happen"),
    ("events", "bypass_event.picture"),
    ("events", "bypass_event.show_sound"),
    ("events", "bypass_event.weight_multiplier"),
    ("events", "carrier_event.desc"),
    ("events", "carrier_event.desc_clear"),
    ("events", "carrier_event.mean_time_to_happen"),
    ("events", "carrier_event.picture"),
    ("events", "carrier_event.show_sound"),
    ("events", "carrier_event.weight_multiplier"),
    ("events", "colony_event.desc"),
    ("events", "colony_event.desc_clear"),
    ("events", "colony_event.mean_time_to_happen"),
    ("events", "colony_event.picture"),
    ("events", "colony_event.show_sound"),
    ("events", "colony_event.weight_multiplier"),
    ("events", "cosmic_storm_event.desc"),
    ("events", "cosmic_storm_event.desc_clear"),
    ("events", "cosmic_storm_event.mean_time_to_happen"),
    ("events", "cosmic_storm_event.picture"),
    ("events", "cosmic_storm_event.show_sound"),
    ("events", "cosmic_storm_event.weight_multiplier"),
    ("events", "cosmic_storm_influence_field_event.desc"),
    ("events", "cosmic_storm_influence_field_event.desc_clear"),
    (
        "events",
        "cosmic_storm_influence_field_event.mean_time_to_happen",
    ),
    ("events", "cosmic_storm_influence_field_event.picture"),
    ("events", "cosmic_storm_influence_field_event.show_sound"),
    (
        "events",
        "cosmic_storm_influence_field_event.weight_multiplier",
    ),
    ("events", "country_event.desc"),
    ("events", "country_event.desc_clear"),
    ("events", "country_event.mean_time_to_happen"),
    ("events", "country_event.picture"),
    ("events", "country_event.show_sound"),
    ("events", "country_event.weight_multiplier"),
    ("events", "espionage_operation_event.desc"),
    ("events", "espionage_operation_event.desc_clear"),
    ("events", "espionage_operation_event.mean_time_to_happen"),
    ("events", "espionage_operation_event.picture"),
    ("events", "espionage_operation_event.show_sound"),
    ("events", "espionage_operation_event.weight_multiplier"),
    ("events", "event.desc"),
    ("events", "event.desc_clear"),
    ("events", "event.mean_time_to_happen"),
    ("events", "event.picture"),
    ("events", "event.show_sound"),
    ("events", "event.weight_multiplier"),
    ("events", "first_contact_event.desc"),
    ("events", "first_contact_event.desc_clear"),
    ("events", "first_contact_event.mean_time_to_happen"),
    ("events", "first_contact_event.picture"),
    ("events", "first_contact_event.show_sound"),
    ("events", "first_contact_event.weight_multiplier"),
    ("events", "fleet_event.desc"),
    ("events", "fleet_event.desc_clear"),
    ("events", "fleet_event.mean_time_to_happen"),
    ("events", "fleet_event.picture"),
    ("events", "fleet_event.show_sound"),
    ("events", "fleet_event.weight_multiplier"),
    ("events", "leader_event.desc"),
    ("events", "leader_event.desc_clear"),
    ("events", "leader_event.mean_time_to_happen"),
    ("events", "leader_event.picture"),
    ("events", "leader_event.show_sound"),
    ("events", "leader_event.weight_multiplier"),
    ("events", "observer_event.desc"),
    ("events", "observer_event.desc_clear"),
    ("events", "observer_event.mean_time_to_happen"),
    ("events", "observer_event.picture"),
    ("events", "observer_event.show_sound"),
    ("events", "observer_event.weight_multiplier"),
    ("events", "planet_event.desc"),
    ("events", "planet_event.desc_clear"),
    ("events", "planet_event.mean_time_to_happen"),
    ("events", "planet_event.picture"),
    ("events", "planet_event.show_sound"),
    ("events", "planet_event.weight_multiplier"),
    ("events", "pop_event.desc"),
    ("events", "pop_event.desc_clear"),
    ("events", "pop_event.mean_time_to_happen"),
    ("events", "pop_event.picture"),
    ("events", "pop_event.show_sound"),
    ("events", "pop_event.weight_multiplier"),
    ("events", "pop_faction_event.desc"),
    ("events", "pop_faction_event.desc_clear"),
    ("events", "pop_faction_event.mean_time_to_happen"),
    ("events", "pop_faction_event.picture"),
    ("events", "pop_faction_event.show_sound"),
    ("events", "pop_faction_event.weight_multiplier"),
    ("events", "pop_group_event.desc"),
    ("events", "pop_group_event.desc_clear"),
    ("events", "pop_group_event.mean_time_to_happen"),
    ("events", "pop_group_event.picture"),
    ("events", "pop_group_event.show_sound"),
    ("events", "pop_group_event.weight_multiplier"),
    ("events", "ship_event.desc"),
    ("events", "ship_event.desc_clear"),
    ("events", "ship_event.mean_time_to_happen"),
    ("events", "ship_event.picture"),
    ("events", "ship_event.show_sound"),
    ("events", "ship_event.weight_multiplier"),
    ("events", "situation_event.desc"),
    ("events", "situation_event.desc_clear"),
    ("events", "situation_event.mean_time_to_happen"),
    ("events", "situation_event.picture"),
    ("events", "situation_event.show_sound"),
    ("events", "situation_event.weight_multiplier"),
    ("events", "starbase_event.desc"),
    ("events", "starbase_event.desc_clear"),
    ("events", "starbase_event.mean_time_to_happen"),
    ("events", "starbase_event.picture"),
    ("events", "starbase_event.show_sound"),
    ("events", "starbase_event.weight_multiplier"),
    ("events", "system_event.desc"),
    ("events", "system_event.desc_clear"),
    ("events", "system_event.mean_time_to_happen"),
    ("events", "system_event.picture"),
    ("events", "system_event.show_sound"),
    ("events", "system_event.weight_multiplier"),
    (
        "gfx/projectiles/planet_destruction",
        "hit_entity_positioning",
    ),
    ("interface/resource_groups", "category"),
    ("interface/resource_groups", "onclick"),
    ("interface/resource_groups", "resources"),
    ("interface/resource_groups", "show_only_if_owned"),
    ("map/galaxy", "arms"),
    ("map/galaxy", "countries"),
    ("map/galaxy", "fallen_empires"),
    ("map/galaxy", "ring"),
];

/// The accepted limit that covers `gap` of the answer of `registry` whose value is `fields`.
fn accepted_limit(gap: &Gap, registry: &str, fields: &[Field]) -> Option<&'static AcceptedLimit> {
    ACCEPTED_LIMITS
        .iter()
        .find(|limit| limit.covers(gap, registry, fields))
}

/// A gap that leaves work open: not `OutsideMethod` and not an accepted limit.
fn is_open(gap: &Gap, registry: &str, fields: &[Field]) -> bool {
    gap.kind != GapKind::OutsideMethod && accepted_limit(gap, registry, fields).is_none()
}

/// The status of each field relationship that applies to `field`, a root field of an answer
/// with these open gaps.
fn field_statuses(field: &Field, open_gaps: &[&Gap]) -> BTreeMap<&'static str, Status> {
    let mut statuses = property_statuses(field);
    for gap in open_gaps {
        let relationship = match &gap.subject {
            Some(GapSubject::Field { name }) if *name == field.name => gap_relationship(gap.kind),
            Some(GapSubject::KeyPath { path }) if path[0] == field.name => Some(WEIGHTS),
            _ => None,
        };
        if let Some(status) = relationship.and_then(|relationship| statuses.get_mut(relationship))
            && *status == Status::Complete
        {
            *status = Status::Partial;
        }
    }
    statuses
}

/// The status of each field relationship that applies to `field`, from its properties alone.
fn property_statuses(field: &Field) -> BTreeMap<&'static str, Status> {
    let unknown_kind = field.reader.kind == ReaderKind::Unknown;
    let mut statuses = BTreeMap::from([
        (READER_KINDS, reader_kind_status(field)),
        (SHAPES_AND_CONDITIONS, shape_status(field)),
        (SCOPE_CONTEXT, scope_status(field)),
    ]);
    let reads_reference = field.reader.kind == ReaderKind::Reference
        || field.read.iter().any(|alternative| {
            matches!(&alternative.outcome, FieldReadOutcome::Read { reader, .. } if reader.kind == ReaderKind::Reference)
        });
    if reads_reference {
        statuses.insert(REFERENCES, reference_status(&field.reference));
    } else if unknown_kind {
        statuses.insert(REFERENCES, Status::Undetermined);
    }
    if matches!(
        field.reader.kind,
        ReaderKind::Integer
            | ReaderKind::FixedPoint
            | ReaderKind::Float
            | ReaderKind::ScopedNumeric
    ) {
        statuses.insert(NUMERICS, numeric_status(&field.reader.numeric));
    } else if unknown_kind {
        statuses.insert(NUMERICS, Status::Undetermined);
    }
    match &field.members {
        FieldMembers::WeightBlock(block) => {
            statuses.insert(WEIGHTS, weight_status(block));
        }
        _ if field.reader.family == BlockFamily::Weight => {
            statuses.insert(WEIGHTS, Status::Failed);
        }
        _ if unknown_kind
            || (field.reader.kind == ReaderKind::Block
                && field.reader.family == BlockFamily::Unknown) =>
        {
            statuses.insert(WEIGHTS, Status::Undetermined);
        }
        _ => {}
    }
    statuses
}

fn reader_kind_status(field: &Field) -> Status {
    Status::of_parts(&[
        Status::established(field.reader.id.is_some()),
        Status::established(field.reader.kind != ReaderKind::Unknown),
    ])
}

fn shape_status(field: &Field) -> Status {
    let value = field.shape.value != ValueShape::Unknown;
    let repeat = field.shape.repeat != RepeatBehavior::Unknown;
    let outcomes = field
        .read
        .iter()
        .filter(|alternative| !matches!(alternative.outcome, FieldReadOutcome::Unresolved))
        .count();
    let conditions = field
        .read
        .iter()
        .all(|alternative| resolved(&alternative.condition));
    if value && repeat && conditions && outcomes == field.read.len() {
        Status::Complete
    } else if !value && !repeat && outcomes == 0 {
        Status::Failed
    } else {
        Status::Partial
    }
}

/// Whether a condition has no unresolved term.
fn resolved(condition: &FieldCondition) -> bool {
    match condition {
        FieldCondition::Unresolved => false,
        FieldCondition::All(terms) => terms.iter().all(resolved),
        _ => true,
    }
}

fn reference_status(reference: &FieldReference) -> Status {
    let FieldReference::Lookups(lookups) = reference else {
        return Status::Failed;
    };
    let targeted = lookups
        .iter()
        .filter(|lookup| !matches!(lookup.target, ReferenceTarget::Unresolved))
        .count();
    let established = lookups.iter().all(|lookup| {
        lookup.stage != LookupStage::Unresolved
            && lookup.key_match != KeyMatch::Unresolved
            && lookup.empty_key != EmptyKey::Unresolved
            && lookup.on_missing != MissingResult::Unresolved
    });
    match targeted {
        0 => Status::Failed,
        count if count == lookups.len() && established => Status::Complete,
        _ => Status::Partial,
    }
}

/// The `Partial` wrapper of a conversion does not count: a `NumericConversion` gap on the field
/// makes it partial (`docs/native/numeric-conversion.md`).
fn numeric_status<T>(numeric: &GrammarProperty<Option<T>>) -> Status {
    match numeric {
        GrammarProperty::Known(Some(_)) | GrammarProperty::Partial(Some(_)) => Status::Complete,
        GrammarProperty::Known(None)
        | GrammarProperty::Partial(None)
        | GrammarProperty::Unresolved => Status::Failed,
    }
}

fn weight_status(block: &WeightBlock) -> Status {
    if weight_established(block) {
        Status::Complete
    } else {
        Status::Partial
    }
}

/// Every property of the block and of each nested weight key is established.
fn weight_established(block: &WeightBlock) -> bool {
    let GrammarProperty::Known(keys) = &block.fixed_keys else {
        return false;
    };
    let nested = keys.iter().all(|key| match &key.members {
        FieldMembers::WeightBlock(nested) => weight_established(nested),
        _ => true,
    });
    let repeat = match &block.operations {
        GrammarProperty::Known(operations) => {
            operations.is_empty() || block.operation_repeat != RepeatBehavior::Unknown
        }
        _ => false,
    };
    nested
        && repeat
        && matches!(block.scalar, GrammarProperty::Known(_))
        && matches!(
            block.other_keys,
            WeightOtherKeys::Rejected | WeightOtherKeys::Triggers(GrammarProperty::Known(_))
        )
}

fn scope_status(field: &Field) -> Status {
    let mut parts = vec![
        match field.read_scope {
            GrammarProperty::Known(_) => Status::Complete,
            GrammarProperty::Partial(_) => Status::Partial,
            GrammarProperty::Unresolved => Status::Failed,
        },
        Status::established(!matches!(
            field.accepted_categories,
            AcceptedCategories::Unresolved
        )),
    ];
    if matches!(
        field.reader.family,
        BlockFamily::Trigger | BlockFamily::Effect | BlockFamily::Weight
    ) {
        parts.push(match field.entry_contexts.as_slice() {
            [] => Status::Failed,
            contexts if contexts.iter().all(established_context) => Status::Complete,
            _ => Status::Partial,
        });
    }
    Status::of_parts(&parts)
}

fn established_context(context: &EntryContext) -> bool {
    [&context.this, &context.root]
        .into_iter()
        .chain(&context.from)
        .chain(&context.prev)
        .all(|scope| !matches!(scope, EntryScope::Unresolved))
}

/// Counts of units by status, with the rate over the denominator.
#[derive(Default)]
struct Counts(BTreeMap<Status, usize>);

impl Counts {
    fn add(&mut self, status: Status) {
        *self.0.entry(status).or_default() += 1;
    }

    fn report(&self) -> Value {
        let denominator: usize = self
            .0
            .iter()
            .filter(|(status, _)| status.in_denominator())
            .map(|(_, count)| count)
            .sum();
        let complete = self.0.get(&Status::Complete).copied().unwrap_or_default();
        let mut report = serde_json::to_value(&self.0).expect("counts serialize");
        report["denominator"] = json!(denominator);
        report["rate"] = if denominator == 0 {
            Value::Null
        } else {
            json!(((complete as f64 / denominator as f64) * 10_000.0).round() / 10_000.0)
        };
        report
    }
}

/// The gaps of one failure shape and the registries that have them.
#[derive(Default, Serialize)]
struct ShapeCount {
    gaps: usize,
    registries: BTreeSet<String>,
}

impl ShapeCount {
    fn add(&mut self, registry: &str) {
        self.gaps += 1;
        self.registries.insert(registry.to_owned());
    }
}

/// One registry's gaps that are not `OutsideMethod`, by shape.
#[derive(Default)]
struct RegistryGaps {
    open: BTreeMap<String, usize>,
    accepted: BTreeMap<String, usize>,
}

/// The report state, gathered registry by registry.
#[derive(Default)]
struct Sweep {
    totals: BTreeMap<&'static str, Counts>,
    /// Each registry's comparison inputs: its registry statuses and its field counts.
    registries: BTreeMap<String, Value>,
    /// Field counts by reader identity, then relationship.
    readers: BTreeMap<String, BTreeMap<&'static str, Counts>>,
    /// Each field's statuses, by `registry#field`.
    fields: BTreeMap<String, BTreeMap<&'static str, Status>>,
    /// Registries whose `registry_fields` question has no answer, so no field units.
    registries_without_field_units: Counts,
    open_shapes: BTreeMap<String, ShapeCount>,
    /// The gaps of each accepted limit, by its decision, then shape.
    accepted_shapes: BTreeMap<&'static str, BTreeMap<String, ShapeCount>>,
    errors: BTreeMap<String, BTreeSet<String>>,
}

impl Sweep {
    /// Add one registry's outcomes of the three registry questions.
    fn add(
        &mut self,
        registry: &str,
        fields: Outcome<Vec<Field>>,
        derived: Outcome<Vec<DerivedName>>,
        families: Outcome<Vec<ModifierFamily>>,
    ) {
        let answered = fields
            .answer()
            .map_or(&[][..], |answer| answer.value.as_slice());
        let mut gaps = RegistryGaps::default();
        self.add_failure_shapes(registry, "registry_fields", &fields, answered, &mut gaps);
        self.add_failure_shapes(registry, "derived_names", &derived, &[], &mut gaps);
        self.add_failure_shapes(registry, "modifier_families", &families, &[], &mut gaps);

        let mut row = serde_json::Map::new();
        for (key, shapes) in [(OPEN_GAPS, gaps.open), (ACCEPTED_GAPS, gaps.accepted)] {
            if !shapes.is_empty() {
                row.insert(key.into(), json!(shapes));
            }
        }
        for (relationship, status) in [
            (FIELDS, fields_status(&fields)),
            (NAMING_RULES, naming_status(&derived, &families)),
        ] {
            self.totals
                .entry(relationship)
                .or_default()
                .add(status.status);
            row.insert(relationship.into(), json!(status.text()));
        }

        let mut counts: BTreeMap<&'static str, Counts> = BTreeMap::new();
        let open_gaps: Vec<_> = match fields.answer() {
            Some(answer) => answer
                .gaps
                .iter()
                .filter(|gap| is_open(gap, registry, answered))
                .collect(),
            None => {
                self.registries_without_field_units
                    .add(fields_status(&fields).status);
                Vec::new()
            }
        };
        for field in answered {
            let statuses = field_statuses(field, &open_gaps);
            let by_reader = self.readers.entry(reader_name(field)).or_default();
            for (relationship, status) in &statuses {
                counts.entry(relationship).or_default().add(*status);
                by_reader.entry(relationship).or_default().add(*status);
                self.totals.entry(relationship).or_default().add(*status);
            }
            self.fields
                .insert(format!("{registry}#{}", field.name), statuses);
        }
        for (relationship, counts) in counts {
            row.insert(relationship.into(), counts.report());
        }
        self.registries.insert(registry.to_owned(), row.into());
    }

    /// Count each gap that is not `OutsideMethod` under its shape, open or accepted, in the
    /// report and in this registry's `gaps`. `fields` is the answer's value when it has fields.
    fn add_failure_shapes<T>(
        &mut self,
        registry: &str,
        operation: &str,
        outcome: &Outcome<T>,
        fields: &[Field],
        gaps: &mut RegistryGaps,
    ) {
        match outcome {
            Outcome::Answer(answer) => {
                for gap in answer
                    .gaps
                    .iter()
                    .filter(|gap| gap.kind != GapKind::OutsideMethod)
                {
                    let shape = format!(
                        "{operation} | {:?}: {}",
                        gap.kind,
                        without_digits(&gap.detail)
                    );
                    let Some(limit) = accepted_limit(gap, registry, fields) else {
                        *gaps.open.entry(shape.clone()).or_default() += 1;
                        self.open_shapes.entry(shape).or_default().add(registry);
                        continue;
                    };

                    *gaps.accepted.entry(shape.clone()).or_default() += 1;
                    self.accepted_shapes
                        .entry(limit.decision)
                        .or_default()
                        .entry(shape)
                        .or_default()
                        .add(registry);
                }
            }
            Outcome::Failed(reason) | Outcome::Unsupported(reason) => {
                self.errors
                    .entry(format!("{operation} | {reason}"))
                    .or_default()
                    .insert(registry.to_owned());
            }
            Outcome::Missing => {
                self.errors
                    .entry(format!("{operation} | not recorded"))
                    .or_default()
                    .insert(registry.to_owned());
            }
        }
    }

    fn report(self, build: Value, registries: Value) -> Value {
        let counts = |groups: BTreeMap<&'static str, Counts>| -> BTreeMap<&'static str, Value> {
            groups
                .into_iter()
                .map(|(relationship, counts)| (relationship, counts.report()))
                .collect()
        };
        json!({
            "build": build,
            "registry_inventory": registries,
            "relationships": counts(self.totals),
            "registries_without_field_units": self.registries_without_field_units.0,
            "registries": self.registries,
            "readers": self
                .readers
                .into_iter()
                .map(|(reader, groups)| (reader, counts(groups)))
                .collect::<BTreeMap<_, _>>(),
            "fields": self.fields,
            "failure_shapes": {
                "gaps": self.open_shapes,
                "accepted": self.accepted_shapes,
                "errors": self.errors,
            },
        })
    }
}

/// The field's reader identity, or `none`. `ReaderId` has no string accessor; it serializes as
/// its text.
fn reader_name(field: &Field) -> String {
    let Some(id) = &field.reader.id else {
        return "none".to_owned();
    };
    match serde_json::to_value(id) {
        Ok(Value::String(name)) => name,
        _ => unreachable!("a reader identity serializes as a string"),
    }
}

/// `text` with each run of digits replaced by `N`.
fn without_digits(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    for character in text.chars() {
        if !character.is_ascii_digit() {
            normalized.push(character);
        } else if !normalized.ends_with('N') {
            normalized.push('N');
        }
    }
    normalized
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    match arguments.as_slice() {
        [flag, directory, installation] if flag == "--record" => record(directory, installation),
        [flag, before, after] if flag == "--diff" => {
            let report = diff(
                &population::read_report(Path::new(before))?,
                &population::read_report(Path::new(after))?,
            )?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        [flag, baseline, directory] if flag == "--baseline" => {
            let report = sweep(Path::new(directory))?;
            population::write_baseline(
                Path::new(baseline),
                &json!({ "build": report["build"], "relationships": report["relationships"] }),
                &comparison_inputs(&report)?,
            )
        }
        [directory] => {
            let report = sweep(Path::new(directory))?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

/// Record the questions that the report reads, errors included.
fn record(directory: &str, installation: &str) -> Result<(), Box<dyn std::error::Error>> {
    if Path::new(directory).exists() {
        return Err(format!("{directory} exists; record into a new directory").into());
    }
    let started = Instant::now();
    let native = Native::open(installation)?.record_answers_to(directory);
    let registries = native.registries()?.value;
    for registry in &registries {
        for result in [
            native.registry_fields(&registry.name).map(drop),
            native.derived_names(&registry.name).map(drop),
            native.modifier_families(&registry.name).map(drop),
        ] {
            if let Err(error @ Error::Recorded(_)) = result {
                return Err(error.into());
            }
        }
    }
    eprintln!(
        "recorded {} registries in {:.1} s",
        registries.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn sweep(directory: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let native = Native::from_recorded_answers(directory)?;
    let registries = native.registries()?;
    let mut sweep = Sweep::default();
    for registry in &registries.value {
        let name = &registry.name;
        let fields = native.registry_fields(name);
        let derived = native.derived_names(name);
        let families = native.modifier_families(name);
        sweep.add(
            name,
            Outcome::of(&fields)?,
            Outcome::of(&derived)?,
            Outcome::of(&families)?,
        );
    }
    Ok(sweep.report(json!(native.build()), serde_json::to_value(&registries)?))
}

/// Each registry's statuses and gaps by shape, from a baseline or a full report.
fn comparison_inputs(report: &Value) -> population::ReportResult<BTreeMap<String, Value>> {
    if let Some(answers) = population::baseline_answers(report)? {
        return Ok(answers);
    }
    let registries = report
        .get("registries")
        .and_then(Value::as_object)
        .ok_or("expected an object at \"registries\"")?;
    Ok(registries
        .iter()
        .map(|(registry, row)| (registry.clone(), row.clone()))
        .collect())
}

/// Each registry and relationship whose statuses, or open or accepted gaps, differ between two
/// reports.
fn diff(before: &Value, after: &Value) -> population::ReportResult<Value> {
    let before = comparison_inputs(before).map_err(|error| format!("before report: {error}"))?;
    let after = comparison_inputs(after).map_err(|error| format!("after report: {error}"))?;
    let registries: BTreeSet<_> = before.keys().chain(after.keys()).collect();
    let mut changes = Vec::new();
    let mut changed = BTreeSet::new();
    for registry in registries {
        let (old, new) = (before.get(registry), after.get(registry));
        let relationships: BTreeSet<_> = [old, new]
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .flat_map(|row| row.keys())
            .collect();
        for relationship in relationships {
            let old = old.and_then(|row| row.get(relationship));
            let new = new.and_then(|row| row.get(relationship));
            if old != new {
                changed.insert(registry);
                changes.push(json!({
                    "registry": registry,
                    "relationship": relationship,
                    "before": old,
                    "after": new,
                }));
            }
        }
    }
    Ok(json!({ "changed_registries": changed.len(), "changes": changes }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn source() -> Value {
        json!({ "build": "example-build", "native_version": "0.1.0", "method": "example/v1", "basis": "StaticAnalysis" })
    }

    fn answer(value: Value, completeness: &str, gaps: Value) -> Value {
        json!({ "Ok": { "value": value, "completeness": completeness, "gaps": gaps, "source": source() } })
    }

    /// A field whose every property is established, as a scalar integer.
    fn established_field(name: &str) -> Value {
        let numeric = json!({ "representation": { "Known": "Integer" }, "width_bits": { "Known": 32 },
            "signedness": { "Known": "Signed" }, "scale": { "Known": null }, "literal_syntax": "Unresolved",
            "accepted_range": "Unresolved" });
        let reader = json!({ "id": "integer", "kind": "Integer", "family": "NotApplicable",
            "numeric": { "Known": numeric }, "scoped_operand": { "Known": null } });
        let shape = json!({ "value": "Scalar", "repeat": "Replace" });
        json!({ "name": name, "reader": reader, "shape": shape,
            "read": [{ "condition": "Always", "outcome": { "Read": { "reader": reader, "shape": shape } } }],
            "members": "None", "domain": "Unknown", "uses": [], "reference": "NotEstablished",
            "entry_contexts": [], "read_scope": { "Known": [] }, "accepted_categories": "NotApplicable", "requirement": "Unknown" })
    }

    fn write(root: &Path, file: &str, value: Value) {
        let path = root.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn write_registry(root: &Path, registry: &str, fields: Value, derived: Value, families: Value) {
        write(root, &format!("registry_fields/{registry}.json"), fields);
        write(root, &format!("derived_names/{registry}.json"), derived);
        write(
            root,
            &format!("modifier_families/{registry}.json"),
            families,
        );
    }

    /// One registry for each control, written by hand.
    fn recording() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let path = root.path();
        let names = [
            "common/empty",
            "common/failed",
            "common/missing",
            "common/mixed",
            "common/unknown",
        ];
        let registries: Vec<_> = names.iter().map(|name| json!({ "name": name })).collect();
        write(path, "build.json", json!("example-build"));
        write(
            path,
            "registries.json",
            answer(json!(registries), "Complete", json!([])),
        );
        let none = || answer(json!([]), "Complete", json!([]));

        let mut mixed = established_field("cost");
        mixed["reader"]["numeric"] = json!({ "Partial": mixed["reader"]["numeric"]["Known"] });
        let mut potential = established_field("potential");
        potential["reader"] = json!({ "id": "trigger", "kind": "Block", "family": "Trigger",
            "numeric": { "Known": null }, "scoped_operand": { "Known": null } });
        potential["shape"]["value"] = json!("Block");
        potential["entry_contexts"] = json!([{ "this": "SelfLink", "root": "SelfLink", "from": ["SelfLink"], "prev": ["SelfLink"] }]);
        let gaps = json!([
            { "kind": "OutsideMethod", "subject": { "kind": "registry", "name": "common/mixed" }, "detail": "Outside." },
            { "kind": "NumericConversion", "subject": { "kind": "field", "name": "cost" }, "detail": "Range." },
            { "kind": "UnresolvedCondition", "subject": { "kind": "field", "name": "cost" }, "detail": "Use-time context." },
            { "kind": "UnresolvedPath", "subject": { "kind": "field", "name": "potential" }, "detail": "More paths (path-limit)." },
        ]);
        let unsupported = json!({ "Err": { "Unsupported": { "operation": "DerivedNames", "reason": "no names here" } } });
        write_registry(
            path,
            "common/mixed",
            answer(json!([mixed, potential]), "Partial", gaps),
            unsupported,
            none(),
        );

        write_registry(path, "common/empty", none(), none(), none());

        let mut unknown = established_field("icon");
        unknown["reader"] = json!({ "id": null, "kind": "Unknown", "family": "Unknown", "numeric": "Unresolved", "scoped_operand": "Unresolved" });
        unknown["shape"] = json!({ "value": "Unknown", "repeat": "Unknown" });
        unknown["read"] = json!([{ "condition": "Unresolved", "outcome": "Unresolved" }]);
        unknown["read_scope"] = json!("Unresolved");
        unknown["accepted_categories"] = json!("Unresolved");
        let gaps = json!([
            { "kind": "UnresolvedReader", "subject": { "kind": "field", "name": "icon" }, "detail": "No reader on 2 paths." },
            { "kind": "UnnamedField", "subject": { "kind": "registry", "name": "common/unknown" }, "detail": "3 paths have no name." },
        ]);
        let partial_names = answer(
            json!([]),
            "Partial",
            json!([{ "kind": "UnresolvedPath", "subject": null, "detail": "Stopped." }]),
        );
        write_registry(
            path,
            "common/unknown",
            answer(json!([unknown]), "Partial", gaps),
            partial_names,
            none(),
        );

        let stopped = json!({ "Err": { "Method": "stopped" } });
        write_registry(path, "common/failed", stopped.clone(), stopped, none());
        root
    }

    #[test]
    fn each_control_has_the_statuses_of_its_rules() {
        let root = recording();
        let report = sweep(root.path()).unwrap();
        let registry = |name: &str| &report["registries"][name];

        assert_eq!(
            registry("common/mixed")["fields"],
            "complete",
            "an OutsideMethod gap keeps the field set complete"
        );
        assert_eq!(
            registry("common/mixed")["naming_rules"],
            "partial: no names here"
        );
        let mixed = &report["fields"]["common/mixed#cost"];
        assert_eq!(
            *mixed,
            json!({ "reader_kinds": "complete", "shapes_and_conditions": "complete", "numerics": "partial", "scope_context": "complete" })
        );
        assert_eq!(
            report["fields"]["common/mixed#potential"]["scope_context"], "partial",
            "a path-limit gap leaves a resolved context partial"
        );

        assert_eq!(
            *registry("common/empty"),
            json!({ "fields": "complete", "naming_rules": "complete" })
        );

        assert_eq!(
            registry("common/unknown")["fields"],
            "partial",
            "an unnamed path can hold a field"
        );
        assert_eq!(
            report["fields"]["common/unknown#icon"],
            json!({ "reader_kinds": "failed", "shapes_and_conditions": "failed", "references": "undetermined",
                "numerics": "undetermined", "weights": "undetermined", "scope_context": "failed" })
        );
        assert_eq!(report["readers"]["none"]["reader_kinds"]["failed"], 1);

        assert_eq!(
            registry("common/failed")["fields"],
            "failed: the method failed: stopped"
        );
        assert_eq!(
            registry("common/failed")["naming_rules"],
            "partial: the method failed: stopped"
        );
        assert_eq!(
            report["failure_shapes"]["errors"]["registry_fields | the method failed: stopped"],
            json!(["common/failed"])
        );
        assert_eq!(
            report["failure_shapes"]["gaps"]["registry_fields | UnnamedField: N paths have no name."],
            json!({ "gaps": 1, "registries": ["common/unknown"] })
        );

        assert_eq!(
            *registry("common/missing"),
            json!({ "fields": "missing", "naming_rules": "missing" })
        );
        assert_eq!(
            report["relationships"]["fields"],
            json!({ "complete": 2, "partial": 1, "failed": 1, "missing": 1, "denominator": 4, "rate": 0.5 })
        );
        assert_eq!(
            report["relationships"]["numerics"],
            json!({ "partial": 1, "undetermined": 1, "denominator": 2, "rate": 0.0 })
        );
    }

    const UNKNOWN_CONVERSION_PATH: &str =
        "Numeric conversion is incomplete: the engine conversion path is not established.";

    fn numeric_gap(subject: Value, detail: &str) -> Value {
        json!({ "kind": "NumericConversion", "subject": subject, "detail": detail })
    }

    /// One registry whose fields each have one numeric gap: accepted, or a near miss.
    fn write_limits(root: &Path, gaps: Value) {
        let mut days = established_field("ai_weight");
        days["name"] = json!("days");
        let mut weight = established_field("ai_weight");
        weight["reader"] = json!({ "id": "weight", "kind": "Block", "family": "Weight",
            "numeric": { "Known": null }, "scoped_operand": { "Known": null } });
        weight["members"] = json!({ "WeightBlock": { "scalar": { "Known": null },
            "fixed_keys": { "Known": [days.clone()] }, "operations": { "Known": [] },
            "operation_repeat": "Replace", "other_keys": "Rejected" } });
        let fields: Vec<_> = ["short_value", "path_value", "kind_value"]
            .into_iter()
            .map(established_field)
            .chain([days, weight])
            .collect();
        let none = answer(json!([]), "Complete", json!([]));
        write_registry(
            root,
            "common/limits",
            answer(json!(fields), "Partial", gaps),
            none.clone(),
            none,
        );
        write(root, "build.json", json!("example-build"));
        write(
            root,
            "registries.json",
            answer(json!([{ "name": "common/limits" }]), "Complete", json!([])),
        );
    }

    fn field(name: &str) -> Value {
        json!({ "kind": "field", "name": name })
    }

    #[test]
    fn only_a_gap_that_an_accepted_limit_covers_leaves_the_open_counts() {
        let root = tempfile::tempdir().unwrap();
        let weight_days = json!({ "kind": "key_path", "path": ["ai_weight", "days"] });
        write_limits(
            root.path(),
            json!([
                numeric_gap(field("short_value"), UNKNOWN_STORAGE),
                numeric_gap(field("path_value"), UNKNOWN_CONVERSION_PATH),
                { "kind": "UnresolvedStorage", "subject": field("kind_value"), "detail": UNKNOWN_RANGE },
                numeric_gap(field("days"), UNKNOWN_RANGE),
                numeric_gap(weight_days.clone(), UNKNOWN_RANGE),
                numeric_gap(weight_days, UNKNOWN_STORAGE),
            ]),
        );
        let report = sweep(root.path()).unwrap();
        let status = |field: &str, relationship: &str| {
            report["fields"][format!("common/limits#{field}")][relationship].clone()
        };

        assert_eq!(status("short_value", NUMERICS), "complete");
        assert_eq!(status("days", NUMERICS), "complete");
        assert_eq!(
            status("path_value", NUMERICS),
            "partial",
            "a path gap stays open"
        );
        assert_eq!(
            status("kind_value", SHAPES_AND_CONDITIONS),
            "partial",
            "the range detail of another kind stays open"
        );

        let shape = |detail: &str| format!("registry_fields | NumericConversion: {detail}");
        let accepted = &report["failure_shapes"]["accepted"];
        let [weight_range, sdk_809] =
            [&ACCEPTED_LIMITS[0], &ACCEPTED_LIMITS[1]].map(|limit| &accepted[limit.decision]);
        assert_eq!(
            *weight_range,
            json!({ shape(UNKNOWN_RANGE): { "gaps": 1, "registries": ["common/limits"] } }),
            "only the weight-block key is SDK-739's; a root field named days is not"
        );
        assert_eq!(sdk_809[shape(UNKNOWN_RANGE)]["gaps"], 1);
        assert_eq!(
            sdk_809[shape(UNKNOWN_STORAGE)]["gaps"],
            2,
            "a storage gap of a weight key is SDK-809's"
        );

        let open = &report["failure_shapes"]["gaps"];
        assert_eq!(open.as_object().unwrap().len(), 2);
        assert_eq!(open[shape(UNKNOWN_CONVERSION_PATH)]["gaps"], 1);
        assert_eq!(
            report["registries"]["common/limits"][OPEN_GAPS],
            json!({ shape(UNKNOWN_CONVERSION_PATH): 1,
                format!("registry_fields | UnresolvedStorage: {UNKNOWN_RANGE}"): 1 })
        );
    }

    #[test]
    fn only_a_listed_field_has_its_loader_condition_accepted() {
        let (registry, field) = LOADER_CONDITIONS[0];
        let gap = |name: &str, detail: &str| Gap {
            kind: GapKind::UnresolvedCondition,
            subject: Some(GapSubject::Field { name: name.into() }),
            detail: detail.into(),
        };
        let decision = |gap: &Gap, registry: &str| {
            accepted_limit(gap, registry, &[]).map(|limit| limit.decision)
        };

        assert_eq!(
            decision(&gap(field, UNRESOLVED_LOADER_CONDITION), registry),
            Some(SDK_811)
        );
        assert_eq!(
            decision(&gap("unlisted", UNRESOLVED_LOADER_CONDITION), registry),
            None,
            "a new loader-condition gap stays open"
        );
        assert_eq!(
            decision(&gap(field, UNRESOLVED_LOADER_CONDITION), "common/unlisted"),
            None,
            "the field is listed for its own registry only"
        );
        assert_eq!(
            decision(&gap("unlisted", USE_TIME_CONTEXT), registry),
            Some(SDK_811),
            "Atlas credits no claim from any use-time selection"
        );
    }

    #[test]
    fn only_the_listed_tooltip_clause_gaps_are_accepted() {
        let (registry, field) = TOOLTIP_CLAUSES[0];
        let on_field = || Some(GapSubject::Field { name: field.into() });
        let on_keys = |keys: &[&str]| {
            Some(GapSubject::KeyPath {
                path: keys.iter().map(|&key| key.into()).collect(),
            })
        };
        let decision = |kind: GapKind, subject: Option<GapSubject>, detail: &str, registry| {
            let gap = Gap {
                kind,
                subject,
                detail: detail.into(),
            };

            accepted_limit(&gap, registry, &[]).map(|limit| limit.decision)
        };
        let storage = GapKind::UnresolvedStorage;
        let accepted = [
            (GapKind::UnresolvedPath, on_field(), TOOLTIP_EMBEDDED_POINT),
            (GapKind::UnresolvedReader, on_field(), TOOLTIP_DELEGATE),
            (storage, on_keys(&[field, "mult"]), SCOPED_DESTINATION),
            (storage, on_keys(&[field, "multiplier"]), SCOPED_DESTINATION),
        ];
        let open = [
            (storage, on_keys(&["unlisted", "mult"]), SCOPED_DESTINATION),
            (storage, on_keys(&[field, "key"]), SCOPED_DESTINATION),
            (
                storage,
                on_keys(&[field, "modifier", "mult"]),
                SCOPED_DESTINATION,
            ),
            (storage, on_field(), SCOPED_DESTINATION),
            (GapKind::UnresolvedPath, on_field(), TOOLTIP_DELEGATE),
            (
                GapKind::UnresolvedPath,
                on_field(),
                "read-scope: scope-argument",
            ),
            (
                GapKind::UnresolvedReader,
                on_keys(&[field, "mult"]),
                TOOLTIP_DELEGATE,
            ),
        ];

        for (kind, subject, detail) in accepted {
            assert_eq!(
                decision(kind, subject.clone(), detail, registry),
                Some(SDK_817),
                "{kind:?} {subject:?}"
            );
            assert_eq!(
                decision(kind, subject, detail, "common/buildings"),
                None,
                "the fields are listed for their own registry only"
            );
        }

        for (kind, subject, detail) in open {
            assert_eq!(
                decision(kind, subject.clone(), detail, registry),
                None,
                "{kind:?} {subject:?} {detail}"
            );
        }
    }

    #[test]
    fn only_an_entry_context_stop_of_a_field_without_a_claim_is_accepted() {
        let (registry, field) = ENTRY_CONTEXTS_WITHOUT_CLAIM[0];
        let gap = |name: &str, detail: &str| Gap {
            kind: GapKind::UnresolvedPath,
            subject: Some(GapSubject::Field { name: name.into() }),
            detail: detail.into(),
        };
        let decision = |gap: &Gap, registry: &str| {
            accepted_limit(gap, registry, &[]).map(|limit| limit.decision)
        };
        let path_limit = ENTRY_CONTEXT_STOPS[3];

        assert_eq!(decision(&gap(field, path_limit), registry), Some(SDK_812));
        assert_eq!(
            decision(
                &gap("tooltip_system_score", path_limit),
                "common/megastructures"
            ),
            None,
            "a block that a replace_scopes claim compares stays open"
        );
        assert_eq!(
            decision(&gap(field, "read-scope: scope-argument"), registry),
            None,
            "another obstacle of a listed field stays open"
        );

        let unnamed = Gap {
            kind: GapKind::UnresolvedPath,
            subject: Some(GapSubject::Registry {
                name: UNNAMED_EVALUATION_REGISTRIES[0].into(),
            }),
            detail: UNNAMED_EVALUATIONS[0].into(),
        };
        assert_eq!(
            decision(&unnamed, UNNAMED_EVALUATION_REGISTRIES[0]),
            Some(SDK_812)
        );
        assert_eq!(
            decision(&unnamed, "common/unlisted"),
            None,
            "a new unnamed evaluation stays open"
        );
    }

    #[test]
    fn only_a_derived_name_property_of_its_own_item_is_accepted() {
        let gap = |kind: GapKind, item: &str, detail: &str| Gap {
            kind,
            subject: Some(GapSubject::AnswerItem { name: item.into() }),
            detail: detail.into(),
        };
        let decision =
            |gap: &Gap| accepted_limit(gap, "common/traditions", &[]).map(|limit| limit.decision);
        let condition = GapKind::UnresolvedCondition;

        for detail in [
            "what a missing {key}_desc gives is not established",
            "the field values under which the engine uses {key}_desc are not established",
            "{key}_desc also depends on field states that the method does not explore",
        ] {
            assert_eq!(
                decision(&gap(condition, "{key}_desc", detail)),
                Some(SDK_810),
                "{detail}"
            );
            assert_eq!(
                decision(&gap(condition, "{key}", detail)),
                None,
                "a detail that names another item stays open"
            );
            assert_eq!(
                decision(&gap(GapKind::UnresolvedPath, "{key}_desc", detail)),
                None,
                "a gap of another kind stays open"
            );
        }

        let every_item = |registry: &str| Gap {
            kind: condition,
            subject: Some(GapSubject::Registry {
                name: registry.into(),
            }),
            detail: EVERY_ITEM_INITIALIZATION[1].into(),
        };
        assert_eq!(
            accepted_limit(&every_item("common/technology"), "common/technology", &[])
                .map(|limit| limit.decision),
            Some(SDK_810)
        );
        assert_eq!(
            accepted_limit(&every_item("common/buildings"), "common/buildings", &[])
                .map(|limit| limit.decision),
            None,
            "a new every-item gap stays open"
        );
    }

    #[test]
    fn the_diff_names_a_registry_whose_open_or_accepted_gaps_change() {
        let root = tempfile::tempdir().unwrap();
        let path_gap = numeric_gap(field("path_value"), UNKNOWN_CONVERSION_PATH);
        write_limits(root.path(), json!([path_gap]));
        let before = sweep(root.path()).unwrap();
        let baseline_root = tempfile::tempdir().unwrap();
        population::write_baseline(
            baseline_root.path(),
            &json!({ "build": before["build"] }),
            &comparison_inputs(&before).unwrap(),
        )
        .unwrap();
        let baseline = population::read_report(baseline_root.path()).unwrap();

        write_limits(
            root.path(),
            json!([
                path_gap,
                numeric_gap(field("path_value"), "Another open obstacle."),
                numeric_gap(field("short_value"), UNKNOWN_STORAGE),
            ]),
        );
        let after = sweep(root.path()).unwrap();

        for before in [&before, &baseline] {
            let changes = diff(before, &after).unwrap()["changes"].clone();
            let changed: Vec<_> = changes
                .as_array()
                .unwrap()
                .iter()
                .map(|change| change["relationship"].as_str().unwrap())
                .collect();
            assert_eq!(
                changed,
                [ACCEPTED_GAPS, OPEN_GAPS],
                "no status changes, so only the gaps name the registry: {changes}"
            );
        }
    }

    #[test]
    fn an_unreadable_recording_is_refused() {
        let root = recording();
        write(
            root.path(),
            "registry_fields/common/empty.json",
            json!({ "Ok": "not an answer" }),
        );
        assert!(sweep(root.path()).is_err());
    }

    #[test]
    fn the_baseline_is_stable_and_the_diff_names_each_changed_relationship() {
        let root = recording();
        let report = sweep(root.path()).unwrap();
        let baseline_root = tempfile::tempdir().unwrap();
        let baseline = |report: &Value| {
            population::write_baseline(
                baseline_root.path(),
                &json!({ "build": report["build"] }),
                &comparison_inputs(report).unwrap(),
            )
            .unwrap();
            fs::read(baseline_root.path().join("common/empty.json")).unwrap()
        };
        let bytes = baseline(&report);
        assert_eq!(bytes, baseline(&sweep(root.path()).unwrap()));
        let parsed = population::read_report(baseline_root.path()).unwrap();
        assert_eq!(diff(&parsed, &report).unwrap()["changed_registries"], 0);

        fs::remove_file(root.path().join("modifier_families/common/empty.json")).unwrap();
        let changed = diff(&parsed, &sweep(root.path()).unwrap()).unwrap();
        assert_eq!(
            changed,
            json!({ "changed_registries": 1, "changes": [{ "registry": "common/empty",
                "relationship": "naming_rules", "before": "complete", "after": "missing" }] })
        );
        assert!(diff(&parsed, &json!({ "cases": [] })).is_err());
    }
}
