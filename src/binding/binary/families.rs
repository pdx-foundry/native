//! Read the input of the modifier-family method for one registry from executable text.
use std::collections::{BTreeMap, BTreeSet};

use crate::AnalysisError;
use crate::engine::analysis::{
    discovery::{CandidateRecord, Symbol},
    families::{
        DatabaseLayout, FamilyInput, Generator, KeyStorageInput, StringFunctions, StringLayout,
    },
};

use super::super::targets::DeclarationRecipe;
use super::declarations::{Text, addresses, read_only_data, unique};
use super::language::{GENERATE_MODIFIER, code, generation_calls};

/// Import stubs keep their raw name when it does not demangle.
const NEVER_RETURN: [&str; 5] = [
    "___stack_chk_fail",
    "__Unwind_Resume",
    "___cxa_throw",
    "___cxa_rethrow",
    "_abort",
];

/// The name of a database's modifier generator.
pub(in crate::binding) fn generator(database: &str) -> String {
    format!("{database}::GenerateModifiers()")
}

/// Read the registry's database generator, its item constructor and the string functions.
///
/// `databases` names the database class of every named registry: a generation call outside all
/// of their generators is not joined to a registry.
pub(in crate::binding) fn read(
    bytes: &[u8],
    symbols: &[Symbol],
    record: &CandidateRecord,
    databases: &[&str],
    recipe: &DeclarationRecipe,
    database: DatabaseLayout,
) -> Result<FamilyInput, AnalysisError> {
    let text = Text::read(bytes, symbols)?;
    let registration = unique(symbols, GENERATE_MODIFIER[0])?;
    let calls = generation_calls(&text, symbols)?;

    let generator_of = |database: &str| -> Result<Option<(u64, u64)>, AnalysisError> {
        let starts = addresses(symbols, &generator(database));
        match starts.len() {
            0 => Ok(None),
            1 => {
                let start = *starts.first().expect("one start");
                Ok(Some((start, start + text.function_length(start))))
            }
            _ => Err(AnalysisError::InvalidRange),
        }
    };

    let mut joined: BTreeSet<u64> = BTreeSet::new();
    for database in databases {
        if let Some((start, end)) = generator_of(database)? {
            joined.extend(calls.iter().filter(|call| (start..end).contains(*call)));
        }
    }
    let unjoined_sites = calls.len() - joined.len();

    let owner = &record.owner_candidate;
    let constructors = addresses(symbols, &format!("{owner}::{owner}(int, CString const&)"));
    let mut functions: Vec<u64> = constructors.iter().copied().collect();
    let generator = match generator_of(&record.database)? {
        Some((start, end)) => {
            functions.push(start);
            Some(Generator {
                function: start,
                sites: text
                    .direct_calls(registration)
                    .into_iter()
                    .filter(|call| (start..end).contains(call))
                    .collect(),
                constructor: constructors.first().copied(),
            })
        }
        None => None,
    };

    Ok(FamilyInput {
        generator,
        unjoined_sites,
        registration,
        category_offset: recipe.dynamic_modifier_category_offset,
        database,
        strings: string_functions(symbols),
        layout: StringLayout {
            flag_byte: recipe.short_string_length_offset,
        },
        code: code(&text, &functions)?,
        data: read_only_data(bytes)?,
    })
}

/// Read only the item constructors needed by live registry key reads.
pub(in crate::binding) fn key_storage(
    bytes: &[u8],
    symbols: &[Symbol],
    record: &CandidateRecord,
    string_tag_offset: u64,
) -> Result<KeyStorageInput, AnalysisError> {
    let owner = &record.owner_candidate;
    let constructors: Vec<_> =
        addresses(symbols, &format!("{owner}::{owner}(int, CString const&)"))
            .into_iter()
            .collect();
    let text = Text::read(bytes, symbols)?;

    Ok(KeyStorageInput {
        code: code(&text, &constructors)?,
        data: read_only_data(bytes)?,
        strings: string_functions(symbols),
        layout: StringLayout {
            flag_byte: string_tag_offset,
        },
        constructors,
    })
}

pub(super) fn string_functions(symbols: &[Symbol]) -> StringFunctions {
    let named = |name: &str| addresses(symbols, name);
    let formatters: BTreeMap<u64, u64> = symbols
        .iter()
        .filter_map(|symbol| {
            let capacity = symbol
                .name
                .strip_prefix("PdxStrFmt<")?
                .split_once(">::PdxStrFmt(char const*, ...)")
                .filter(|(_, rest)| rest.is_empty())?
                .0
                .parse()
                .ok()?;
            Some((symbol.address, capacity))
        })
        .collect();
    let never_return = symbols
        .iter()
        .filter(|symbol| {
            NEVER_RETURN.contains(&symbol.name.as_str()) || symbol.name.contains("::__throw_")
        })
        .map(|symbol| symbol.address)
        .collect();

    StringFunctions {
        from_text: named("CString::CString(char const*)"),
        append_string: named("CString::operator+=(CString const&)"),
        append_text: named("CString::operator+=(char const*)"),
        formatters,
        allocators: named("CPdxCommonStringAllocator::allocate(unsigned long, void const*)"),
        array_allocators: named("operator new[](unsigned long)"),
        releases: named("CPdxCommonStringAllocator::deallocate(char*, unsigned long)"),
        lengths: named("_strlen"),
        copies: named("_memmove")
            .union(&named("_memcpy"))
            .copied()
            .collect(),
        never_return,
    }
}
