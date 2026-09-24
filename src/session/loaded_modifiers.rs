//! The live question of the loaded modifier inventory, and its join with the static answers.
//!
//! The game gives the table as the engine holds it after all content has loaded
//! (`engine::operations::loaded_modifiers`), with the item keys of each registry that has a
//! database generator. This module joins each entry with two static authorities of the same
//! build: the names that `Native::modifiers` declares, and the families that
//! `Native::modifier_families` gives for each of those registries, applied to the registry's
//! loaded keys. A family explains a name only when its template gives exactly that name for a
//! loaded key; nothing is matched by similarity. Tags follow the rule of `Native::modifiers` for
//! the loaded mask.
//!
//! The static side is prepared before the game starts, so that a static failure starts no game.
use std::collections::{BTreeMap, BTreeSet};

use super::Native;
use super::families::{public_family, unjoined_sites, unjoined_summary};
use super::language::gap;
use super::questions::error;
use crate::answer::{
    Answer, Basis, BuildId, Completeness, DeclaredTags, Error, Gap, GapKind, GeneratedName,
    LoadedContent, LoadedModifier, LoadedModifiers, ModifierFamily, Operation, Source,
};
use crate::engine::analysis::{
    families,
    modifiers::{self, CategoryInput, CategoryNames, DefinitionSite, Tags},
};
use crate::engine::operations::loaded_modifiers::ObservedModifiers;
use crate::protocol::observation::RegistryKeys;

/// Name and revision of the loaded modifier method.
pub(crate) const METHOD: &str = "loaded-modifiers/v1";

/// The static side of the join.
pub(crate) struct ModifierJoin {
    tables: JoinTables,
    /// The engine's category-name function, run for each mask that the loaded table uses.
    categories: CategoryInput,
}

/// The names and families that the join applies.
struct JoinTables {
    declared: BTreeSet<String>,
    /// The families of each registry that has at least one, by content directory.
    families: BTreeMap<String, Vec<ModifierFamily>>,
    /// Limits of the static side that change which names the join can mark or explain.
    gaps: Vec<Gap>,
    content: LoadedContent,
}

impl Native {
    /// Prepare the static side of the loaded modifier join for a session with this fixture.
    pub(crate) fn modifier_join(
        &self,
        fixture: Option<&crate::FixtureRequest>,
    ) -> Result<ModifierJoin, Error> {
        let operation = Operation::LoadedModifiers;
        let analysis = self.declaration_analysis(operation)?;
        let modifier_input = analysis
            .modifier_input()
            .map_err(|failure| error(operation, failure))?;
        let declarations = modifiers::analyze(&modifier_input)
            .map_err(|error| Error::Method(error.to_string()))?;

        let mut declared = BTreeSet::new();
        let mut unreadable = 0;
        for site in &declarations.sites {
            match site {
                DefinitionSite::Declared { name, .. } => {
                    declared.insert(name.clone());
                }
                DefinitionSite::RuntimeToken => {}
                DefinitionSite::Unreadable => unreadable += 1,
            }
        }

        let index = analysis
            .family_index()
            .map_err(|failure| error(operation, failure))?;
        let results: Vec<_> = index
            .registries
            .iter()
            .filter_map(|(registry, code)| {
                families::analyze(&index.input, code).map(|result| (registry.clone(), result))
            })
            .collect();
        let masks = results
            .iter()
            .flat_map(|(_, result)| &result.families)
            .filter_map(|family| family.mask);
        let family_categories = modifiers::category_names(&modifier_input.categories, masks);

        let mut gaps = Vec::new();
        if unreadable > 0 {
            gaps.push(gap(
                GapKind::UnresolvedPath,
                None,
                format!(
                    "{unreadable} direct modifier definitions could not be followed to their names; those loaded names are not marked declared"
                ),
            ));
        }
        let mut families = BTreeMap::new();
        for (registry, result) in results {
            if let Err(reason) = result.key_offset {
                gaps.push(gap(
                    GapKind::UnresolvedPath,
                    Some(&registry),
                    format!(
                        "the place of the item key could not be established at {}; no family of this registry explains a name",
                        reason.0
                    ),
                ));
            }
            for (reason, count) in &result.failures {
                gaps.push(gap(
                    GapKind::UnresolvedPath,
                    Some(&registry),
                    format!(
                        "{count} names or generation calls of the registry's code could not be followed at {reason}; they explain no name"
                    ),
                ));
            }
            let registry_families: Vec<_> = result
                .families
                .iter()
                .map(|family| public_family(family, &family_categories))
                .collect();
            if !registry_families.is_empty() {
                families.insert(registry, registry_families);
            }
        }
        for (registry, join) in &index.joins.registries {
            if join.unnamed_input {
                gaps.push(gap(
                    GapKind::UnnamedDeclaration,
                    Some(registry),
                    "names that combine this registry's keys with the keys of content that Native does not name as a registry are unexplained",
                ));
            }
        }
        if let Some(summary) = unjoined_summary(&unjoined_sites(&index.joins)) {
            gaps.push(gap(
                GapKind::UnnamedDeclaration,
                None,
                format!("{summary}; the names that they add are unexplained"),
            ));
        }

        Ok(ModifierJoin {
            tables: JoinTables {
                declared,
                families,
                gaps,
                content: match fixture {
                    None => LoadedContent::Installation,
                    Some(fixture) => LoadedContent::Fixture {
                        registry: fixture.registry().into(),
                        files: fixture.files.keys().cloned().collect(),
                    },
                },
            },
            categories: modifier_input.categories,
        })
    }
}

impl ModifierJoin {
    /// The registries whose loaded item keys the join needs.
    pub(crate) fn registries(&self) -> Vec<String> {
        self.tables.families.keys().cloned().collect()
    }

    /// Join the observed table with the static side.
    pub(crate) fn assemble(
        &self,
        observed: &ObservedModifiers,
        build: BuildId,
    ) -> Answer<LoadedModifiers> {
        let masks = observed.entries.iter().map(|entry| u64::from(entry.mask));
        let categories = modifiers::category_names(&self.categories, masks);
        self.tables.join(observed, &categories, build)
    }
}

impl JoinTables {
    fn join(
        &self,
        observed: &ObservedModifiers,
        categories: &CategoryNames,
        build: BuildId,
    ) -> Answer<LoadedModifiers> {
        let mut gaps = self.gaps.clone();

        let mut generated: BTreeMap<String, Vec<GeneratedName>> = BTreeMap::new();
        let mut registry_items = BTreeMap::new();
        for (registry, families) in &self.families {
            let keys = match observed.registries.get(registry) {
                Some(RegistryKeys::Keys(keys)) => keys,
                Some(RegistryKeys::Unavailable(reason)) => {
                    gaps.push(gap(
                        GapKind::IncompleteObservation,
                        Some(registry),
                        format!(
                            "the registry's loaded item keys could not be read ({reason}); its families explain no name"
                        ),
                    ));
                    continue;
                }
                None => {
                    gaps.push(gap(
                        GapKind::IncompleteObservation,
                        Some(registry),
                        "the registry's loaded item keys were not read; its families explain no name",
                    ));
                    continue;
                }
            };
            registry_items.insert(registry.clone(), keys.clone());
            for key in keys {
                for family in families {
                    if let Some(name) = family.name_for(key) {
                        generated.entry(name).or_default().push(GeneratedName {
                            registry: registry.clone(),
                            item: key.clone(),
                            template: family.name.clone(),
                        });
                    }
                }
            }
        }

        let mut unresolved_masks = BTreeSet::new();
        let mut unexplained = 0;
        let modifiers = observed
            .entries
            .iter()
            .map(|entry| {
                let mask = u64::from(entry.mask);
                let category_tags = match modifiers::tags(categories, mask) {
                    Tags::Listed(tags) => DeclaredTags::Listed(tags),
                    Tags::Unresolved(_) => {
                        unresolved_masks.insert(mask);
                        DeclaredTags::Unresolved
                    }
                };
                let declared = self.declared.contains(&entry.name);
                let generated_by = generated.remove(&entry.name).unwrap_or_default();
                if !declared && generated_by.is_empty() {
                    unexplained += 1;
                }
                LoadedModifier {
                    name: entry.name.clone(),
                    category_tags,
                    declared,
                    generated_by,
                }
            })
            .collect();

        for mask in unresolved_masks {
            gaps.push(gap(
                GapKind::UnresolvedPath,
                None,
                format!("the category tags of mask {mask:#x} could not be followed"),
            ));
        }
        if unexplained > 0 {
            gaps.push(gap(
                GapKind::UnnamedDeclaration,
                None,
                format!(
                    "{unexplained} loaded modifiers are neither declared by the executable nor given by a returned family for a loaded item"
                ),
            ));
        }
        gaps.push(gap(
            GapKind::OutsideMethod,
            None,
            "The table is read where the engine documents its modifiers, after content loads. Modifiers added later, such as during a game, and where a modifier takes effect are outside it.",
        ));

        let completeness = if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
            Completeness::Complete
        } else {
            Completeness::Partial
        };
        Answer {
            value: LoadedModifiers {
                modifiers,
                registry_items,
                content: self.content.clone(),
            },
            completeness,
            gaps,
            source: Source::new(build, METHOD, Basis::LiveObservation),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer::{GenerationCondition, NamePart};
    use crate::engine::analysis::evaluate::Unresolved;
    use crate::protocol::observation::ModifierEntry;

    const COLONY: u64 = 0x4000_0000;
    const SHIPS: u64 = 0x100;
    const ECONOMY: u64 = 0x1000_0000;

    /// Named single bits, one named whole mask, and one mask whose name the switch cannot give.
    fn categories() -> CategoryNames {
        BTreeMap::from([
            (COLONY, Ok(Some("Colony".into()))),
            (SHIPS, Ok(Some("Ships".into()))),
            (ECONOMY, Ok(Some("AI Economy".into()))),
            (COLONY | ECONOMY, Ok(None)),
            (SHIPS | ECONOMY, Ok(Some("Ship Economy".into()))),
            (0x2, Err(Unresolved("category-name"))),
        ])
    }

    fn family(prefix: &str, suffix: &str, limit: Option<usize>) -> ModifierFamily {
        ModifierFamily {
            name: vec![
                NamePart::Literal(prefix.into()),
                NamePart::ItemKey,
                NamePart::Literal(suffix.into()),
            ],
            category_tags: DeclaredTags::Listed(vec!["Colony".into()]),
            condition: GenerationCondition::Always,
            name_limit: limit,
        }
    }

    fn tables() -> JoinTables {
        JoinTables {
            declared: BTreeSet::from(["pop_happiness".into(), "terraforming_cost_mult".into()]),
            families: BTreeMap::from([
                (
                    "common/buildings".into(),
                    vec![family("planet_", "_build_speed_mult", None)],
                ),
                (
                    "common/districts".into(),
                    vec![family("planet_", "_build_speed_mult", None)],
                ),
                (
                    "common/bypass".into(),
                    vec![family("", "_ship_windup_mult", Some(30))],
                ),
            ]),
            gaps: Vec::new(),
            content: LoadedContent::Installation,
        }
    }

    fn entry(name: &str, mask: u64) -> ModifierEntry {
        ModifierEntry {
            name: name.into(),
            mask: mask as u32,
        }
    }

    fn observed() -> ObservedModifiers {
        ObservedModifiers {
            entries: vec![
                entry("pop_happiness", COLONY),
                entry("terraforming_cost_mult", COLONY | ECONOMY),
                entry("planet_shared_build_speed_mult", COLONY),
                entry("gateway_ship_windup_mult", SHIPS | ECONOMY),
                entry("planet_capital_build_speed_mult_extra", COLONY),
                entry("long_gateway_name_ship_windup_mult", 0x2),
            ],
            registries: BTreeMap::from([
                (
                    "common/buildings".into(),
                    RegistryKeys::Keys(vec!["shared".into(), "capital".into()]),
                ),
                (
                    "common/districts".into(),
                    RegistryKeys::Keys(vec!["shared".into()]),
                ),
                (
                    "common/bypass".into(),
                    RegistryKeys::Keys(vec!["gateway".into(), "long_gateway_name".into()]),
                ),
            ]),
        }
    }

    fn join(observed: &ObservedModifiers) -> Answer<LoadedModifiers> {
        tables().join(observed, &categories(), BuildId("test".into()))
    }

    fn named<'a>(answer: &'a Answer<LoadedModifiers>, name: &str) -> &'a LoadedModifier {
        answer
            .value
            .modifiers
            .iter()
            .find(|modifier| modifier.name == name)
            .unwrap()
    }

    #[test]
    fn loaded_tags_follow_the_rule_of_the_declarations() {
        let answer = join(&observed());
        let tags = |name| named(&answer, name).category_tags.clone();
        assert_eq!(
            tags("pop_happiness"),
            DeclaredTags::Listed(vec!["Colony".into()])
        );
        // An unnamed whole mask lists each named bit; a named whole mask is one tag.
        assert_eq!(
            tags("terraforming_cost_mult"),
            DeclaredTags::Listed(vec!["AI Economy".into(), "Colony".into()])
        );
        assert_eq!(
            tags("gateway_ship_windup_mult"),
            DeclaredTags::Listed(vec!["Ship Economy".into()])
        );
        assert_eq!(
            tags("long_gateway_name_ship_windup_mult"),
            DeclaredTags::Unresolved
        );
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnresolvedPath && gap.detail.contains("0x2"))
        );
        assert_eq!(answer.source.basis, Basis::LiveObservation);
        assert_eq!(answer.source.method, METHOD);
    }

    #[test]
    fn a_name_is_explained_only_by_an_exact_family_name_for_a_loaded_key() {
        let answer = join(&observed());
        assert!(named(&answer, "pop_happiness").declared);
        assert!(named(&answer, "pop_happiness").generated_by.is_empty());
        // Two registries give the same name for their loaded items; both are kept.
        let shared = named(&answer, "planet_shared_build_speed_mult");
        assert!(!shared.declared);
        assert_eq!(
            shared
                .generated_by
                .iter()
                .map(|generated| (generated.registry.as_str(), generated.item.as_str()))
                .collect::<Vec<_>>(),
            [
                ("common/buildings", "shared"),
                ("common/districts", "shared")
            ]
        );
        assert_eq!(
            named(&answer, "gateway_ship_windup_mult").generated_by[0].template,
            family("", "_ship_windup_mult", None).name
        );
        // A similar name, and a name longer than the family's limit, are not explained.
        for name in [
            "planet_capital_build_speed_mult_extra",
            "long_gateway_name_ship_windup_mult",
        ] {
            let modifier = named(&answer, name);
            assert!(
                !modifier.declared && modifier.generated_by.is_empty(),
                "{name}"
            );
        }
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnnamedDeclaration
                    && gap.detail.starts_with("2 loaded modifiers"))
        );
        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(
            answer.value.registry_items["common/buildings"],
            ["shared", "capital"]
        );
    }

    #[test]
    fn a_registry_without_loaded_keys_explains_nothing_and_has_a_gap() {
        let mut observed = observed();
        observed.registries.insert(
            "common/districts".into(),
            RegistryKeys::Unavailable("registry database instance is null".into()),
        );
        observed.registries.remove("common/bypass");
        let answer = join(&observed);
        let shared = named(&answer, "planet_shared_build_speed_mult");
        assert_eq!(shared.generated_by.len(), 1);
        for registry in ["common/districts", "common/bypass"] {
            assert!(
                answer
                    .gaps
                    .iter()
                    .any(|gap| gap.kind == GapKind::IncompleteObservation
                        && gap.subject.as_deref() == Some(registry))
            );
            assert!(!answer.value.registry_items.contains_key(registry));
        }
    }

    #[test]
    fn a_fully_explained_table_is_complete_with_its_scope_stated() {
        let observed = ObservedModifiers {
            entries: vec![
                entry("pop_happiness", COLONY),
                entry("planet_capital_build_speed_mult", COLONY),
            ],
            registries: observed().registries,
        };
        let answer = join(&observed);
        assert_eq!(answer.completeness, Completeness::Complete);
        assert!(
            answer
                .gaps
                .iter()
                .all(|gap| gap.kind == GapKind::OutsideMethod)
        );
        assert_eq!(answer.value.content, LoadedContent::Installation);
    }
}
