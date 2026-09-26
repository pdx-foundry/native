//! Command member dispatch, joined to the concrete factory receiver.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    declarations::{self, CommandReader, DeclarationInput},
    discovery::Symbol,
    fields::{
        self, Condition, DataSection, DispatchInput, FieldGap, PathOutcome, ReaderJoin, RootField,
        Token, TokenPath, Value,
    },
    stop::Unresolved,
};
use crate::BlockFamily;

mod numeric;
mod ordering;

/// Collection storage used by command readers in one exact build.
#[derive(Clone, Copy)]
pub struct ChildLayout {
    pub data: i64,
    pub count: i64,
    pub token: i64,
}

pub const METHOD: &str = "command-grammar/v1";
const DELEGATION_LIMIT: usize = 8;
const PATH_LIMIT: usize = 4096;

/// Inputs collected from one verified executable buffer.
pub struct GrammarInput {
    pub declarations: DeclarationInput,
    pub child_layout: ChildLayout,
    pub numeric_decoder: u64,
    pub reader_token_offset: u64,
    pub symbols: Vec<Symbol>,
    pub data: Vec<DataSection>,
    pub tokens: BTreeMap<i64, Token>,
    /// Shared family dispatch boundaries inspected for this exact build.
    pub families: BTreeMap<String, BlockFamily>,
}

#[derive(Debug)]
pub enum OrderOutcome {
    Reader(ReaderJoin),
    Family(BlockFamily),
}

pub struct GrammarResult {
    #[cfg(test)]
    pub reader: CommandReader,
    pub reader_name: String,
    pub reader_kind: crate::ReaderKind,
    pub reader_family: BlockFamily,
    pub member_name: String,
    pub numeric: Option<Box<GrammarResult>>,
    pub ordering: Vec<ordering::Rule>,
    pub fields: ChildFields,
    pub families: Vec<BlockFamily>,
    pub stops: Vec<Unresolved>,
}

pub struct ChildFields {
    pub fields: Vec<RootField>,
    pub paths: Vec<TokenPath>,
    pub gaps: Vec<FieldGap>,
}

/// Follow only member delegates that receive the original token, reader and owner.
pub fn analyze(input: &GrammarInput, factory: u64) -> Result<GrammarResult, Unresolved> {
    let reader = declarations::command_reader(&input.declarations, factory)?;
    analyze_reader(input, reader, 0)
}

fn analyze_reader(
    input: &GrammarInput,
    reader: CommandReader,
    depth: usize,
) -> Result<GrammarResult, Unresolved> {
    if depth >= DELEGATION_LIMIT {
        return Err(Unresolved::new("grammar-nesting-limit"));
    }
    let names: BTreeSet<_> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.address == reader.member)
        .map(|symbol| symbol.name.as_str())
        .collect();
    if names.len() != 1 {
        return Err(Unresolved::new("command-member-name"));
    }
    let root = *names.first().unwrap();
    let read_names: BTreeSet<_> = input
        .symbols
        .iter()
        .filter(|symbol| symbol.address == reader.read)
        .map(|symbol| symbol.name.as_str())
        .collect();
    if read_names.len() != 1 {
        return Err(Unresolved::new("command-reader-name"));
    }
    let reader_name = read_names.first().unwrap().to_string();
    let dispatch = DispatchInput::command(
        &input.declarations.functions,
        &input.symbols,
        &input.data,
        input.reader_token_offset,
    );
    let root_family = input.families.get(root).copied();
    let (paths, mut gaps) = if root_family.is_some() {
        (Vec::new(), Vec::new())
    } else {
        fields::explore_member(&dispatch, root)
    };
    let mut pending: Vec<_> = paths
        .into_iter()
        .map(|path| (path, vec![root.to_owned()]))
        .collect();
    let mut leaves = Vec::new();
    let mut families: Vec<_> = root_family.into_iter().collect();
    let mut stops = Vec::new();
    let mut ordering = Vec::new();
    let mut numeric_reader = None;
    let mut numeric_failed = false;
    let mut visited = 0;
    while let Some((path, chain)) = pending.pop() {
        visited += 1;
        if visited > PATH_LIMIT {
            stops.push(Unresolved::new("grammar-path-limit"));
            break;
        }
        if let Some(rule) = ordering::rule(input, &path) {
            ordering.push(rule);
        }
        match numeric::reader(input, reader.member, &path) {
            Ok(Some(child)) if numeric_reader.is_none_or(|known| known == child) => {
                numeric_reader = Some(child);
                continue;
            }
            Ok(Some(_)) => {
                numeric_failed = true;
                stops.push(Unresolved::new("ambiguous-numeric-reader"));
                continue;
            }
            Err(stop) => {
                numeric_failed = true;
                stops.push(stop);
                continue;
            }
            Ok(None) => {}
        }
        let PathOutcome::Reader(ReaderJoin::Joined {
            callee, arguments, ..
        }) = &path.outcome
        else {
            leaves.push(path);
            continue;
        };
        if let Some(&family) = input.families.get(callee) {
            if path.conditions.is_empty() && !families.contains(&family) {
                families.push(family);
            } else if !path.conditions.is_empty() {
                stops.push(Unresolved::new("conditional-child-family"));
                // Keep a named child's delegated alternative in the field ledger. Removing
                // it could make the remaining field-reader paths look unconditional.
                if path.domain[0] == path.domain[1] {
                    leaves.push(path);
                }
            }
            continue;
        }
        let member = crate::engine::analysis::readers::is_member(callee);
        if !member {
            leaves.push(path);
            continue;
        }
        let Some(Value::Owner(offset)) = arguments.get("x0") else {
            stops.push(Unresolved::new("delegate-receiver"));
            leaves.push(path);
            continue;
        };
        if chain.len() >= DELEGATION_LIMIT || chain.contains(callee) {
            stops.push(Unresolved::new("grammar-delegation-limit"));
            leaves.push(path);
            continue;
        }
        let (children, child_gaps) = fields::explore_member(&dispatch, callee);
        gaps.extend(child_gaps);
        let mut chain = chain;
        chain.push(callee.clone());
        for mut child in children {
            child.domain = [
                child.domain[0].max(path.domain[0]),
                child.domain[1].min(path.domain[1]),
            ];
            if child.domain[0] > child.domain[1] {
                continue;
            }
            translate(&mut child, *offset);
            child
                .conditions
                .splice(0..0, path.conditions.iter().cloned());
            child
                .instructions
                .splice(0..0, path.instructions.iter().copied());
            pending.push((child, chain.clone()));
        }
    }
    let (fields, field_gaps) = fields::fields_and_gaps(&leaves, &input.tokens);
    gaps.extend(field_gaps);
    for path in &leaves {
        match &path.outcome {
            PathOutcome::Gap(stop) | PathOutcome::Reader(ReaderJoin::Missing(stop)) => {
                stops.push(*stop)
            }
            _ => {}
        }
    }
    let numeric = if numeric_failed {
        None
    } else if let Some(child) = numeric_reader {
        match analyze_reader(input, child, depth + 1) {
            Ok(grammar) => Some(Box::new(grammar)),
            Err(stop) => {
                stops.push(stop);
                None
            }
        }
    } else {
        None
    };
    let (reader_kind, reader_family) = super::readers::entry(&reader_name);
    stops.sort();
    stops.dedup();
    Ok(GrammarResult {
        #[cfg(test)]
        reader,
        reader_kind,
        reader_family,
        reader_name,
        member_name: root.into(),
        numeric,
        ordering,
        families,
        stops,
        fields: ChildFields {
            fields,
            paths: leaves,
            gaps,
        },
    })
}

fn translate(path: &mut TokenPath, offset: i64) {
    for Condition { value: tested, .. } in &mut path.conditions {
        if let Some(tested) = tested {
            translate_value(tested, offset);
        }
    }
    if let PathOutcome::Reader(ReaderJoin::Joined { arguments, .. }) = &mut path.outcome {
        for argument in arguments.values_mut() {
            translate_value(argument, offset);
        }
    }
}

fn translate_value(value: &mut Value, offset: i64) {
    match value {
        Value::Owner(at) => *at += offset,
        Value::Load(base, _) | Value::Offset(base, _) | Value::EqualsAny(base, _) => {
            translate_value(base, offset)
        }
        Value::Indexed(base, index, _) => {
            translate_value(base, offset);
            translate_value(index, offset);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::analysis::fields::Function;
    use crate::engine::analysis::{
        assembler::{Arm64, arm64},
        declarations::{Composition, Function as Body, ParserSlots, ScopeSlots},
        evaluate::ReadOnlyData,
        families::{StringFunctions, StringLayout},
    };
    const FACTORY: u64 = 0x10000;
    const VTABLE: u64 = 0x11000;
    const CREATE: u64 = 0x1000;
    const READ: u64 = 0x2000;
    const ROOT: u64 = 0x3000;
    const DELEGATE: u64 = 0x4000;
    const FAMILY: u64 = 0x5000;
    const CHILD: u64 = 0x6000;
    const NEW: u64 = 0x9000;

    fn returning(address: u64) -> Arm64 {
        let mut body = Arm64::at(address);
        arm64!(body; ret);
        body
    }

    fn input(root: Arm64, delegate: Arm64) -> GrammarInput {
        let mut create = Arm64::at(CREATE);
        arm64!(create; mov w0, #128; bl extern NEW as usize; mov x19, x0);
        create.address(8, VTABLE);
        arm64!(create; str x8, [x19]; ret);
        let functions: Vec<_> = [
            ("factory", create),
            ("shared read", returning(READ)),
            ("COuter::ReadMember(CReader&, int)", root),
            ("CInner::ReadMember(CReader&, int)", delegate),
            ("CChildren::ReadMember(CReader&, int)", returning(FAMILY)),
            ("CTrigger::Read(CReader&, EScopeType)", returning(CHILD)),
        ]
        .into_iter()
        .map(|(name, body)| Function {
            name: name.into(),
            address: body.start(),
            code: body.bytes(),
        })
        .collect();
        let symbols = functions
            .iter()
            .map(|body| Symbol {
                name: body.name.clone(),
                address: body.address,
            })
            .collect();
        let declarations = DeclarationInput {
            tokens: BTreeMap::new(),
            registrars: vec![],
            register_entry: BTreeSet::new(),
            entry_helpers: BTreeSet::new(),
            operator_new: BTreeSet::from([NEW]),
            constructors: BTreeMap::new(),
            functions: functions
                .iter()
                .map(|body| {
                    (
                        body.address,
                        Body {
                            address: body.address,
                            code: body.code.clone(),
                        },
                    )
                })
                .collect(),
            pointers: BTreeMap::from([
                (FACTORY + 0x10, CREATE),
                (VTABLE + 0x10, READ),
                (VTABLE + 0x18, ROOT),
            ]),
            strings: BTreeMap::new(),
            slots: ScopeSlots {
                create: 0x10,
                supported_scopes: 0x80,
            },
            parser_slots: ParserSlots {
                read: 0x10,
                member: 0x18,
            },
            scope_names: None,
            composition: Composition {
                bodies: BTreeMap::new(),
                callers: BTreeMap::new(),
                composers: BTreeSet::new(),
                dynamic_token: BTreeSet::new(),
                create_database: BTreeSet::new(),
                strings: StringFunctions::default(),
                layout: StringLayout { flag_byte: 0x17 },
                data: ReadOnlyData::default(),
            },
        };
        GrammarInput {
            child_layout: ChildLayout {
                data: 0x10,
                count: 0x1c,
                token: 0x20,
            },
            numeric_decoder: 0x8000,
            reader_token_offset: 0x38,
            declarations,
            symbols,
            data: vec![],
            tokens: BTreeMap::from([(
                7,
                Token {
                    name: "limit".into(),
                    constructor: 1,
                    ambiguous: false,
                },
            )]),
            families: BTreeMap::from([(
                "CChildren::ReadMember(CReader&, int)".into(),
                BlockFamily::Trigger,
            )]),
        }
    }

    fn leaf() -> Arm64 {
        let mut body = Arm64::at(DELEGATE);
        arm64!(body; cmp w2, #7; b.ne extern (DELEGATE + 20) as usize;
            add x0, x0, #32; mov w2, #4; b extern CHILD as usize;
            b extern FAMILY as usize);
        body
    }

    #[test]
    fn inherited_member_dispatch_keeps_fixed_children_and_shared_family_separate() {
        let mut root = Arm64::at(ROOT);
        arm64!(root; add x0, x0, #16; b extern DELEGATE as usize);
        let input = input(root, leaf());
        let result = analyze(&input, FACTORY).unwrap();
        assert!(result.stops.is_empty(), "{:?}", result.stops);
        assert_eq!(result.families, [BlockFamily::Trigger]);
        assert_eq!(result.fields.fields.len(), 1);
        let child = &result.fields.fields[0];
        assert_eq!(child.name, "limit");
        assert_eq!(
            crate::engine::analysis::readers::classify(&child.readers).family,
            BlockFamily::Trigger
        );
        assert_eq!(
            crate::engine::analysis::readers::destination(&child.readers[0]),
            Some(48)
        );
    }

    #[test]
    fn unresolved_delegate_arguments_do_not_establish_child_grammar() {
        let mut root = Arm64::at(ROOT);
        arm64!(root; mov x1, x3; b extern DELEGATE as usize);
        let result = analyze(&input(root, leaf()), FACTORY).unwrap();
        assert!(result.families.is_empty());
        assert!(result.fields.fields.is_empty());
        assert!(
            result
                .stops
                .iter()
                .any(|stop| stop.reason == "reader-routing")
        );
    }

    #[test]
    fn recursive_member_delegation_stays_unresolved() {
        let mut root = Arm64::at(ROOT);
        arm64!(root; b extern DELEGATE as usize);
        let mut delegate = Arm64::at(DELEGATE);
        arm64!(delegate; b extern ROOT as usize);
        let result = analyze(&input(root, delegate), FACTORY).unwrap();
        assert!(result.families.is_empty());
        assert!(
            result
                .stops
                .iter()
                .any(|stop| stop.reason == "grammar-delegation-limit")
        );
    }
    fn ordered_input() -> GrammarInput {
        let mut root = Arm64::at(ROOT);
        arm64!(root;
            cmp w2, #9; // conditionally routed child
            b.ne extern (ROOT + 44) as usize;
            ldrsw x8, [x0, #0x1c]; // prior child count
            cbz w8, extern (ROOT + 48) as usize;
            ldr x9, [x0, #0x10]; // child pointer buffer
            add x8, x9, x8, lsl #3;
            ldur x8, [x8, #-8];
            ldr w8, [x8, #0x20]; // previous child's key
            cmp w8, #7;
            ccmp w8, #8, #4, ne;
            b.ne extern (ROOT + 48) as usize;
            b extern FAMILY as usize;
            add x0, x0, #64;
            mov w2, #4;
            b extern CHILD as usize
        );
        let mut input = input(root, leaf());
        for (token, name) in [(7, "first_branch"), (8, "next_branch"), (9, "fallback")] {
            input.tokens.insert(
                token,
                Token {
                    name: name.into(),
                    constructor: 1,
                    ambiguous: false,
                },
            );
        }
        input
    }

    #[test]
    fn ordering_preserves_first_and_previous_child_reader_routes() {
        use crate::ChildOrderCondition;
        let result = analyze(&ordered_input(), FACTORY).unwrap();
        assert_eq!(result.ordering.len(), 3, "{:?}", result.ordering);
        let field = result
            .fields
            .fields
            .iter()
            .find(|field| field.name == "fallback")
            .unwrap();
        assert_eq!(field.readers.len(), 3);
        assert_eq!(
            crate::engine::analysis::readers::classify(&field.readers).callee,
            None
        );
        assert!(
            result
                .ordering
                .iter()
                .any(|rule| rule.conditions == [ChildOrderCondition::First(true)]
                    && matches!(rule.outcome, OrderOutcome::Reader(_)))
        );
        for matches in [true, false] {
            let rule = result
                .ordering
                .iter()
                .find(|rule| {
                    rule.conditions.contains(&ChildOrderCondition::Previous {
                        keys: vec!["first_branch".into(), "next_branch".into()],
                        matches,
                    })
                })
                .unwrap();
            assert!(rule.conditions.contains(&ChildOrderCondition::First(false)));
            assert_eq!(rule.child, "fallback");
            assert_eq!(matches!(rule.outcome, OrderOutcome::Family(_)), matches);
        }
    }

    #[test]
    fn ordering_requires_bound_storage_and_unambiguous_token_names() {
        let mut input = ordered_input();
        input.child_layout.count += 4;
        assert!(analyze(&input, FACTORY).unwrap().ordering.is_empty());
        let mut input = ordered_input();
        input.tokens.get_mut(&7).unwrap().ambiguous = true;
        let result = analyze(&input, FACTORY).unwrap();
        assert_eq!(result.ordering.len(), 1);
        assert_eq!(
            result.ordering[0].conditions,
            [crate::ChildOrderCondition::First(true)]
        );
    }

    #[test]
    fn token_loaded_from_reader_and_conditional_compare_retain_both_fixed_keys() {
        let mut root = Arm64::at(ROOT);
        arm64!(root;
            ldr w8, [x1, #0x38];
            cmp w8, #7;
            ccmp w8, #8, #4, ne;
            b.ne extern (ROOT + 28) as usize;
            add x0, x0, #32;
            mov w2, #4;
            b extern CHILD as usize;
            b extern FAMILY as usize
        );
        let mut input = input(root, leaf());
        input.tokens.insert(
            8,
            Token {
                name: "second_key".into(),
                constructor: 1,
                ambiguous: false,
            },
        );
        let result = analyze(&input, FACTORY).unwrap();
        assert!(result.stops.is_empty(), "{:?}", result.stops);
        assert_eq!(
            result
                .fields
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["limit", "second_key"]
        );
        input.reader_token_offset += 8;
        let result = analyze(&input, FACTORY).unwrap();
        assert!(result.fields.fields.is_empty());
        assert!(result.families.is_empty());
    }

    fn numeric_input() -> GrammarInput {
        const CONSTRUCTOR: u64 = 0x8100;
        const CHILD_VTABLE: u64 = 0x12000;
        let mut root = Arm64::at(ROOT);
        arm64!(root;
            cmp w2, #12; // integer key token
            b.eq extern (ROOT + 16) as usize;
            mov x0, x1;
            b extern 0xa000;
            sub sp, sp, #48;
            stp x29, x30, [sp, #32];
            mov x20, x1;
            add x0, x1, #0x38; // key token in the reader
            add x1, sp, #12;
            bl extern 0x8000;
            mov w0, #128;
            bl extern NEW as usize;
            mov x19, x0;
            bl extern CONSTRUCTOR as usize;
            mov x0, x19;
            mov x1, x20;
            ldr x8, [x0];
            ldr x8, [x8, #16];
            blr x8;
            ret
        );
        let mut input = input(root, leaf());
        input.symbols.push(Symbol {
            address: 0xa000,
            name: "CReader::ReportUnexpected()".into(),
        });
        input
            .declarations
            .constructors
            .insert(CONSTRUCTOR, BTreeMap::from([(0, CHILD_VTABLE)]));
        input
            .declarations
            .pointers
            .extend([(CHILD_VTABLE + 0x10, READ), (CHILD_VTABLE + 0x18, DELEGATE)]);
        input
    }

    #[test]
    fn numeric_key_joins_its_constructed_reader_and_nested_fixed_keys() {
        let result = analyze(&numeric_input(), FACTORY).unwrap();
        assert!(result.stops.is_empty(), "{:?}", result.stops);
        assert!(result.fields.fields.is_empty());
        assert!(result.families.is_empty());
        let child = result.numeric.unwrap();
        assert_eq!(child.reader.member, DELEGATE);
        assert_eq!(child.families, [BlockFamily::Trigger]);
        assert_eq!(child.fields.fields[0].name, "limit");
    }

    #[test]
    fn numeric_key_requires_the_original_token_and_a_concrete_child() {
        for missing in ["token", "constructor", "member"] {
            let mut input = numeric_input();
            match missing {
                "token" => input.reader_token_offset += 8,
                "constructor" => input.declarations.constructors.clear(),
                "member" => {
                    input.declarations.pointers.remove(&(0x12000 + 0x18));
                }
                _ => unreachable!(),
            }
            let result = analyze(&input, FACTORY).unwrap();
            assert!(result.numeric.is_none(), "{missing}");
            assert!(!result.stops.is_empty(), "{missing}");
        }
    }

    #[test]
    #[ignore = "requires the exact installed M45 executable"]
    fn m45_control_grammar_reader_join() {
        let path = std::env::var("STELLARIS_PATH").expect("set STELLARIS_PATH");
        let native = crate::Native::open(path).unwrap();
        let mut failures = Vec::new();
        for (kind, commands) in [
            (
                crate::DeclarationKind::Trigger,
                ["and", "or", "not", "if", "else_if", "else"],
            ),
            (
                crate::DeclarationKind::Effect,
                [
                    "if",
                    "else_if",
                    "else",
                    "hidden_effect",
                    "random_list",
                    "every_owned_planet",
                ],
            ),
        ] {
            let (input, inventory) = native
                .bound()
                .analysis
                .as_ref()
                .unwrap()
                .grammar_input(kind)
                .unwrap();
            let mut found = BTreeSet::new();
            for (_, site) in inventory.sites {
                let declarations::Site::Declared { name, factory, .. } = site else {
                    continue;
                };
                if !commands.contains(&name.as_str()) {
                    continue;
                }
                found.insert(name.clone());
                match analyze(&input, factory) {
                    Ok(result) => {
                        if kind == crate::DeclarationKind::Effect
                            && ["if", "else_if", "else"].contains(&name.as_str())
                        {
                            assert_eq!(result.ordering.len(), 3, "{name}");
                        }
                        if kind == crate::DeclarationKind::Effect && name == "random_list" {
                            let child = result.numeric.as_ref().expect("weighted entry reader");
                            assert_eq!(child.families, [BlockFamily::Effect]);
                            assert_ne!(child.reader.member, result.reader.member);
                        }
                        eprintln!(
                            "{kind:?} {name}: reader={:?}; families={:?}; keys={:?}; stops={:?}; numeric={:?}; ordering={:?}",
                            result.reader,
                            result.families,
                            result
                                .fields
                                .fields
                                .iter()
                                .map(|field| &field.name)
                                .collect::<Vec<_>>(),
                            result.stops,
                            result.numeric.as_ref().map(|child| (
                                &child.reader,
                                &child.families,
                                &child.stops
                            )),
                            result.ordering
                        );
                    }
                    Err(stop) => failures.push(format!("{kind:?} {name}: {stop:?}")),
                }
            }
            assert_eq!(found.len(), commands.len(), "{kind:?}: {found:?}");
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
