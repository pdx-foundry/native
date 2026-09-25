//! Static language questions: modifiers, modifier categories, scopes and scope links.
use std::collections::{BTreeMap, BTreeSet};

use super::Native;
use super::questions::{error, scope_id, scope_references};
use crate::answer::{
    Answer, Basis, BuildId, Completeness, DeclaredScopes, DeclaredTags, Error, Gap, GapKind,
    GapSubject, LinkData, ModifierCategory, ModifierDeclaration, Operation, OutputScope,
    ScopeDeclaration, ScopeGroup, ScopeInventory, ScopeLink, Source,
};
use crate::engine::analysis::{
    declarations::ScopeOutcome,
    modifiers::{self, CATEGORY_METHOD, DefinitionSite, MODIFIER_METHOD, ModifierResult, Tags},
    scopes::{self, LINK_METHOD, LinkResult, Output, SCOPE_METHOD, ScopeResult},
    stop::Unresolved,
};

impl Native {
    /// Read the built-in modifiers from direct definition calls in executable text.
    ///
    /// Repeated names share one declaration; unresolved or conflicting category tags remain
    /// unresolved. Modifiers that content generates at run time, such as
    /// one per resource or job, are not listed: each call site that generates them is an
    /// [`GapKind::UnnamedDeclaration`] gap. [`Native::modifier_families`] gives the name templates
    /// of a registry's generated modifiers. Category tags are intended-use tags; where a modifier
    /// takes effect is outside this method.
    pub fn modifiers(&self) -> Result<Answer<Vec<ModifierDeclaration>>, Error> {
        self.answer("modifiers", None, || {
            let result = self.modifier_result(Operation::Modifiers)?;
            Ok(normalized_modifiers(&result, self.build()))
        })
    }

    /// Read the modifier category names from the engine's category-name switch.
    ///
    /// The list holds the name of each single category and of each whole category mask that a
    /// built-in modifier uses. A category is an intended-use tag, not an application context.
    pub fn modifier_categories(&self) -> Result<Answer<Vec<ModifierCategory>>, Error> {
        self.answer("modifier_categories", None, || {
            let result = self.modifier_result(Operation::ModifierCategories)?;
            Ok(normalized_categories(&result, self.build()))
        })
    }

    /// Read the scope types and the script keywords that the engine resolves to each.
    ///
    /// Keywords are grouped only by the engine's own keyword-to-scope map. A keyword that matches
    /// several types, such as `carrier`, is a [`ScopeGroup`]. Keywords created at run time are
    /// outside this method.
    pub fn scopes(&self) -> Result<Answer<ScopeInventory>, Error> {
        self.answer("scopes", None, || {
            let operation = Operation::Scopes;
            let input = self
                .declaration_analysis(operation)?
                .scope_input()
                .map_err(|failure| error(operation, failure))?;
            let result =
                scopes::scopes(&input).map_err(|error| Error::Method(error.to_string()))?;
            Ok(normalized_scopes(&result, self.build()))
        })
    }

    /// Read the scope links that the engine documents and the link prefixes that take data, such
    /// as `event_target:`, each with its declared input and output scopes.
    ///
    /// Declared scopes are what the engine states. Which saved target or parameter a value names,
    /// and whether a link gives a useful target in a running game, are outside this method.
    pub fn scope_links(&self) -> Result<Answer<Vec<ScopeLink>>, Error> {
        self.answer("scope_links", None, || {
            let operation = Operation::ScopeLinks;
            let input = self
                .declaration_analysis(operation)?
                .scope_input()
                .map_err(|failure| error(operation, failure))?;
            let result = scopes::links(&input).map_err(|error| Error::Method(error.to_string()))?;
            Ok(normalized_links(&result, self.build()))
        })
    }

    fn modifier_result(&self, operation: Operation) -> Result<ModifierResult, Error> {
        let input = self
            .declaration_analysis(operation)?
            .modifier_input()
            .map_err(|failure| error(operation, failure))?;
        modifiers::analyze(&input).map_err(|error| Error::Method(error.to_string()))
    }
}

pub(super) fn gap(kind: GapKind, subject: Option<&str>, detail: impl Into<String>) -> Gap {
    gap_for_subject(kind, subject.map(GapSubject::answer_item), detail)
}

pub(super) fn registry_gap(
    kind: GapKind,
    registry: Option<&str>,
    detail: impl Into<String>,
) -> Gap {
    gap_for_subject(kind, registry.map(GapSubject::registry), detail)
}

pub(super) fn gap_for_subject(
    kind: GapKind,
    subject: Option<GapSubject>,
    detail: impl Into<String>,
) -> Gap {
    Gap {
        kind,
        subject,
        detail: detail.into(),
    }
}

/// Sort the value by name and derive completeness from the gaps.
fn answer<T>(
    mut value: Vec<T>,
    gaps: Vec<Gap>,
    name: impl Fn(&T) -> &str,
    build: BuildId,
    method: &str,
) -> Answer<Vec<T>> {
    value.sort_by(|left, right| name(left).cmp(name(right)));
    declared(value, gaps, build, method)
}

/// A declared answer, complete exactly when every gap is outside the method.
pub(super) fn declared<T>(value: T, gaps: Vec<Gap>, build: BuildId, method: &str) -> Answer<T> {
    let completeness = if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
        Completeness::Complete
    } else {
        Completeness::Partial
    };
    Answer {
        value,
        completeness,
        gaps,
        source: Source::new(build, method, Basis::Declared),
    }
}

pub(crate) fn normalized_modifiers(
    result: &ModifierResult,
    build: BuildId,
) -> Answer<Vec<ModifierDeclaration>> {
    let mut declarations = BTreeMap::new();
    let mut gaps = Vec::new();

    for site in &result.sites {
        match site {
            DefinitionSite::Declared { name, tags } => {
                let category_tags = match tags {
                    Tags::Listed(tags) => DeclaredTags::Listed(tags.clone()),
                    Tags::Unresolved(Unresolved { reason, .. }) => {
                        gaps.push(gap(
                            GapKind::UnresolvedPath,
                            Some(name),
                            format!("category tags not followed at {reason}"),
                        ));
                        DeclaredTags::Unresolved
                    }
                };
                match declarations.entry(name.clone()) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(category_tags);
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        match (entry.get(), &category_tags) {
                            (DeclaredTags::Listed(previous), DeclaredTags::Listed(current)) => {
                                if previous != current {
                                    gaps.push(gap(
                                        GapKind::UnresolvedPath,
                                        Some(name),
                                        "modifier registrations declare conflicting category tags",
                                    ));
                                    entry.insert(DeclaredTags::Unresolved);
                                }
                            }
                            _ => {
                                entry.insert(DeclaredTags::Unresolved);
                            }
                        }
                    }
                }
            }
            DefinitionSite::RuntimeToken => gaps.push(gap(
                GapKind::UnnamedDeclaration,
                None,
                "a direct modifier definition passes a name that is composed at run time",
            )),
            DefinitionSite::Unreadable => gaps.push(gap(
                GapKind::UnresolvedPath,
                None,
                "a direct modifier definition call could not be followed to its arguments",
            )),
        }
    }

    for _ in 0..result.generation_sites {
        gaps.push(gap(
            GapKind::UnnamedDeclaration,
            None,
            "a generated modifier family composes its names at run time from content",
        ));
    }
    gaps.push(gap(
        GapKind::OutsideMethod,
        None,
        "The search covers every direct modifier definition call in executable text. Generated modifier families, whose templates modifier_families gives for each registry, and where a modifier takes effect are outside it.",
    ));

    let value = declarations
        .into_iter()
        .map(|(name, category_tags)| ModifierDeclaration {
            name,
            category_tags,
        })
        .collect();
    answer(
        value,
        gaps,
        |modifier| &modifier.name,
        build,
        MODIFIER_METHOD,
    )
}

pub(crate) fn normalized_categories(
    result: &ModifierResult,
    build: BuildId,
) -> Answer<Vec<ModifierCategory>> {
    let mut names = BTreeSet::new();
    let mut unresolved = 0;
    for name in result.categories.values() {
        match name {
            Ok(Some(name)) => {
                names.insert(name.clone());
            }
            Ok(None) => {}
            Err(_) => unresolved += 1,
        }
    }

    let mut gaps = Vec::new();
    if unresolved > 0 {
        gaps.push(gap(
            GapKind::UnresolvedPath,
            None,
            format!("{unresolved} category masks could not be followed to a name"),
        ));
    }
    gaps.push(gap(
        GapKind::OutsideMethod,
        None,
        "The search covers each single category and each whole mask that a built-in modifier uses. Categories are intended-use tags; where a modifier takes effect is outside it.",
    ));

    let value = names
        .into_iter()
        .map(|name| ModifierCategory { name })
        .collect();
    answer(
        value,
        gaps,
        |category| &category.name,
        build,
        CATEGORY_METHOD,
    )
}

pub(crate) fn normalized_scopes(result: &ScopeResult, build: BuildId) -> Answer<ScopeInventory> {
    let mut types: Vec<_> = result
        .scopes
        .iter()
        .map(|(scope, keywords)| {
            let mut keywords = keywords.clone();
            keywords.sort();
            ScopeDeclaration {
                id: scope_id(scope),
                name: scope.name.clone(),
                keywords,
            }
        })
        .collect();
    types.sort_by(|left, right| left.name.cmp(&right.name));

    let mut gaps = Vec::new();
    if result.table_missing {
        gaps.push(gap(
            GapKind::UnreadableInput,
            None,
            "scope name table not found",
        ));
    }
    let mut groups = Vec::new();
    for (keyword, scopes) in &result.groups {
        match scopes {
            ScopeOutcome::Listed(scopes) => groups.push(ScopeGroup {
                keyword: keyword.clone(),
                scopes: scope_references(scopes),
            }),
            ScopeOutcome::Any | ScopeOutcome::Unresolved(_) => gaps.push(gap(
                GapKind::UnresolvedPath,
                Some(keyword),
                "the keyword matches several scope types that could not all be named",
            )),
        }
    }
    groups.sort_by(|left, right| left.keyword.cmp(&right.keyword));
    if result.unnamed_types > 0 {
        gaps.push(gap(
            GapKind::UnresolvedPath,
            None,
            format!(
                "{} scope types that keywords resolve to have no name in the scope name table",
                result.unnamed_types
            ),
        ));
    }
    if result.unnamed_keywords > 0 {
        gaps.push(gap(
            GapKind::UnnamedDeclaration,
            None,
            format!(
                "{} scope keywords have no literal name",
                result.unnamed_keywords
            ),
        ));
    }
    if result.unresolved_tokens > 0 {
        gaps.push(gap(
            GapKind::UnresolvedPath,
            None,
            format!(
                "{} tokens could not be followed through the keyword-to-scope map",
                result.unresolved_tokens
            ),
        ));
    }
    gaps.push(gap(
        GapKind::OutsideMethod,
        None,
        "The search covers every literal token. Keywords created at run time are outside it.",
    ));

    declared(ScopeInventory { types, groups }, gaps, build, SCOPE_METHOD)
}

pub(crate) fn normalized_links(result: &LinkResult, build: BuildId) -> Answer<Vec<ScopeLink>> {
    let mut value = Vec::new();
    let mut gaps = Vec::new();

    for link in &result.links {
        let input_scopes = match &link.input {
            ScopeOutcome::Any => DeclaredScopes::Any,
            ScopeOutcome::Listed(types) => DeclaredScopes::Listed(scope_references(types)),
            ScopeOutcome::Unresolved(Unresolved { reason, .. }) => {
                if *reason != "scope-table" {
                    gaps.push(gap(
                        GapKind::UnresolvedPath,
                        Some(&link.name),
                        format!("input scopes not followed at {reason}"),
                    ));
                }
                DeclaredScopes::Unresolved
            }
        };
        let output_scope = match &link.output {
            Output::Listed(types) => OutputScope::Listed(scope_references(types)),
            Output::Various => OutputScope::Various,
            Output::Unresolved(Unresolved { reason, .. }) => {
                if *reason != "scope-table" {
                    gaps.push(gap(
                        GapKind::UnresolvedPath,
                        Some(&link.name),
                        format!("output scope not followed at {reason}"),
                    ));
                }
                OutputScope::Unresolved
            }
        };
        value.push(ScopeLink {
            name: link.name.clone(),
            input_scopes,
            output_scope,
            data: link.prefix.clone().map_or(LinkData::None, LinkData::Prefix),
        });
    }

    if result.table_missing {
        gaps.push(gap(
            GapKind::UnreadableInput,
            None,
            "scope name table not found",
        ));
    }
    for _ in 0..result.unnamed_links {
        gaps.push(gap(
            GapKind::UnnamedDeclaration,
            None,
            "a documented link has no literal name",
        ));
    }
    if result.unresolved_tokens > 0 {
        gaps.push(gap(
            GapKind::UnresolvedPath,
            None,
            format!(
                "{} tokens could not be followed through the link documentation",
                result.unresolved_tokens
            ),
        ));
    }
    gaps.push(gap(
        GapKind::OutsideMethod,
        None,
        "The search covers every literal token that the engine's link documentation prints, and the data prefixes of its special-value parser. The parser's other forms are not links: `.` joins links into a chain, a trailing `?` sets a run-time option, and `@` in an event_target value names a dynamic flag. Which target or parameter a value names, value prefixes such as `value:`, and actual scope availability are outside it.",
    ));

    answer(value, gaps, |link| &link.name, build, LINK_METHOD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::declarations::ScopeType;

    fn modifier_registrations(tags: Vec<Tags>) -> Answer<Vec<ModifierDeclaration>> {
        let result = ModifierResult {
            sites: tags
                .into_iter()
                .map(|tags| DefinitionSite::Declared {
                    name: "example_modifier".into(),
                    tags,
                })
                .collect(),
            categories: BTreeMap::new(),
            generation_sites: 0,
            type_masks: BTreeMap::new(),
        };
        normalized_modifiers(&result, BuildId("test".into()))
    }

    #[test]
    fn equal_modifier_registrations_keep_one_established_declaration() {
        let tags = Tags::Listed(vec!["country".into()]);
        let answer = modifier_registrations(vec![tags.clone(), tags]);
        assert_eq!(answer.completeness, Completeness::Complete);
        assert_eq!(answer.value.len(), 1);
        assert_eq!(
            answer.value[0].category_tags,
            DeclaredTags::Listed(vec!["country".into()])
        );
        assert!(
            answer
                .gaps
                .iter()
                .all(|gap| gap.kind == GapKind::OutsideMethod)
        );
    }

    #[test]
    fn conflicting_modifier_registrations_remain_unresolved() {
        for tags in [
            vec!["country", "planet", "country"],
            vec!["planet", "country", "planet"],
        ] {
            let answer = modifier_registrations(
                tags.into_iter()
                    .map(|tag| Tags::Listed(vec![tag.into()]))
                    .collect(),
            );
            assert_eq!(answer.completeness, Completeness::Partial);
            assert_eq!(answer.value.len(), 1);
            assert_eq!(answer.value[0].category_tags, DeclaredTags::Unresolved);
            assert!(answer.gaps.iter().any(|gap| {
                gap.kind == GapKind::UnresolvedPath
                    && gap.subject == Some(GapSubject::answer_item("example_modifier"))
                    && gap.detail == "modifier registrations declare conflicting category tags"
            }));
        }
    }

    #[test]
    fn unresolved_modifier_registration_is_kept_in_either_order() {
        let known = Tags::Listed(vec!["country".into()]);
        let unknown = Tags::Unresolved(Unresolved::new("unreadable category mask"));
        let first = modifier_registrations(vec![known.clone(), unknown.clone()]);
        let second = modifier_registrations(vec![unknown, known]);
        assert_eq!(first, second);
        assert_eq!(first.completeness, Completeness::Partial);
        assert_eq!(first.value.len(), 1);
        assert_eq!(first.value[0].category_tags, DeclaredTags::Unresolved);
        assert!(first.gaps.iter().any(|gap| {
            gap.kind == GapKind::UnresolvedPath
                && gap.subject == Some(GapSubject::answer_item("example_modifier"))
                && gap.detail == "category tags not followed at unreadable category mask"
        }));
    }

    fn scope(bit: usize, name: &str) -> ScopeType {
        ScopeType {
            bit,
            name: name.into(),
        }
    }

    #[test]
    fn scope_types_keep_their_identity_when_names_repeat() {
        let result = ScopeResult {
            scopes: vec![
                (scope(1, "planet"), vec!["planet".into()]),
                (scope(2, "country"), vec!["country".into()]),
                (scope(3, "ship"), vec!["ship".into()]),
                (scope(19, "country"), vec!["observer".into()]),
            ],
            groups: vec![
                (
                    "carrier".into(),
                    ScopeOutcome::Listed(vec![scope(1, "planet"), scope(3, "ship")]),
                ),
                (
                    "unnamed".into(),
                    ScopeOutcome::Unresolved(Unresolved::new("scope-name")),
                ),
            ],
            unnamed_types: 0,
            unnamed_keywords: 0,
            unresolved_tokens: 0,
            table_missing: false,
        };
        let answer = normalized_scopes(&result, BuildId("test".into()));
        let types = &answer.value.types;

        let names: Vec<_> = types.iter().map(|scope| scope.name.as_str()).collect();
        assert_eq!(names, ["country", "country", "planet", "ship"]);
        assert_ne!(types[0].id, types[1].id);
        assert_eq!(types[1].keywords, ["observer"]);

        let group = &answer.value.groups[0];
        assert_eq!(group.keyword, "carrier");
        let joined: Vec<_> = group
            .scopes
            .iter()
            .map(|reference| {
                types
                    .iter()
                    .find(|scope| scope.id == reference.id)
                    .map(|scope| scope.name.as_str())
            })
            .collect();
        assert_eq!(joined, [Some("planet"), Some("ship")]);

        assert_eq!(answer.completeness, Completeness::Partial);
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::UnresolvedPath
                    && gap.subject.as_ref().map(|subject| subject.name()) == Some("unnamed"))
        );
    }
}
