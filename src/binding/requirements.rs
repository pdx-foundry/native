//! Which root fields a definition must contain, from one recorded census run per build.
//!
//! A recorded manual exception. Claim: a root field is required where loading a definition
//! without it logs a report of its absence, and optional where it logs none. Conditions: the
//! exact build of the census, and a registry whose empty definition the run read. Obstacle: the
//! checks are one-off member tests in post-read hooks, with no shared shape, and the method
//! cannot join a tested member to its key. Removal route: a static reading of those hooks.
//! A check that depends on another field's value is stated as cases where a static trace of its
//! hook on the exact build gives them; the trace is the same kind of recorded exception.
//! `docs/native/registry-fields.md`, "Required fields", has the run, the joins, the traces and
//! the limits.
use crate::{FieldCondition, FieldRequirement, RequirementCase};

/// One build's census of required root fields.
pub(in crate::binding) struct RequirementCensus {
    /// Registries whose empty definition the census run read, and whose every absence report
    /// joins a field key or names a key outside the registry's fields.
    pub registries: &'static [&'static str],
    /// `(registry, field)` pairs whose absence an unconditional check reports. A pair can be
    /// outside `registries` when a static check establishes it.
    pub required: &'static [(&'static str, &'static str)],
    /// `(registry, field)` pairs whose absence check depends on another field's value, with no
    /// traced cases.
    pub conditional: &'static [(&'static str, &'static str)],
    /// Fields whose absence check a static trace states by the definition's other fields.
    pub traced: &'static [TracedRequirement],
    /// Registries whose definitions the census also probed with stored selector fields set.
    pub subtypes: &'static [SubtypeCensus],
}

/// The definitions of one registry that the census probed with selector fields set, beside its
/// empty definition.
pub(in crate::binding) struct SubtypeCensus {
    pub registry: &'static str,
    /// The stored state of the selector fields in each probed definition, and the fields whose
    /// absence it reported.
    pub probed: &'static [(&'static [TracedCondition], &'static [&'static str])],
    /// The stored state that no probed definition holds, for which the empty definition stands,
    /// when one condition states it.
    pub other: Option<&'static [TracedCondition]>,
}

impl SubtypeCensus {
    /// Whether `field` is a selector of the probed states.
    fn selects(&self, field: &str) -> bool {
        self.probed
            .iter()
            .flat_map(|(state, _)| state.iter())
            .chain(self.other.into_iter().flatten())
            .any(|term| term.field() == field)
    }

    /// One case for each probed state, and one for the other state where `empty` (the empty
    /// definition's answer) is established.
    fn cases(&self, field: &str, empty: &FieldRequirement) -> FieldRequirement {
        let mut cases: Vec<_> = self
            .probed
            .iter()
            .map(|(state, reported)| RequirementCase {
                condition: all(state),
                required: reported.contains(&field),
            })
            .collect();
        let empty_required = match empty {
            FieldRequirement::Required => Some(true),
            FieldRequirement::Optional => Some(false),
            _ => None,
        };

        if let (Some(state), Some(required)) = (self.other, empty_required) {
            cases.push(RequirementCase {
                condition: all(state),
                required,
            });
        }

        FieldRequirement::Cases(cases)
    }
}

/// The condition that every term of `state` holds.
fn all(state: &[TracedCondition]) -> FieldCondition {
    match state {
        [term] => term.condition(),
        terms => FieldCondition::All(terms.iter().map(TracedCondition::condition).collect()),
    }
}

/// The cases of one field's conditional absence check, from a static trace of its hook.
pub(in crate::binding) struct TracedRequirement {
    pub registry: &'static str,
    pub field: &'static str,
    /// Each case's condition and whether the field is required where it holds.
    pub cases: &'static [(TracedCondition, bool)],
}

/// A condition on one stored root field of the definition.
pub(in crate::binding) enum TracedCondition {
    /// The field's stored scalar is zero, or nonzero.
    Zero { field: &'static str, zero: bool },
    /// The field's stored keyword is, or is not, `keyword`.
    Keyword {
        field: &'static str,
        keyword: &'static str,
        equal: bool,
    },
}

/// The stored scalar of `field` is zero, or nonzero.
const fn zero(field: &'static str, zero: bool) -> TracedCondition {
    TracedCondition::Zero { field, zero }
}

/// The stored keyword of `field` is, or is not, `keyword`.
const fn keyword(field: &'static str, keyword: &'static str, equal: bool) -> TracedCondition {
    TracedCondition::Keyword {
        field,
        keyword,
        equal,
    }
}

impl TracedCondition {
    fn field(&self) -> &'static str {
        match *self {
            TracedCondition::Zero { field, .. } | TracedCondition::Keyword { field, .. } => field,
        }
    }

    fn condition(&self) -> FieldCondition {
        match *self {
            TracedCondition::Zero { field, zero } => FieldCondition::FieldZero {
                path: vec![field.into()],
                zero,
            },
            TracedCondition::Keyword {
                field,
                keyword,
                equal,
            } => FieldCondition::FieldKeyword {
                path: vec![field.into()],
                keyword: keyword.into(),
                equal,
            },
        }
    }
}

impl RequirementCensus {
    pub(in crate::binding) fn requirement(&self, registry: &str, field: &str) -> FieldRequirement {
        let listed = |pairs: &[(&str, &str)]| pairs.contains(&(registry, field));
        let traced = self
            .traced
            .iter()
            .find(|traced| traced.registry == registry && traced.field == field);

        if let Some(traced) = traced {
            let cases = traced
                .cases
                .iter()
                .map(|(condition, required)| RequirementCase {
                    condition: condition.condition(),
                    required: *required,
                })
                .collect();

            return FieldRequirement::Cases(cases);
        }

        let empty = if listed(self.required) {
            FieldRequirement::Required
        } else if listed(self.conditional) || !self.registries.contains(&registry) {
            FieldRequirement::Unknown
        } else {
            FieldRequirement::Optional
        };
        let subtypes = self
            .subtypes
            .iter()
            .find(|subtypes| subtypes.registry == registry)
            .filter(|subtypes| self.registries.contains(&registry) && !subtypes.selects(field));

        match subtypes {
            Some(subtypes) => subtypes.cases(field, &empty),
            None => empty,
        }
    }
}

/// The M452 census of 2026-10-14 (SDK-829).
pub(in crate::binding) const M452_REQUIREMENTS: RequirementCensus = RequirementCensus {
    registries: M452_CENSUS_REGISTRIES,
    required: &[
        ("common/ai_budget", "category"),
        ("common/ai_budget", "resource"),
        ("common/archaeological_site_types", "on_roll_failed"),
        ("common/astral_rifts", "event"),
        ("common/buildings", "category"),
        ("common/colony_automation", "category"),
        ("common/country_focus/card_categories", "desc"),
        ("common/country_focus/card_categories", "name"),
        ("common/country_focus/focus_rewards", "category"),
        ("common/espionage_operation_types", "on_roll_failed"),
        ("common/ethics", "category"),
        ("common/ethics", "cost"),
        ("common/first_contact", "on_roll_failed"),
        ("common/galactic_focuses", "effect"),
        ("common/galactic_focuses", "trigger"),
        ("common/game_concept_categories", "icon"),
        ("common/game_concept_categories", "name"),
        ("common/mutations", "icon"),
        ("common/mutations", "ship_components"),
        ("common/patrons", "text_color_code"),
        ("common/patrons/callings", "tier"),
        ("common/portrait_sets", "species_class"),
        ("common/resolution_categories", "group"),
        ("common/resolution_categories", "resolution_types"),
        ("common/situations", "stages"),
        ("common/species_classes", "graphical_culture"),
        ("common/specimens", "type"),
        ("common/star_classes/randomizers", "stars"),
        ("common/tradition_categories", "adoption_bonus"),
        ("common/tradition_categories", "finish_bonus"),
        ("common/war_goals", "casus_belli"),
        ("events", "id"),
        ("map/galaxy", "stars_min_dist"),
    ],
    conditional: &[
        ("common/ascension_perks", "custom_tooltip"),
        ("common/ascension_perks", "custom_tooltip_with_modifiers"),
        ("common/ascension_perks", "modifier"),
        ("common/ascension_perks", "on_enabled"),
        ("common/ascension_perks", "triggered_modifier"),
        ("common/ascension_perks", "unlocks_agenda"),
        ("common/buildings", "building_sets"),
        ("common/country_focus/card_categories", "background"),
        (
            "common/country_focus/card_categories",
            "background_expanded",
        ),
        ("common/country_focus/card_categories", "icon"),
        ("common/missions/missions", "counter"),
        ("common/missions/missions", "icon"),
        ("common/planet_classes", "icon"),
        ("common/planet_classes", "icon_large"),
        ("common/resolutions", "resources"),
        ("common/ship_categories", "namelist"),
        ("common/traditions", "custom_tooltip"),
        ("common/traditions", "custom_tooltip_with_modifiers"),
        ("common/traditions", "modifier"),
        ("common/traditions", "on_enabled"),
        ("common/traditions", "triggered_modifier"),
        ("common/traditions", "unlocks_agenda"),
        ("events", "desc"),
        ("events", "is_triggered_only"),
        ("events", "mean_time_to_happen"),
        ("events", "option"),
        ("events", "picture"),
        ("events", "title"),
    ],
    subtypes: M452_SUBTYPES,
    traced: &[
        // `CEspionageOperationType::InitPostRead()` logs unless `stages` equals the count of
        // `stage` blocks.
        TracedRequirement {
            registry: "common/espionage_operation_types",
            field: "stage",
            cases: &[
                (
                    TracedCondition::Zero {
                        field: "stages",
                        zero: true,
                    },
                    false,
                ),
                (
                    TracedCondition::Zero {
                        field: "stages",
                        zero: false,
                    },
                    true,
                ),
            ],
        },
        // `CPatronType::PostReadInit()` tests `position` for each set category, and its default
        // (-1, -1) fails every test. An unset category runs no position test.
        TracedRequirement {
            registry: "common/patrons",
            field: "position",
            cases: &[
                (
                    TracedCondition::Keyword {
                        field: "category",
                        keyword: "minor_patron",
                        equal: true,
                    },
                    true,
                ),
                (
                    TracedCondition::Keyword {
                        field: "category",
                        keyword: "major_patron",
                        equal: true,
                    },
                    true,
                ),
                (
                    TracedCondition::Keyword {
                        field: "category",
                        keyword: "great_patron",
                        equal: true,
                    },
                    true,
                ),
            ],
        },
        // `CPatronType::PostReadInit()` parses the first contact event for every category but
        // great, and logs when it is empty.
        TracedRequirement {
            registry: "common/patrons",
            field: "first_contact_event",
            cases: &[
                (
                    TracedCondition::Keyword {
                        field: "category",
                        keyword: "great_patron",
                        equal: true,
                    },
                    false,
                ),
                (
                    TracedCondition::Keyword {
                        field: "category",
                        keyword: "great_patron",
                        equal: false,
                    },
                    true,
                ),
            ],
        },
        // It requires the covenant event of a major patron only.
        TracedRequirement {
            registry: "common/patrons",
            field: "covenant_event",
            cases: &[
                (
                    TracedCondition::Keyword {
                        field: "category",
                        keyword: "major_patron",
                        equal: true,
                    },
                    true,
                ),
                (
                    TracedCondition::Keyword {
                        field: "category",
                        keyword: "major_patron",
                        equal: false,
                    },
                    false,
                ),
            ],
        },
    ],
};

/// The M452 subtype census of 2026-10-20: definitions probed with selector fields set. A
/// registry whose selectors include a key reference, a block or a list, or whose selector
/// defaults are not all zero, keeps its empty-definition answer
/// (`docs/native/registry-fields.md`, "Required fields").
const M452_SUBTYPES: &[SubtypeCensus] = &[
    SubtypeCensus {
        registry: "common/armies",
        probed: &[(&[zero("is_pop_spawned", false)], &[])],
        other: Some(&[zero("is_pop_spawned", true)]),
    },
    // The constructor clears both selectors (`CDepositType`, `+0x990` and `+0x998`).
    SubtypeCensus {
        registry: "common/deposits",
        probed: &[
            (
                &[zero("is_for_colonizable", false), zero("is_null", true)],
                &[],
            ),
            (
                &[zero("is_for_colonizable", true), zero("is_null", false)],
                &[],
            ),
        ],
        other: Some(&[zero("is_for_colonizable", true), zero("is_null", true)]),
    },
    SubtypeCensus {
        registry: "common/districts",
        probed: &[(&[zero("default_starting_district", false)], &[])],
        other: Some(&[zero("default_starting_district", true)]),
    },
    SubtypeCensus {
        registry: "common/economic_categories",
        probed: &[(&[zero("hidden", false)], &[])],
        other: Some(&[zero("hidden", true)]),
    },
    SubtypeCensus {
        registry: "common/espionage_operation_types",
        probed: &[
            (&[keyword("target", "army", true)], &["on_roll_failed"]),
            (&[keyword("target", "country", true)], &["on_roll_failed"]),
            (
                &[keyword("target", "federation", true)],
                &["on_roll_failed"],
            ),
            (&[keyword("target", "fleet", true)], &["on_roll_failed"]),
            (
                &[keyword("target", "megastructure", true)],
                &["on_roll_failed"],
            ),
            (
                &[keyword("target", "pop_faction", true)],
                &["on_roll_failed"],
            ),
            (
                &[keyword("target", "spynetwork", true)],
                &["on_roll_failed"],
            ),
            (&[keyword("target", "starbase", true)], &["on_roll_failed"]),
        ],
        other: Some(&[
            keyword("target", "army", false),
            keyword("target", "country", false),
            keyword("target", "federation", false),
            keyword("target", "fleet", false),
            keyword("target", "megastructure", false),
            keyword("target", "pop_faction", false),
            keyword("target", "spynetwork", false),
            keyword("target", "starbase", false),
        ]),
    },
    SubtypeCensus {
        registry: "common/governments/civics",
        probed: &[
            (&[zero("is_origin", true)], &[]),
            (&[zero("is_origin", false)], &[]),
        ],
        other: None,
    },
    SubtypeCensus {
        registry: "common/opinion_modifiers",
        probed: &[(&[zero("block_triggered", false)], &[])],
        other: Some(&[zero("block_triggered", true)]),
    },
    // The constructor clears all three selector bits of the word at `+0x4a0` (`CPlanetClass`).
    SubtypeCensus {
        registry: "common/planet_classes",
        probed: &[
            (
                &[
                    zero("colonizable", false),
                    zero("star", true),
                    zero("ringworld", true),
                ],
                &["icon", "icon_large"],
            ),
            (
                &[
                    zero("colonizable", true),
                    zero("star", true),
                    zero("ringworld", true),
                ],
                &[],
            ),
            (
                &[
                    zero("colonizable", true),
                    zero("star", false),
                    zero("ringworld", true),
                ],
                &[],
            ),
            (
                &[
                    zero("colonizable", true),
                    zero("star", true),
                    zero("ringworld", false),
                ],
                &[],
            ),
        ],
        other: None,
    },
    SubtypeCensus {
        registry: "common/situations",
        probed: &[(
            &[keyword("progress_direction", "bidirectional", true)],
            &["stages"],
        )],
        other: Some(&[keyword("progress_direction", "bidirectional", false)]),
    },
    SubtypeCensus {
        registry: "common/static_modifiers",
        probed: &[(&[zero("icon_frame", false)], &[])],
        other: Some(&[zero("icon_frame", true)]),
    },
    SubtypeCensus {
        registry: "common/traits",
        probed: &[(&[zero("auto_mod", false)], &[])],
        other: Some(&[zero("auto_mod", true)]),
    },
];

const M452_CENSUS_REGISTRIES: &[&str] = &[
    "common/agreement_presets",
    "common/agreement_resources",
    "common/agreement_term_values",
    "common/agreement_terms",
    "common/ai_budget",
    "common/ai_espionage/operations",
    "common/ai_espionage/spynetworks",
    "common/ai_espionage/targets",
    "common/anomalies",
    "common/archaeological_site_types",
    "common/armies",
    "common/artifact_actions",
    "common/ascension_perk_categories",
    "common/ascension_perks",
    "common/asteroid_belts",
    "common/astral_actions",
    "common/astral_rifts",
    "common/attitudes",
    "common/bombardment_stances",
    "common/buildings",
    "common/button_effects",
    "common/bypass",
    "common/casus_belli",
    "common/cloaking_strength_levels",
    "common/colony_automation",
    "common/colony_automation_categories",
    "common/colony_automation_exceptions",
    "common/colony_types",
    "common/component_slot_templates",
    "common/council_agendas",
    "common/country_customization",
    "common/country_focus/card_categories",
    "common/country_focus/focus_cards",
    "common/country_focus/focus_rewards",
    "common/country_limits/ownership_limits",
    "common/country_limits/ship_of_size_limits",
    "common/country_types",
    "common/crisis_levels",
    "common/crisis_objectives",
    "common/crisis_paths",
    "common/decisions",
    "common/deposit_categories",
    "common/deposits",
    "common/diplomatic_actions",
    "common/districts",
    "common/dust_clouds",
    "common/dynamic_text",
    "common/economic_categories",
    "common/economic_plans",
    "common/edicts",
    "common/espionage_assets",
    "common/espionage_operation_types",
    "common/ethic_categories",
    "common/ethics",
    "common/event_chains",
    "common/fallen_empires",
    "common/federation_law_categories",
    "common/federation_laws",
    "common/federation_perks",
    "common/federation_types",
    "common/first_contact",
    "common/frontend_backgrounds",
    "common/galactic_community_actions",
    "common/galactic_focuses",
    "common/game_concept_categories",
    "common/game_concepts",
    "common/governments",
    "common/governments/authorities",
    "common/governments/civics",
    "common/governments/councilors",
    "common/greeting_overlay_sounds",
    "common/intel_categories",
    "common/intel_levels",
    "common/lawsuits",
    "common/leader_classes",
    "common/leader_tiers",
    "common/map_modes",
    "common/megastructure_overclock_types",
    "common/megastructures",
    "common/menace_perks",
    "common/missions/mission_categories",
    "common/missions/missions",
    "common/mutations",
    "common/named_colors",
    "common/notification_modifiers",
    "common/observation_station_missions",
    "common/on_actions",
    "common/opinion_modifiers",
    "common/patrons",
    "common/patrons/callings",
    "common/patrons/deeds",
    "common/patrons/psionic_auras",
    "common/personalities",
    "common/planet_classes",
    "common/planet_modifiers",
    "common/policies",
    "common/policy_categories",
    "common/pop_categories",
    "common/pop_faction_types",
    "common/pop_jobs",
    "common/portrait_categories",
    "common/portrait_sets",
    "common/precursor_civilizations",
    "common/prescripted_flags",
    "common/relics",
    "common/resolution_categories",
    "common/resolution_groups",
    "common/resolutions",
    "common/resource_converters",
    "common/resource_regions",
    "common/scripted_modifiers",
    "common/sector_types",
    "common/ship_categories",
    "common/ship_sets",
    "common/ship_sizes",
    "common/situation_log/categories",
    "common/situations",
    "common/special_projects",
    "common/specialist_subject_perks",
    "common/specialist_subject_types",
    "common/species_archetypes",
    "common/species_classes",
    "common/species_rights/citizenship_types",
    "common/species_rights/colonization_controls",
    "common/species_rights/living_standards",
    "common/species_rights/migration_controls",
    "common/species_rights/military_service_types",
    "common/species_rights/population_controls",
    "common/species_rights/purge_types",
    "common/species_rights/slavery_types",
    "common/species_rights/subspecies_integration_types",
    "common/specimens",
    "common/star_classes",
    "common/star_classes/randomizers",
    "common/starbase_buildings",
    "common/starbase_levels",
    "common/starbase_modules",
    "common/starbase_types",
    "common/static_modifiers",
    "common/storm_types",
    "common/strategic_resources",
    "common/system_tooltips",
    "common/system_types",
    "common/target_types",
    "common/technology/category",
    "common/technology/tier",
    "common/technology_ages/preftl",
    "common/timeline_events",
    "common/tradable_actions",
    "common/traditions",
    "common/traits",
    "common/war_goals",
    "common/zone_slots",
    "common/zones",
    "dlc_metadata/dlc_recommendations",
    "events",
    "gfx/portraits/sprite_configurations",
    "gfx/projectiles/planet_destruction",
    "interface/resource_groups",
    "map/galaxy",
    "prescripted_countries",
    "sound/advisor_voice_types",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn census_separates_required_conditional_and_unobserved_fields() {
        let requirement = |registry, field| M452_REQUIREMENTS.requirement(registry, field);

        assert_eq!(
            requirement("common/ethics", "category"),
            FieldRequirement::Required
        );
        assert_eq!(
            requirement("common/ethics", "playable"),
            FieldRequirement::Optional
        );
        assert_eq!(
            requirement("common/traditions", "modifier"),
            FieldRequirement::Unknown
        );
        assert_eq!(
            requirement("common/traditions", "potential"),
            FieldRequirement::Optional
        );
        assert_eq!(
            requirement("common/ship_categories", "namelist"),
            FieldRequirement::Unknown
        );
        assert_eq!(
            requirement("common/diplomatic_actions", "icon"),
            FieldRequirement::Optional
        );
        assert_eq!(
            requirement("common/tradition_categories", "adoption_bonus"),
            FieldRequirement::Required
        );
        assert_eq!(
            requirement("common/tradition_categories", "traditions"),
            FieldRequirement::Unknown
        );
        assert_eq!(
            requirement("common/galactic_focuses", "trigger"),
            FieldRequirement::Required
        );
        assert_eq!(
            requirement("common/astral_rifts", "event"),
            FieldRequirement::Required
        );
        let FieldRequirement::Cases(position) = requirement("common/patrons", "position") else {
            panic!("patrons position has no cases");
        };
        assert_eq!(position.len(), 3);
        assert!(position.iter().all(|case| case.required));
    }

    #[test]
    fn a_probed_subtype_gives_its_case_and_the_empty_definition_the_other_state() {
        let requirement = |registry, field| M452_REQUIREMENTS.requirement(registry, field);
        let spawned = |zero| FieldCondition::FieldZero {
            path: vec!["is_pop_spawned".into()],
            zero,
        };

        assert_eq!(
            requirement("common/armies", "spawn_chance"),
            FieldRequirement::Cases(vec![
                RequirementCase {
                    condition: spawned(false),
                    required: false,
                },
                RequirementCase {
                    condition: spawned(true),
                    required: false,
                },
            ])
        );
        assert_eq!(
            requirement("common/armies", "is_pop_spawned"),
            FieldRequirement::Optional
        );

        let FieldRequirement::Cases(icon) = requirement("common/planet_classes", "icon") else {
            panic!("planet class icon has no cases");
        };
        let required: Vec<_> = icon.iter().map(|case| case.required).collect();
        assert_eq!(required, [true, false, false, false]);
    }

    #[test]
    fn a_traced_conditional_check_gives_one_case_for_each_side_of_its_condition() {
        let stages = |zero| FieldCondition::FieldZero {
            path: vec!["stages".into()],
            zero,
        };

        assert_eq!(
            M452_REQUIREMENTS.requirement("common/espionage_operation_types", "stage"),
            FieldRequirement::Cases(vec![
                RequirementCase {
                    condition: stages(true),
                    required: false,
                },
                RequirementCase {
                    condition: stages(false),
                    required: true,
                },
            ])
        );
    }
}
