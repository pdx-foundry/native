//! Static modifier node graph from node type symbols and node constructor calls.
use super::Native;
use super::language::gap;
use super::questions::error;
use crate::answer::{
    Answer, Basis, BuildId, Completeness, Error, Gap, GapKind, KeptCategories, ModifierNode,
    ModifierNodeId, ModifierNodeOwner, Operation, Source,
};
use crate::engine::analysis::modifier_nodes::{self, Masks, ModifierNodeResult};
use crate::engine::analysis::modifiers::{CategoryNames, Tags, tags};

/// Why the answer holds no supported scopes.
const TAKES_EFFECT: &str = "Where a modifier takes effect is decided at application by each receiver's category mask and the include and exclude masks of each propagation edge. The engine reports no diagnostic for a filtered entry. These masks are not established; supported scopes stay outside this method.";

/// The mask that keeps every category. A node with it says nothing about one category.
const EVERY_CATEGORY: u64 = 0xffff_ffff;

impl Native {
    /// Read the engine's modifier node graph: each node's source nodes, the engine types that
    /// construct it, and the categories that it keeps.
    ///
    /// A node keeps a modifier entry only when the entry's categories meet the node's mask. That
    /// is one filter among others: where a modifier takes effect is a gap, and the answer holds no
    /// supported scopes. A category that no node keeps, apart from nodes that keep every category,
    /// is a gap.
    pub fn modifier_nodes(&self) -> Result<Answer<Vec<ModifierNode>>, Error> {
        self.answer("modifier_nodes", None, || {
            let operation = Operation::ModifierNodes;
            let input = self
                .declaration_analysis(operation)?
                .modifier_node_input()
                .map_err(|failure| error(operation, failure))?;
            Ok(normalize(&modifier_nodes::analyze(&input), self.build()))
        })
    }
}

fn normalize(result: &ModifierNodeResult, build: BuildId) -> Answer<Vec<ModifierNode>> {
    let mut gaps = vec![gap(GapKind::OutsideMethod, None, TAKES_EFFECT)];
    let mut nodes = Vec::new();
    let mut kept = 0;
    let mut unresolved_mask = false;

    for (&id, read) in &result.nodes {
        let subject = format!("modifier node {id}");
        let source_nodes = match &read.sources {
            Ok(sources) => sources.iter().copied().map(ModifierNodeId).collect(),
            Err(unresolved) => {
                let detail = format!(
                    "the node type symbols do not establish the node's sources: {}",
                    unresolved.reason
                );
                gaps.push(gap(GapKind::UnreadableInput, Some(&subject), detail));
                Vec::new()
            }
        };

        if read.owners.is_empty() {
            unresolved_mask = true;
            let detail = "the method followed no constructor call of the node, so its owner and mask are unresolved";
            gaps.push(gap(GapKind::UnresolvedPath, Some(&subject), detail));
        }

        let mut owners = Vec::new();
        for owner in &read.owners {
            let masks = match &owner.masks {
                Ok(Masks::Constant(mask)) => vec![*mask],
                Ok(Masks::Recalculated(masks)) => masks.iter().copied().collect(),
                Err(unresolved) => {
                    unresolved_mask = true;
                    let detail = format!(
                        "the node's category mask is unresolved: {}",
                        unresolved.reason
                    );
                    gaps.push(gap(GapKind::UnresolvedPath, Some(&subject), detail));
                    Vec::new()
                }
            };
            for mask in masks.into_iter().filter(|&mask| mask != EVERY_CATEGORY) {
                kept |= mask;
            }

            let kept_categories = match kept_categories(&result.categories, &owner.masks) {
                Ok(categories) => categories,
                Err(reason) => {
                    let detail = format!("the node's categories could not be named: {reason}");
                    gaps.push(gap(GapKind::UnresolvedPath, Some(&subject), detail));
                    KeptCategories::Unresolved
                }
            };
            if matches!(kept_categories, KeptCategories::Recalculated(_)) {
                let detail = "the node's calculation chooses one of these masks at run time";
                gaps.push(gap(GapKind::OutsideMethod, Some(&subject), detail));
            }

            let entry = ModifierNodeOwner {
                owner: owner.owner.clone(),
                kept_categories,
            };
            if !owners.contains(&entry) {
                owners.push(entry);
            }
        }

        nodes.push(ModifierNode {
            id: ModifierNodeId(id),
            source_nodes,
            owners,
        });
    }

    gaps.extend(categories_without_node(
        &result.categories,
        kept,
        unresolved_mask,
    ));
    gaps.dedup();

    Answer {
        value: nodes,
        completeness: Completeness::from_gaps(&gaps),
        gaps,
        source: Source::new(build, modifier_nodes::METHOD, Basis::StaticAnalysis),
    }
}

/// The named categories of each mask, or the reason that one could not be named. An unresolved
/// mask is `Unresolved`, since its gap is already given.
fn kept_categories(
    categories: &CategoryNames,
    masks: &Result<Masks, crate::engine::analysis::stop::Unresolved>,
) -> Result<KeptCategories, &'static str> {
    let names = |mask: u64| match tags(categories, mask) {
        Tags::Listed(names) => Ok(names),
        Tags::Unresolved(unresolved) => Err(unresolved.reason),
    };

    match masks {
        Ok(Masks::Constant(mask)) => Ok(KeptCategories::Constant(names(*mask)?)),
        Ok(Masks::Recalculated(masks)) => {
            let names = masks
                .iter()
                .map(|&mask| names(mask))
                .collect::<Result<_, _>>()?;
            Ok(KeptCategories::Recalculated(names))
        }
        Err(_) => Ok(KeptCategories::Unresolved),
    }
}

/// One gap for each named single category that no resolved mask keeps, apart from masks that keep
/// every category. While a mask is unresolved, that mask may keep it, so the gap says so.
fn categories_without_node(
    categories: &CategoryNames,
    kept: u64,
    unresolved_mask: bool,
) -> Vec<Gap> {
    let (kind, detail) = if unresolved_mask {
        (
            GapKind::UnresolvedPath,
            "no resolved modifier node mask keeps this category, apart from masks that keep every category; a node whose mask is unresolved may keep it",
        )
    } else {
        (
            GapKind::OutsideMethod,
            "no modifier node keeps this category, apart from nodes that keep every category",
        )
    };

    (0..32)
        .map(|bit| 1u64 << bit)
        .filter(|bit| kept & bit == 0)
        .filter_map(|bit| match categories.get(&bit) {
            Some(Ok(Some(name))) => Some(gap(kind, Some(name), detail)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::engine::analysis::modifier_nodes::{NodeRead, OwnerRead};
    use crate::engine::analysis::stop::Unresolved;

    /// Node 1 keeps Pops, node 2 keeps every category, and node 3's mask is `masks`.
    fn result(masks: Result<Masks, Unresolved>) -> ModifierNodeResult {
        let node = |owner: &str, masks| NodeRead {
            sources: Ok(vec![]),
            owners: vec![OwnerRead {
                owner: owner.into(),
                masks,
            }],
        };

        ModifierNodeResult {
            nodes: BTreeMap::from([
                (1, node("CPlanet", Ok(Masks::Constant(0x2)))),
                (2, node("CLeader", Ok(Masks::Constant(EVERY_CATEGORY)))),
                (3, node("CShip", masks)),
            ]),
            categories: BTreeMap::from([
                (0x1, Ok(None)),
                (0x2, Ok(Some("Pops".into()))),
                (0x4, Ok(Some("Armies".into()))),
                (0x8, Ok(Some("Fleets".into()))),
                (0xc, Ok(None)),
                (EVERY_CATEGORY, Ok(Some("All".into()))),
            ]),
        }
    }

    fn category_gaps(answer: &Answer<Vec<ModifierNode>>) -> Vec<(GapKind, &str)> {
        answer
            .gaps
            .iter()
            .filter(|gap| gap.detail.contains("keeps this category"))
            .map(|gap| (gap.kind, gap.subject.as_ref().unwrap().name()))
            .collect()
    }

    #[test]
    fn a_category_that_only_every_category_masks_keep_is_a_gap() {
        let answer = normalize(&result(Ok(Masks::Constant(0x8))), BuildId("build".into()));

        assert_eq!(answer.completeness, Completeness::Complete);
        assert_eq!(category_gaps(&answer), [(GapKind::OutsideMethod, "Armies")]);
        assert_eq!(answer.gaps[0].detail, TAKES_EFFECT);
        assert_eq!(
            answer.value[1].owners[0].kept_categories,
            KeptCategories::Constant(vec!["All".into()])
        );
    }

    #[test]
    fn an_unresolved_mask_leaves_each_category_without_a_resolved_node_unresolved() {
        let answer = normalize(
            &result(Err(Unresolved::new("path-limit"))),
            BuildId("build".into()),
        );

        assert_eq!(answer.completeness, Completeness::Partial);
        assert_eq!(
            category_gaps(&answer),
            [
                (GapKind::UnresolvedPath, "Armies"),
                (GapKind::UnresolvedPath, "Fleets")
            ]
        );
        assert_eq!(
            answer.value[2].owners[0].kept_categories,
            KeptCategories::Unresolved
        );
    }

    #[test]
    fn a_recalculated_node_lists_each_mask_and_states_the_run_time_choice() {
        let masks = Masks::Recalculated(BTreeSet::from([0x4, 0x8, 0xc]));
        let answer = normalize(&result(Ok(masks)), BuildId("build".into()));

        assert_eq!(
            answer.value[2].owners[0].kept_categories,
            KeptCategories::Recalculated(vec![
                vec!["Armies".into()],
                vec!["Fleets".into()],
                vec!["Armies".into(), "Fleets".into()]
            ])
        );
        assert!(
            answer
                .gaps
                .iter()
                .any(|gap| gap.kind == GapKind::OutsideMethod
                    && gap.subject.as_ref().map(|subject| subject.name())
                        == Some("modifier node 3"))
        );
        assert!(category_gaps(&answer).is_empty());
    }
}
