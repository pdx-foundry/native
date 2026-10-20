//! The stated definition-name rules of the loaders of a registry.
//!
//! **Recorded manual exception (stated definition names).** Claim: a template loader
//! (`TSingleObjectGameDatabase<…>`) names each root definition by its key; the events loader
//! names a `namespace = x` statement by its value and every other accepted root statement by its
//! `id` field; a tradition, technology or authority item names each of its swap blocks by its
//! `name` field; each custom loader in [`database_rules`] names its definitions as its rule says,
//! names none of them, or has an obstacle that no form states (a renaming field, a reserved root
//! key, a name from a file or a nested key). Conditions: the functions that each rule names are
//! bound on the build; otherwise that rule is missing. Obstacle: the name reaches the item, the
//! namespace table or the swap's member through stack copies, inlined construction and a later
//! lookup, which no method follows. Removal route: a reader that follows a key or field value
//! into the member that the loader or a lookup compares. `docs/native/loader-rules.md` records
//! the disassembly and the checks.
use std::collections::BTreeSet;

use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::loader_rules::{Rule, StatedRule};
use crate::{DefinitionForm, DefinitionName, DefinitionStatement};

/// A template loader's function that reads the root statements of one file.
const LOAD_FROM_READER: &str = "::LoadFromReader(CReader&, bool)";

/// The obstacle of a block field that replaces the name that a root key gives.
const KEY_FIELD_RENAMES: &str = "a `key` field in a definition's block replaces its name";

/// The obstacle of names that are the keys of nested statements.
const NESTED_KEYS: &str =
    "definitions are named by the keys of nested statements at a depth that the root key selects";

/// A template loader of the registry: its loader class and its item class.
pub(in crate::binding) struct TemplateLoader<'a> {
    pub loader_class: &'a str,
    pub item: &'a str,
}

/// A rule, and the functions that a build binds when the rule holds.
struct Stated {
    functions: &'static [&'static str],
    rule: Rule,
}

/// The stated rules of `registry`, whose loaders are `templates` and the scheduled `databases`,
/// in rule order: each template's root definitions and then the blocks they hold, then each
/// database's rules for the registry.
pub(in crate::binding) fn stated_rules(
    symbols: &[Symbol],
    templates: &[TemplateLoader<'_>],
    databases: &BTreeSet<String>,
    registry: &str,
) -> Vec<StatedRule> {
    let bound: BTreeSet<&str> = symbols.iter().map(|symbol| symbol.name.as_str()).collect();
    let holds = |functions: &[&str]| functions.iter().all(|name| bound.contains(name));
    let held = |stated: Stated| holds(stated.functions).then_some(stated.rule);
    let mut rules = Vec::new();

    for template in templates {
        let loader = format!("{}{LOAD_FROM_READER}", template.loader_class);
        rules.push(holds(&[&loader]).then(|| named(vec![root_key()])));
        rules.extend(item_rules(template.item).into_iter().map(held));
    }

    for database in databases {
        rules.extend(database_rules(database, registry).into_iter().map(held));
    }

    rules
}

/// The rules of the swap blocks that an item class holds.
fn item_rules(item: &str) -> Vec<Stated> {
    match item {
        "CTraditionType" => vec![Stated {
            functions: &[
                "CTraditionType::ReadMember(CReader&, int)",
                "CTraditionSwap::ReadMember(CReader&, int)",
            ],
            rule: named(vec![swap(&["tradition_swap"])]),
        }],
        "CTechnology" => vec![Stated {
            functions: &[
                "CTechnology::ReadMember(CReader&, int)",
                "CTechnologySwap::ReadMember(CReader&, int)",
            ],
            rule: named(vec![swap(&["technology_swap"])]),
        }],
        "CGovernmentAuthorityType" => vec![Stated {
            functions: &[
                "CGovernmentAuthorityType::ReadMember(CReader&, int)",
                "CAdvancedAuthoritySwap::ReadMember(CReader&, int)",
                "CGovernmentAuthorityType::GetSwapByKey(CString) const",
            ],
            rule: named(vec![swap(&["advanced_authority_swap"])]),
        }],
        "CGovernmentCivicType" => vec![Stated {
            functions: &[
                "CGovernmentCivicType::ReadMember(CReader&, int)",
                "CGovernmentCivicTypeSwap::Initialize(CString const&)",
            ],
            rule: obstructed(
                "a `swap_type` block without a `name` field takes its name from its owner",
            ),
        }],
        "CMiscEconomicUnitCollection" => vec![Stated {
            functions: &[
                "CMiscEconomicUnitCollection::ReadMember(CReader&, int)",
                "CMiscEconomicUnit::CMiscEconomicUnit(CString const&)",
            ],
            rule: obstructed(
                "the root statement's block holds the definitions, each an economic unit named by its key",
            ),
        }],
        "CJobType" => vec![Stated {
            functions: &[
                "CJobType::ReadMember(CReader&, int)",
                "CSwapCollection<SPopJobTypeSwap, (EScopeType)1099511627776>::CSerializer::ReadMember(CReader&, int)",
            ],
            rule: obstructed(
                "a `swappable_data` `swap_type` block without a `name` field takes its owner's key",
            ),
        }],
        _ => vec![],
    }
}

/// The rules of a scheduled database whose own loader enumerates its files.
fn database_rules(database: &str, registry: &str) -> Vec<Stated> {
    match database {
        "CEventManager" => vec![
            Stated {
                functions: &[
                    "CEventManager::ReadEventsFile(CReader&, int&, bool)",
                    "CHashTable<CString, CNameSpace, ClassicHashKeyTraits<CString, CNameSpace> >::Find(CString const&)",
                ],
                rule: named(vec![DefinitionForm {
                    statement: DefinitionStatement::RootKey("namespace".into()),
                    name: DefinitionName::Value,
                }]),
            },
            Stated {
                functions: &[
                    "CEventManager::ReadEventsFile(CReader&, int&, bool)",
                    "CEvent::ReadMember(CReader&, int)",
                    "CEventManager::GetIDInNameSpace(CString const&) const",
                ],
                rule: named(vec![DefinitionForm {
                    statement: DefinitionStatement::Root,
                    name: DefinitionName::Field("id".into()),
                }]),
            },
        ],
        "CFallenEmpiresDatabase" => root_keys(&[
            "CFallenEmpiresDatabase::InitFromFile(CString const&)",
            "CFallenEmpiresDatabase::GetFallenEmpireByName(char const*) const",
        ]),
        "COnActionDatabase" => root_keys(&[
            "COnActionDatabase::Init()",
            "COnActionDatabase::GetOnActionList(CString const&) const",
        ]),
        "COpinionModifierDatabase" => root_keys(&[
            "COpinionModifierDatabase::Init()",
            "COpinionModifierDatabase::GetModifier(CString const&) const",
        ]),
        "CStrategicResourceDatabase" => root_keys(&[
            "CStrategicResourceDatabase::Init()",
            "CStrategicResourceDatabase::GetResource(CString const&) const",
        ]),
        "CTraitDatabase" => root_keys(&[
            "CTraitDatabase::ReadTraits(CReader&, int&)",
            "CTraitDatabase::GetTrait(CString const&) const",
        ]),
        "CShipBehaviorDatabase" => vec![Stated {
            functions: &[
                "CShipBehaviorDatabase::InitInstance()",
                "CShipBehaviorReader::CreateAndRead(CReader&)",
                "CShipBehaviorDatabase::AddBehavior(CShipBehavior const*)",
            ],
            rule: named(vec![keyed_field("ship_behavior", "name")]),
        }],
        "CSpecialProjectDatabase" => vec![Stated {
            functions: &[
                "CSpecialProjectDatabase::Init()",
                "CSpecialProjectType::ReadMember(CReader&, int)",
            ],
            rule: named(vec![keyed_field("special_project", "key")]),
        }],
        "CWorldGfxDatabase" => vec![Stated {
            functions: &[
                "CWorldGfxDatabase::ReadMember(CReader&, int)",
                "CWorldGraphicalMapData::ReadMember(CReader&, int)",
                "CWorldGfxDatabase::GetWorldGfxIndex(char const*) const",
            ],
            rule: named(vec![keyed_field("gfx_settings", "world")]),
        }],
        "CColorsDatabase" => unnamed(&["CColorsDatabase::Init()"]),
        "CStartScreenMessageDatabase" => unnamed(&[
            "CStartScreenMessageDatabase::InitInstance()",
            "CStartScreenMessage::ReadMember(CReader&, int)",
        ]),
        "CTerraformDatabase" => unnamed(&[
            "CTerraformDatabase::InitInstance()",
            "CTerraformLink::ReadMember(CReader&, int)",
        ]),
        "CAmbientObjectDatabase" => vec![Stated {
            functions: &[
                "CAmbientObjectDatabase::InitInstance()",
                "CAmbientObjectData::ReadMember(CReader&, int)",
            ],
            rule: obstructed("a `tag` field sets the same name as the `name` field"),
        }],
        "CDesignerDatabase" => vec![Stated {
            functions: &[
                "CDesignerDatabase::Init()",
                "CShipDesign::ReadMember(CReader&, int)",
                "CPersistentName::Read(CReader&)",
            ],
            rule: obstructed(
                "a `name` field whose value is a block names the definition by the block's `key` field",
            ),
        }],
        "CPlanetClassDatabase" => vec![Stated {
            functions: &["CPlanetClassDatabase::Init()"],
            rule: obstructed("a root statement with the key `random_list` is not a definition"),
        }],
        "CStaticModifierDatabase" => vec![Stated {
            functions: &[
                "CStaticModifierDatabase::Init(bool)",
                "CStaticModifier::ReadMember(CReader&, int)",
            ],
            rule: obstructed(KEY_FIELD_RENAMES),
        }],
        "CEmpireDesignDatabase" => vec![Stated {
            functions: &[
                "CEmpireDesignDatabase::InitFromFile(char const*, bool)",
                "CEmpireDesign::ReadMember(CReader&, int)",
            ],
            rule: Rule {
                forms: vec![],
                obstacles: vec![
                    "a root statement with the key `default` is not a definition",
                    KEY_FIELD_RENAMES,
                ],
            },
        }],
        "CPrescriptedSpeciesDatabase" => vec![Stated {
            functions: &[
                "CPrescriptedSpeciesDatabase::InitInstance()",
                "CPrescriptedSpecies::ReadMember(CReader&, int)",
            ],
            rule: obstructed(
                "a second loader reads root statements with the key `prescripted_country` and names them by their `key` field",
            ),
        }],
        "CWeaponTagDatabase" => vec![Stated {
            functions: &[
                "CWeaponTagDatabase::InitInstance()",
                "SComponentTag::RegisterTag(char const*)",
            ],
            rule: obstructed("each token of a file is a definition named by its text"),
        }],
        "CSpeciesNamesDatabase" => vec![Stated {
            functions: &[
                "CSpeciesNamesDatabase::Init()",
                "CSpeciesNamesDatabase::ReadNameCollection(CSpeciesNameCollection&, CReader&)",
            ],
            rule: obstructed(NESTED_KEYS),
        }],
        "CDiploPhraseDatabase" => vec![Stated {
            functions: &["CDiploPhraseDatabase::Init()"],
            rule: obstructed(NESTED_KEYS),
        }],
        "CEmpireFlagDatabase" => vec![Stated {
            functions: &["CEmpireFlagDatabase::Init()"],
            rule: obstructed("definitions are image files named by their file name"),
        }],
        "CShipDesignTemplatesDatabase" => ship_design_template_rules(registry),
        "CNameListDatabase" => root_keys(&[
            "CNameListDatabase::Init()",
            "CNameListDatabase::GetNameList(CString const&) const",
        ]),
        "CGfxCultureDatabase" => root_keys(&[
            "CGfxCultureDatabase::Init()",
            "CGfxCultureDatabase::GetCulture(CString const&) const",
        ]),
        "CGameRules" => root_keys(&["CGameRules::InitFromFile(CString const&)"]),
        "CSongDatabase" => vec![Stated {
            functions: &[
                "CSongDatabase::InitFromFile(CReader&)",
                "CSong::ReadMember(CReader&, int)",
            ],
            rule: named(vec![keyed_field("song", "name")]),
        }],
        "CRandomNameDatabase" => random_name_rules(registry),
        _ => vec![],
    }
}

/// The rules of the three directories that `CShipDesignTemplatesDatabase::Init(bool)` loads:
/// each takes only its root keys and names a definition by its `key` field, whose hash the
/// directory's lookup compares.
fn ship_design_template_rules(registry: &str) -> Vec<Stated> {
    let (functions, root_keys): (&'static [&'static str], &[&str]) = match registry {
        "common/section_templates" => (
            &[
                "CShipDesignTemplatesDatabase::Init(bool)",
                "CSectionTemplate::ReadMember(CReader&, int)",
                "CShipDesignTemplatesDatabase::GetSectionTemplate(CString const&) const",
            ],
            &["ship_section_template"],
        ),
        "common/component_sets" => (
            &[
                "CShipDesignTemplatesDatabase::Init(bool)",
                "CComponentSet::ReadMember(CReader&, int)",
                "CShipDesignTemplatesDatabase::GetComponentSet(CString const&) const",
            ],
            &["component_set"],
        ),
        "common/component_templates" => (
            &[
                "CShipDesignTemplatesDatabase::Init(bool)",
                "CComponentTemplate::ReadMember(CReader&, int)",
                "CShipDesignTemplatesDatabase::GetComponentTemplate(CString const&, bool) const",
            ],
            &[
                "strike_craft_component_template",
                "utility_component_template",
                "weapon_component_template",
            ],
        ),
        _ => return vec![],
    };
    let forms = root_keys
        .iter()
        .map(|root_key| keyed_field(root_key, "key"))
        .collect();

    vec![Stated {
        functions,
        rule: named(forms),
    }]
}

/// The rules of the two directories that `CRandomNameDatabase` loads. In `common/random_names`
/// each `*_name_parts_list` statement is named by its `key` field, which `GetPartFromList`
/// compares, and a `*_name_format` statement has no name. In `common/random_names/base` each
/// root key selects a fixed list, and the lists of `custom_star_names` and
/// `custom_planet_names` are named by their nested keys.
fn random_name_rules(registry: &str) -> Vec<Stated> {
    match registry {
        "common/random_names" => vec![Stated {
            functions: &[
                "CRandomNameDatabase::ReadRandomNames()",
                "CCompositNamePartList::ReadMember(CReader&, int)",
                "CRandomNameDatabase::GetPartFromList(CPdxArray<CCompositNamePartList, int> const&, CString const&, int, ERandomNameDatabaseListType, CRandomNameDatabaseVariables*) const",
            ],
            rule: named(
                [
                    "empire_name_parts_list",
                    "federation_name_parts_list",
                    "pop_faction_name_parts_list",
                    "pre_communications_name_parts_list",
                    "war_name_parts_list",
                ]
                .iter()
                .map(|root_key| keyed_field(root_key, "key"))
                .collect(),
            ),
        }],
        "common/random_names/base" => vec![Stated {
            functions: &["CRandomNameDatabase::FillDatabase()"],
            rule: obstructed(NESTED_KEYS),
        }],
        _ => vec![],
    }
}

fn named(forms: Vec<DefinitionForm>) -> Rule {
    Rule {
        forms,
        obstacles: vec![],
    }
}

fn obstructed(obstacle: &'static str) -> Rule {
    Rule {
        forms: vec![],
        obstacles: vec![obstacle],
    }
}

/// The rule of a custom loader that names each root definition by its key.
fn root_keys(functions: &'static [&'static str]) -> Vec<Stated> {
    vec![Stated {
        functions,
        rule: named(vec![root_key()]),
    }]
}

/// The rule of a custom loader that keeps its definitions by position and names none of them.
fn unnamed(functions: &'static [&'static str]) -> Vec<Stated> {
    vec![Stated {
        functions,
        rule: named(vec![]),
    }]
}

fn root_key() -> DefinitionForm {
    DefinitionForm {
        statement: DefinitionStatement::Root,
        name: DefinitionName::Key,
    }
}

/// Root statements with the key `root_key`, each named by its `field`.
fn keyed_field(root_key: &str, field: &str) -> DefinitionForm {
    DefinitionForm {
        statement: DefinitionStatement::RootKey(root_key.into()),
        name: DefinitionName::Field(field.into()),
    }
}

/// Swap blocks at `field_path` in a root definition, each named by its `name` field.
fn swap(field_path: &[&str]) -> DefinitionForm {
    DefinitionForm {
        statement: DefinitionStatement::Nested {
            field_path: field_path.iter().map(|&key| key.into()).collect(),
        },
        name: DefinitionName::Field("name".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADER: &str = "TSingleObjectGameDatabase<CTraditionTypeDatabase, CTraditionType, false>";

    fn symbols(names: &[&str]) -> Vec<Symbol> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| Symbol {
                name: (*name).to_owned(),
                address: 0x1000 + index as u64 * 4,
            })
            .collect()
    }

    fn template(item: &str) -> TemplateLoader<'_> {
        TemplateLoader {
            loader_class: LOADER,
            item,
        }
    }

    fn load_from_reader() -> String {
        format!("{LOADER}{LOAD_FROM_READER}")
    }

    fn database(name: &str) -> BTreeSet<String> {
        BTreeSet::from([name.to_owned()])
    }

    fn functions(rules: &[Stated]) -> Vec<&'static str> {
        rules
            .iter()
            .flat_map(|stated| stated.functions.iter().copied())
            .collect()
    }

    #[test]
    fn a_template_loader_names_root_definitions_by_their_key() {
        let symbols = symbols(&[&load_from_reader()]);

        let stated = stated_rules(
            &symbols,
            &[template("CExample")],
            &BTreeSet::new(),
            "common/examples",
        );

        assert_eq!(stated, [Some(named(vec![root_key()]))]);
    }

    #[test]
    fn a_template_loader_without_its_reader_states_no_rule() {
        let stated = stated_rules(
            &[],
            &[template("CExample")],
            &BTreeSet::new(),
            "common/examples",
        );

        assert_eq!(stated, [None]);
    }

    #[test]
    fn a_tradition_item_names_its_swaps_by_their_name_field() {
        let loader = load_from_reader();
        let mut names = vec![loader.as_str()];
        names.extend(functions(&item_rules("CTraditionType")));

        let stated = stated_rules(
            &symbols(&names),
            &[template("CTraditionType")],
            &BTreeSet::new(),
            "common/traditions",
        );

        assert_eq!(
            stated,
            [
                Some(named(vec![root_key()])),
                Some(named(vec![swap(&["tradition_swap"])])),
            ]
        );
    }

    #[test]
    fn an_item_without_the_swap_reader_misses_the_swap_rule() {
        let loader = load_from_reader();
        let symbols = symbols(&[&loader, "CTechnology::ReadMember(CReader&, int)"]);

        let stated = stated_rules(
            &symbols,
            &[template("CTechnology")],
            &BTreeSet::new(),
            "common/technology",
        );

        assert_eq!(stated[1], None);
    }

    #[test]
    fn a_civic_swap_with_a_name_from_its_owner_states_an_obstacle_and_no_form() {
        let loader = load_from_reader();
        let mut names = vec![loader.as_str()];
        names.extend(functions(&item_rules("CGovernmentCivicType")));

        let stated = stated_rules(
            &symbols(&names),
            &[template("CGovernmentCivicType")],
            &BTreeSet::new(),
            "common/governments/civics",
        );

        let swap_rule = stated[1].as_ref().expect("the swap rule holds");
        assert!(swap_rule.forms.is_empty());
        assert_eq!(swap_rule.obstacles.len(), 1);
    }

    #[test]
    fn a_collection_whose_block_keys_name_its_units_states_an_obstacle() {
        let loader = load_from_reader();
        let mut names = vec![loader.as_str()];
        names.extend(functions(&item_rules("CMiscEconomicUnitCollection")));

        let stated = stated_rules(
            &symbols(&names),
            &[template("CMiscEconomicUnitCollection")],
            &BTreeSet::new(),
            "common/country_container",
        );

        let unit_rule = stated[1].as_ref().expect("the unit rule holds");
        assert!(unit_rule.forms.is_empty());
        assert_eq!(unit_rule.obstacles.len(), 1);
    }

    #[test]
    fn the_events_loader_names_namespaces_by_value_and_events_by_id() {
        let rules = database_rules("CEventManager", "events");
        let symbols = symbols(&functions(&rules));

        let stated = stated_rules(&symbols, &[], &database("CEventManager"), "events");

        assert_eq!(stated.len(), 2);
        assert!(stated.iter().all(Option::is_some));
    }

    #[test]
    fn the_events_loader_without_the_id_reader_misses_only_the_event_rule() {
        let rules = database_rules("CEventManager", "events");
        let symbols = symbols(rules[0].functions);

        let stated = stated_rules(&symbols, &[], &database("CEventManager"), "events");

        assert_eq!(stated, [Some(rules[0].rule.clone()), None]);
    }

    #[test]
    fn a_custom_loader_that_names_by_a_field_of_one_root_key_states_that_form() {
        let rules = database_rules("CWorldGfxDatabase", "gfx/worldgfx");
        let symbols = symbols(&functions(&rules));

        let stated = stated_rules(
            &symbols,
            &[],
            &database("CWorldGfxDatabase"),
            "gfx/worldgfx",
        );

        assert_eq!(
            stated,
            [Some(named(vec![keyed_field("gfx_settings", "world")]))]
        );
    }

    #[test]
    fn a_loader_of_several_directories_states_the_rule_of_the_asked_directory() {
        let loader = "CShipDesignTemplatesDatabase";
        let stated = |registry| {
            let rules = database_rules(loader, registry);
            let symbols = symbols(&functions(&rules));

            stated_rules(&symbols, &[], &database(loader), registry)
        };

        assert_eq!(
            stated("common/component_sets"),
            [Some(named(vec![keyed_field("component_set", "key")]))]
        );
        assert_eq!(
            stated("common/component_templates"),
            [Some(named(vec![
                keyed_field("strike_craft_component_template", "key"),
                keyed_field("utility_component_template", "key"),
                keyed_field("weapon_component_template", "key"),
            ]))]
        );
        assert!(stated("common/other").is_empty());
    }

    #[test]
    fn a_custom_loader_with_a_renaming_field_states_its_obstacle_and_no_form() {
        let rules = database_rules("CStaticModifierDatabase", "common/static_modifiers");
        let symbols = symbols(&functions(&rules));

        let stated = stated_rules(
            &symbols,
            &[],
            &database("CStaticModifierDatabase"),
            "common/static_modifiers",
        );

        assert_eq!(stated, [Some(obstructed(KEY_FIELD_RENAMES))]);
    }

    #[test]
    fn a_custom_loader_that_names_no_definition_states_no_form_and_no_obstacle() {
        let rules = database_rules("CTerraformDatabase", "common/terraform");
        let symbols = symbols(&functions(&rules));

        let stated = stated_rules(
            &symbols,
            &[],
            &database("CTerraformDatabase"),
            "common/terraform",
        );

        assert_eq!(stated, [Some(named(vec![]))]);
    }

    #[test]
    fn a_custom_loader_without_its_functions_misses_its_rule() {
        let stated = stated_rules(&[], &[], &database("CTraitDatabase"), "common/traits");

        assert_eq!(stated, [None]);
    }

    #[test]
    fn a_database_with_no_stated_rule_states_nothing() {
        let stated = stated_rules(&[], &[], &database("CExampleDatabase"), "common/examples");

        assert!(stated.is_empty());
    }
}
