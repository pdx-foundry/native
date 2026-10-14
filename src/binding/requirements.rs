//! Which root fields a definition must contain, from one recorded census run per build.
//!
//! A recorded manual exception. Claim: a root field is required where loading a definition
//! without it logs a report of its absence, and optional where it logs none. Conditions: the
//! exact build of the census, and a registry whose empty definition the run read. Obstacle: the
//! checks are one-off member tests in post-read hooks, with no shared shape, and the method
//! cannot join a tested member to its key. Removal route: a static reading of those hooks.
//! `docs/native/registry-fields.md`, "Required fields", has the run, the joins and the limits.
use crate::FieldRequirement;

/// One build's census of required root fields.
pub(in crate::binding) struct RequirementCensus {
    /// Registries whose empty definition the census run read, and whose every absence report
    /// joins a field key or names a key outside the registry's fields.
    pub registries: &'static [&'static str],
    /// `(registry, field)` pairs whose absence an unconditional check reports. A pair can be
    /// outside `registries` when a static check establishes it.
    pub required: &'static [(&'static str, &'static str)],
    /// `(registry, field)` pairs whose absence check depends on another field's value.
    pub conditional: &'static [(&'static str, &'static str)],
}

impl RequirementCensus {
    pub(in crate::binding) fn requirement(&self, registry: &str, field: &str) -> FieldRequirement {
        let listed = |pairs: &[(&str, &str)]| pairs.contains(&(registry, field));

        if listed(self.required) {
            FieldRequirement::Required
        } else if listed(self.conditional) || !self.registries.contains(&registry) {
            FieldRequirement::Unknown
        } else {
            FieldRequirement::Optional
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
        ("common/patrons", "first_contact_event"),
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
    ],
};

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
            requirement("common/planet_classes", "icon"),
            FieldRequirement::Unknown
        );
        assert_eq!(
            requirement("common/diplomatic_actions", "icon"),
            FieldRequirement::Unknown
        );
        assert_eq!(
            requirement("common/tradition_categories", "adoption_bonus"),
            FieldRequirement::Required
        );
        assert_eq!(
            requirement("common/tradition_categories", "traditions"),
            FieldRequirement::Unknown
        );
    }
}
