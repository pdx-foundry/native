//! Conservative classification of already-joined shared readers.
use crate::answer::{BlockFamily, ReaderKind};
use crate::engine::analysis::fields::ReaderJoin;
use crate::engine::analysis::references::{self, ReaderForm};
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
            ReaderJoin::Joined { callee, .. } | ReaderJoin::Stored { callee, .. } => {
                Some(callee.as_str())
            }
            ReaderJoin::Missing(_) => None,
        })
        .collect();
    let all_joined = !readers.is_empty()
        && readers.iter().all(|reader| {
            matches!(
                reader,
                ReaderJoin::Joined { .. } | ReaderJoin::Stored { .. }
            )
        });
    let callee = (all_joined && callees.len() == 1).then(|| *callees.first().unwrap());
    let kinds: Vec<_> = readers
        .iter()
        .map(|reader| match reader {
            ReaderJoin::Stored { kind, .. } => *kind,
            ReaderJoin::Joined { callee, .. } => classify_callee(callee),
            ReaderJoin::Missing(_) => ReaderKind::Unknown,
        })
        .collect();
    let kind = if callee.is_some() && kinds.iter().all(|kind| *kind == kinds[0]) {
        kinds[0]
    } else {
        ReaderKind::Unknown
    };
    let families: Vec<_> = callees
        .iter()
        .map(|callee| family_of_callee(callee))
        .collect();
    let family = families.first().copied().unwrap_or(BlockFamily::Unknown);
    Classification {
        callee,
        kind,
        family: if !matches!(kind, ReaderKind::Block | ReaderKind::Unknown) {
            BlockFamily::NotApplicable
        } else if all_joined && families.iter().all(|candidate| *candidate == family) {
            family
        } else {
            BlockFamily::Unknown
        },
    }
}

/// The two member-dispatch signatures supported by the token path evaluator.
pub(crate) fn is_member(callee: &str) -> bool {
    callee.ends_with("::ReadMember(CReader&, int)")
        || callee.ends_with("::ReadMember(CReader&, int, EScopeType)")
}

/// Broad facts about an established reader entry, independent of routing provenance.
pub(crate) fn entry(callee: &str) -> (ReaderKind, BlockFamily) {
    (classify_callee(callee), family_of_callee(callee))
}

/// The destination width of a shared scalar reader, when its parameter type fixes it.
pub(crate) fn scalar_width(callee: &str) -> Option<u64> {
    match callee {
        "CReader::Read(bool&)"
        | "CReader::Read(signed char&)"
        | "CReader::Read(unsigned char&)" => Some(1),
        "CReader::Read(short&)" | "CReader::Read(unsigned short&)" => Some(2),
        "CReader::Read(int&)" | "CReader::Read(unsigned int&)" | "CReader::Read(float&)" => Some(4),
        "CReader::Read(long long&)"
        | "CReader::Read(unsigned long long&)"
        | "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)" => {
            Some(8)
        }
        _ => None,
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

pub(crate) fn classify_callee(callee: &str) -> ReaderKind {
    match callee {
        "CVariableValue::Read(CReader&, EScopeType)"
        | "CVariableValue::Assign(CToken const&, EScopeType, CString const&)" => {
            ReaderKind::ScopedNumeric
        }
        "CReader::Read(bool&)" => ReaderKind::Boolean,
        "CReader::Read(float&)" => ReaderKind::Float,
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
        _ if references::reader(callee).is_some() => ReaderKind::Reference,
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

/// A nonempty template argument of ASCII letters, digits and `_`.
fn is_simple_template_argument(argument: &str) -> bool {
    !argument.is_empty()
        && argument
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// Registers that affect a supported reader call, excluding caller scratch state.
pub(crate) fn call_arguments(callee: &str) -> Option<&'static [&'static str]> {
    if callee == "CVariableValue::Assign(CToken const&, EScopeType, CString const&)" {
        Some(&["x0", "x1", "x2", "x3"])
    } else if callee == "CVariableValue::Read(CReader&, EScopeType)"
        || callee == "CReader::Read(CString&, bool)"
        || matches!(
            family_of_callee(callee),
            BlockFamily::Trigger | BlockFamily::Effect
        )
        || references::reader(callee).is_some()
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
    if let ReaderJoin::Stored { destination, .. } = join {
        return Some(*destination);
    }
    let ReaderJoin::Joined {
        callee, arguments, ..
    } = join
    else {
        return None;
    };
    let destination = if let Some(reference) = references::reader(callee) {
        match reference.form {
            ReaderForm::Deferred
            | ReaderForm::DeferredList
            | ReaderForm::DeferredIndex
            | ReaderForm::ImmediateList => "x2",
            ReaderForm::Immediate => return None,
        }
    } else if matches!(
        callee.as_str(),
        "CVariableValue::Read(CReader&, EScopeType)"
            | "CVariableValue::Assign(CToken const&, EScopeType, CString const&)"
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

/// Whether the call arguments have the provenance required by a shared reader.
pub(crate) fn arguments_join(
    name: &str,
    arguments: &std::collections::BTreeMap<String, crate::engine::analysis::fields::Value>,
    member_delegates: bool,
    reader_value_token_offset: Option<i64>,
) -> bool {
    use crate::engine::analysis::fields::Value;
    let get = |key: &str| arguments.get(key);
    let owner = |value: Option<&Value>| matches!(value, Some(Value::Owner(_)));
    if name == "CVariableValue::Assign(CToken const&, EScopeType, CString const&)" {
        owner(get("x0"))
            && reader_value_token_offset
                .is_some_and(|offset| get("x1") == Some(&Value::Reader(offset)))
    } else if is_member(name) {
        member_delegates
            && owner(get("x0"))
            && get("x1") == Some(&Value::Reader(0))
            && matches!(get("x2"), Some(Value::Token | Value::TokenWord(0)))
    } else if name.ends_with("::Read(CReader&, EScopeType)") {
        owner(get("x0")) && get("x1") == Some(&Value::Reader(0))
    } else if name.starts_with("CReader::Read(") {
        get("x0") == Some(&Value::Reader(0)) && owner(get("x1"))
    } else if name == "CVariableValue::Read(CReader&, EScopeType)" {
        owner(get("x0")) && get("x1") == Some(&Value::Reader(0))
    } else if name.starts_with("void NParserUtil::ReadEffect<")
        || name.starts_with("void NParserUtil::ReadTrigger<")
    {
        get("x0") == Some(&Value::Reader(0)) && owner(get("x1"))
    } else if let Some(reference) = crate::engine::analysis::references::reader(name) {
        use crate::engine::analysis::references::ReaderForm;

        match reference.form {
            ReaderForm::Deferred | ReaderForm::DeferredList | ReaderForm::DeferredIndex => {
                owner(get("x0")) && get("x1") == Some(&Value::Reader(0)) && owner(get("x2"))
            }
            ReaderForm::Immediate => get("x0") == Some(&Value::Reader(0)),
            ReaderForm::ImmediateList => get("x0") == Some(&Value::Reader(0)) && owner(get("x2")),
        }
    } else {
        false
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
    fn scoped_assign_requires_owner_and_value_token_provenance() {
        let callee = "CVariableValue::Assign(CToken const&, EScopeType, CString const&)";
        let mut arguments = BTreeMap::from([
            ("x0".into(), super::super::fields::Value::Owner(0xa8)),
            ("x1".into(), super::super::fields::Value::Reader(0x278)),
            ("x3".into(), super::super::fields::Value::Owner(0x28)),
        ]);
        assert!(arguments_join(callee, &arguments, true, Some(0x278)));
        assert_eq!(
            destination(&ReaderJoin::Joined {
                callee: callee.into(),
                arguments: arguments.clone(),
                tail: true,
            }),
            Some(0xa8)
        );

        arguments.insert("x1".into(), super::super::fields::Value::Reader(0));
        assert!(!arguments_join(callee, &arguments, true, Some(0x278)));
        arguments.insert("x1".into(), super::super::fields::Value::Reader(0x278));
        arguments.insert("x0".into(), super::super::fields::Value::Constant(0xa8));
        assert!(!arguments_join(callee, &arguments, true, Some(0x278)));
        arguments.insert("x0".into(), super::super::fields::Value::Owner(0xa8));
        assert!(!arguments_join(callee, &arguments, true, None));
    }

    #[test]
    fn scalar_width_follows_the_parameter_type() {
        for (callee, width) in [
            ("CReader::Read(bool&)", Some(1)),
            ("CReader::Read(unsigned char&)", Some(1)),
            ("CReader::Read(short&)", Some(2)),
            ("CReader::Read(int&)", Some(4)),
            ("CReader::Read(long long&)", Some(8)),
            ("CReader::Read(CFixedPoint&)", None),
            ("CReader::Read(CString&, bool)", None),
        ] {
            assert_eq!(scalar_width(callee), width, "{callee}");
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
        assert_eq!(
            classify(&[joined("CReader::Read(float&)")]).kind,
            ReaderKind::Float
        );
        for callee in [
            "CReader::Read(CUTF8String&)",
            "void NParserUtil::ReadTrigger<A>(CReader&, B&, EScopeType)",
        ] {
            assert_eq!(classify(&[joined(callee)]).kind, ReaderKind::Unknown);
        }
        assert_eq!(
            classify(&[joined("CVariableValue::Read(CReader&, EScopeType)")]).kind,
            ReaderKind::ScopedNumeric
        );
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
