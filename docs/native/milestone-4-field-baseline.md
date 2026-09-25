# Milestone 4 field baseline (SDK-596)

Date: 2026-09-24. M45-release, executable SHA-256
`07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`.
Field discovery, reader classification and binding code are unchanged from Native
`3777d2c139ef4f0e899965b4fee7dd8e83bdbdc6`. The SDK-596 working tree adds report
columns and a modifier-answer repair; neither changes this field population. This is one
`registry-fields/v3` run before SDK-563 or any Milestone 4 field method change.

The [discovery notes](discovery.md#milestone-4-field-baseline-sdk-596) explain the failure shapes
and the four identified signatures whose kinds remain unknown. This page holds the per-registry
and per-reader counts. It measures operation completeness, not Atlas claim coverage.

## Reproduce

```sh
cargo run --release --example registry-field-sweep -- "$STELLARIS_PATH" > field-baseline.json
```

Use the exact executable above. The original report is `.local/sdk-596/field-baseline.json`.
The local read-only symbol probe and its dependency lockfile are in
`.local/sdk-596/reader-symbols/`; its result is `.local/sdk-596/reader-signatures.json`.
It verifies the executable hash, demangles ARM64 definition symbols with the same pinned
`cpp_demangle` version as Native, and joins the SHA-256 name prefixes to the reported reader IDs.
All 19 IDs match exactly one signature. The probe does not infer a reader for the 221 fields
without an established identity. Keep these private inputs until their findings have a retained
replacement; the Markdown tables preserve the measurements independently of routine logs.

## Totals

| Result | Count |
| --- | ---: |
| Complete registry answers | 28 |
| Partial registry answers | 136 |
| Failed registry queries | 0 |
| Fields found | 878 |
| Fields with established reader identity | 657 |
| Fields without one reader identity | 221 |
| Fields with known reader kind | 646 |
| Fields with unknown reader kind | 232 |
| Distinct established reader identities | 19 |

One complete answer has zero root fields. Complete means the bounded root search completed;
it does not establish nested grammar or runtime behavior. The run took 66.198 seconds after
opening Native. Failed queries cannot be assigned to reader identities.

## Per registry

Known and unknown count reader kinds. Gaps exclude `OutsideMethod`; several gaps can describe
one unresolved field or path. No registry query failed in this run.

| Registry | Answer | Fields | Known | Unknown | Gaps |
| --- | --- | ---: | ---: | ---: | ---: |
| `common/agreement_presets` | Partial | 10 | 8 | 2 | 2 |
| `common/agreement_resources` | Partial | 1 | 0 | 1 | 1 |
| `common/agreement_term_values` | Partial | 12 | 8 | 4 | 6 |
| `common/agreement_terms` | Partial | 2 | 2 | 0 | 2 |
| `common/ai_budget` | Partial | 6 | 6 | 0 | 2 |
| `common/ai_espionage/operations` | Partial | 0 | 0 | 0 | 3 |
| `common/ai_espionage/spynetworks` | Partial | 4 | 2 | 2 | 2 |
| `common/ai_espionage/targets` | Partial | 0 | 0 | 0 | 3 |
| `common/anomalies` | Partial | 9 | 7 | 2 | 4 |
| `common/archaeological_site_types` | Partial | 11 | 9 | 2 | 4 |
| `common/armies` | Partial | 21 | 19 | 2 | 4 |
| `common/artifact_actions` | Partial | 6 | 5 | 1 | 1 |
| `common/ascension_perk_categories` | Partial | 1 | 0 | 1 | 1 |
| `common/ascension_perks` | Partial | 11 | 9 | 2 | 2 |
| `common/asteroid_belts` | Partial | 0 | 0 | 0 | 2 |
| `common/astral_actions` | Partial | 6 | 5 | 1 | 3 |
| `common/astral_rifts` | Partial | 0 | 0 | 0 | 2 |
| `common/attitudes` | Partial | 1 | 1 | 0 | 2 |
| `common/bombardment_stances` | Partial | 5 | 5 | 0 | 2 |
| `common/buildings` | Partial | 36 | 21 | 15 | 17 |
| `common/button_effects` | Complete | 3 | 3 | 0 | 0 |
| `common/bypass` | Partial | 5 | 5 | 0 | 2 |
| `common/casus_belli` | Partial | 8 | 7 | 1 | 1 |
| `common/cloaking_strength_levels` | Complete | 4 | 4 | 0 | 0 |
| `common/colony_automation` | Partial | 5 | 2 | 3 | 5 |
| `common/colony_automation_categories` | Complete | 1 | 1 | 0 | 0 |
| `common/colony_automation_exceptions` | Partial | 3 | 3 | 0 | 3 |
| `common/colony_types` | Partial | 10 | 5 | 5 | 7 |
| `common/component_slot_templates` | Partial | 0 | 0 | 0 | 3 |
| `common/council_agendas` | Partial | 10 | 9 | 1 | 1 |
| `common/country_container` | Partial | 0 | 0 | 0 | 3 |
| `common/country_customization` | Complete | 3 | 3 | 0 | 0 |
| `common/country_focus/card_categories` | Partial | 10 | 8 | 2 | 2 |
| `common/country_focus/focus_cards` | Partial | 0 | 0 | 0 | 2 |
| `common/country_focus/focus_rewards` | Partial | 5 | 3 | 2 | 4 |
| `common/country_limits/ownership_limits` | Partial | 1 | 0 | 1 | 1 |
| `common/country_limits/ship_of_size_limits` | Partial | 5 | 1 | 4 | 4 |
| `common/country_types` | Partial | 13 | 6 | 7 | 9 |
| `common/crisis_levels` | Partial | 4 | 3 | 1 | 3 |
| `common/crisis_objectives` | Complete | 3 | 3 | 0 | 0 |
| `common/crisis_paths` | Partial | 3 | 1 | 2 | 2 |
| `common/decisions` | Partial | 15 | 14 | 1 | 3 |
| `common/deposit_categories` | Complete | 2 | 2 | 0 | 0 |
| `common/deposits` | Partial | 17 | 13 | 4 | 6 |
| `common/diplomacy_economy` | Partial | 0 | 0 | 0 | 2 |
| `common/diplomatic_actions` | Partial | 10 | 9 | 1 | 3 |
| `common/districts` | Partial | 0 | 0 | 0 | 2 |
| `common/dust_clouds` | Complete | 6 | 6 | 0 | 0 |
| `common/dynamic_text` | Partial | 3 | 1 | 2 | 4 |
| `common/economic_categories` | Partial | 6 | 3 | 3 | 5 |
| `common/economic_plans` | Partial | 0 | 0 | 0 | 2 |
| `common/edicts` | Partial | 17 | 10 | 7 | 9 |
| `common/espionage_assets` | Partial | 6 | 5 | 1 | 1 |
| `common/espionage_operation_categories` | Partial | 0 | 0 | 0 | 2 |
| `common/espionage_operation_types` | Partial | 11 | 5 | 6 | 8 |
| `common/ethic_categories` | Complete | 0 | 0 | 0 | 0 |
| `common/ethics` | Partial | 13 | 10 | 3 | 5 |
| `common/event_chains` | Partial | 5 | 4 | 1 | 1 |
| `common/federation_law_categories` | Partial | 2 | 1 | 1 | 3 |
| `common/federation_laws` | Complete | 9 | 9 | 0 | 0 |
| `common/federation_perks` | Partial | 2 | 2 | 0 | 2 |
| `common/federation_types` | Partial | 9 | 8 | 1 | 3 |
| `common/first_contact` | Partial | 7 | 5 | 2 | 4 |
| `common/frontend_backgrounds` | Partial | 2 | 2 | 0 | 3 |
| `common/galactic_community_actions` | Partial | 1 | 0 | 1 | 1 |
| `common/galactic_focuses` | Partial | 3 | 3 | 0 | 2 |
| `common/game_concept_categories` | Complete | 2 | 2 | 0 | 0 |
| `common/game_concepts` | Partial | 5 | 4 | 1 | 3 |
| `common/game_scenarios` | Partial | 0 | 0 | 0 | 2 |
| `common/governments` | Complete | 9 | 9 | 0 | 0 |
| `common/governments/authorities` | Partial | 18 | 12 | 6 | 8 |
| `common/governments/civics` | Partial | 25 | 14 | 11 | 13 |
| `common/governments/councilors` | Partial | 14 | 11 | 3 | 3 |
| `common/greeting_overlay_sounds` | Complete | 3 | 3 | 0 | 0 |
| `common/intel_categories` | Partial | 1 | 0 | 1 | 1 |
| `common/intel_levels` | Partial | 2 | 1 | 1 | 4 |
| `common/job_tags` | Partial | 0 | 0 | 0 | 3 |
| `common/lawsuits` | Partial | 3 | 2 | 1 | 1 |
| `common/leader_classes` | Partial | 7 | 7 | 0 | 2 |
| `common/leader_tiers` | Complete | 2 | 2 | 0 | 0 |
| `common/map_modes` | Partial | 5 | 4 | 1 | 3 |
| `common/megastructure_overclock_types` | Partial | 10 | 9 | 1 | 3 |
| `common/megastructures` | Partial | 33 | 26 | 7 | 9 |
| `common/menace_perks` | Partial | 5 | 4 | 1 | 1 |
| `common/missions/mission_categories` | Complete | 4 | 4 | 0 | 0 |
| `common/missions/missions` | Partial | 17 | 15 | 2 | 4 |
| `common/mutations` | Partial | 7 | 6 | 1 | 3 |
| `common/named_colors` | Partial | 1 | 0 | 1 | 1 |
| `common/notification_modifiers` | Complete | 1 | 1 | 0 | 0 |
| `common/observation_station_missions` | Partial | 11 | 7 | 4 | 4 |
| `common/patrons` | Partial | 9 | 6 | 3 | 5 |
| `common/patrons/callings` | Partial | 5 | 4 | 1 | 1 |
| `common/patrons/deeds` | Partial | 2 | 2 | 0 | 3 |
| `common/patrons/psionic_auras` | Partial | 4 | 4 | 0 | 2 |
| `common/personalities` | Partial | 0 | 0 | 0 | 2 |
| `common/planet_modifiers` | Complete | 4 | 4 | 0 | 0 |
| `common/policies` | Partial | 5 | 3 | 2 | 4 |
| `common/policy_categories` | Complete | 1 | 1 | 0 | 0 |
| `common/pop_categories` | Partial | 21 | 11 | 10 | 12 |
| `common/pop_faction_types` | Partial | 16 | 15 | 1 | 3 |
| `common/pop_jobs` | Partial | 19 | 6 | 13 | 15 |
| `common/portrait_categories` | Partial | 0 | 0 | 0 | 2 |
| `common/portrait_sets` | Partial | 6 | 0 | 6 | 6 |
| `common/precursor_civilizations` | Complete | 3 | 3 | 0 | 0 |
| `common/prescripted_flags` | Partial | 1 | 0 | 1 | 1 |
| `common/relics` | Partial | 10 | 8 | 2 | 2 |
| `common/resolution_categories` | Partial | 4 | 3 | 1 | 1 |
| `common/resolution_groups` | Partial | 2 | 1 | 1 | 1 |
| `common/resolutions` | Partial | 8 | 6 | 2 | 4 |
| `common/resource_converters` | Partial | 0 | 0 | 0 | 2 |
| `common/resource_regions` | Complete | 1 | 1 | 0 | 0 |
| `common/script_values` | Partial | 0 | 0 | 0 | 3 |
| `common/scripted_actions` | Partial | 0 | 0 | 0 | 2 |
| `common/scripted_effects` | Partial | 0 | 0 | 0 | 3 |
| `common/scripted_modifiers` | Partial | 0 | 0 | 0 | 2 |
| `common/scripted_triggers` | Partial | 0 | 0 | 0 | 3 |
| `common/sector_types` | Partial | 2 | 2 | 0 | 2 |
| `common/ship_categories` | Partial | 9 | 8 | 1 | 1 |
| `common/ship_sets` | Complete | 2 | 2 | 0 | 0 |
| `common/ship_sizes` | Partial | 0 | 0 | 0 | 2 |
| `common/situation_log/categories` | Partial | 4 | 3 | 1 | 3 |
| `common/situations` | Partial | 0 | 0 | 0 | 2 |
| `common/specialist_subject_perks` | Partial | 8 | 7 | 1 | 1 |
| `common/specialist_subject_types` | Partial | 0 | 0 | 0 | 2 |
| `common/species_archetypes` | Partial | 7 | 5 | 2 | 2 |
| `common/species_classes` | Partial | 11 | 6 | 5 | 7 |
| `common/species_rights/citizenship_types` | Partial | 0 | 0 | 0 | 2 |
| `common/species_rights/colonization_controls` | Partial | 1 | 1 | 0 | 3 |
| `common/species_rights/living_standards` | Partial | 0 | 0 | 0 | 3 |
| `common/species_rights/migration_controls` | Partial | 1 | 1 | 0 | 3 |
| `common/species_rights/military_service_types` | Partial | 0 | 0 | 0 | 3 |
| `common/species_rights/population_controls` | Partial | 1 | 1 | 0 | 3 |
| `common/species_rights/purge_types` | Partial | 4 | 3 | 1 | 4 |
| `common/species_rights/slavery_types` | Partial | 3 | 3 | 0 | 3 |
| `common/species_rights/subspecies_integration_types` | Partial | 1 | 1 | 0 | 3 |
| `common/specimens` | Partial | 5 | 2 | 3 | 5 |
| `common/star_classes` | Partial | 11 | 8 | 3 | 6 |
| `common/star_classes/randomizers` | Partial | 1 | 0 | 1 | 1 |
| `common/starbase_buildings` | Partial | 4 | 2 | 2 | 5 |
| `common/starbase_levels` | Partial | 10 | 8 | 2 | 4 |
| `common/starbase_modules` | Partial | 5 | 2 | 3 | 6 |
| `common/starbase_types` | Complete | 5 | 5 | 0 | 0 |
| `common/storm_types` | Partial | 6 | 6 | 0 | 2 |
| `common/system_tooltips` | Complete | 1 | 1 | 0 | 0 |
| `common/system_types` | Complete | 2 | 2 | 0 | 0 |
| `common/target_types` | Partial | 1 | 0 | 1 | 1 |
| `common/technology` | Partial | 0 | 0 | 0 | 2 |
| `common/technology/category` | Complete | 3 | 3 | 0 | 0 |
| `common/technology/tier` | Complete | 2 | 2 | 0 | 0 |
| `common/technology_ages/preftl` | Complete | 1 | 1 | 0 | 0 |
| `common/timeline_events` | Partial | 7 | 3 | 4 | 4 |
| `common/tradable_actions` | Partial | 4 | 4 | 0 | 2 |
| `common/tradition_categories` | Partial | 7 | 3 | 4 | 4 |
| `common/traditions` | Partial | 11 | 9 | 2 | 2 |
| `common/trait_tags` | Partial | 0 | 0 | 0 | 3 |
| `common/war_goals` | Partial | 15 | 14 | 1 | 3 |
| `common/zone_slots` | Partial | 0 | 0 | 0 | 3 |
| `common/zones` | Partial | 0 | 0 | 0 | 3 |
| `dlc_metadata/dlc_recommendations` | Partial | 0 | 0 | 0 | 2 |
| `gfx/portraits/sprite_configurations` | Complete | 3 | 3 | 0 | 0 |
| `gfx/projectiles/planet_destruction` | Partial | 14 | 8 | 6 | 8 |
| `interface/resource_groups` | Partial | 10 | 4 | 6 | 9 |
| `map/galaxy` | Partial | 2 | 2 | 0 | 2 |
| `sound/advisor_voice_types` | Complete | 4 | 4 | 0 | 0 |

## Per reader identity

Complete/partial columns count registry answers that contain each reader. They do not assert
completeness of the reader’s full semantics; a registry can contain several readers.
All identities here apply only to this exact build. Each reader has zero attributable failed
queries; failed queries, when present, have no established reader to join.

| Reader ID | Callee | Kind | Fields | Complete registries | Partial registries |
| --- | --- | --- | ---: | ---: | ---: |
| `231abbf59ab285b4` | `void NParserUtil::ReadKeyReferenceDeferred<CStarbaseLevelTypeDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CStarbaseLevelTypeDatabase::ValueType const**)` | Reference | 1 | 0 | 1 |
| `325efaa17499c32d` | `CReader::Read(CString&, bool)` | String | 171 | 13 | 65 |
| `5d6a4255a3ebd82d` | `CReader::Read(short&)` | Integer | 2 | 0 | 1 |
| `6e28a363faf61c52` | `void NParserUtil::ReadEffect<CRootEffect>(CReader&, CRootEffect&, EScopeType)` | Block | 62 | 3 | 31 |
| `784ec4ab3c469836` | `CReader::Read(CVector2FixedPoint&)` | Unknown | 1 | 0 | 1 |
| `87b0c16089f35350` | `CReader::Read(float&)` | Unknown | 1 | 0 | 1 |
| `97942c9a3d3c9c1b` | `CReader::Read(CPersistent&)` | Block | 119 | 14 | 48 |
| `99b26ea3b826bc63` | `void NParserUtil::ReadTrigger<CAndTrigger>(CReader&, CAndTrigger&, EScopeType)` | Block | 3 | 2 | 1 |
| `a2ff84da2cff0bb5` | `CReader::Read(CColor&)` | Unknown | 3 | 0 | 3 |
| `a430a4b92eb2c8f6` | `void NParserUtil::ReadEffect<CEffect>(CReader&, CEffect&, EScopeType)` | Block | 2 | 0 | 1 |
| `a9818fec780f8313` | `CReader::Read(CFixedPoint&)` | FixedPoint | 28 | 1 | 17 |
| `ae2bd2a2d5591e04` | `void NParserUtil::ReadKeyReferenceDeferred<CStaticModifierDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CStaticModifierDatabase::ValueType const**)` | Reference | 2 | 1 | 1 |
| `b894933b2b2853a5` | `CReader::Read(bool&)` | Boolean | 86 | 7 | 38 |
| `c61abc171edc30aa` | `CVariableValue::Read(CReader&, EScopeType)` | Unknown | 6 | 0 | 4 |
| `c81625955ad8e679` | `void NParserUtil::ReadTrigger<CCustomTooltipTrigger>(CReader&, CCustomTooltipTrigger&, EScopeType)` | Block | 2 | 1 | 1 |
| `d2aff87f9b4b84b1` | `void NParserUtil::ReadKeyReferenceDeferred<CSituationLogCategoryDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CSituationLogCategoryDatabase::ValueType const**)` | Reference | 3 | 0 | 3 |
| `d7a95ab8c8d44489` | `CReader::Read(int&)` | Integer | 60 | 7 | 34 |
| `e044b50825a1841d` | `void NParserUtil::ReadTrigger<CRootTrigger>(CReader&, CRootTrigger&, EScopeType)` | Block | 104 | 12 | 47 |
| `e7571cd65529a619` | `void NParserUtil::ReadKeyReferenceDeferred<CShipSizeDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CShipSizeDatabase::ValueType const**)` | Reference | 1 | 0 | 1 |
