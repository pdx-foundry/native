//! Bounded initialization analysis retained from SDK-482.
//!
//! This is an internal reusable method, not a consumer operation. It recognizes four complete
//! ARM64 compiler shapes. Registers, widths, branches, calls, and same-type null joins must all
//! match. Unknown shapes remain unknown.
use super::InputError;
use super::decode::Instruction;
use super::discovery::Symbol;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MAX_FUNCTIONS: usize = 16;
const MAX_INSTRUCTIONS: usize = 4096;

/// One complete decoded function used by the bounded reference method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecodedFunction {
    /// Exact demangled function name.
    pub name: String,
    /// Complete decoded body.
    pub instructions: Vec<Instruction>,
}

/// Inputs derived from one verified executable.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceInput {
    /// Whether the executable identity is still established.
    pub target_available: bool,
    /// Requested engine owner. This selects functions, never a class answer.
    pub owner: String,
    /// Complete functions needed for this owner and any checked wrapper.
    pub functions: Vec<DecodedFunction>,
    /// Exact executable symbol inventory.
    pub symbols: Vec<Symbol>,
    /// Imported or chained global location to exact demangled binding name.
    pub global_bindings: BTreeMap<u64, String>,
    /// Engine token identities to literal authored field names.
    pub token_names: BTreeMap<i64, String>,
}

/// Whether the internal method established a bounded candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceStatus {
    /// At least one bounded candidate is established, with explicit remaining gaps.
    Partial,
    /// The function exists, but no qualified shape establishes a candidate.
    Unknown,
    /// The target or requested initializer is unavailable.
    Unavailable,
}

/// One conditional outcome retained from the matched function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceAlternative {
    /// Condition described by the bounded compiler shape.
    pub when: String,
    /// Resulting destination action.
    pub action: String,
}

/// A bounded initialization-time reference candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceCandidate {
    /// Owner-relative input slot. It is target-local and remains internal.
    pub input_offset: i64,
    /// Owner-relative destination slot. It is target-local and remains internal.
    pub output_offset: i64,
    /// Candidate class derived from a typed call and same-type null, or from the typed null alone.
    pub reference_class: String,
    /// Exact database symbol derived from inspected global provenance.
    pub database: String,
    /// Bounded method that selected the candidate.
    pub mechanism: String,
    /// Basis for the candidate class.
    pub reference_class_basis: String,
    /// Authored root field when the bounded reader path joins this exact input slot.
    pub authored_field: Option<String>,
    /// Conditional alternatives preserved from the matched function.
    pub alternatives: Vec<ReferenceAlternative>,
}

/// Internal result of the initialization method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceResult {
    /// Requested owner.
    pub owner: String,
    /// Method outcome.
    pub status: ReferenceStatus,
    /// Established bounded candidates.
    pub candidates: Vec<ReferenceCandidate>,
    /// Truthful remaining boundaries.
    pub gaps: Vec<String>,
    /// The body has conditional processing that this method does not interpret.
    pub unresolved_conditional: bool,
}

/// Analyze one owner's initialization function.
pub fn analyze(input: &ReferenceInput) -> Result<ReferenceResult, InputError> {
    if input.functions.len() > MAX_FUNCTIONS
        || input
            .functions
            .iter()
            .map(|function| function.instructions.len())
            .sum::<usize>()
            > MAX_INSTRUCTIONS
    {
        return Err(InputError("reference input budget exceeded".into()));
    }
    if !input.target_available {
        return Ok(unavailable(input, "the exact target is unavailable"));
    }
    let initializer_name = format!("{}::PostInit()", input.owner);
    let Some(initializer) = unique_function(input, &initializer_name) else {
        return Ok(unavailable(
            input,
            "the initializer is missing or ambiguous",
        ));
    };
    let names = symbol_names(&input.symbols);
    let (kind, bindings) = match_pattern(&initializer.instructions, &names);
    let candidate = match kind {
        Some(PatternKind::InlineMap) => inline_map(input, &bindings),
        Some(PatternKind::GetterWrapper) => getter_wrapper(input, &bindings, &names),
        Some(PatternKind::LinearScan) => linear_scan(input, &bindings),
        Some(PatternKind::NullGetter) | None => None,
    };
    let Some(mut candidate) = candidate else {
        return Ok(ReferenceResult {
            owner: input.owner.clone(),
            status: ReferenceStatus::Unknown,
            candidates: vec![],
            gaps: vec![
                "No qualified reference shape and provenance join matched the complete function."
                    .into(),
            ],
            unresolved_conditional: initializer.instructions.iter().any(is_conditional),
        });
    };
    candidate.authored_field = authored_field(input, candidate.input_offset, &names);
    let gaps = match candidate.mechanism.as_str() {
        "linear-key-comparison" => vec![
            "The collection element type and database content loader are not established.".into(),
            "Registration, validation, and runtime outcomes are not established.".into(),
        ],
        "typed-map-call" => vec![
            "The typed map callee internals are not established; only its signature and caller selection are established.".into(),
            "Registration, definition loading, validation, and runtime outcomes are not established.".into(),
        ],
        _ => vec![
            "The hash-table callee internals are not established; only the checked wrapper is established.".into(),
            "Registration, definition loading, validation, and runtime outcomes are not established.".into(),
        ],
    };
    Ok(ReferenceResult {
        owner: input.owner.clone(),
        status: ReferenceStatus::Partial,
        candidates: vec![candidate],
        gaps,
        unresolved_conditional: false,
    })
}

fn unavailable(input: &ReferenceInput, reason: &str) -> ReferenceResult {
    ReferenceResult {
        owner: input.owner.clone(),
        status: ReferenceStatus::Unavailable,
        candidates: vec![],
        gaps: vec![reason.into()],
        unresolved_conditional: false,
    }
}

fn unique_function<'a>(input: &'a ReferenceInput, name: &str) -> Option<&'a DecodedFunction> {
    let mut functions = input
        .functions
        .iter()
        .filter(|function| function.name == name);
    let function = functions.next()?;
    functions.next().is_none().then_some(function)
}

fn symbol_names(symbols: &[Symbol]) -> BTreeMap<u64, Option<&str>> {
    let mut names = BTreeMap::new();
    for symbol in symbols {
        names
            .entry(symbol.address)
            .and_modify(|name| {
                if *name != Some(symbol.name.as_str()) {
                    *name = None;
                }
            })
            .or_insert(Some(symbol.name.as_str()));
    }
    names
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PatternKind {
    InlineMap,
    GetterWrapper,
    NullGetter,
    LinearScan,
}

fn match_pattern(
    rows: &[Instruction],
    names: &BTreeMap<u64, Option<&str>>,
) -> (Option<PatternKind>, BTreeMap<String, String>) {
    for (kind, template) in PATTERNS {
        let lines = canonical(rows, names);
        if lines.len() != template.len() {
            continue;
        }
        let mut bindings = BTreeMap::new();
        if lines
            .iter()
            .zip(*template)
            .all(|(actual, expected)| match_line(actual, expected, &mut bindings))
        {
            return (Some(*kind), bindings);
        }
    }
    (None, BTreeMap::new())
}

fn canonical(rows: &[Instruction], names: &BTreeMap<u64, Option<&str>>) -> Vec<String> {
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.address, index))
        .collect();
    rows.iter()
        .map(|row| {
            let mut operands = canonical_immediates(&row.operands);
            if is_branch(&row.operation)
                && let Some((before, target)) = branch_target(&operands)
            {
                let label = indexes.get(&target).map_or_else(
                    || {
                        format!(
                            "CALL:{}",
                            names
                                .get(&target)
                                .copied()
                                .flatten()
                                .unwrap_or("unresolved")
                        )
                    },
                    |index| format!("@{index}"),
                );
                operands = format!("{before}{label}");
            }
            format!("{} {operands}", row.operation)
        })
        .collect()
}

fn canonical_immediates(operands: &str) -> String {
    let mut result = String::with_capacity(operands.len());
    let bytes = operands.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            let negative = bytes.get(index + 1) == Some(&b'-');
            let digits_start = index + 1 + usize::from(negative);
            if bytes.get(digits_start) == Some(&b'0') && bytes.get(digits_start + 1) == Some(&b'x')
            {
                result.push('#');
                index += 1;
                continue;
            }
            let mut digits_end = digits_start;
            while bytes.get(digits_end).is_some_and(u8::is_ascii_digit) {
                digits_end += 1;
            }
            if digits_end > digits_start {
                let value = operands[digits_start..digits_end]
                    .parse::<u64>()
                    .expect("ASCII digits");
                if negative {
                    result.push_str("#-0x");
                } else {
                    result.push_str("#0x");
                }
                result.push_str(&format!("{value:x}"));
                index = digits_end;
                continue;
            }
        }
        result.push(bytes[index] as char);
        index += 1;
    }
    result
}

fn is_branch(operation: &str) -> bool {
    matches!(operation, "b" | "bl" | "cbz" | "cbnz" | "tbz" | "tbnz") || operation.starts_with("b.")
}

fn is_conditional(row: &Instruction) -> bool {
    matches!(
        row.operation.as_str(),
        "cbz" | "cbnz" | "tbz" | "tbnz" | "br"
    ) || row.operation.starts_with("b.")
}

fn branch_target(operands: &str) -> Option<(&str, u64)> {
    let split = operands.rfind(',').map_or(0, |index| index + 1);
    let target = parse_number(&operands[split..])? as u64;
    Some((&operands[..split], target))
}

fn parse_number(value: &str) -> Option<i64> {
    let value = value.strip_prefix('#').unwrap_or(value);
    let (negative, digits) = value
        .strip_prefix('-')
        .map_or((false, value), |digits| (true, digits));
    let number = digits.strip_prefix("0x").map_or_else(
        || digits.parse().ok(),
        |hex| i64::from_str_radix(hex, 16).ok(),
    )?;
    negative.then(|| -number).or(Some(number))
}

fn match_line(actual: &str, template: &str, bindings: &mut BTreeMap<String, String>) -> bool {
    let Some(open) = template.find('{') else {
        return actual == template;
    };
    let Some(close) = template[open + 1..].find('}').map(|index| open + 1 + index) else {
        return false;
    };
    if template[close + 1..].contains('{') {
        return false;
    }
    let prefix = &template[..open];
    let suffix = &template[close + 1..];
    let Some(captured) = actual
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .filter(|capture| !capture.is_empty())
    else {
        return false;
    };
    let name = &template[open + 1..close];
    match bindings.get(name) {
        Some(previous) => previous == captured,
        None => {
            bindings.insert(name.into(), captured.into());
            true
        }
    }
}

fn required_offset(bindings: &BTreeMap<String, String>, name: &str) -> Option<i64> {
    bindings.get(name).and_then(|value| parse_number(value))
}

fn global_address(bindings: &BTreeMap<String, String>, page: &str, offset: &str) -> Option<u64> {
    let page = required_offset(bindings, page)?;
    let offset = required_offset(bindings, offset)?;
    page.checked_add(offset)
        .and_then(|value| u64::try_from(value).ok())
}

fn inline_map(
    input: &ReferenceInput,
    bindings: &BTreeMap<String, String>,
) -> Option<ReferenceCandidate> {
    let field = required_offset(bindings, "input")?;
    let output = required_offset(bindings, "output")?;
    let database = input
        .global_bindings
        .get(&global_address(bindings, "dbPage", "dbOffset")?)?;
    let null = input
        .global_bindings
        .get(&global_address(bindings, "nullPage", "nullOffset")?)?;
    let find = bindings.get("find")?;
    let class = between(find, "CPdxUnorderedMap<CString, ", " const*")?;
    if null != &format!("TPdxNullObject<{class}>::_pInstance")
        || !database.starts_with("TGameDatabase<")
        || required_offset(bindings, "tag")? != field + 23
        || required_offset(bindings, "length")? != field + 8
        || !find.contains("::Find<CString>(CString const&) const")
    {
        return None;
    }
    Some(candidate(
        field,
        output,
        class,
        database,
        "typed-map-call",
        "typed call and same-type missing object",
        &[
            ("stored-string-empty", "destination-not-written"),
            ("returned-iterator-is-sentinel", "store-typed-null-object"),
            (
                "returned-iterator-is-not-sentinel",
                "store-returned-entry-value",
            ),
        ],
    ))
}

fn getter_wrapper(
    input: &ReferenceInput,
    bindings: &BTreeMap<String, String>,
    names: &BTreeMap<u64, Option<&str>>,
) -> Option<ReferenceCandidate> {
    let address = global_address(bindings, "dbPage", "dbOffset")?;
    let database = names.get(&address).copied().flatten()?;
    let getter = bindings.get("getter")?;
    if !database.ends_with("::_pInstance") {
        return None;
    }
    let nested = unique_function(input, getter)?;
    let (kind, nested_bindings) = match_pattern(&nested.instructions, names);
    if kind != Some(PatternKind::NullGetter) {
        return None;
    }
    let find = nested_bindings.get("find")?;
    let class = between(find, "CHashTable<CString, ", ",")?;
    let null =
        input
            .global_bindings
            .get(&global_address(&nested_bindings, "nullPage", "nullOffset")?)?;
    let database_type = database.strip_suffix("::_pInstance")?;
    if null != &format!("TPdxNullObject<{class}>::_pInstance")
        || !getter.starts_with(&format!("{database_type}::"))
        || !find.ends_with("::Find(CString const&) const")
    {
        return None;
    }
    Some(candidate(
        required_offset(bindings, "input")?,
        required_offset(bindings, "output")?,
        class,
        database,
        "qualified-wrapper-around-typed-call",
        "typed call and same-type missing object",
        &[
            ("hash-call-returns-null", "store-typed-null-object"),
            ("hash-call-returns-non-null", "store-call-return"),
        ],
    ))
}

fn linear_scan(
    input: &ReferenceInput,
    bindings: &BTreeMap<String, String>,
) -> Option<ReferenceCandidate> {
    let field = required_offset(bindings, "input")?;
    let database = input
        .global_bindings
        .get(&global_address(bindings, "dbPage", "dbOffset")?)?;
    let null = input
        .global_bindings
        .get(&global_address(bindings, "nullPage", "nullOffset")?)?;
    let class = null
        .strip_prefix("TPdxNullObject<")?
        .strip_suffix(">::_pInstance")?;
    if !database.starts_with("TGameDatabase<")
        || required_offset(bindings, "tag")? != field + 23
        || required_offset(bindings, "length")? != field + 8
    {
        return None;
    }
    Some(candidate(
        field,
        required_offset(bindings, "output")?,
        class,
        database,
        "linear-key-comparison",
        "typed missing object; collection element type remains unestablished",
        &[
            (
                "first-equal-length-and-equal-bytes-item",
                "store-collection-item",
            ),
            (
                "empty-collection-or-no-matching-item",
                "store-typed-null-object",
            ),
        ],
    ))
}

fn candidate(
    input_offset: i64,
    output_offset: i64,
    reference_class: &str,
    database: &str,
    mechanism: &str,
    basis: &str,
    alternatives: &[(&str, &str)],
) -> ReferenceCandidate {
    ReferenceCandidate {
        input_offset,
        output_offset,
        reference_class: reference_class.into(),
        database: database
            .strip_suffix("::_pInstance")
            .unwrap_or(database)
            .into(),
        mechanism: mechanism.into(),
        reference_class_basis: basis.into(),
        authored_field: None,
        alternatives: alternatives
            .iter()
            .map(|(when, action)| ReferenceAlternative {
                when: (*when).into(),
                action: (*action).into(),
            })
            .collect(),
    }
}

fn between<'a>(value: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    let rest = value.split_once(prefix)?.1;
    let candidate = rest.split_once(suffix)?.0;
    (!candidate.is_empty()).then_some(candidate)
}

fn authored_field(
    input: &ReferenceInput,
    input_offset: i64,
    names: &BTreeMap<u64, Option<&str>>,
) -> Option<String> {
    let function_name = format!("{}::ReadMember(CReader&, int, EScopeType)", input.owner);
    let function = unique_function(input, &function_name)?;
    let candidates: BTreeSet<_> = function
        .instructions
        .iter()
        .filter(|row| row.operation == "mov")
        .filter_map(|row| {
            let (destination, value) = row.operands.split_once(',')?;
            destination.starts_with('w').then_some(())?;
            parse_number(value).filter(|value| *value >= 10_000)
        })
        .collect();
    candidates.into_iter().find_map(|token| {
        (trace_reader(&function.instructions, token, names) == Some(input_offset))
            .then(|| input.token_names.get(&token).cloned())
            .flatten()
    })
}

fn trace_reader(
    rows: &[Instruction],
    token: i64,
    names: &BTreeMap<u64, Option<&str>>,
) -> Option<i64> {
    #[derive(Clone, PartialEq, Eq)]
    enum Value {
        Owner(i64),
        Reader,
        Integer(i64),
    }
    let indexes: BTreeMap<_, _> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.address, index))
        .collect();
    let mut registers = BTreeMap::from([
        ("x0".to_owned(), Value::Owner(0)),
        ("x1".to_owned(), Value::Reader),
        ("x2".to_owned(), Value::Integer(token)),
    ]);
    let mut flags = None;
    let mut pc = 0;
    let mut seen = BTreeSet::new();
    for _ in 0..500 {
        if !seen.insert(pc) {
            return None;
        }
        let row = rows.get(pc)?;
        pc += 1;
        let operands: Vec<_> = row.operands.split(',').collect();
        let register = |name: &str| {
            name.strip_prefix('w')
                .map(|digits| format!("x{digits}"))
                .unwrap_or_else(|| name.to_owned())
        };
        let value = |operand: &str, registers: &BTreeMap<String, Value>| {
            parse_number(operand)
                .map(Value::Integer)
                .or_else(|| registers.get(&register(operand)).cloned())
        };
        match (row.operation.as_str(), operands.as_slice()) {
            ("b" | "bl", [target]) => {
                let target = parse_number(target)? as u64;
                if row.operation == "b" && indexes.contains_key(&target) {
                    pc = indexes[&target];
                    continue;
                }
                return (names.get(&target).copied().flatten()
                    == Some("CReader::Read(CString&, bool)")
                    && registers.get("x0") == Some(&Value::Reader))
                .then(|| match registers.get("x1") {
                    Some(Value::Owner(offset)) => Some(*offset),
                    _ => None,
                })
                .flatten();
            }
            (operation, [target]) if operation.starts_with("b.") => {
                let (left, right) = flags?;
                let take = match operation {
                    "b.eq" => left == right,
                    "b.ne" => left != right,
                    "b.gt" => left > right,
                    "b.le" => left <= right,
                    "b.lt" => left < right,
                    "b.ge" => left >= right,
                    _ => return None,
                };
                if take {
                    pc = *indexes.get(&(parse_number(target)? as u64))?;
                }
            }
            ("mov", [destination, source]) => {
                let mut next = value(source, &registers);
                if destination.starts_with('w') {
                    next = match next {
                        Some(Value::Integer(value)) => {
                            Some(Value::Integer(value as u32 as i32 as i64))
                        }
                        _ => None,
                    };
                }
                let destination = register(destination);
                registers.remove(&destination);
                if let Some(next) = next {
                    registers.insert(destination, next);
                }
            }
            ("add" | "sub", [destination, left, right]) => {
                let amount = match value(right, &registers) {
                    Some(Value::Integer(amount)) => {
                        if row.operation == "sub" {
                            -amount
                        } else {
                            amount
                        }
                    }
                    _ => {
                        registers.remove(&register(destination));
                        continue;
                    }
                };
                let next = match value(left, &registers) {
                    Some(Value::Integer(value)) => value.checked_add(amount).map(Value::Integer),
                    Some(Value::Owner(offset)) if destination.starts_with('x') => {
                        offset.checked_add(amount).map(Value::Owner)
                    }
                    _ => None,
                };
                let destination = register(destination);
                registers.remove(&destination);
                if let Some(next) = next {
                    registers.insert(destination, next);
                }
            }
            ("cmp", [left, right]) => {
                flags = match (value(left, &registers), value(right, &registers)) {
                    (Some(Value::Integer(left)), Some(Value::Integer(right))) => {
                        Some((left, right))
                    }
                    _ => None,
                };
            }
            ("stp", _) if row.operands.contains("[sp") => {}
            ("ldp", [first, second, ..]) if row.operands.contains("[sp") => {
                registers.remove(&register(first));
                registers.remove(&register(second));
            }
            _ => return None,
        }
    }
    None
}

const INLINE_MAP: &[&str] = &[
    "stp x20,x19,[sp,#-0x20]!",
    "stp x29,x30,[sp,#0x10]",
    "add x29,sp,#0x10",
    "mov x19,x0",
    "ldrsb w8,[x0,#{tag}]",
    "tbnz w8,#0x1f,@25",
    "and x8,x8,#0xff",
    "cbz x8,@22",
    "add x1,x19,#{input}",
    "adrp x8,{dbPage}",
    "ldr x8,[x8,#{dbOffset}]",
    "ldr x8,[x8]",
    "add x0,x8,#0x58",
    "bl CALL:{find}",
    "ldrb w8,[x0,#0x4]",
    "add x9,x0,#0x30",
    "adrp x10,{nullPage}",
    "ldr x10,[x10,#{nullOffset}]",
    "cmp w8,#0xff",
    "csel x8,x10,x9,eq",
    "ldr x8,[x8]",
    "str x8,[x19,#{output}]",
    "ldp x29,x30,[sp,#0x10]",
    "ldp x20,x19,[sp],#0x20",
    "ret ",
    "ldr x8,[x19,#{length}]",
    "cbnz x8,@8",
    "b @22",
];
const GETTER_WRAPPER: &[&str] = &[
    "stp x20,x19,[sp,#-0x20]!",
    "stp x29,x30,[sp,#0x10]",
    "add x29,sp,#0x10",
    "mov x19,x0",
    "adrp x8,{dbPage}",
    "add x8,x8,#{dbOffset}",
    "ldr x0,[x8]",
    "add x1,x19,#{input}",
    "bl CALL:{getter}",
    "str x0,[x19,#{output}]",
    "ldp x29,x30,[sp,#0x10]",
    "ldp x20,x19,[sp],#0x20",
    "ret ",
];
const NULL_GETTER: &[&str] = &[
    "stp x29,x30,[sp,#-0x10]!",
    "mov x29,sp",
    "add x0,x0,#0x10",
    "bl CALL:{find}",
    "adrp x8,{nullPage}",
    "ldr x8,[x8,#{nullOffset}]",
    "ldr x8,[x8]",
    "cmp x0,#0x0",
    "csel x0,x8,x0,eq",
    "ldp x29,x30,[sp],#0x10",
    "ret ",
];
const LINEAR_SCAN: &[&str] = &[
    "stp x26,x25,[sp,#-0x50]!",
    "stp x24,x23,[sp,#0x10]",
    "stp x22,x21,[sp,#0x20]",
    "stp x20,x19,[sp,#0x30]",
    "stp x29,x30,[sp,#0x40]",
    "add x29,sp,#0x40",
    "mov x19,x0",
    "adrp x8,{dbPage}",
    "ldr x8,[x8,#{dbOffset}]",
    "ldr x8,[x8]",
    "ldr w21,[x8,#0x54]",
    "cmp w21,#0x1",
    "b.lt @58",
    "mov x22,#0x0",
    "add x23,x19,#{input}",
    "ldr x24,[x8,#0x48]",
    "ldrb w8,[x19,#{tag}]",
    "sxtb w25,w8",
    "ldr x9,[x19,#{length}]",
    "cmp w25,#0x0",
    "csel x20,x9,x8,lt",
    "b @25",
    "add x22,x22,#0x1",
    "cmp x22,x21",
    "b.eq @58",
    "ldr x26,[x24,x22,lsl#0x3]",
    "ldrb w9,[x26,#0x27]",
    "sxtb w8,w9",
    "ldr x10,[x26,#0x18]",
    "cmp w8,#0x0",
    "csel x8,x10,x9,lt",
    "cmp x8,x20",
    "b.ne @22",
    "add x8,x26,#0x10",
    "sxtb w10,w9",
    "ldr x11,[x8]",
    "cmp w10,#0x0",
    "csel x0,x11,x8,lt",
    "ldr x10,[x23]",
    "cmp w25,#0x0",
    "csel x1,x10,x23,lt",
    "tbnz w9,#0x7,@53",
    "cbz x20,@61",
    "sub x9,x9,#0x1",
    "ldrb w10,[x8],#0x1",
    "ldrb w11,[x1],#0x1",
    "cmp w10,w11",
    "ccmp x9,#0x0,#0x4,eq",
    "sub x9,x9,#0x1",
    "b.ne @44",
    "cmp w10,w11",
    "b.ne @22",
    "b @61",
    "cbz x20,@61",
    "mov x2,x20",
    "bl CALL:_memcmp",
    "cbnz w0,@22",
    "b @61",
    "adrp x8,{nullPage}",
    "ldr x8,[x8,#{nullOffset}]",
    "ldr x26,[x8]",
    "str x26,[x19,#{output}]",
    "ldp x29,x30,[sp,#0x40]",
    "ldp x20,x19,[sp,#0x30]",
    "ldp x22,x21,[sp,#0x20]",
    "ldp x24,x23,[sp,#0x10]",
    "ldp x26,x25,[sp],#0x50",
    "ret ",
];
const PATTERNS: &[(PatternKind, &[&str])] = &[
    (PatternKind::InlineMap, INLINE_MAP),
    (PatternKind::GetterWrapper, GETTER_WRAPPER),
    (PatternKind::NullGetter, NULL_GETTER),
    (PatternKind::LinearScan, LINEAR_SCAN),
];

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        input: ReferenceInput,
        next_symbol: u64,
    }

    impl Fixture {
        fn new(owner: &str) -> Self {
            Self {
                input: ReferenceInput {
                    target_available: true,
                    owner: owner.into(),
                    functions: vec![],
                    symbols: vec![],
                    global_bindings: BTreeMap::new(),
                    token_names: BTreeMap::new(),
                },
                next_symbol: 0x9000,
            }
        }

        fn function(
            &mut self,
            name: &str,
            base: u64,
            template: &[&str],
            replacements: &[(&str, &str)],
        ) {
            let mut lines: Vec<String> = template.iter().map(|line| (*line).into()).collect();
            for line in &mut lines {
                for (name, value) in replacements {
                    *line = line.replace(&format!("{{{name}}}"), value);
                }
            }
            let addresses: Vec<_> = (0..lines.len())
                .map(|index| base + index as u64 * 4)
                .collect();
            let instructions = lines
                .into_iter()
                .enumerate()
                .map(|(index, line)| {
                    let (operation, mut operands) = line
                        .split_once(' ')
                        .map(|(operation, operands)| (operation, operands.to_owned()))
                        .unwrap();
                    while let Some(at) = operands.find('@') {
                        let end = operands[at + 1..]
                            .find(|character: char| !character.is_ascii_digit())
                            .map_or(operands.len(), |length| at + 1 + length);
                        let target = operands[at + 1..end].parse::<usize>().unwrap();
                        operands.replace_range(at..end, &format!("{:#x}", addresses[target]));
                    }
                    if let Some(at) = operands.find("CALL:") {
                        let external = operands[at + 5..].to_owned();
                        let address = self.next_symbol;
                        self.next_symbol += 4;
                        self.input.symbols.push(Symbol {
                            name: external,
                            address,
                        });
                        operands.replace_range(at.., &format!("{address:#x}"));
                    }
                    Instruction {
                        address: addresses[index],
                        bytes: [0; 4],
                        operation: operation.into(),
                        operands,
                    }
                })
                .collect();
            self.input.functions.push(DecodedFunction {
                name: name.into(),
                instructions,
            });
        }

        fn bind(&mut self, address: u64, name: &str) {
            self.input.global_bindings.insert(address, name.into());
        }

        fn direct_symbol(&mut self, address: u64, name: &str) {
            self.input.symbols.push(Symbol {
                name: name.into(),
                address,
            });
        }

        fn reader(&mut self, field: &str, offset: i64) {
            let token = 10_000;
            let call = self.next_symbol;
            self.next_symbol += 4;
            self.input.symbols.push(Symbol {
                name: "CReader::Read(CString&, bool)".into(),
                address: call,
            });
            self.input.token_names.insert(token, field.into());
            self.input.functions.push(DecodedFunction {
                name: format!(
                    "{}::ReadMember(CReader&, int, EScopeType)",
                    self.input.owner
                ),
                instructions: vec![
                    instruction(0x8000, "mov", "w3,#0x2710"),
                    instruction(0x8004, "cmp", "w2,w3"),
                    instruction(0x8008, "b.ne", "0x801c"),
                    instruction(0x800c, "mov", "x19,x0"),
                    instruction(0x8010, "mov", "x0,x1"),
                    instruction(0x8014, "add", &format!("x1,x19,#{offset:#x}")),
                    instruction(0x8018, "b", &format!("{call:#x}")),
                    instruction(0x801c, "ret", ""),
                ],
            });
        }
    }

    fn instruction(address: u64, operation: &str, operands: &str) -> Instruction {
        Instruction {
            address,
            bytes: [0; 4],
            operation: operation.into(),
            operands: operands.into(),
        }
    }

    fn ship() -> Fixture {
        let mut fixture = Fixture::new("CCreateShipEffect");
        fixture.function(
            "CCreateShipEffect::PostInit()",
            0x1000,
            INLINE_MAP,
            &[
                ("tag", "0x617"),
                ("input", "0x600"),
                ("dbPage", "0x2000"),
                ("dbOffset", "0x20"),
                (
                    "find",
                    "CPdxUnorderedMap<CString, CShipSize const*>::Find<CString>(CString const&) const",
                ),
                ("nullPage", "0x3000"),
                ("nullOffset", "0x30"),
                ("output", "0x120"),
                ("length", "0x608"),
            ],
        );
        fixture.bind(0x2020, "TGameDatabase<CShipSizeDatabase>::_pInstance");
        fixture.bind(0x3030, "TPdxNullObject<CShipSize>::_pInstance");
        fixture.reader("random_existing_design", 0x600);
        fixture
    }

    fn district(owner: &str, class: &str, field: &str) -> Fixture {
        let mut fixture = Fixture::new(owner);
        fixture.function(
            &format!("{owner}::PostInit()"),
            0x2000,
            LINEAR_SCAN,
            &[
                ("dbPage", "0x4000"),
                ("dbOffset", "0x40"),
                ("input", "0x80"),
                ("tag", "0x97"),
                ("length", "0x88"),
                ("nullPage", "0x5000"),
                ("nullOffset", "0x50"),
                ("output", "0xa0"),
            ],
        );
        fixture.bind(
            0x4040,
            &format!("TGameDatabase<C{class}Database>::_pInstance"),
        );
        fixture.bind(0x5050, &format!("TPdxNullObject<C{class}>::_pInstance"));
        fixture.reader(field, 0x80);
        fixture
    }

    fn planet() -> Fixture {
        let mut fixture = Fixture::new("CChangePlanetClassEffect");
        let getter = "CPlanetClassDatabase::GetPlanetClass(CString const&) const";
        fixture.function(
            "CChangePlanetClassEffect::PostInit()",
            0x3000,
            GETTER_WRAPPER,
            &[
                ("dbPage", "0x6000"),
                ("dbOffset", "0x60"),
                ("input", "0x100"),
                ("getter", getter),
                ("output", "0x118"),
            ],
        );
        fixture.function(
            getter,
            0x4000,
            NULL_GETTER,
            &[
                (
                    "find",
                    "CHashTable<CString, CPlanetClass, int>::Find(CString const&) const",
                ),
                ("nullPage", "0x7000"),
                ("nullOffset", "0x70"),
            ],
        );
        fixture.direct_symbol(0x6060, "CPlanetClassDatabase::_pInstance");
        fixture.bind(0x7070, "TPdxNullObject<CPlanetClass>::_pInstance");
        fixture
    }

    fn replace_operand(function: &mut DecodedFunction, before: &str, after: &str) {
        let row = function
            .instructions
            .iter_mut()
            .find(|row| row.operands == before)
            .unwrap();
        row.operands = after.into();
    }

    fn result(fixture: &Fixture) -> ReferenceResult {
        analyze(&fixture.input).unwrap()
    }

    #[test]
    fn qualification_controls_cover_all_twenty_seven_retained_cases() {
        let mut passed = Vec::new();
        let mut check = |name: &str, outcome: bool| {
            assert!(outcome, "{name}");
            passed.push(name.to_owned());
        };

        for (owner, baseline) in [
            ("CCreateShipEffect", ship()),
            (
                "CAddDistrictEffect",
                district("CAddDistrictEffect", "District", "district_type"),
            ),
            ("CChangePlanetClassEffect", planet()),
        ] {
            check(
                &format!("{owner}: baseline candidate"),
                result(&baseline).status == ReferenceStatus::Partial,
            );
            let mut wrong = baseline;
            let initializer = &mut wrong.input.functions[0];
            replace_operand(initializer, "x19,x0", "x19,x1");
            check(
                &format!("{owner}: incorrect incoming owner"),
                result(&wrong).status == ReferenceStatus::Unknown,
            );

            let mut wrong = match owner {
                "CCreateShipEffect" => ship(),
                "CAddDistrictEffect" => district(owner, "District", "district_type"),
                _ => planet(),
            };
            let store = wrong.input.functions[0]
                .instructions
                .iter_mut()
                .find(|row| row.operation == "str" && row.operands.contains("[x19,"))
                .unwrap();
            store.operands = format!("x2,{}", store.operands.split_once(',').unwrap().1);
            check(
                &format!("{owner}: clobbered stored value"),
                result(&wrong).status == ReferenceStatus::Unknown,
            );

            let mut wrong = match owner {
                "CCreateShipEffect" => ship(),
                "CAddDistrictEffect" => district(owner, "District", "district_type"),
                _ => planet(),
            };
            let initializer = &mut wrong.input.functions[0];
            let index = initializer
                .instructions
                .iter()
                .position(|row| row.operands == "x19,x0")
                .unwrap();
            initializer
                .instructions
                .insert(index + 1, instruction(1, "mov", "x19,x1"));
            check(
                &format!("{owner}: intervening owner clobber"),
                result(&wrong).status == ReferenceStatus::Unknown,
            );
        }

        let mut wrong = ship();
        let initializer = &mut wrong.input.functions[0];
        let target = initializer.instructions[8].address;
        let branch = initializer
            .instructions
            .iter_mut()
            .find(|row| row.operation == "cbz")
            .unwrap();
        branch.operands = format!("x8,{target:#x}");
        check(
            "changed empty-input path",
            result(&wrong).status == ReferenceStatus::Unknown,
        );

        let mut wrong = ship();
        replace_operand(
            &mut wrong.input.functions[0],
            "x8,x10,x9,eq",
            "x8,x10,x9,ne",
        );
        check(
            "inverted missing-object choice",
            result(&wrong).status == ReferenceStatus::Unknown,
        );

        let mut wrong = ship();
        replace_operand(
            &mut wrong.input.functions[0],
            "w8,[x0,#0x617]",
            "w8,[x0,#0x618]",
        );
        check(
            "inconsistent string layout",
            result(&wrong).status == ReferenceStatus::Unknown,
        );

        let mut wrong = ship();
        wrong
            .input
            .global_bindings
            .insert(0x3030, "TPdxNullObject<CPlanetClass>::_pInstance".into());
        check(
            "wrong null-object type",
            result(&wrong).status == ReferenceStatus::Unknown,
        );

        let mut moved = ship();
        for row in &mut moved.input.functions[0].instructions {
            for (before, after) in [
                ("#0x600", "#0x700"),
                ("#0x617", "#0x717"),
                ("#0x608", "#0x708"),
                ("#0x120", "#0x220"),
            ] {
                row.operands = row.operands.replace(before, after);
            }
        }
        let moved_result = result(&moved);
        let original = result(&ship());
        check(
            "coherent changed slots are extracted",
            moved_result.status == ReferenceStatus::Partial
                && moved_result.candidates[0].input_offset != original.candidates[0].input_offset
                && moved_result.candidates[0].output_offset != original.candidates[0].output_offset,
        );

        let mut wrong = district("CAddDistrictEffect", "District", "district_type");
        let initializer = &mut wrong.input.functions[0];
        let target = initializer.instructions[61].address;
        let branch = initializer
            .instructions
            .iter_mut()
            .find(|row| row.operation == "b.ne")
            .unwrap();
        branch.operands = format!("{target:#x}");
        check(
            "changed scan mismatch path",
            result(&wrong).status == ReferenceStatus::Unknown,
        );

        let mut wrong = district("CAddDistrictEffect", "District", "district_type");
        let memcmp = wrong
            .input
            .symbols
            .iter_mut()
            .find(|symbol| symbol.name == "_memcmp")
            .unwrap();
        memcmp.name = "_strlen".into();
        check(
            "changed comparison callee",
            result(&wrong).status == ReferenceStatus::Unknown,
        );

        let mut wrong = planet();
        let getter = wrong
            .input
            .functions
            .iter_mut()
            .find(|function| function.name.starts_with("CPlanetClassDatabase::"))
            .unwrap();
        replace_operand(getter, "x0,x8,x0,eq", "x0,x8,x0,ne");
        check(
            "changed getter body invalidates summary",
            result(&wrong).status == ReferenceStatus::Unknown,
        );

        let baseline = ship();
        check(
            "reader provenance baseline",
            result(&baseline).candidates[0].authored_field.as_deref()
                == Some("random_existing_design"),
        );
        for (label, before, after) in [
            ("wrong reader owner", "x19,x0", "x19,x1"),
            ("32-bit pointer truncation", "x19,x0", "w19,w0"),
            (
                "clobbered reader destination",
                "x1,x19,#0x600",
                "x1,x2,#0x600",
            ),
        ] {
            let mut wrong = ship();
            let reader = wrong.input.functions.last_mut().unwrap();
            replace_operand(reader, before, after);
            check(label, result(&wrong).candidates[0].authored_field.is_none());
        }
        let mut wrong = ship();
        let reader = wrong.input.functions.last_mut().unwrap();
        reader
            .instructions
            .insert(5, instruction(0x8012, "bl", "0xdead"));
        check(
            "unknown call stops reader join",
            result(&wrong).candidates[0].authored_field.is_none(),
        );

        let mut unknown_owner = ship();
        unknown_owner.input.owner = "CNotAnObservedOwner".into();
        check(
            "unknown owner capability",
            result(&unknown_owner).status == ReferenceStatus::Unavailable,
        );
        let mut unavailable = ship();
        unavailable.input.target_available = false;
        check(
            "unqualified target capability",
            result(&unavailable).status == ReferenceStatus::Unavailable,
        );
        assert_eq!(passed.len(), 27, "{passed:#?}");
    }

    #[test]
    fn army_stays_unknown_and_the_relic_uses_the_unchanged_scan_method() {
        let mut army = Fixture::new("CCreateArmyEffect");
        army.input.functions.push(DecodedFunction {
            name: "CCreateArmyEffect::PostInit()".into(),
            instructions: vec![
                instruction(0x1000, "cbz", "x8,0x1008"),
                instruction(0x1004, "br", "x9"),
                instruction(0x1008, "ret", ""),
            ],
        });
        let army = result(&army);
        assert_eq!(army.status, ReferenceStatus::Unknown);
        assert!(army.unresolved_conditional);

        let relic = result(&district("CAddRelicEffect", "Relic", "relic"));
        assert_eq!(relic.status, ReferenceStatus::Partial);
        assert_eq!(relic.candidates[0].reference_class, "CRelic");
        assert_eq!(relic.candidates[0].mechanism, "linear-key-comparison");
    }

    #[test]
    fn decimal_immediate_normalization_preserves_the_numeric_value() {
        assert_eq!(
            canonical_immediates("x0,#10,#-12,#0x20"),
            "x0,#0xa,#-0xc,#0x20"
        );
    }
}
