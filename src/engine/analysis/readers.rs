//! Conservative classification of already-joined shared readers.
use crate::answer::{BlockFamily, ReaderKind};
use crate::engine::analysis::fields::ReaderJoin;
use std::collections::BTreeSet;

/// One callee shared by every path, with its broad value kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification<'a> {
    /// Exact joined callee, only when every path agrees.
    pub callee: Option<&'a str>,
    /// Conservative broad value form.
    pub kind: ReaderKind,
    /// Established block family, separately from the reader identity.
    pub family: BlockFamily,
}

/// Classify a reader only after every alternative joins the same callee.
pub fn classify(readers: &[ReaderJoin]) -> Classification<'_> {
    let callees: BTreeSet<_> = readers
        .iter()
        .filter_map(|reader| match reader {
            ReaderJoin::Joined { callee, .. } => Some(callee.as_str()),
            ReaderJoin::Missing(_) => None,
        })
        .collect();
    let all_joined = !readers.is_empty()
        && readers
            .iter()
            .all(|reader| matches!(reader, ReaderJoin::Joined { .. }));
    let callee = (all_joined && callees.len() == 1).then(|| *callees.first().unwrap());
    let kind = callee.map_or(ReaderKind::Unknown, classify_callee);
    let families: Vec<_> = callees
        .iter()
        .map(|callee| family_of_callee(callee))
        .collect();
    let family = families.first().copied().unwrap_or(BlockFamily::Unknown);
    Classification {
        callee,
        kind,
        family: if all_joined && families.iter().all(|candidate| *candidate == family) {
            family
        } else {
            BlockFamily::Unknown
        },
    }
}

fn family_of_callee(callee: &str) -> BlockFamily {
    if matching_template(callee, "ReadTrigger") || callee == "CTrigger::Read(CReader&, EScopeType)"
    {
        BlockFamily::Trigger
    } else if matching_template(callee, "ReadEffect")
        || callee == "CEffect::Read(CReader&, EScopeType)"
    {
        BlockFamily::Effect
    } else if matches!(
        classify_callee(callee),
        ReaderKind::Block | ReaderKind::Unknown
    ) {
        BlockFamily::Unknown
    } else {
        BlockFamily::NotApplicable
    }
}

fn classify_callee(callee: &str) -> ReaderKind {
    match callee {
        "CReader::Read(bool&)" => ReaderKind::Boolean,
        "CReader::Read(signed char&)"
        | "CReader::Read(unsigned char&)"
        | "CReader::Read(short&)"
        | "CReader::Read(unsigned short&)"
        | "CReader::Read(int&)"
        | "CReader::Read(unsigned int&)"
        | "CReader::Read(long long&)"
        | "CReader::Read(unsigned long long&)" => ReaderKind::Integer,
        "CReader::Read(CFixedPoint&)"
        | "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)" => {
            ReaderKind::FixedPoint
        }
        "CReader::Read(CString&, bool)" => ReaderKind::String,
        "CReader::Read(CPersistent&)"
        | "CPersistent::Read(CReader&)"
        | "CTrigger::Read(CReader&, EScopeType)"
        | "CEffect::Read(CReader&, EScopeType)" => ReaderKind::Block,
        _ if matching_template(callee, "ReadTrigger")
            || matching_template(callee, "ReadEffect") =>
        {
            ReaderKind::Block
        }
        _ if matching_deferred_reference(callee) => ReaderKind::Reference,
        _ => ReaderKind::Unknown,
    }
}

fn matching_template(callee: &str, method: &str) -> bool {
    let prefix = format!("void NParserUtil::{method}<");
    let Some(rest) = callee.strip_prefix(&prefix) else {
        return false;
    };
    let Some((parameter, arguments)) = rest.split_once('>') else {
        return false;
    };
    is_simple_template_argument(parameter)
        && arguments == format!("(CReader&, {parameter}&, EScopeType)")
}

fn matching_deferred_reference(callee: &str) -> bool {
    let Some(rest) = callee.strip_prefix("void NParserUtil::ReadKeyReferenceDeferred<") else {
        return false;
    };
    let Some((database, arguments)) = rest.split_once('>') else {
        return false;
    };
    is_simple_template_argument(database)
        && arguments
            == format!(
                "(CGlobalDeferredDatabaseObject const&, CReader&, {database}::ValueType const**)"
            )
}

/// A nonempty template argument of ASCII letters, digits and `_`.
fn is_simple_template_argument(argument: &str) -> bool {
    !argument.is_empty()
        && argument
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Registers that affect a supported reader call, excluding caller scratch state.
pub(crate) fn call_arguments(callee: &str) -> Option<&'static [&'static str]> {
    if callee == "CReader::Read(CString&, bool)"
        || matches!(
            family_of_callee(callee),
            BlockFamily::Trigger | BlockFamily::Effect
        )
        || matching_deferred_reference(callee)
    {
        Some(&["x0", "x1", "x2"])
    } else if classify_callee(callee) != ReaderKind::Unknown {
        Some(&["x0", "x1"])
    } else {
        None
    }
}

/// The owner-derived output object of a supported shared reader.
pub(crate) fn destination(join: &ReaderJoin) -> Option<i64> {
    let ReaderJoin::Joined {
        callee, arguments, ..
    } = join
    else {
        return None;
    };
    let destination = if matching_deferred_reference(callee) {
        "x2"
    } else if matches!(
        callee.as_str(),
        "CVariableValue::Read(CReader&, EScopeType)"
            | "CTrigger::Read(CReader&, EScopeType)"
            | "CEffect::Read(CReader&, EScopeType)"
    ) {
        "x0"
    } else if call_arguments(callee).is_some() {
        "x1"
    } else {
        return None;
    };
    match arguments.get(destination) {
        Some(super::fields::Value::Owner(offset)) => Some(*offset),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::stop::Unresolved;
    use std::collections::BTreeMap;

    fn joined(callee: &str) -> ReaderJoin {
        ReaderJoin::Joined {
            callee: callee.into(),
            arguments: BTreeMap::new(),
            tail: true,
        }
    }

    #[test]
    fn exact_and_tightly_parsed_signatures_cover_the_six_known_kinds() {
        for (callee, expected) in [
            ("CReader::Read(bool&)", ReaderKind::Boolean),
            ("CReader::Read(unsigned long long&)", ReaderKind::Integer),
            ("CReader::Read(CFixedPoint&)", ReaderKind::FixedPoint),
            ("CReader::Read(CString&, bool)", ReaderKind::String),
            (
                "void NParserUtil::ReadKeyReferenceDeferred<CExampleDatabase>(CGlobalDeferredDatabaseObject const&, CReader&, CExampleDatabase::ValueType const**)",
                ReaderKind::Reference,
            ),
            (
                "void NParserUtil::ReadTrigger<CRootTrigger>(CReader&, CRootTrigger&, EScopeType)",
                ReaderKind::Block,
            ),
        ] {
            assert_eq!(classify(&[joined(callee)]).kind, expected, "{callee}");
        }
    }

    #[test]
    fn unsupported_signatures_and_unjoined_alternatives_stay_unknown() {
        for callee in [
            "CVariableValue::Read(CReader&, EScopeType)",
            "CReader::Read(float&)",
            "CReader::Read(CUTF8String&)",
            "void NParserUtil::ReadTrigger<A>(CReader&, B&, EScopeType)",
        ] {
            assert_eq!(classify(&[joined(callee)]).kind, ReaderKind::Unknown);
        }
        let missing = ReaderJoin::Missing(Unresolved::new("reader-routing"));
        assert_eq!(
            classify(&[joined("CReader::Read(bool&)"), missing]).callee,
            None
        );
        assert_eq!(
            classify(&[
                joined("CReader::Read(bool&)"),
                joined("CReader::Read(int&)")
            ])
            .kind,
            ReaderKind::Unknown
        );
    }

    #[test]
    fn block_family_requires_all_reader_alternatives_to_agree() {
        let trigger = joined(
            "void NParserUtil::ReadTrigger<CRootTrigger>(CReader&, CRootTrigger&, EScopeType)",
        );
        let effect =
            joined("void NParserUtil::ReadEffect<CEffect>(CReader&, CEffect&, EScopeType)");
        assert_eq!(
            classify(std::slice::from_ref(&trigger)).family,
            BlockFamily::Trigger
        );
        assert_eq!(
            classify(std::slice::from_ref(&effect)).family,
            BlockFamily::Effect
        );
        let direct = joined("CTrigger::Read(CReader&, EScopeType)");
        let same_family = [trigger.clone(), direct];
        assert_eq!(classify(&same_family).family, BlockFamily::Trigger);
        assert_eq!(classify(&same_family).callee, None);
        assert_eq!(
            classify(&[trigger.clone(), effect]).family,
            BlockFamily::Unknown
        );
        assert_eq!(
            classify(&[
                trigger,
                ReaderJoin::Missing(Unresolved::new("missing-path"))
            ])
            .family,
            BlockFamily::Unknown
        );
        for callee in [
            "CPersistent::Read(CReader&)",
            "CReader::Read(CPersistent&)",
            "void NParserUtil::ReadTrigger<A>(CReader&, B&, EScopeType)",
        ] {
            assert_eq!(classify(&[joined(callee)]).family, BlockFamily::Unknown);
        }
        assert_eq!(
            classify(&[joined("CReader::Read(int&)")]).family,
            BlockFamily::NotApplicable
        );
        assert_eq!(classify(&[]).family, BlockFamily::Unknown);
    }
}
