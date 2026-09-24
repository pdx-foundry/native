//! The static question of the modifier families that a registry generates.
use super::Native;
use super::language::gap;
use super::questions::error;
use crate::answer::{
    Answer, Basis, BuildId, Completeness, DeclaredTags, Error, GapKind, GenerationCondition,
    ModifierFamily, NamePart, Operation, Source,
};
use crate::engine::analysis::{
    families::{self, Condition, Family, FamilyResult, METHOD, Part},
    modifiers::{self, CategoryNames, DefinitionSite, Tags},
};

impl Native {
    /// Read the modifier families that one registry's database generator registers for each of
    /// its items, such as `planet_{key}_build_speed_mult` for `common/buildings`.
    ///
    /// Each family is a name template with the place of the item key, the category tags, and
    /// whether every item generates it. Apply [`ModifierFamily::name_for`] to item keys. Other
    /// code that generates modifiers from content, such as generators shared by several
    /// registries, is not joined to a registry: an [`GapKind::UnnamedDeclaration`] gap counts it,
    /// so the answer is partial while such code exists. Where a modifier takes effect is outside
    /// this method.
    ///
    /// The name is a content directory from [`Native::registries`]; another name is
    /// [`Error::UnknownRegistry`].
    pub fn modifier_families(&self, registry: &str) -> Result<Answer<Vec<ModifierFamily>>, Error> {
        let registry = registry.trim_end_matches('/');
        self.answer("modifier_families", Some(registry), || {
            let operation = Operation::ModifierFamilies;
            let analysis = self.declaration_analysis(operation)?;
            let input = analysis
                .family_input(registry)
                .map_err(|failure| error(operation, failure))?
                .ok_or_else(|| Error::UnknownRegistry {
                    name: registry.into(),
                })?;
            let modifier_input = analysis
                .modifier_input()
                .map_err(|failure| error(operation, failure))?;
            let declarations = modifiers::analyze(&modifier_input)
                .map_err(|error| Error::Method(error.to_string()))?;

            let result = families::analyze(&input);
            let masks = result
                .iter()
                .flat_map(|result| &result.sites)
                .filter_map(|(_, family)| family.as_ref().ok().map(|family| family.mask));
            let categories = modifiers::category_names(&modifier_input.categories, masks);
            let runtime_names = declarations
                .sites
                .iter()
                .filter(|site| **site == DefinitionSite::RuntimeToken)
                .count();

            Ok(normalized_families(
                registry,
                result.as_ref(),
                input.unjoined_sites + runtime_names,
                &categories,
                self.build(),
            ))
        })
    }
}

/// The families of one registry, sorted by template. `unjoined` counts the code that generates
/// modifier names and is not joined to any registry.
fn normalized_families(
    registry: &str,
    result: Option<&FamilyResult>,
    unjoined: usize,
    categories: &CategoryNames,
    build: BuildId,
) -> Answer<Vec<ModifierFamily>> {
    let mut value = Vec::new();
    let mut gaps = Vec::new();
    let subject = Some(registry);

    if let Some(result) = result {
        if let Err(reason) = result.key_offset {
            gaps.push(gap(
                GapKind::UnresolvedPath,
                subject,
                format!(
                    "the place of the item key could not be established at {}; no name of the database generator was followed",
                    reason.0
                ),
            ));
        }

        for (_, family) in &result.sites {
            match family {
                Ok(family) => {
                    let family = public_family(family, categories);
                    let template = template(&family);
                    if family.category_tags == DeclaredTags::Unresolved {
                        gaps.push(gap(
                            GapKind::UnresolvedPath,
                            subject,
                            format!("category tags of {template} could not be followed"),
                        ));
                    }
                    if family.condition == GenerationCondition::Unresolved {
                        gaps.push(gap(
                            GapKind::UnresolvedPath,
                            subject,
                            format!("the method could not establish that every item generates {template}"),
                        ));
                    }
                    value.push(family);
                }
                Err(reason) => gaps.push(gap(
                    GapKind::UnresolvedPath,
                    subject,
                    format!(
                        "a name that the database generator registers could not be followed at {}",
                        reason.0
                    ),
                )),
            }
        }
    }

    if unjoined > 0 {
        gaps.push(gap(
            GapKind::UnnamedDeclaration,
            None,
            format!(
                "{unjoined} sites that compose modifier names at run time are not joined to a registry; some may generate this registry's modifiers"
            ),
        ));
    }
    gaps.push(gap(
        GapKind::OutsideMethod,
        subject,
        "The search covers the registry's database generator. Where a modifier takes effect, the tags that a later registration of the same name gives, and names longer than a fixed-size buffer keeps are outside it.",
    ));

    value.sort_by_cached_key(|family| (template(family), format!("{:?}", family.category_tags)));
    let completeness = if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
        Completeness::Complete
    } else {
        Completeness::Partial
    };
    Answer {
        value,
        completeness,
        gaps,
        source: Source::new(build, METHOD, Basis::StaticAnalysis),
    }
}

pub(super) fn public_family(family: &Family, categories: &CategoryNames) -> ModifierFamily {
    let name = family
        .parts
        .iter()
        .filter_map(|part| match part {
            Part::Literal(text) => Some(NamePart::Literal(text.clone())),
            Part::ItemKey => Some(NamePart::ItemKey),
            Part::Unresolved => None,
        })
        .collect();
    let category_tags = match modifiers::tags(categories, family.mask) {
        Tags::Listed(tags) => DeclaredTags::Listed(tags),
        Tags::Unresolved(_) => DeclaredTags::Unresolved,
    };
    let condition = match family.condition {
        Condition::Always => GenerationCondition::Always,
        Condition::Unresolved => GenerationCondition::Unresolved,
    };

    ModifierFamily {
        name,
        category_tags,
        condition,
        name_limit: family.limit.map(|limit| limit as usize),
    }
}

/// The name with `{key}` for the item key, such as `planet_{key}_build_speed_mult`.
fn template(family: &ModifierFamily) -> String {
    family
        .name
        .iter()
        .map(|part| match part {
            NamePart::Literal(text) => text.as_str(),
            NamePart::ItemKey => "{key}",
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::evaluate::Unresolved;
    use std::collections::BTreeMap;

    fn categories() -> CategoryNames {
        BTreeMap::from([
            (1, Ok(Some("Colony".into()))),
            (2, Ok(Some("Ships".into()))),
            (3, Ok(None)),
            (4, Ok(None)),
        ])
    }

    fn family(parts: Vec<Part>, mask: u64, condition: Condition) -> Family {
        Family {
            parts,
            limit: None,
            mask,
            condition,
        }
    }

    fn build() -> BuildId {
        BuildId("test".into())
    }

    #[test]
    fn families_are_sorted_and_gaps_name_each_unresolved_part() {
        let result = FamilyResult {
            key_offset: Ok(0x10),
            sites: vec![
                (
                    1,
                    Ok(family(
                        vec![Part::ItemKey, Part::Literal("_b".into())],
                        3,
                        Condition::Always,
                    )),
                ),
                (
                    2,
                    Ok(family(
                        vec![Part::Literal("a_".into()), Part::ItemKey],
                        1,
                        Condition::Unresolved,
                    )),
                ),
                (3, Err(Unresolved("name"))),
                (
                    4,
                    Ok(family(
                        vec![Part::ItemKey, Part::Literal("_c".into())],
                        4,
                        Condition::Always,
                    )),
                ),
            ],
        };
        let answer = normalized_families("common/x", Some(&result), 0, &categories(), build());

        let templates: Vec<_> = answer.value.iter().map(template).collect();
        assert_eq!(templates, ["a_{key}", "{key}_b", "{key}_c"]);
        assert_eq!(
            answer.value[1].category_tags,
            DeclaredTags::Listed(vec!["Colony".into(), "Ships".into()])
        );
        assert_eq!(answer.value[2].category_tags, DeclaredTags::Unresolved);
        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(answer.source.basis, Basis::StaticAnalysis);

        let details: Vec<_> = answer.gaps.iter().map(|gap| gap.detail.as_str()).collect();
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("every item generates a_{key}"))
        );
        assert!(details.iter().any(|detail| detail.ends_with("at name")));
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("category tags of {key}_c"))
        );
        assert!(
            answer
                .gaps
                .iter()
                .filter(|gap| gap.kind != GapKind::OutsideMethod)
                .all(|gap| gap.subject.as_deref() == Some("common/x"))
        );
    }

    #[test]
    fn unjoined_generation_keeps_every_answer_partial() {
        let complete = normalized_families("common/x", None, 0, &categories(), build());
        assert!(complete.value.is_empty());
        assert_eq!(complete.completeness, Completeness::Complete);

        let partial = normalized_families("common/x", None, 3, &categories(), build());
        assert_eq!(partial.completeness, Completeness::Partial);
        let gap = &partial.gaps[0];
        assert_eq!(gap.kind, GapKind::UnnamedDeclaration);
        assert!(gap.detail.starts_with("3 sites"));

        let missing_key = FamilyResult {
            key_offset: Err(Unresolved("key-storage")),
            sites: Vec::new(),
        };
        let answer = normalized_families("common/x", Some(&missing_key), 0, &categories(), build());
        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(answer.gaps[0].kind, GapKind::UnresolvedPath);
    }

    #[test]
    fn a_name_longer_than_the_limit_has_no_name() {
        let family = ModifierFamily {
            name: vec![NamePart::ItemKey, NamePart::Literal("_mult".into())],
            category_tags: DeclaredTags::Unresolved,
            condition: GenerationCondition::Always,
            name_limit: Some(8),
        };
        assert_eq!(family.name_for("abc").as_deref(), Some("abc_mult"));
        assert_eq!(family.name_for("abcd"), None);
    }
}
