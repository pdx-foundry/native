//! Scope types, the keywords that name them, and scope links, from the engine's compiled
//! switches.
//!
//! Scope types are the bits of the engine's scope-name table. The engine maps a script keyword to
//! a scope type with one compiled switch; running it for every token groups the keywords of each
//! type. Only that engine map groups keywords; no other list is an input.
//!
//! Scope links are the tokens that the engine's own link documentation prints. Its loop tests
//! each token with a compiled switch, so running that loop body for every token gives the links.
//! For each link, the method runs the engine's supported-scope and output-scope functions on the
//! link's token. Those functions read only the target's token, which the target constructor
//! stores; a link written as a plain keyword is one target with no chained part.
//!
//! Links that take data are the literal prefixes, such as `event_target:`, that the engine's
//! special-value parser tests. The parser stores the value of a prefixed target in other fields
//! and does not change its token. The scope-type argument reaches only the `@` form of an
//! `event_target:` value, which names a dynamic flag. A prefixed text is not a literal name, so the
//! target holds a token that no literal names; the lexer chooses which one. The method runs the
//! supported-scope and output-scope functions on every such token value and keeps a scope set only
//! when all of them agree.
//!
//! The parser's other forms are not links: `.` joins targets into a chain, and a trailing `?` sets
//! an option that the target reads at run time.
use std::collections::{BTreeMap, BTreeSet};

use super::InputError;
use super::declarations::{ScopeOutcome, ScopeType, number, scope_mask};
use super::decode::Instruction;
use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData, Unresolved};

/// Name and revision of the scope method.
pub const SCOPE_METHOD: &str = "scope-declarations/v1";

/// Name and revision of the scope-link method.
pub const LINK_METHOD: &str = "scope-links/v2";

/// How deeply a supported-scope function may ask for the scopes of another link.
const LINK_DEPTH: usize = 4;

/// Entry addresses of the engine functions that the methods run.
pub struct ScopeFunctions {
    pub scope_of_token: u64,
    pub link_documentation: u64,
    pub supported_scopes: u64,
    pub output_scope: u64,
    pub target_constructor: BTreeSet<u64>,
    pub target_destructor: BTreeSet<u64>,
    pub target_documentation: u64,
    pub token_type_count: u64,
    pub token_type: u64,
}

/// Executable-derived input for the scope and scope-link methods.
pub struct ScopeInput {
    pub tokens: BTreeMap<u64, String>,
    /// Scope names indexed by scope-type bit, or `None` when the table was not read.
    pub scope_names: Option<Vec<String>>,
    pub functions: ScopeFunctions,
    /// Offset of the token in an event target object.
    pub token_offset: u64,
    /// The special-value parser, whose literal prefixes mark links that take data.
    pub special_values: Vec<Instruction>,
    pub code: Code,
    pub data: ReadOnlyData,
}

/// Scope types and the keywords that the engine maps to each.
pub struct ScopeResult {
    /// Each named scope type in bit order, with its keywords in token order.
    pub scopes: Vec<(ScopeType, Vec<String>)>,
    /// Keywords that match several scope types at once, with those types' names, such as
    /// `carrier` (planet or ship).
    pub groups: Vec<(String, ScopeOutcome)>,
    /// Scope types that keywords map to but the name table does not name.
    pub unnamed_types: usize,
    /// Token values that map to a scope type but have no literal name.
    pub unnamed_keywords: usize,
    /// Token values whose scope type could not be evaluated.
    pub unresolved_tokens: usize,
    pub table_missing: bool,
}

/// One declared scope link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub name: String,
    /// The prefix, such as `event_target:`, of a link that takes data after its name.
    pub prefix: Option<String>,
    pub input: ScopeOutcome,
    pub output: Output,
}

/// The declared output scope of one link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Listed(Vec<ScopeType>),
    Various,
    Unresolved(&'static str),
}

/// Every link, including those that take data, and what could not be named or followed.
pub struct LinkResult {
    pub links: Vec<Link>,
    pub unnamed_links: usize,
    pub unresolved_tokens: usize,
    pub table_missing: bool,
}

/// Group every keyword under its scope type.
pub fn scopes(input: &ScopeInput) -> Result<ScopeResult, InputError> {
    let mut keywords = BTreeMap::<u64, Vec<String>>::new();
    let mut unnamed_keywords = 0;
    let mut unresolved_tokens = 0;

    for token in token_range(input)? {
        match scope_of_token(input, token) {
            Ok(0) => {}
            Ok(scope_type) => match input.tokens.get(&token) {
                Some(name) => keywords.entry(scope_type).or_default().push(name.clone()),
                None => unnamed_keywords += 1,
            },
            Err(_) => unresolved_tokens += 1,
        }
    }

    let names = input.scope_names.as_deref().unwrap_or_default();
    let mut scopes = Vec::new();
    for (bit, name) in names
        .iter()
        .enumerate()
        .filter(|(_, name)| !name.is_empty())
    {
        let keywords = keywords.remove(&(1u64 << bit)).unwrap_or_default();
        let scope = ScopeType {
            bit,
            name: name.clone(),
        };
        scopes.push((scope, keywords));
    }

    let (groups, unnamed): (Vec<_>, Vec<_>) = keywords
        .into_iter()
        .partition(|(scope_type, _)| !scope_type.is_power_of_two());
    let groups = groups
        .into_iter()
        .flat_map(|(scope_type, keywords)| {
            let types = scope_mask(scope_type, input.scope_names.as_deref());
            keywords
                .into_iter()
                .map(move |keyword| (keyword, types.clone()))
        })
        .collect();

    Ok(ScopeResult {
        scopes,
        groups,
        unnamed_types: unnamed.len(),
        unnamed_keywords,
        unresolved_tokens,
        table_missing: input.scope_names.is_none(),
    })
}

/// Find every token that the link documentation prints, with its declared scopes.
pub fn links(input: &ScopeInput) -> Result<LinkResult, InputError> {
    let mut links = Vec::new();
    let mut unnamed_links = 0;
    let mut unresolved_tokens = 0;

    for token in token_range(input)? {
        match is_link(input, token) {
            Ok(false) => {}
            Ok(true) => match input.tokens.get(&token) {
                Some(name) => links.push(Link {
                    name: name.clone(),
                    prefix: None,
                    input: input_scopes(input, token),
                    output: output_scope(input, token),
                }),
                None => unnamed_links += 1,
            },
            Err(_) => unresolved_tokens += 1,
        }
    }

    if links.is_empty() && unnamed_links == 0 {
        return Err(InputError(
            "the link documentation selected no token".into(),
        ));
    }

    let unnamed = unnamed_tokens(input)?;
    for prefix in data_prefixes(input) {
        links.push(prefix_link(input, prefix, &unnamed));
    }

    Ok(LinkResult {
        links,
        unnamed_links,
        unresolved_tokens,
        table_missing: input.scope_names.is_none(),
    })
}

/// Every token value from zero to the largest literal token. Larger values are created at run
/// time and no compiled switch can name them.
fn token_range(input: &ScopeInput) -> Result<std::ops::RangeInclusive<u64>, InputError> {
    let last = input
        .tokens
        .keys()
        .max()
        .ok_or_else(|| InputError("the literal token table is empty".into()))?;
    Ok(0..=*last)
}

/// Every token value in the literal range that no literal names, and the first value after it,
/// which stands for every token created at run time.
fn unnamed_tokens(input: &ScopeInput) -> Result<Vec<u64>, InputError> {
    let range = token_range(input)?;
    let run_time = range.end() + 1;
    let mut unnamed: Vec<_> = range
        .filter(|token| !input.tokens.contains_key(token))
        .collect();
    unnamed.push(run_time);
    Ok(unnamed)
}

/// A link that takes data. Its target holds whichever `unnamed` token the lexer gives the written
/// text, so a scope set is declared only when every unnamed token declares the same one.
fn prefix_link(input: &ScopeInput, prefix: String, unnamed: &[u64]) -> Link {
    let name = prefix.trim_end_matches(':').to_owned();
    let literal_prefix = input
        .tokens
        .values()
        .any(|literal| literal.starts_with(&prefix));
    if literal_prefix {
        return Link {
            name,
            prefix: Some(prefix),
            input: ScopeOutcome::Unresolved("literal-prefix"),
            output: Output::Unresolved("literal-prefix"),
        };
    }

    let inputs = unnamed
        .iter()
        .map(|&token| match input_scopes(input, token) {
            ScopeOutcome::Unresolved(reason) => Err(reason),
            scopes => Ok(scopes),
        });
    let outputs = unnamed
        .iter()
        .map(|&token| match output_scope(input, token) {
            Output::Unresolved(reason) => Err(reason),
            output => Ok(output),
        });
    Link {
        name,
        prefix: Some(prefix),
        input: agreed(inputs).unwrap_or_else(ScopeOutcome::Unresolved),
        output: agreed(outputs).unwrap_or_else(Output::Unresolved),
    }
}

/// The one declaration that every token gives, or the reason there is none. The first unresolved
/// declaration keeps its own reason.
fn agreed<T: PartialEq>(
    declarations: impl Iterator<Item = Result<T, &'static str>>,
) -> Result<T, &'static str> {
    let mut agreed = None;
    for declaration in declarations {
        let declaration = declaration?;
        match &agreed {
            None => agreed = Some(declaration),
            Some(first) if *first != declaration => return Err("token-dependent"),
            Some(_) => {}
        }
    }
    agreed.ok_or("token-dependent")
}

fn scope_of_token(input: &ScopeInput, token: u64) -> Result<u64, Unresolved> {
    let mut machine = Machine::new(&input.code, &input.data);
    machine.set_register(0, token);
    let exit = machine.run(input.functions.scope_of_token, &mut |_, _| {
        Err(Unresolved("call"))
    })?;
    returned_value(&machine, exit)
}

/// Run one iteration of the link documentation loop for `token`. The loop body prints a link by
/// asking for its documentation; any other token continues the loop.
fn is_link(input: &ScopeInput, token: u64) -> Result<bool, Unresolved> {
    let functions = &input.functions;
    let mut machine = Machine::new(&input.code, &input.data);
    let cell = machine.allocate(4);
    machine.write(cell, 4, token);

    let mut counted = false;
    let exit = machine.run(functions.link_documentation, &mut |target, _| {
        if target == functions.token_type_count {
            if counted {
                return Ok(Call::Stop);
            }
            counted = true;
            return Ok(Call::Return(Some(1)));
        }
        if target == functions.token_type {
            return Ok(Call::Return(Some(cell)));
        }
        if target == functions.target_documentation {
            return Ok(Call::Stop);
        }
        Ok(Call::Return(None))
    })?;

    match exit {
        Exit::Stopped(target) if target == functions.target_documentation => Ok(true),
        Exit::Stopped(target) if target == functions.token_type_count => Ok(false),
        _ => Err(Unresolved("exit")),
    }
}

fn input_scopes(input: &ScopeInput, token: u64) -> ScopeOutcome {
    match supported_scopes(input, token, 0) {
        Ok(mask) => scope_mask(mask, input.scope_names.as_deref()),
        Err(Unresolved(reason)) => ScopeOutcome::Unresolved(reason),
    }
}

/// Run the supported-scope function on a target that holds `token`. One case asks for the scopes
/// of another target and adds to them, so a constructed target is followed the same way.
fn supported_scopes(input: &ScopeInput, token: u64, depth: usize) -> Result<u64, Unresolved> {
    if depth > LINK_DEPTH {
        return Err(Unresolved("link-depth"));
    }
    let functions = &input.functions;
    let offset = input.token_offset;
    let mut machine = Machine::new(&input.code, &input.data);
    let target = machine.allocate(offset + 8);
    machine.write(target + offset, 4, token);
    machine.set_register(0, target);

    let exit = machine.run(functions.supported_scopes, &mut |callee, machine| {
        let object = machine.register(0).ok_or(Unresolved("target"))?;
        if functions.target_constructor.contains(&callee) {
            let token = machine.register(1).ok_or(Unresolved("target"))?;
            machine.write(object + offset, 4, token & 0xffff_ffff);
            return Ok(Call::Return(Some(object)));
        }
        if callee == functions.supported_scopes {
            let token = machine
                .read(object + offset, 4)
                .ok_or(Unresolved("target"))?;
            return Ok(Call::Return(Some(supported_scopes(
                input,
                token,
                depth + 1,
            )?)));
        }
        if functions.target_destructor.contains(&callee) {
            return Ok(Call::Return(None));
        }
        Err(Unresolved("call"))
    })?;
    returned_value(&machine, exit)
}

fn output_scope(input: &ScopeInput, token: u64) -> Output {
    let mut machine = Machine::new(&input.code, &input.data);
    machine.set_register(0, token);
    machine.set_register(1, 0);
    let scope_type = machine
        .run(input.functions.output_scope, &mut |_, _| {
            Err(Unresolved("call"))
        })
        .and_then(|exit| returned_value(&machine, exit));

    match scope_type {
        Ok(0) => Output::Various,
        Ok(scope_type) => output_names(input, scope_type),
        Err(Unresolved(reason)) => Output::Unresolved(reason),
    }
}

/// A link can declare more than one output type, such as `carrier` (planet or ship).
fn output_names(input: &ScopeInput, scope_type: u64) -> Output {
    match scope_mask(scope_type, input.scope_names.as_deref()) {
        ScopeOutcome::Listed(names) => Output::Listed(names),
        ScopeOutcome::Any => Output::Unresolved("scope-type"),
        ScopeOutcome::Unresolved(reason) => Output::Unresolved(reason),
    }
}

fn returned_value(machine: &Machine, exit: Exit) -> Result<u64, Unresolved> {
    if exit != Exit::Returned {
        return Err(Unresolved("exit"));
    }
    machine.register(0).ok_or(Unresolved("result"))
}

/// Literal strings that end with `:` and that the special-value parser loads.
fn data_prefixes(input: &ScopeInput) -> Vec<String> {
    let mut pages = BTreeMap::<&str, u64>::new();
    let mut prefixes = BTreeSet::new();
    for row in &input.special_values {
        let arguments: Vec<_> = row.operands.split(',').collect();
        match (row.operation.as_str(), arguments.as_slice()) {
            ("adrp", [register, page]) => {
                if let Some(page) = number(page) {
                    pages.insert(register, page);
                }
            }
            ("add", [_, base, offset]) => {
                let address = pages
                    .get(base)
                    .zip(number(offset))
                    .map(|(page, offset)| page + offset);
                if let Some(text) = address.and_then(|address| input.data.string(address))
                    && text.len() > 1
                    && text.ends_with(':')
                {
                    prefixes.insert(text);
                }
            }
            _ => {}
        }
    }
    prefixes.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(bit: usize, name: &str) -> ScopeType {
        ScopeType {
            bit,
            name: name.into(),
        }
    }

    fn row(address: u64, operation: &str, operands: &str) -> Instruction {
        Instruction {
            address,
            bytes: [0; 4],
            operation: operation.into(),
            operands: operands.into(),
        }
    }

    /// Tokens 1 and 2 are links. Token 2's scopes are token 1's scopes plus bit 3.
    fn input() -> ScopeInput {
        let rows = vec![
            // Scope type of a token: 1 and 3 are countries, 2 is a planet, 4 is a planet or ship.
            row(0x0f8, "cmp", "w0,#4"),
            row(0x0fc, "b.eq", "#0x120"),
            row(0x100, "cmp", "w0,#2"),
            row(0x104, "b.eq", "#0x118"),
            row(0x108, "and", "w8,w0,#1"),
            row(0x10c, "lsl", "x0,x8,#2"),
            row(0x110, "ret", ""),
            row(0x118, "mov", "x0,#2"),
            row(0x11c, "ret", ""),
            row(0x120, "mov", "x0,#0xa"),
            row(0x124, "ret", ""),
            // Link documentation loop.
            row(0x200, "bl", "#0x900"),
            row(0x204, "mov", "w0,#0"),
            row(0x208, "bl", "#0x904"),
            row(0x20c, "ldr", "w28,[x0]"),
            row(0x210, "sub", "w8,w28,#1"),
            row(0x214, "cmp", "w8,#1"),
            row(0x218, "b.hi", "#0x224"),
            row(0x21c, "mov", "x0,x28"),
            row(0x220, "bl", "#0x908"),
            row(0x224, "bl", "#0x900"),
            row(0x228, "ret", ""),
            // Supported scopes.
            row(0x300, "ldr", "w8,[x0,#0x58]"),
            row(0x304, "cmp", "w8,#2"),
            row(0x308, "b.eq", "#0x314"),
            row(0x30c, "mov", "x0,#4"),
            row(0x310, "ret", ""),
            row(0x314, "sub", "sp,sp,#0x100"),
            row(0x318, "mov", "x0,sp"),
            row(0x31c, "mov", "w1,#1"),
            row(0x320, "bl", "#0x90c"),
            row(0x324, "mov", "x0,sp"),
            row(0x328, "bl", "#0x300"),
            row(0x32c, "orr", "x0,x0,#0x8"),
            row(0x330, "add", "sp,sp,#0x100"),
            row(0x334, "ret", ""),
            // Output scope: token 1 has a varying output.
            row(0x400, "cmp", "w0,#1"),
            row(0x404, "mov", "x8,#4"),
            row(0x408, "csel", "x0,xzr,x8,eq"),
            row(0x40c, "ret", ""),
        ];
        ScopeInput {
            tokens: BTreeMap::from([
                (1, "owner".into()),
                (2, "capital_scope".into()),
                (3, "country".into()),
                (4, "carrier".into()),
            ]),
            scope_names: Some(vec![
                String::new(),
                "planet".into(),
                "country".into(),
                "ship".into(),
            ]),
            functions: ScopeFunctions {
                scope_of_token: 0x0f8,
                link_documentation: 0x200,
                supported_scopes: 0x300,
                output_scope: 0x400,
                target_constructor: BTreeSet::from([0x90c]),
                target_destructor: BTreeSet::new(),
                target_documentation: 0x908,
                token_type_count: 0x900,
                token_type: 0x904,
            },
            token_offset: 0x58,
            special_values: vec![
                row(0x500, "adrp", "x1,#0x1000"),
                row(0x504, "add", "x1,x1,#0x10"),
                row(0x508, "adrp", "x1,#0x1000"),
                row(0x50c, "add", "x1,x1,#0x20"),
            ],
            code: Code::from_rows(rows),
            data: ReadOnlyData::new(vec![(0x1010, b"event_target:\0\0\0plain\0".to_vec())]),
        }
    }

    #[test]
    fn keywords_group_under_the_scope_type_that_the_engine_maps() {
        let result = scopes(&input()).unwrap();
        assert_eq!(
            result.scopes,
            vec![
                (scope(1, "planet"), vec!["capital_scope".into()]),
                (scope(2, "country"), vec!["owner".into(), "country".into()]),
                (scope(3, "ship"), vec![]),
            ]
        );
        assert_eq!(
            result.groups,
            vec![(
                "carrier".into(),
                ScopeOutcome::Listed(vec![scope(1, "planet"), scope(3, "ship")])
            )]
        );
        assert_eq!(result.unnamed_types, 0);
        assert_eq!(result.unresolved_tokens, 0);
    }

    #[test]
    fn links_follow_membership_scopes_and_outputs() {
        let result = links(&input()).unwrap();
        assert_eq!(
            result.links,
            vec![
                Link {
                    name: "owner".into(),
                    prefix: None,
                    input: ScopeOutcome::Listed(vec![scope(2, "country")]),
                    output: Output::Various,
                },
                Link {
                    name: "capital_scope".into(),
                    prefix: None,
                    input: ScopeOutcome::Listed(vec![scope(2, "country"), scope(3, "ship")]),
                    output: Output::Listed(vec![scope(2, "country")]),
                },
                Link {
                    name: "event_target".into(),
                    prefix: Some("event_target:".into()),
                    input: ScopeOutcome::Listed(vec![scope(2, "country")]),
                    output: Output::Listed(vec![scope(2, "country")]),
                },
            ]
        );
        assert_eq!(result.unnamed_links, 0);
    }

    /// Unnamed token 1 declares a varying output and unnamed tokens 0 and 5 declare a country, so
    /// the output of a prefixed target depends on the lexer's token.
    #[test]
    fn a_prefix_link_declares_only_what_every_unnamed_token_declares() {
        let mut input = input();
        input.tokens.remove(&1);

        let result = links(&input).unwrap();
        let event_target = result.links.last().unwrap();
        assert_eq!(
            event_target.input,
            ScopeOutcome::Listed(vec![scope(2, "country")])
        );
        assert_eq!(event_target.output, Output::Unresolved("token-dependent"));
    }

    #[test]
    fn a_literal_with_the_prefix_leaves_the_prefix_link_unresolved() {
        let mut input = input();
        input.tokens.insert(5, "event_target:saved".into());

        let result = links(&input).unwrap();
        let event_target = result.links.last().unwrap();
        assert_eq!(
            event_target.input,
            ScopeOutcome::Unresolved("literal-prefix")
        );
        assert_eq!(event_target.output, Output::Unresolved("literal-prefix"));
    }

    #[test]
    fn outputs_keep_every_declared_type_and_refuse_unnamed_ones() {
        let input = input();
        assert_eq!(
            output_names(&input, 0b1010),
            Output::Listed(vec![scope(1, "planet"), scope(3, "ship")])
        );
        assert_eq!(output_names(&input, 1), Output::Unresolved("scope-name"));
    }
}
