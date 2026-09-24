//! Static define inventory from the engine's read helpers.
use std::collections::{BTreeMap, BTreeSet};

use super::Native;
use super::questions::error;
use crate::answer::{
    Answer, Basis, Completeness, Define, DefineValueType, Error, Gap, GapKind, GapSubject,
    Operation, Source,
};
use crate::engine::analysis::defines::{self, SiteOutcome};

impl Native {
    /// Return the defines the engine reads, with the type requested by each read helper.
    ///
    /// The search covers compiled `NDefines` and `NUncheckedDefines` helpers in the
    /// executable. Defaults, bounds, documentation and shipped-file entries are outside it.
    pub fn defines(&self) -> Result<Answer<Vec<Define>>, Error> {
        self.answer("defines", None, || {
            let operation = Operation::Defines;
            let input = self
                .declaration_analysis(operation)?
                .defines_input()
                .map_err(|failure| error(operation, failure))?;
            Ok(normalize(defines::analyze(&input), self.build()))
        })
    }
}

fn normalize(outcomes: Vec<SiteOutcome>, build: crate::BuildId) -> Answer<Vec<Define>> {
    let mut names: BTreeMap<(String, String), BTreeSet<DefineValueType>> = BTreeMap::new();
    let mut gaps = Vec::new();
    for outcome in outcomes {
        match outcome {
            SiteOutcome::Resolved(define) => {
                names
                    .entry((define.namespace, define.name))
                    .or_default()
                    .insert(define.value_type);
            }
            SiteOutcome::Unresolved { subject, reason } => gaps.push(Gap {
                kind: GapKind::UnresolvedReader,
                subject: subject.map(GapSubject::answer_item),
                detail: reason.into(),
            }),
        }
    }
    let mut value = Vec::new();
    for ((namespace, name), types) in names {
        if types.len() != 1 {
            gaps.push(Gap {
                kind: GapKind::UnresolvedReader,
                subject: Some(GapSubject::answer_item(format!("{namespace}.{name}"))),
                detail: "the engine reads this define with conflicting value types".into(),
            });
            continue;
        }
        value.push(Define {
            namespace,
            name,
            value_type: *types.first().unwrap(),
        });
    }
    gaps.push(Gap {
        kind: GapKind::OutsideMethod,
        subject: None,
        detail: "The search covers compiled define read helpers; dynamic readers without a helper are outside it.".into(),
    });
    Answer {
        value,
        completeness: if gaps.iter().all(|gap| gap.kind == GapKind::OutsideMethod) {
            Completeness::Complete
        } else {
            Completeness::Partial
        },
        gaps,
        source: Source::new(build, defines::METHOD, Basis::StaticAnalysis),
    }
}
