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

/// Calls that write an engine diagnostic.
pub(crate) const DIAGNOSTICS: &[&str] = &[
    "CPdxLogFileAndLine::operator()(char const*, ...)",
    "CLogger::Log(char const*, unsigned int, int)",
    "CReader::ReportMalformed()",
    "CReader::ReportUnexpected()",
];

/// Calls that locate a diagnostic in the reader or the engine source and change no script state.
pub(crate) const DIAGNOSTIC_LOCATIONS: &[&str] = &[
    "CPdxLogFileAndLine::CPdxLogFileAndLine(char const*, unsigned int, unsigned int)",
    "CReader::GetFileName() const",
    "CReader::GetLineNumber() const",
    "CReader::GetFileLocationDescription() const",
];

/// Import stubs keep their raw name when it does not demangle.
const NEVER_RETURN: [&str; 5] = [
    "___stack_chk_fail",
    "__Unwind_Resume",
    "___cxa_throw",
    "___cxa_rethrow",
    "_abort",
];

/// Whether `callee` never returns: the engine's throw helpers and the runtime's.
pub(crate) fn never_returns(callee: &str) -> bool {
    NEVER_RETURN.contains(&callee) || callee.contains("::__throw_")
}

/// Measures a C string, reading only its bytes.
pub(crate) const TEXT_LENGTH: &str = "_strlen";

/// Whether `callee` assigns a standard string from a character pointer and a length:
/// `std::basic_string<char, …>::__assign_external(char const*, unsigned long)`.
pub(crate) fn assigns_string_text(callee: &str) -> bool {
    callee.starts_with("std::__1::basic_string<char, ")
        && callee.ends_with("::__assign_external(char const*, unsigned long)")
}

/// Releases a string's heap buffer.
pub(crate) const STRING_RELEASE: &str =
    "CPdxCommonStringAllocator::deallocate(char*, unsigned long)";

/// Whether a call writes or locates a diagnostic, or releases a string. It changes no script
/// state: a validated rule, which the weight and keyword methods state.
pub(crate) fn changes_no_script_state(callee: &str) -> bool {
    DIAGNOSTICS.contains(&callee)
        || DIAGNOSTIC_LOCATIONS.contains(&callee)
        || callee == STRING_RELEASE
}

/// A pure converter from a token to an engine enum: `E TokenToEnum<E>(int const&)`, which reads
/// the token through a pointer, or `Get…FromToken(int)`, which takes it in `w0`.
pub(crate) fn converter(callee: &str) -> Option<ConverterArgument> {
    if callee.contains(" TokenToEnum<") && callee.ends_with("(int const&)") {
        return Some(ConverterArgument::Pointer);
    }
    let name = callee.strip_suffix("FromToken(int)")?;

    (name.starts_with("Get") && !name.contains(['(', ' '])).then_some(ConverterArgument::Word)
}

/// How a [`converter`] receives its token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConverterArgument {
    /// `x0` points at a word that holds the token.
    Pointer,
    /// `w0` holds the token.
    Word,
}

/// The two member-dispatch signatures supported by the token path evaluator.
pub(crate) fn is_member(callee: &str) -> bool {
    callee.ends_with("::ReadMember(CReader&, int)")
        || callee.ends_with("::ReadMember(CReader&, int, EScopeType)")
}

/// Whether `callee` is a member reader that another member reader can continue in for the tokens
/// it does not handle: every `ReadMember(CReader&, int)` except the base rejection.
pub(crate) fn is_inheritable_member(callee: &str) -> bool {
    callee.ends_with("::ReadMember(CReader&, int)")
        && callee != "CPersistent::ReadMember(CReader&, int)"
}

/// Whether `callee` has the signature `F(T&, CReader&)` of a value reader that a member reader
/// can pass the tokens it does not handle: it reads the token from the reader itself and returns
/// false for a token it does not read, such as `ReadAttackInfoValue(NCombatRules::SAttackInfo&,
/// CReader&)`.
pub(crate) fn is_member_value_reader(callee: &str) -> bool {
    let Some(head) = callee.strip_suffix(", CReader&)") else {
        return false;
    };
    let Some((_, receiver)) = head.rsplit_once('(') else {
        return false;
    };

    receiver.ends_with('&') && !receiver.contains([',', '<', '>'])
}

/// The function that the thunk symbol `name` runs, or `None` when `name` is not a thunk. A thunk
/// is `non-virtual thunk to <target>` or `{virtual override thunk(<offset>, <target>)}`; a
/// covariant return adds a second offset before the target.
pub(crate) fn thunk_target(name: &str) -> Option<&str> {
    if let Some(target) = name.strip_prefix("non-virtual thunk to ") {
        return Some(target);
    }

    let offsets_and_target = name
        .strip_prefix("{virtual override thunk(")?
        .strip_suffix(")}")?;
    let mut target = after_call_offset(offsets_and_target)?;
    while let Some(after) = after_call_offset(target) {
        target = after;
    }

    Some(target)
}

/// The text after a leading `{offset(…)}` or `{virtual offset(…)}` of a thunk symbol.
fn after_call_offset(text: &str) -> Option<&str> {
    let group = text
        .strip_prefix("{offset(")
        .or_else(|| text.strip_prefix("{virtual offset("))?;

    Some(group.split_once("}, ")?.1)
}

/// Scope argument in the established scoped-reader ABI, after the token for a member reader.
pub(crate) fn scope_argument<'a>(
    callee: &str,
    arguments: &'a std::collections::BTreeMap<String, crate::engine::analysis::fields::Value>,
) -> Option<&'a crate::engine::analysis::fields::Value> {
    arguments.get(scope_register(callee)?)
}

/// The register of the scope in the established scoped-reader ABI: after the reader and
/// destination, or after the token for a member reader.
pub(crate) fn scope_register(callee: &str) -> Option<&'static str> {
    takes_scope(callee).then_some(if is_member(callee) { "x3" } else { "x2" })
}

/// Whether a reader takes a scope in the established scoped-reader ABI.
pub(crate) fn takes_scope(callee: &str) -> bool {
    callee.ends_with(", EScopeType)")
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
    } else if is_uniform_list(callee) {
        BlockFamily::NotApplicable
    } else if matches!(
        classify_callee(callee),
        ReaderKind::Block | ReaderKind::Unknown
    ) {
        BlockFamily::Unknown
    } else {
        BlockFamily::NotApplicable
    }
}

/// The broad kind that a conversion of the reader's assigned value token produces.
/// `CToken::GetFloat` returns the raw fixed-point value: its callers multiply it with a stored
/// fixed-point value and divide the product by the fixed-point scale.
/// [`TRIGGER_LOOKUP`] creates the trigger that the token names, so the result is a reference to
/// a trigger command.
pub(crate) fn conversion_kind(callee: &str) -> Option<ReaderKind> {
    match callee {
        "CToken::GetInt() const" => Some(ReaderKind::Integer),
        "CToken::GetFloat() const" => Some(ReaderKind::FixedPoint),
        TRIGGER_LOOKUP => Some(ReaderKind::Reference),
        _ => None,
    }
}

/// Looks up the trigger command that a token names, or makes a scripted-trigger placeholder for
/// a name that no trigger command has. It takes the trigger database, the token and a location.
pub(crate) const TRIGGER_LOOKUP: &str =
    "CTriggerDatabase::CreateTriggerOrScriptedPlaceholder(CToken const&, CString const&) const";

/// Reads past the reader's current value: one token, or the whole block that it opens.
pub(crate) const SKIP: &str = "CReader::SkipToken()";

/// The shared reader of an embedded persistent object.
pub(crate) const PERSISTENT_READ: &str = "CReader::Read(CPersistent&)";

/// Assigns the reader's value token to a scoped numeric operand. `CVariableValue::Read` is the
/// operand's other reader.
pub(crate) const SCOPED_ASSIGN: &str =
    "CVariableValue::Assign(CToken const&, EScopeType, CString const&)";

/// Reads one pre_trigger key's value into the `CPreTriggers` object in `x0`; `x2` is the key.
/// The value token `yes` sets the key's bit in the required-true mask and `no` in the
/// required-false mask. Any other value is ignored with no diagnostic. A repeat is logged and
/// still sets its bit.
pub(crate) const PRE_TRIGGER_VALUE: &str = "CPreTriggers::ReadValue(CReader&, EPreTriggers)";

pub(crate) fn classify_callee(callee: &str) -> ReaderKind {
    match callee {
        "CVariableValue::Read(CReader&, EScopeType)" | SCOPED_ASSIGN => ReaderKind::ScopedNumeric,
        "CReader::Read(bool&)" | PRE_TRIGGER_VALUE => ReaderKind::Boolean,
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
        SKIP => ReaderKind::Skipped,
        PERSISTENT_READ
        | "CPersistent::Read(CReader&)"
        | "CTrigger::Read(CReader&, EScopeType)"
        | "CEffect::Read(CReader&, EScopeType)" => ReaderKind::Block,
        _ if matching_template(callee, "ReadTrigger")
            || matching_template(callee, "ReadEffect") =>
        {
            ReaderKind::Block
        }
        _ if references::reader(callee).is_some() => ReaderKind::Reference,
        _ if is_uniform_list(callee) => ReaderKind::Block,
        _ => ReaderKind::Unknown,
    }
}

/// Whether `callee` is `CReader::ReadUniform<T>`, which appends each value of a `{ a b c }` list
/// to the array that it receives. The template's `enable_if` return type differs for an
/// enumeration element and for any other element.
pub(crate) fn is_uniform_list(callee: &str) -> bool {
    let Some(rest) = callee.strip_prefix("std::__1::enable_if<") else {
        return false;
    };
    let Some((_, reader)) = rest.split_once(", void>::type CReader::ReadUniform<") else {
        return false;
    };
    let Some((element, parameter)) = reader.split_once(">(") else {
        return false;
    };

    parameter == format!("CPdxArray<{}, int>&)", element.trim_end())
}

/// Whether the join is a shared scoped numeric reader.
pub(crate) fn reads_scoped_numeric(join: &ReaderJoin) -> bool {
    matches!(join, ReaderJoin::Joined { callee, .. }
        if classify_callee(callee) == ReaderKind::ScopedNumeric)
}

/// Whether the join reads into an object that the owner's constructor embeds: a persistent
/// object, or a scoped numeric operand.
pub(crate) fn reads_embedded_object(join: &ReaderJoin) -> bool {
    matches!(join, ReaderJoin::Joined { callee, .. } if callee == PERSISTENT_READ)
        || reads_scoped_numeric(join)
}

/// Whether the reader empties its stored trigger or effect block before it reads into it again.
/// `ReadTrigger<T>` and `ReadEffect<T>` log `Duplicate trigger` or `Duplicate effect` for a
/// nonempty block, delete its children and read into the same object; `CTrigger::Read` and
/// `CEffect::Read` do not.
pub(crate) fn clears_before_reading(callee: &str) -> bool {
    matching_template(callee, "ReadTrigger") || matching_template(callee, "ReadEffect")
}

/// Whether the reader adds each statement of the block to the trigger or effect that it
/// receives and keeps the children that an earlier occurrence added.
pub(crate) fn adds_children(callee: &str) -> bool {
    matches!(
        callee,
        "CTrigger::Read(CReader&, EScopeType)" | "CEffect::Read(CReader&, EScopeType)"
    )
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
    if callee == SKIP {
        Some(&["x0"])
    } else if callee == SCOPED_ASSIGN {
        Some(&["x0", "x1", "x2", "x3"])
    } else if callee == "CVariableValue::Read(CReader&, EScopeType)"
        || callee == PRE_TRIGGER_VALUE
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
        return *destination;
    }
    let ReaderJoin::Joined {
        callee, arguments, ..
    } = join
    else {
        return None;
    };
    // The value goes to one of two masks, so the reader has no one destination.
    if callee == PRE_TRIGGER_VALUE {
        return None;
    }

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
            | SCOPED_ASSIGN
            | "CTrigger::Read(CReader&, EScopeType)"
            | "CEffect::Read(CReader&, EScopeType)"
    ) {
        "x0"
    } else if call_arguments(callee).is_some_and(|registers| registers.contains(&"x1")) {
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
    if name == SCOPED_ASSIGN {
        owner(get("x0"))
            && reader_value_token_offset
                .is_some_and(|offset| get("x1") == Some(&Value::Reader(offset)))
    } else if is_member(name) {
        member_delegates
            && owner(get("x0"))
            && get("x1") == Some(&Value::Reader(0))
            && matches!(get("x2"), Some(Value::Token | Value::TokenWord(0)))
    } else if name.ends_with("::Read(CReader&, EScopeType)") || name == PRE_TRIGGER_VALUE {
        owner(get("x0")) && get("x1") == Some(&Value::Reader(0))
    } else if name == SKIP {
        get("x0") == Some(&Value::Reader(0))
    } else if is_uniform_list(name) {
        // A list read into a stack array or a serializer's member, which the loader converts
        // after the call, still has the reader's value form; only its storage is unknown.
        get("x0") == Some(&Value::Reader(0))
            && (owner(get("x1"))
                || matches!(get("x1"), Some(Value::Stack(_) | Value::Serializer(_))))
    } else if name.starts_with("CReader::Read(") {
        let stack_temporary = match classify_callee(name) {
            ReaderKind::Integer => scalar_width(name) == Some(4),
            ReaderKind::Boolean => true,
            _ => false,
        };
        get("x0") == Some(&Value::Reader(0))
            && (owner(get("x1")) || (stack_temporary && matches!(get("x1"), Some(Value::Stack(_)))))
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

    #[test]
    fn a_thunk_name_gives_the_function_that_it_runs() {
        let target = "A::ReadMember(CReader&, int)";

        for name in [
            "non-virtual thunk to A::ReadMember(CReader&, int)",
            "{virtual override thunk({offset(-8)}, A::ReadMember(CReader&, int))}",
            "{virtual override thunk({virtual offset(-8, -16)}, A::ReadMember(CReader&, int))}",
            "{virtual override thunk({offset(-8)}, {offset(16)}, A::ReadMember(CReader&, int))}",
        ] {
            assert_eq!(thunk_target(name), Some(target), "{name}");
        }
    }

    #[test]
    fn a_name_that_is_not_a_thunk_has_no_thunk_target() {
        for name in [
            "A::ReadMember(CReader&, int)",
            "{virtual override thunk(A::ReadMember(CReader&, int))}",
            "{virtual override thunk({offset(-8)}, A::ReadMember(CReader&, int))",
            "{lambda(int)#1}::operator()(int) const",
        ] {
            assert_eq!(thunk_target(name), None, "{name}");
        }
    }

    #[test]
    fn a_word_integer_reader_can_join_a_stack_temporary() {
        let mut arguments = BTreeMap::from([
            (
                "x0".into(),
                crate::engine::analysis::fields::Value::Reader(0),
            ),
            (
                "x1".into(),
                crate::engine::analysis::fields::Value::Stack(-16),
            ),
        ]);
        assert!(arguments_join(
            "CReader::Read(int&)",
            &arguments,
            false,
            None
        ));
        for callee in [
            "CReader::Read(short&)",
            "CReader::Read(long long&)",
            "CReader::Read(float&)",
        ] {
            assert!(!arguments_join(callee, &arguments, false, None));
        }
        arguments.remove("x0");
        assert!(!arguments_join(
            "CReader::Read(int&)",
            &arguments,
            false,
            None
        ));
    }

    #[test]
    fn a_boolean_reader_can_join_a_stack_temporary_but_not_an_unknown_destination() {
        use crate::engine::analysis::fields::Value;

        let mut arguments = BTreeMap::from([
            ("x0".into(), Value::Reader(0)),
            ("x1".into(), Value::Stack(8)),
        ]);
        assert!(arguments_join(
            "CReader::Read(bool&)",
            &arguments,
            false,
            None
        ));

        arguments.insert("x1".into(), Value::Constant(8));
        assert!(!arguments_join(
            "CReader::Read(bool&)",
            &arguments,
            false,
            None
        ));
    }

    #[test]
    fn a_skipped_value_joins_with_the_reader_and_has_no_destination() {
        use crate::engine::analysis::fields::Value;

        let mut arguments = BTreeMap::from([
            ("x0".into(), Value::Reader(0)),
            ("x1".into(), Value::Owner(0x40)),
        ]);
        assert!(arguments_join(SKIP, &arguments, false, None));
        assert_eq!(
            destination(&ReaderJoin::Joined {
                callee: SKIP.into(),
                arguments: arguments.clone(),
                tail: true,
            }),
            None
        );

        arguments.insert("x0".into(), Value::Owner(0));
        assert!(!arguments_join(SKIP, &arguments, false, None));
    }

    #[test]
    fn a_pre_trigger_value_is_a_boolean_that_joins_the_owner_and_the_reader() {
        use crate::engine::analysis::fields::Value;

        let mut arguments = BTreeMap::from([
            ("x0".into(), Value::Owner(0)),
            ("x1".into(), Value::Reader(0)),
            ("x2".into(), Value::Constant(3)),
        ]);
        assert!(arguments_join(PRE_TRIGGER_VALUE, &arguments, false, None));
        assert_eq!(classify_callee(PRE_TRIGGER_VALUE), ReaderKind::Boolean);
        assert_eq!(
            call_arguments(PRE_TRIGGER_VALUE),
            Some(["x0", "x1", "x2"].as_slice())
        );
        assert_eq!(
            destination(&ReaderJoin::Joined {
                callee: PRE_TRIGGER_VALUE.into(),
                arguments: arguments.clone(),
                tail: true,
            }),
            None
        );

        arguments.insert("x1".into(), Value::Reader(0x278));
        assert!(!arguments_join(PRE_TRIGGER_VALUE, &arguments, false, None));

        arguments.insert("x1".into(), Value::Reader(0));
        arguments.insert("x0".into(), Value::Stack(8));
        assert!(!arguments_join(PRE_TRIGGER_VALUE, &arguments, false, None));
    }

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
    fn the_scoped_operand_read_joins_the_owner_and_the_reader() {
        use crate::engine::analysis::fields::Value;

        let callee = "CVariableValue::Read(CReader&, EScopeType)";
        let mut arguments = BTreeMap::from([
            ("x0".into(), Value::Owner(0xa8)),
            ("x1".into(), Value::Reader(0)),
        ]);
        assert!(arguments_join(callee, &arguments, false, None));

        arguments.insert("x1".into(), Value::Constant(0));
        assert!(!arguments_join(callee, &arguments, false, None));
        arguments.insert("x1".into(), Value::Reader(0));
        arguments.insert("x0".into(), Value::Reader(0));
        assert!(!arguments_join(callee, &arguments, false, None));
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
    fn both_operand_readers_and_no_other_reader_read_a_scoped_numeric() {
        for callee in ["CVariableValue::Read(CReader&, EScopeType)", SCOPED_ASSIGN] {
            assert!(reads_scoped_numeric(&joined(callee)), "{callee}");
        }

        for callee in [
            PERSISTENT_READ,
            "CReader::Read(int&)",
            "CVariableValue::ReadVariable(CString&)",
        ] {
            assert!(!reads_scoped_numeric(&joined(callee)), "{callee}");
        }

        assert!(!reads_scoped_numeric(&ReaderJoin::Missing(
            Unresolved::new("reader-routing")
        )));
    }

    #[test]
    fn an_embedded_object_is_read_by_a_persistent_or_scoped_numeric_reader() {
        for callee in [
            PERSISTENT_READ,
            "CVariableValue::Read(CReader&, EScopeType)",
            SCOPED_ASSIGN,
        ] {
            assert!(reads_embedded_object(&joined(callee)), "{callee}");
        }

        for callee in [
            "CPersistent::Read(CReader&)",
            "CReader::Read(CString&, bool)",
            "CReader::Read(int&)",
            "CTrigger::Read(CReader&, EScopeType)",
        ] {
            assert!(!reads_embedded_object(&joined(callee)), "{callee}");
        }

        let stored = ReaderJoin::Stored {
            callee: PERSISTENT_READ.into(),
            kind: ReaderKind::Block,
            destination: Some(0x40),
            repeat: crate::RepeatBehavior::Merges,
        };
        assert!(!reads_embedded_object(&stored));
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
    fn a_uniform_list_of_any_element_is_a_block_reader_of_an_owner_array() {
        for callee in [
            "std::__1::enable_if<!(std::is_enum<CString>::value), void>::type CReader::ReadUniform<CString>(CPdxArray<CString, int>&)",
            "std::__1::enable_if<std::is_enum<EScopeType>::value, void>::type CReader::ReadUniform<EScopeType>(CPdxArray<EScopeType, int>&)",
            "std::__1::enable_if<!(std::is_enum<TPdxRef<CShipDesign> >::value), void>::type CReader::ReadUniform<TPdxRef<CShipDesign> >(CPdxArray<TPdxRef<CShipDesign>, int>&)",
        ] {
            assert_eq!(classify_callee(callee), ReaderKind::Block, "{callee}");
            assert_eq!(
                family_of_callee(callee),
                BlockFamily::NotApplicable,
                "{callee}"
            );
            let mut arguments = BTreeMap::from([
                (
                    "x0".into(),
                    crate::engine::analysis::fields::Value::Reader(0),
                ),
                (
                    "x1".into(),
                    crate::engine::analysis::fields::Value::Owner(0x40),
                ),
            ]);
            assert!(arguments_join(callee, &arguments, false, None), "{callee}");
            arguments.insert(
                "x1".into(),
                crate::engine::analysis::fields::Value::Stack(0x10),
            );
            assert!(arguments_join(callee, &arguments, false, None), "{callee}");
            arguments.insert("x1".into(), crate::engine::analysis::fields::Value::Token);
            assert!(!arguments_join(callee, &arguments, false, None), "{callee}");
        }

        for callee in [
            "std::__1::enable_if<!(std::is_enum<CString>::value), void>::type CReader::ReadUniform<CString>(CPdxArray<int, int>&)",
            "void NSerializer::ReadUniformWithCustomSerializer<CSpecialistSubjectLevel::CSerializer, CSpecialistSubjectLevel, >(CReader&, CPdxArray<CSpecialistSubjectLevel, int>&, &&)",
        ] {
            assert_eq!(classify_callee(callee), ReaderKind::Unknown, "{callee}");
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
