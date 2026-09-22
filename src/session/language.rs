//! Static language questions: modifiers, modifier categories, scopes and scope links.
use std::collections::BTreeSet;

use super::Native;
use super::questions::error;
use crate::answer::{
    Answer, Basis, BuildId, Completeness, DeclaredScopes, DeclaredTags, Error, Gap, GapKind,
    LinkData, ModifierCategory, ModifierDeclaration, Operation, OutputScope, ScopeDeclaration,
    ScopeLink, Source,
};
use crate::engine::analysis::{
    declarations::ScopeOutcome,
    modifiers::{self, CATEGORY_METHOD, DefinitionSite, MODIFIER_METHOD, ModifierResult, Tags},
    scopes::{self, LINK_METHOD, LinkResult, Output, SCOPE_METHOD, ScopeResult},
};

impl Native {
    /// Read the built-in modifiers from direct definition calls in executable text.
    ///
    /// Each direct call is one modifier. Modifiers that content generates at run time, such as
    /// one per resource or job, are not listed: each call site that generates them is an
    /// [`GapKind::UnnamedDeclaration`] gap. Category tags are intended-use tags; where a modifier
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
    /// Keywords are grouped only by the engine's own keyword-to-scope map. Keywords created at
    /// run time are outside this method.
    pub fn scopes(&self) -> Result<Answer<Vec<ScopeDeclaration>>, Error> {
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

    /// Read the scope links that the engine documents, with their declared input and output
    /// scopes, and the link prefixes that take data.
    ///
    /// Declared scopes are what the engine states. Whether a link gives a useful target in a
    /// running game, and variable or parameter syntax, are outside this method.
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

fn gap(kind: GapKind, subject: Option<&str>, detail: impl Into<String>) -> Gap {
    Gap {
        kind,
        subject: subject.map(str::to_owned),
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
    let mut value = Vec::new();
    let mut names = BTreeSet::new();
    let mut gaps = Vec::new();

    for site in &result.sites {
        match site {
            DefinitionSite::Declared { name, tags } => {
                if !names.insert(name.clone()) {
                    continue;
                }
                let category_tags = match tags {
                    Tags::Listed(tags) => DeclaredTags::Listed(tags.clone()),
                    Tags::Unresolved(reason) => {
                        gaps.push(gap(
                            GapKind::UnresolvedPath,
                            Some(name),
                            format!("category tags not followed at {reason}"),
                        ));
                        DeclaredTags::Unresolved
                    }
                };
                value.push(ModifierDeclaration {
                    name: name.clone(),
                    category_tags,
                });
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
        "The search covers every direct modifier definition call in executable text. Generated modifier families and where a modifier takes effect are outside it.",
    ));

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

pub(crate) fn normalized_scopes(
    result: &ScopeResult,
    build: BuildId,
) -> Answer<Vec<ScopeDeclaration>> {
    let value = result
        .scopes
        .iter()
        .map(|(name, keywords)| {
            let mut keywords = keywords.clone();
            keywords.sort();
            ScopeDeclaration {
                name: name.clone(),
                keywords,
            }
        })
        .collect();

    let mut gaps = Vec::new();
    if result.table_missing {
        gaps.push(gap(
            GapKind::UnreadableInput,
            None,
            "scope name table not found",
        ));
    }
    for (keyword, types) in &result.combined {
        let types = match types {
            ScopeOutcome::Listed(names) => names.join(", "),
            ScopeOutcome::Any | ScopeOutcome::Unresolved(_) => "several types".into(),
        };
        gaps.push(gap(
            GapKind::OutsideMethod,
            Some(keyword),
            format!("the keyword resolves to more than one scope type ({types}), so it names no single scope"),
        ));
    }
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

    answer(value, gaps, |scope| &scope.name, build, SCOPE_METHOD)
}

pub(crate) fn normalized_links(result: &LinkResult, build: BuildId) -> Answer<Vec<ScopeLink>> {
    let mut value = Vec::new();
    let mut gaps = Vec::new();

    for link in &result.links {
        let input_scopes = match &link.input {
            ScopeOutcome::Any => DeclaredScopes::Any,
            ScopeOutcome::Listed(names) => DeclaredScopes::Listed(names.clone()),
            ScopeOutcome::Unresolved(reason) => {
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
            Output::Listed(names) => OutputScope::Listed(names.clone()),
            Output::Various => OutputScope::Various,
            Output::Unresolved(reason) => {
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
            data: LinkData::None,
        });
    }

    for prefix in &result.prefixes {
        let name = prefix.trim_end_matches(':');
        gaps.push(gap(
            GapKind::UnresolvedPath,
            Some(name),
            "the scopes of a link that takes data are not followed by this method",
        ));
        value.push(ScopeLink {
            name: name.to_owned(),
            input_scopes: DeclaredScopes::Unresolved,
            output_scope: OutputScope::Unresolved,
            data: LinkData::Prefix(prefix.clone()),
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
        "The search covers every literal token that the engine's link documentation prints, and the data prefixes of its special-value parser. Variables, other dynamic target syntax, and actual scope availability are outside it.",
    ));

    answer(value, gaps, |link| &link.name, build, LINK_METHOD)
}
