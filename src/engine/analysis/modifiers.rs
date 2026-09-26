//! Modifier and modifier-category declarations from direct registration calls.
//!
//! The engine registers each built-in modifier with one direct call to the modifier definition
//! function: the token in `w0` and the category mask in a stack argument. The code between the
//! previous call and that call is straight-line, so the evaluator reads both values. The token
//! names the modifier through the literal token table.
//!
//! Category names come from the engine's compiled category-name switch, run once for each mask.
//! A modifier's tags follow the engine's own documentation rule: the name of the whole mask when
//! the switch has one, otherwise the name of each set bit. Categories are intended-use tags; they
//! do not establish where a modifier takes effect.
//!
//! Outside the method: modifiers that content generates at run time. Each call site of a dynamic
//! modifier function is one gap, because that family composes its names from loaded content.
use std::collections::{BTreeMap, BTreeSet};

use super::InputError;
use super::decode::Instruction;
use super::evaluate::{Call, Code, Exit, Machine, ReadOnlyData};
use super::stop::Unresolved;

/// The width of the engine's category mask, a 32-bit stack argument.
const CATEGORY_MASK_BITS: u32 = 32;

/// Name and revision of the modifier method.
pub const MODIFIER_METHOD: &str = "modifier-declarations/v1";

/// Name and revision of the modifier-category method.
pub const CATEGORY_METHOD: &str = "modifier-categories/v1";

/// Executable-derived input for the modifier and category methods.
pub struct ModifierInput {
    pub tokens: BTreeMap<u64, String>,
    /// Straight-line code from the previous call up to and including each definition call.
    pub definition_sites: Vec<Vec<Instruction>>,
    pub define: u64,
    /// Offset of the category argument from the stack pointer at the definition call.
    pub category_offset: u64,
    /// Call sites of the functions that add modifiers generated from content.
    pub generation_sites: usize,
    pub categories: CategoryInput,
}

/// The engine's category-name function and what running it needs.
pub struct CategoryInput {
    pub category_name: u64,
    /// Size of the engine's string object that the category-name function fills.
    pub string_object_size: u64,
    /// Offset of a short string's length byte in that object. Its characters precede it.
    pub short_length_offset: u64,
    /// Functions that assign a literal of a given length to a string.
    pub assign_literal: BTreeSet<u64>,
    pub code: Code,
    pub data: ReadOnlyData,
}

/// The name of each category mask, or `Ok(None)` when the switch has none for it.
pub type CategoryNames = BTreeMap<u64, Result<Option<String>, Unresolved>>;

/// One direct definition call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionSite {
    Declared { name: String, tags: Tags },
    RuntimeToken,
    Unreadable,
}

/// The category tags of one modifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tags {
    Listed(Vec<String>),
    Unresolved(Unresolved),
}

/// Every definition site, every category name, and the input-wide counts. Category names cover
/// each single bit, the mask of every bit, and each mask that a definition uses.
pub struct ModifierResult {
    /// One for each definition site of the input, in its order.
    pub sites: Vec<DefinitionSite>,
    pub categories: CategoryNames,
    pub generation_sites: usize,
    /// The category mask of each modifier type that a definition call passes as a constant.
    pub type_masks: BTreeMap<u64, u64>,
}

/// Read every direct definition call and name every category mask that the calls use.
pub fn analyze(input: &ModifierInput) -> Result<ModifierResult, InputError> {
    if input.definition_sites.is_empty() {
        return Err(InputError("no direct modifier definition sites".into()));
    }

    let arguments: Vec<_> = input
        .definition_sites
        .iter()
        .map(|rows| definition_arguments(input, rows))
        .collect();

    let categories = category_names(
        &input.categories,
        arguments
            .iter()
            .filter_map(|arguments| arguments.mask.as_ref().ok().copied()),
    );
    let type_masks = arguments
        .iter()
        .filter_map(|arguments| {
            let mask = arguments.mask.as_ref().ok().copied();
            arguments.modifier_type.zip(mask)
        })
        .collect();

    let sites = arguments
        .into_iter()
        .map(|arguments| definition_site(input, &categories, arguments))
        .collect();

    Ok(ModifierResult {
        sites,
        categories,
        generation_sites: input.generation_sites,
        type_masks,
    })
}

/// Name each single category bit, the mask of every bit, and each of `masks`.
pub fn category_names(
    input: &CategoryInput,
    masks: impl IntoIterator<Item = u64>,
) -> CategoryNames {
    (0..CATEGORY_MASK_BITS)
        .map(|bit| 1u64 << bit)
        .chain([(1u64 << CATEGORY_MASK_BITS) - 1])
        .chain(masks)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|mask| (mask, category_name(input, mask)))
        .collect()
}

type Argument = Result<u64, Unresolved>;

/// The arguments of one definition call.
struct Arguments {
    token: Argument,
    /// The modifier type in `w1`, when it is a constant.
    modifier_type: Option<u64>,
    mask: Argument,
}

impl Arguments {
    fn unreadable() -> Self {
        Self {
            token: Err(Unresolved::new("site")),
            modifier_type: None,
            mask: Err(Unresolved::new("site")),
        }
    }
}

/// The token, modifier type and category mask passed to one definition call.
fn definition_arguments(input: &ModifierInput, rows: &[Instruction]) -> Arguments {
    let Some(first) = rows.first() else {
        return Arguments::unreadable();
    };

    let code = Code::from_rows(rows.iter().cloned());
    let data = ReadOnlyData::default();
    let mut machine = Machine::new(&code, &data);
    let exit = machine.run(first.address, &mut |target, _| {
        Ok(if target == input.define {
            Call::Stop
        } else {
            Call::Return(None)
        })
    });

    match exit {
        Ok(Exit::Stopped(_)) => {
            let stack = machine.stack_pointer();
            let token = machine
                .register(0)
                .map(|token| token as u32 as u64)
                .ok_or(Unresolved::new("token"));
            let mask = machine
                .read(
                    stack + input.category_offset,
                    u64::from(CATEGORY_MASK_BITS / 8),
                )
                .ok_or(Unresolved::new("category-mask"));
            Arguments {
                token,
                modifier_type: machine.register(1).map(|value| value as u32 as u64),
                mask,
            }
        }
        Ok(Exit::Returned | Exit::Trapped | Exit::Reached | Exit::Looped) | Err(_) => {
            Arguments::unreadable()
        }
    }
}

/// A token that is not a literal constant, or that has no literal name, is composed at run time.
fn definition_site(
    input: &ModifierInput,
    categories: &CategoryNames,
    Arguments { token, mask, .. }: Arguments,
) -> DefinitionSite {
    if matches!(token, Err(Unresolved { reason: "site", .. })) {
        return DefinitionSite::Unreadable;
    }
    let Some(name) = token.ok().and_then(|token| input.tokens.get(&token)) else {
        return DefinitionSite::RuntimeToken;
    };
    let tags = match mask {
        Ok(mask) => tags(categories, mask),
        Err(unresolved) => Tags::Unresolved(Unresolved {
            reason: "category-mask",
            ..unresolved
        }),
    };
    DefinitionSite::Declared {
        name: name.clone(),
        tags,
    }
}

/// The engine's documentation rule: the whole mask's name, or else the name of each set bit.
/// `categories` must name `mask` and each single bit.
pub fn tags(categories: &CategoryNames, mask: u64) -> Tags {
    match categories.get(&mask) {
        Some(Ok(Some(name))) => return Tags::Listed(vec![name.clone()]),
        Some(Err(unresolved)) => {
            return Tags::Unresolved(unresolved_category_name(unresolved.clone()));
        }
        None => return Tags::Unresolved(Unresolved::new("category-name")),
        Some(Ok(None)) => {}
    }

    let mut names = Vec::new();
    for bit in (0..CATEGORY_MASK_BITS).filter(|bit| mask >> bit & 1 == 1) {
        match categories.get(&(1 << bit)) {
            Some(Ok(Some(name))) => names.push(name.clone()),
            Some(Ok(None)) => return Tags::Unresolved(Unresolved::new("unnamed-category")),
            Some(Err(unresolved)) => {
                return Tags::Unresolved(unresolved_category_name(unresolved.clone()));
            }
            None => return Tags::Unresolved(Unresolved::new("category-name")),
        }
    }
    Tags::Listed(names)
}

/// A category name that did not resolve, under the one reason that public gap text quotes.
fn unresolved_category_name(unresolved: Unresolved) -> Unresolved {
    Unresolved {
        reason: "category-name",
        ..unresolved
    }
}

/// Run the category-name switch for one mask. `Ok(None)` means that the switch has no name for
/// it.
fn category_name(input: &CategoryInput, mask: u64) -> Result<Option<String>, Unresolved> {
    let mut machine = Machine::new(&input.code, &input.data);
    let object = machine.allocate(input.string_object_size);
    machine.set_register(0, mask);
    machine.set_register(1, object);

    let mut assigned = None;
    let exit = machine.run(input.category_name, &mut |target, machine| {
        if !input.assign_literal.contains(&target) {
            return Err(Unresolved::new("call"));
        }
        let literal = machine.known_register(1, "literal")?;
        let length = machine.known_register(2, "literal")?;
        let text = input
            .data
            .string(literal)
            .ok_or(Unresolved::new("literal"))?;
        assigned = Some(
            text.get(..length as usize)
                .ok_or(Unresolved::new("literal"))?
                .to_owned(),
        );
        Ok(Call::Return(Some(object)))
    })?;
    if exit != Exit::Returned {
        return Err(Unresolved::new("exit"));
    }

    if machine.known_register(0, "result")? & 1 == 0 {
        return Ok(None);
    }
    if let Some(text) = assigned {
        return Ok(Some(text));
    }
    let length = machine
        .read(object + input.short_length_offset, 1)
        .ok_or(Unresolved::new("short-string"))?;
    if length >= input.short_length_offset {
        return Err(Unresolved::new("short-string"));
    }
    let bytes: Option<Vec<u8>> = (0..length)
        .map(|offset| machine.read(object + offset, 1).map(|byte| byte as u8))
        .collect();
    let bytes = bytes.ok_or(Unresolved::new("short-string"))?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| Unresolved::new("short-string"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(address: u64, operation: &str, operands: &str) -> Instruction {
        Instruction {
            address,
            bytes: [0; 4],
            operation: operation.into(),
            operands: operands.into(),
        }
    }

    /// Masks 1 and 2 have names, 4 has none, and 3 has a whole-mask name.
    fn category_switch() -> Code {
        Code::from_rows([
            row(0x100, "mov", "x8,x0"),
            row(0x104, "mov", "w0,#0"),
            row(0x108, "cmp", "w8,#1"),
            row(0x10c, "b.eq", "#0x130"),
            row(0x110, "cmp", "w8,#2"),
            row(0x114, "b.eq", "#0x148"),
            row(0x118, "cmp", "w8,#3"),
            row(0x11c, "b.eq", "#0x160"),
            row(0x120, "ret", ""),
            row(0x130, "mov", "w8,#4"),
            row(0x134, "strb", "w8,[x1,#0x17]"),
            row(0x138, "mov", "w8,#0x6f50"),
            row(0x13c, "movk", "w8,#0x7370,lsl#16"),
            row(0x140, "str", "w8,[x1]"),
            row(0x144, "b", "#0x174"),
            row(0x148, "mov", "x0,x1"),
            row(0x14c, "adrp", "x1,#0x1000"),
            row(0x150, "add", "x1,x1,#0x10"),
            row(0x154, "mov", "w2,#5"),
            row(0x158, "bl", "#0x900"),
            row(0x15c, "b", "#0x174"),
            row(0x160, "mov", "w8,#3"),
            row(0x164, "strb", "w8,[x1,#0x17]"),
            row(0x168, "mov", "w8,#0x6c41"),
            row(0x16c, "movk", "w8,#0x6c,lsl#16"),
            row(0x170, "str", "w8,[x1]"),
            row(0x174, "mov", "w0,#1"),
            row(0x178, "ret", ""),
        ])
    }

    fn input(definition_sites: Vec<Vec<Instruction>>) -> ModifierInput {
        ModifierInput {
            tokens: BTreeMap::from([(7, "blank_modifier".into()), (8, "fleet_speed".into())]),
            definition_sites,
            define: 0x800,
            category_offset: 4,
            generation_sites: 2,
            categories: CategoryInput {
                category_name: 0x100,
                string_object_size: 24,
                short_length_offset: 0x17,
                assign_literal: BTreeSet::from([0x900]),
                code: category_switch(),
                data: ReadOnlyData::new(vec![(0x1010, b"Ships\0".to_vec())]),
            },
        }
    }

    fn site(token: &str, mask: &str) -> Vec<Instruction> {
        vec![
            row(0x2000, "sub", "sp,sp,#0x20"),
            row(0x2004, "mov", &format!("w8,{mask}")),
            row(0x2008, "str", "w8,[sp,#0x4]"),
            row(0x200c, "mov", &format!("w0,{token}")),
            row(0x2010, "bl", "#0x800"),
        ]
    }

    #[test]
    fn constant_modifier_types_keep_their_masks() {
        let mut typed = site("#7", "#3");
        typed.insert(4, row(0x2010, "mov", "w1,#0x3c"));
        typed.last_mut().unwrap().address = 0x2014;
        let result = analyze(&input(vec![typed, site("#8", "#1")])).unwrap();
        assert_eq!(result.type_masks, BTreeMap::from([(0x3c, 3)]));
    }

    #[test]
    fn category_names_come_from_both_string_forms() {
        let input = input(vec![site("#7", "#1")]).categories;
        assert_eq!(category_name(&input, 1), Ok(Some("Pops".into())));
        assert_eq!(category_name(&input, 2), Ok(Some("Ships".into())));
        assert_eq!(category_name(&input, 3), Ok(Some("All".into())));
        assert_eq!(category_name(&input, 4), Ok(None));
    }

    #[test]
    fn tags_prefer_the_whole_mask_name_and_refuse_unnamed_bits() {
        let categories = BTreeMap::from([
            (1, Ok(Some("Pops".into()))),
            (2, Ok(Some("Ships".into()))),
            (3, Ok(None)),
            (4, Ok(None)),
            (5, Ok(None)),
            (6, Ok(Some("Both".into()))),
        ]);
        assert_eq!(tags(&categories, 6), Tags::Listed(vec!["Both".into()]));
        assert_eq!(
            tags(&categories, 3),
            Tags::Listed(vec!["Pops".into(), "Ships".into()])
        );
        assert_eq!(
            tags(&categories, 5),
            Tags::Unresolved(Unresolved::new("unnamed-category"))
        );
        assert_eq!(
            tags(&categories, 9),
            Tags::Unresolved(Unresolved::new("category-name"))
        );
    }

    #[test]
    fn definition_sites_give_names_runtime_tokens_and_unreadable_masks() {
        let unreadable_mask = vec![
            row(0x2000, "ldr", "w8,[x21]"),
            row(0x2004, "str", "w8,[sp,#0x4]"),
            row(0x2008, "mov", "w0,#8"),
            row(0x200c, "bl", "#0x800"),
        ];
        let input = input(vec![
            site("#7", "#3"),
            site("#99", "#1"),
            unreadable_mask,
            site("w19", "#1"),
        ]);
        let result = analyze(&input).unwrap();
        assert_eq!(
            result.sites,
            vec![
                DefinitionSite::Declared {
                    name: "blank_modifier".into(),
                    tags: Tags::Listed(vec!["All".into()]),
                },
                DefinitionSite::RuntimeToken,
                DefinitionSite::Declared {
                    name: "fleet_speed".into(),
                    tags: Tags::Unresolved(Unresolved::new("category-mask")),
                },
                DefinitionSite::RuntimeToken,
            ]
        );
        assert_eq!(result.categories.get(&4), Some(&Ok(None)));
        assert_eq!(result.generation_sites, 2);
    }
}
