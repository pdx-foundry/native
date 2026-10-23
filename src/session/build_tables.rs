//! The tables of one build that the Ghidra export reads, for Native's developers: the engine's
//! token table, the scope types with their bits, and where a reader holds its key text and value
//! token. `examples/ghidra-tables` writes them to a build directory's `tables.json`. Not a
//! consumer API.
//!
//! ```no_run
//! use pdx_native::Native;
//! use pdx_native::internals::build_tables;
//!
//! let native = Native::open("/path/to/Stellaris")?;
//! let tables = build_tables::read(&native)?;
//! println!("{} tokens", tables.tokens.len());
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
use std::collections::BTreeMap;

use super::Native;
use super::questions::{error, scope_id};
use crate::engine::analysis::declarations::ScopeType;
use crate::{BuildId, Error, Operation, ScopeId};

/// The tables of the opened build.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BuildTables {
    /// The build that the tables were read from.
    pub build: BuildId,
    /// The literal name of each token in the engine's token table, by token identity.
    pub tokens: BTreeMap<u64, String>,
    /// Each named scope type, in bit order.
    pub scope_types: Vec<ScopeTypeBit>,
    /// Offsets from a reader.
    pub reader: ReaderWords,
}

/// A scope type, its bit in a scope mask, and its identity in public answers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScopeTypeBit {
    /// The type's bit in a scope mask.
    pub bit: usize,
    /// The engine's name for the type.
    pub name: String,
    /// The identity that `Native::scopes` and read scopes give the type.
    pub id: ScopeId,
}

/// Where a reader holds the words that a member reader tests.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReaderWords {
    /// The pointer to the text of the statement's key.
    pub key_text: u64,
    /// The token of the statement's value.
    pub value_token: u64,
}

/// Read the tables of `native`'s build. A `Native` over recorded answers is
/// `Error::Unsupported`; a build whose scope-name table cannot be read has no scope types.
pub fn read(native: &Native) -> Result<BuildTables, Error> {
    let operation = Operation::Scopes;
    native.method_result(operation, || {
        let analysis = &native.bound().analysis;
        let input = analysis.scope_input().map_err(|e| error(operation, e))?;
        let (key_text, value_token) = analysis.reader_words();

        Ok(BuildTables {
            build: native.build(),
            tokens: input.tokens,
            scope_types: scope_types(input.scope_names.unwrap_or_default()),
            reader: ReaderWords {
                key_text,
                value_token,
            },
        })
    })
}

/// The named scope types of a name table indexed by bit. A bit with no name is not a type.
fn scope_types(names: Vec<String>) -> Vec<ScopeTypeBit> {
    names
        .into_iter()
        .enumerate()
        .filter(|(_, name)| !name.is_empty())
        .map(|(bit, name)| ScopeTypeBit {
            id: scope_id(&ScopeType {
                bit,
                name: name.clone(),
            }),
            bit,
            name,
        })
        .collect()
}
