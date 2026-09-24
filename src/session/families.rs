//! The static question of the modifier families that a registry generates.
use std::collections::BTreeMap;

use super::Native;
use super::language::registry_gap;
use super::questions::error;
use crate::answer::{
    Answer, Basis, BuildId, Completeness, DeclaredTags, Error, GapKind, GenerationCondition,
    ModifierFamily, NamePart, Operation, Source,
};
use crate::engine::analysis::{
    families::{
        self, Condition, Family, FamilyResult, METHOD, Part,
        joins::{Joins, Reason, SiteJoin},
    },
    modifiers::{self, CategoryNames, Tags},
};

impl Native {
    /// Read the modifier families that one registry's code registers for each of its items, such
    /// as `planet_{key}_build_speed_mult` for `common/buildings`.
    ///
    /// Each family is a name template with the place of the item key, the category tags, and
    /// whether every item generates it. Apply [`ModifierFamily::name_for`] to item keys. Code that
    /// generates modifiers from content that is not joined to this registry, such as code of
    /// content that Native does not name as a registry, is counted in an
    /// [`GapKind::UnnamedDeclaration`] gap, so the answer is partial while such code exists. Where
    /// a modifier takes effect is outside this method.
    ///
    /// The name is a content directory from [`Native::registries`]; another name is
    /// [`Error::UnknownRegistry`].
    pub fn modifier_families(&self, registry: &str) -> Result<Answer<Vec<ModifierFamily>>, Error> {
        let registry = registry.trim_end_matches('/');
        self.answer("modifier_families", Some(registry), || {
            let operation = Operation::ModifierFamilies;
            let analysis = self.declaration_analysis(operation)?;
            let index = analysis
                .family_index()
                .map_err(|failure| error(operation, failure))?;
            let code = index
                .registries
                .get(registry)
                .ok_or_else(|| Error::UnknownRegistry {
                    name: registry.into(),
                })?;
            let modifier_input = analysis
                .modifier_input()
                .map_err(|failure| error(operation, failure))?;

            let result = families::analyze(&index.input, code);
            let masks = result
                .iter()
                .flat_map(|result| &result.families)
                .filter_map(|family| family.mask);
            let categories = modifiers::category_names(&modifier_input.categories, masks);
            let unnamed_input = index
                .joins
                .registries
                .get(registry)
                .is_some_and(|join| join.unnamed_input);

            Ok(normalized_families(
                registry,
                result.as_ref(),
                unnamed_input,
                &unjoined_sites(&index.joins),
                &categories,
                self.build(),
            ))
        })
    }
}

/// The generation calls that are not joined to a registry, by reason.
pub(super) fn unjoined_sites(joins: &Joins) -> BTreeMap<Reason, usize> {
    let mut counts = BTreeMap::new();
    for join in joins.sites.values() {
        if let SiteJoin::Unjoined(reason) = join {
            *counts.entry(*reason).or_default() += 1;
        }
    }
    counts
}

/// The count of unjoined generation calls with each reason, in words, or `None` when every call
/// is joined.
pub(super) fn unjoined_summary(unjoined: &BTreeMap<Reason, usize>) -> Option<String> {
    let total: usize = unjoined.values().sum();
    if total == 0 {
        return None;
    }

    let reasons: Vec<String> = unjoined
        .iter()
        .map(|(reason, count)| {
            let reason = match reason {
                Reason::RegistrationFunction => "inside the registration function",
                Reason::UnnamedInput => {
                    "combine a registry's keys with the keys of content that Native does not name as a registry"
                }
                Reason::UnnamedContent => {
                    "reached from the post-read code of content objects that are not items of a named registry"
                }
                Reason::NoRoot => "reached by no chain of calls from a registry's code",
            };
            format!("{count} {reason}")
        })
        .collect();
    Some(format!(
        "{total} sites that compose modifier names at run time are not joined to a registry ({})",
        reasons.join("; ")
    ))
}

/// The families of one registry, sorted by template. `unjoined` counts the code that generates
/// modifier names and is not joined to any registry.
fn normalized_families(
    registry: &str,
    result: Option<&FamilyResult>,
    unnamed_input: bool,
    unjoined: &BTreeMap<Reason, usize>,
    categories: &CategoryNames,
    build: BuildId,
) -> Answer<Vec<ModifierFamily>> {
    let mut value = Vec::new();
    let mut gaps = Vec::new();
    let subject = Some(registry);

    if let Some(result) = result {
        if let Err(reason) = result.key_offset {
            gaps.push(registry_gap(
                GapKind::UnresolvedPath,
                subject,
                format!(
                    "the place of the item key could not be established at {}; no name of the registry's code was followed",
                    reason.0
                ),
            ));
        }
        for (reason, count) in &result.failures {
            gaps.push(registry_gap(
                GapKind::UnresolvedPath,
                subject,
                format!(
                    "{count} names or generation calls of the registry's code could not be followed at {reason}"
                ),
            ));
        }

        for family in &result.families {
            let public = public_family(family, categories);
            let template = template(&public);
            if public.category_tags == DeclaredTags::Unresolved {
                gaps.push(registry_gap(
                    GapKind::UnresolvedPath,
                    subject,
                    format!("category tags of {template} could not be followed"),
                ));
            }
            match family.condition {
                Condition::Always => {}
                Condition::Unresolved => gaps.push(registry_gap(
                    GapKind::UnresolvedPath,
                    subject,
                    format!("the method could not establish that every item generates {template}"),
                )),
                Condition::ItemRoot => gaps.push(registry_gap(
                    GapKind::UnresolvedPath,
                    subject,
                    format!(
                        "only an item's post-read code registers {template}; that the engine runs it for every item is not established"
                    ),
                )),
            }
            value.push(public);
        }
    }

    if unnamed_input {
        gaps.push(registry_gap(
            GapKind::UnnamedDeclaration,
            subject,
            "names that combine this registry's keys with the keys of content that Native does not name as a registry are not returned",
        ));
    }
    if let Some(summary) = unjoined_summary(unjoined) {
        gaps.push(registry_gap(
            GapKind::UnnamedDeclaration,
            None,
            format!("{summary}; some may generate this registry's modifiers"),
        ));
    }
    gaps.push(registry_gap(
        GapKind::OutsideMethod,
        subject,
        "The search covers the registry's database generator and post-read code. Where a modifier takes effect, the tags that a later registration of the same name gives, the tags of a declared modifier after content registers it again, and names longer than a fixed-size buffer keeps are outside it.",
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
    let category_tags = match family.mask.map(|mask| modifiers::tags(categories, mask)) {
        Some(Tags::Listed(tags)) => DeclaredTags::Listed(tags),
        Some(Tags::Unresolved(_)) | None => DeclaredTags::Unresolved,
    };
    let condition = match family.condition {
        Condition::Always => GenerationCondition::Always,
        Condition::Unresolved | Condition::ItemRoot => GenerationCondition::Unresolved,
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
    use crate::GapSubject;
    use crate::engine::analysis::evaluate::Unresolved;

    fn categories() -> CategoryNames {
        BTreeMap::from([
            (1, Ok(Some("Colony".into()))),
            (2, Ok(Some("Ships".into()))),
            (3, Ok(None)),
            (4, Ok(None)),
        ])
    }

    fn family(parts: Vec<Part>, mask: Option<u64>, condition: Condition) -> Family {
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

    fn answer(
        result: Option<&FamilyResult>,
        unnamed_input: bool,
        unjoined: &[(Reason, usize)],
    ) -> Answer<Vec<ModifierFamily>> {
        normalized_families(
            "common/x",
            result,
            unnamed_input,
            &unjoined.iter().copied().collect(),
            &categories(),
            build(),
        )
    }

    #[test]
    fn families_are_sorted_and_gaps_name_each_unresolved_part() {
        let result = FamilyResult {
            key_offset: Ok(0x10),
            families: vec![
                family(
                    vec![Part::ItemKey, Part::Literal("_b".into())],
                    Some(3),
                    Condition::Always,
                ),
                family(
                    vec![Part::Literal("a_".into()), Part::ItemKey],
                    Some(1),
                    Condition::Unresolved,
                ),
                family(
                    vec![Part::ItemKey, Part::Literal("_c".into())],
                    Some(4),
                    Condition::Always,
                ),
                family(
                    vec![Part::ItemKey, Part::Literal("_d".into())],
                    None,
                    Condition::ItemRoot,
                ),
            ],
            failures: BTreeMap::from([("name", 2)]),
        };
        let answer = answer(Some(&result), false, &[]);

        let templates: Vec<_> = answer.value.iter().map(template).collect();
        assert_eq!(templates, ["a_{key}", "{key}_b", "{key}_c", "{key}_d"]);
        assert_eq!(
            answer.value[1].category_tags,
            DeclaredTags::Listed(vec!["Colony".into(), "Ships".into()])
        );
        assert_eq!(answer.value[2].category_tags, DeclaredTags::Unresolved);
        assert_eq!(answer.value[3].category_tags, DeclaredTags::Unresolved);
        assert_eq!(answer.value[3].condition, GenerationCondition::Unresolved);
        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(answer.source.basis, Basis::StaticAnalysis);

        let details: Vec<_> = answer.gaps.iter().map(|gap| gap.detail.as_str()).collect();
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("every item generates a_{key}"))
        );
        assert!(details.iter().any(|detail| detail.starts_with("2 names")));
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("category tags of {key}_c"))
        );
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("only an item's post-read code registers {key}_d"))
        );
        assert!(
            answer
                .gaps
                .iter()
                .filter(|gap| gap.kind != GapKind::OutsideMethod)
                .all(|gap| gap.subject.as_ref().map(|subject| subject.name()) == Some("common/x"))
        );
    }

    #[test]
    fn unjoined_generation_keeps_every_answer_partial() {
        let complete = answer(None, false, &[]);
        assert!(complete.value.is_empty());
        assert_eq!(complete.completeness, Completeness::Complete);

        let partial = answer(
            None,
            false,
            &[(Reason::UnnamedContent, 2), (Reason::NoRoot, 1)],
        );
        assert_eq!(partial.completeness, Completeness::Partial);
        let gap = &partial.gaps[0];
        assert_eq!(gap.kind, GapKind::UnnamedDeclaration);
        assert!(gap.detail.starts_with("3 sites"), "{}", gap.detail);
        assert!(gap.detail.contains("2 reached from the post-read code"));
        assert!(gap.detail.contains("1 reached by no chain"));

        let matrix = answer(None, true, &[]);
        assert_eq!(matrix.completeness, Completeness::Partial);
        assert_eq!(
            matrix.gaps[0].subject,
            Some(GapSubject::registry("common/x"))
        );

        let missing_key = FamilyResult {
            key_offset: Err(Unresolved("key-storage")),
            families: Vec::new(),
            failures: BTreeMap::new(),
        };
        let answer = answer(Some(&missing_key), false, &[]);
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
