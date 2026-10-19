//! Collect shared modifier readers from executable pointer slots and bind the reference database.
//!
//! The name object that a modifier block reads `name` through is stated to derive from the
//! recipe's `modifier_name_base`; [`name_reads`] derives the rest from the executable's
//! type information.
use super::{declarations::Text, numeric::NUMERIC_MODIFIER_MEMBER, references::Image};
use crate::binding::{analysis::NamedCandidate, targets::DeclarationRecipe};
use crate::engine::analysis::{
    declarations::Function,
    decode::{Instruction, decode_arm64},
    directories::Directory,
    discovery::Symbol,
    evaluate::ReadOnlyData,
    fields::{self, ConcreteReader},
    modifier_blocks::ModifierBlockInput,
    modifier_blocks::parameters::{self, ParameterList},
    modifier_blocks::reference,
    numeric::lexer::page_address,
    stop::Unresolved,
};
use crate::{AnalysisError, BlockFamily};
use std::collections::{BTreeMap, BTreeSet};

/// The vtable address points whose anchored slot holds a `family` anchor of the recipe, with the
/// readers of each.
pub(super) fn family_points(
    image: &Image<'_>,
    names: &BTreeMap<u64, String>,
    recipe: &DeclarationRecipe,
    family: BlockFamily,
) -> BTreeMap<u64, ConcreteReader> {
    let persistent = &recipe.persistent;
    let name = |address: u64| names.get(&address).cloned();
    let mut points = BTreeMap::new();

    for (&slot, &target) in image.pointers {
        let Some(held) = names.get(&target) else {
            continue;
        };

        for anchor in persistent
            .families
            .iter()
            .filter(|anchor| anchor.family == family && anchor.symbol == held)
        {
            let Some(point) = slot.checked_sub(persistent.anchor_offset(anchor.slot)) else {
                continue;
            };

            if let Some(reader) =
                super::receivers::concrete_reader(persistent, image.pointers, point, &name)
                    .filter(|reader| reader.family == family)
            {
                points.insert(point, reader);
            }
        }
    }

    points
}

pub(in crate::binding) fn read(
    image: &Image<'_>,
    candidates: &[NamedCandidate],
    recipe: &DeclarationRecipe,
) -> Result<ModifierBlockInput, AnalysisError> {
    let text = Text::read(image.bytes, image.symbols)?;
    let names =
        super::references::names(image.symbols, image.pointers, image.imports, image.strings);
    let points = family_points(image, &names, recipe, BlockFamily::Modifier);
    let mut functions = BTreeMap::new();
    let mut pending: BTreeSet<_> = points
        .values()
        .map(|reader| reader.member.clone())
        .collect();
    pending.insert(recipe.modifier_reference_member.into());
    pending.insert(NUMERIC_MODIFIER_MEMBER.into());
    while let Some(name) = pending.pop_first() {
        let addresses: BTreeSet<_> = image
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.address)
            .collect();
        if addresses.len() != 1 {
            continue;
        }
        let address = *addresses.first().unwrap();
        if functions.contains_key(&address) {
            continue;
        }
        let Ok((address, code)) = text.function(address) else {
            continue;
        };
        let rows = decode_arm64(code, address).map_err(|_| AnalysisError::InvalidRange)?;
        for row in &rows {
            if !matches!(row.operation.as_str(), "bl" | "b") {
                continue;
            }
            let Some(callee) = crate::engine::analysis::decode::number(&row.operands)
                .and_then(|target| names.get(&target))
            else {
                continue;
            };
            if callee.ends_with("::ReadMember(CReader&, int)")
                || callee.ends_with("::TryReadMember(CReader&, int)")
            {
                pending.insert(callee.clone());
            }
        }
        functions.insert(
            address,
            Function {
                address,
                code: code.to_vec(),
            },
        );
    }
    let tokens = text.token_table(image.symbols, image.strings)?;
    let key_readers = super::fields::key_readers(
        image.symbols,
        &tokens,
        &recipe.persistent,
        recipe.reader_token_offset,
    )?;
    let base = super::numeric::modifier_input(&text, image.symbols, &names, recipe)?;
    let name_reads = name_read_functions(image, &text, &base, recipe);
    let directories = super::references::database_directories(
        &text,
        image,
        candidates,
        BTreeSet::from([recipe.modifier_reference_database.to_owned()]),
    );
    let target = match directories.get(recipe.modifier_reference_database) {
        Some(Directory::Named(name)) => crate::ReferenceTarget::Registry { name: name.clone() },
        _ => crate::ReferenceTarget::Unresolved,
    };
    let database = format!("{}::_pInstance", recipe.modifier_reference_database);
    let null = "TPdxNullObject<CStaticModifier>::_pInstance";
    let aliases = [
        (database.as_str(), "database"),
        (null, "null_item"),
        (NUMERIC_MODIFIER_MEMBER, "base_member"),
        ("CString::CString(char const*)", "key_string"),
        ("CToken::GetFloat() const", "fixed_value"),
        ("StringToFixedPoint(char const*)", "fixed_conversion"),
        ("_memcmp", "compare_bytes"),
    ];
    let semantic_names = names
        .iter()
        .map(|(&address, name)| {
            (
                address,
                aliases
                    .iter()
                    .find_map(|(symbol, alias)| (*symbol == name).then_some(*alias))
                    .unwrap_or(name)
                    .to_owned(),
            )
        })
        .collect();
    let reference_input = reference::Input {
        member: text.unique_body(image.symbols, recipe.modifier_reference_member)?,
        conversion: text.unique_body(image.symbols, "CToken::GetFloat() const")?,
        names: semantic_names,
        key_text_offset: recipe.reader_token_offset + recipe.persistent.token_text,
        value_token_offset: recipe.persistent.value_token,
        token_text_offset: recipe.persistent.token_text,
    };
    let reference = reference::analyze(&reference_input).map(|()| target);
    let mut serializer_constructors = BTreeMap::new();
    let mut parameter_lists = Vec::new();
    for symbol in image
        .symbols
        .iter()
        .filter(|symbol| symbol.name == recipe.modifier_serializer_constructor)
    {
        let Ok((address, code)) = text.function(symbol.address) else {
            continue;
        };
        let Ok(rows) = decode_arm64(code, address) else {
            continue;
        };
        let Some(slot) = fields::serializer_owner_slot(&rows) else {
            continue;
        };
        serializer_constructors.insert(address, slot);
        parameter_lists.extend(parameter_list(
            image, &text, &names, &tokens, recipe, &rows, slot,
        ));
    }
    Ok(ModifierBlockInput {
        points,
        functions,
        symbols: image.symbols.to_vec(),
        pointers: image.pointers.clone(),
        data: super::declarations::read_only_sections(image.bytes)?,
        tokens,
        serializer_constructors,
        key_readers,
        reader_token_offset: recipe.reader_token_offset,
        base_member: NUMERIC_MODIFIER_MEMBER.into(),
        base,
        references: BTreeMap::from([(recipe.modifier_reference_member.into(), reference)]),
        name_reads,
        parameter_lists,
    })
}

/// The parameter list of the serializer whose constructor is `constructor`: the member in its
/// vtable's member slot, and the entry reader in the read slot of the vtable that the member's
/// list route stores. `None` when the serializer has no member body to attach a result to.
fn parameter_list(
    image: &Image<'_>,
    text: &Text<'_>,
    names: &BTreeMap<u64, String>,
    tokens: &BTreeMap<i64, fields::Token>,
    recipe: &DeclarationRecipe,
    constructor: &[Instruction],
    wrapped: i64,
) -> Option<ParameterList> {
    let point = page_address(constructor.first()?, constructor.get(1)?)?;
    let member = *image
        .pointers
        .get(&(point + recipe.persistent.member_slot))?;
    let (member, code) = text.function(member).ok()?;
    let rows = decode_arm64(code, member).ok()?;
    let value_token = recipe.persistent.value_token;
    let list = parameters::list_route(&rows, names, wrapped, value_token).and_then(|route| {
        let entry = image
            .pointers
            .get(&(route.entry_point + recipe.persistent.read_slot))
            .and_then(|&entry| text.function(entry).ok())
            .and_then(|(entry, code)| decode_arm64(code, entry).ok())
            .ok_or(Unresolved::new("parameter-entry-body"))?;
        Ok(parameters::List {
            token: route.token,
            branch: route.branch,
            reader: names.get(&member).cloned().unwrap_or_default(),
            accepted: parameters::accepted_kinds(&entry, names, tokens, value_token)?,
        })
    });

    Some(ParameterList {
        body: member..member + code.len() as u64,
        list,
    })
}

/// The bodies of the name object's read functions, for the base member's read slot.
fn name_read_functions(
    image: &Image<'_>,
    text: &Text<'_>,
    base: &crate::engine::analysis::numeric::ModifierInput,
    recipe: &DeclarationRecipe,
) -> Result<Vec<Function>, Unresolved> {
    let tokens = crate::engine::analysis::numeric::modifier::member_tokens(base)?;
    let data = super::language::constant_data(image.bytes, image.pointers, image.bound_slots)
        .map_err(|_| name_implementations_stop())?;
    let reads = name_reads(
        image.symbols,
        image.pointers,
        &data,
        recipe.modifier_name_base,
        tokens.name_read_slot,
    )?;
    let mut functions = Vec::new();

    for address in reads {
        let (address, code) = text
            .function(address)
            .map_err(|_| name_implementations_stop())?;
        functions.push(Function {
            address,
            code: code.to_vec(),
        });
    }

    Ok(functions)
}

/// The function in the read slot of each class that a name object can have: `base` and every
/// class that derives from it through single inheritance. A class with no vtable in the image has
/// no objects, so it adds no function. Multiple inheritance from a class in the set, a second
/// vtable, or a read slot that holds no local function fails closed.
fn name_reads(
    symbols: &[Symbol],
    pointers: &BTreeMap<u64, u64>,
    data: &ReadOnlyData,
    base: &str,
    slot: u64,
) -> Result<BTreeSet<u64>, Unresolved> {
    let typeinfos: BTreeMap<u64, &str> = symbols
        .iter()
        .filter_map(|symbol| Some((symbol.address, symbol.name.strip_prefix("typeinfo for ")?)))
        .collect();
    let base = typeinfos
        .iter()
        .find_map(|(&address, &class)| (class == base).then_some(address))
        .ok_or_else(name_implementations_stop)?;
    let derived = derived_typeinfos(symbols, data, &typeinfos, base)?;
    let mut reads = BTreeSet::new();

    for class in derived.iter().map(|typeinfo| typeinfos[typeinfo]) {
        let vtable = format!("vtable for {class}");
        if !symbols.iter().any(|symbol| symbol.name == vtable) {
            continue;
        }

        let group = super::families::vtable_group(symbols, data, class)
            .filter(|group| group.address_points.len() == 1)
            .ok_or_else(name_implementations_stop)?;
        let read = group
            .primary
            .get(&slot)
            .filter(|&&target| pointers.get(&(group.address_points[&0] + slot)) == Some(&target))
            .ok_or_else(name_implementations_stop)?;
        reads.insert(*read);
    }

    Ok(reads)
}

/// `base` and the typeinfo of every class that derives from it through single inheritance, whose
/// type information names its one base at `+0x10`. Any other word of a type-information record
/// that points into the set is a multiple-inheritance base, which fails closed.
fn derived_typeinfos(
    symbols: &[Symbol],
    data: &ReadOnlyData,
    typeinfos: &BTreeMap<u64, &str>,
    base: u64,
) -> Result<BTreeSet<u64>, Unresolved> {
    let mut derived = BTreeSet::from([base]);

    loop {
        let found: Vec<_> = typeinfos
            .keys()
            .filter(|typeinfo| !derived.contains(typeinfo))
            .filter(|&&typeinfo| {
                data.read(typeinfo + 0x10, 8)
                    .is_some_and(|parent| derived.contains(&parent))
            })
            .copied()
            .collect();
        if found.is_empty() {
            break;
        }

        derived.extend(found);
    }

    let ends: BTreeSet<u64> = symbols.iter().map(|symbol| symbol.address).collect();
    for &typeinfo in typeinfos.keys() {
        let end = ends
            .range(typeinfo + 1..)
            .next()
            .copied()
            .unwrap_or(typeinfo);
        let other_base = (typeinfo + 0x18..end)
            .step_by(8)
            .any(|at| data.read(at, 8).is_some_and(|word| derived.contains(&word)));
        if other_base {
            return Err(name_implementations_stop());
        }
    }

    Ok(derived)
}

fn name_implementations_stop() -> Unresolved {
    Unresolved::new("modifier-name-implementations")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: u64 = 0x9000;
    const DERIVED: u64 = 0x9020;
    const FURTHER: u64 = 0x9040;

    fn symbol(address: u64, name: &str) -> Symbol {
        Symbol {
            address,
            name: name.into(),
        }
    }

    /// `CDerived` derives from the vtable-less `CBase`, and `CFurther` from `CDerived`. Each
    /// vtable holds a read function at slot `0x10`.
    fn image() -> (Vec<Symbol>, BTreeMap<u64, u64>, BTreeMap<u64, u64>) {
        let symbols = vec![
            symbol(0x8000, "vtable for CDerived"),
            symbol(0x8028, "vtable for CFurther"),
            symbol(0x8050, "end of vtables"),
            symbol(BASE, "typeinfo for CBase"),
            symbol(DERIVED, "typeinfo for CDerived"),
            symbol(FURTHER, "typeinfo for CFurther"),
            symbol(0x9060, "end of typeinfo"),
        ];
        let mut words = BTreeMap::new();
        for (vtable, typeinfo, read) in [(0x8000, DERIVED, 0x1000), (0x8028, FURTHER, 0x2000)] {
            words.insert(vtable, 0);
            words.insert(vtable + 8, typeinfo);
            words.insert(vtable + 0x10, 0x100);
            words.insert(vtable + 0x18, 0x104);
            words.insert(vtable + 0x20, read);
        }
        words.insert(DERIVED + 0x10, BASE);
        words.insert(FURTHER + 0x10, DERIVED);
        let pointers = words
            .iter()
            .filter(|&(_, &word)| word != 0)
            .map(|(&slot, &word)| (slot, word))
            .collect();
        (symbols, words, pointers)
    }

    fn reads(
        symbols: &[Symbol],
        words: &BTreeMap<u64, u64>,
        pointers: &BTreeMap<u64, u64>,
    ) -> Result<BTreeSet<u64>, Unresolved> {
        let data = ReadOnlyData::new(vec![]).with_words(words);
        name_reads(symbols, pointers, &data, "CBase", 0x10)
    }

    #[test]
    fn every_single_inheritance_descendant_gives_its_read_slot() {
        let (symbols, words, pointers) = image();

        assert_eq!(
            reads(&symbols, &words, &pointers).unwrap(),
            BTreeSet::from([0x1000, 0x2000])
        );
    }

    #[test]
    fn an_unresolved_read_slot_or_a_multiple_inheritance_base_fails_closed() {
        let (symbols, words, mut pointers) = image();
        pointers.remove(&0x8048);
        assert!(reads(&symbols, &words, &pointers).is_err());

        let (mut symbols, mut words, pointers) = image();
        symbols.push(symbol(0x9060, "typeinfo for CMixed"));
        symbols.push(symbol(0x9090, "end of mixed typeinfo"));
        words.insert(0x9070, 0x2_0000_0000);
        words.insert(0x9078, 0x7000);
        words.insert(0x9088, DERIVED);
        assert!(reads(&symbols, &words, &pointers).is_err());
    }
}
