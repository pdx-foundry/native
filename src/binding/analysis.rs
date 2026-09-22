//! The static methods bound to one installation: every read checks that the executable is
//! still the one that was opened.
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

use super::{binary, installation::Installation};
use crate::engine::analysis::discovery::{SchedulerLayout, Symbol};
use crate::{AnalysisError, UnavailableReason};

pub(crate) struct BoundAnalysis {
    layout: SchedulerLayout,
    declarations: Option<&'static super::targets::DeclarationRecipe>,
    installation: Installation,
    /// The first change that a read saw. It stays, even when the original bytes come back.
    invalidated: Mutex<Option<UnavailableReason>>,
    catalog: OnceLock<Result<Catalog, AnalysisError>>,
}

struct Catalog {
    candidates: Vec<NamedCandidate>,
    symbols: Vec<Symbol>,
    strings: BTreeMap<u64, String>,
    pointers: BTreeMap<u64, u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FixtureLoader {
    pub load_entry: u64,
    pub reader_entry: u64,
    pub reader_return: u64,
    pub constructor_entry: u64,
    pub member_entry: u64,
}

/// One fresh integrity check and the immutable analysis derived from this installation.
pub(crate) struct VerifiedAnalysis<'a> {
    executable: Vec<u8>,
    catalog: &'a Catalog,
}

impl VerifiedAnalysis<'_> {
    pub(in crate::binding) fn declaration_input(
        &self,
        kind: crate::DeclarationKind,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<crate::engine::analysis::declarations::DeclarationInput, AnalysisError> {
        binary::declarations::read(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            &self.catalog.pointers,
            kind,
            recipe,
        )
    }

    fn modifier_input(
        &self,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<crate::engine::analysis::modifiers::ModifierInput, AnalysisError> {
        binary::language::modifiers(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            recipe,
        )
    }

    fn scope_input(
        &self,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<crate::engine::analysis::scopes::ScopeInput, AnalysisError> {
        binary::language::scopes(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            recipe,
        )
    }

    pub(crate) fn named_candidates(&self) -> &[NamedCandidate] {
        &self.catalog.candidates
    }

    /// Read fields for a candidate from this analysis; reject records from another source.
    pub(crate) fn field_input(
        &self,
        selection: crate::engine::analysis::discovery::CandidateRecord,
    ) -> Result<crate::engine::analysis::fields::FieldInput, AnalysisError> {
        if !self
            .catalog
            .candidates
            .iter()
            .any(|candidate| candidate.record == selection)
        {
            return Err(AnalysisError::InvalidRange);
        }
        binary::fields::read(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            selection,
        )
    }
}

impl BoundAnalysis {
    /// Locate the file reader return and the matching owner's constructor and member reader.
    pub(crate) fn fixture_loader(
        &self,
        registry: &str,
    ) -> Result<Option<FixtureLoader>, AnalysisError> {
        use crate::engine::analysis::{decode::decode_arm64, directories::Directory};

        let verified = self.verified()?;
        let candidate = |name: &str| {
            let mut matching = verified
                .named_candidates()
                .iter()
                .filter(|candidate| candidate.directory == Directory::Named(name.into()));
            match (matching.next(), matching.next()) {
                (Some(candidate), None) => Some(candidate),
                _ => None,
            }
        };
        let Some(selected) = candidate(registry) else {
            return Ok(None);
        };
        let Some(address) = selected
            .record
            .address
            .strip_prefix("0x")
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        else {
            return Ok(None);
        };
        let expected_reader = format!(
            "TSingleObjectGameDatabase<{}, {}, false>::LoadFromReader(CReader&, bool)",
            selected.record.database, selected.record.owner_candidate
        );
        let code = binary::code_range(&verified.executable, address, 256)?;
        let rows = decode_arm64(&code, address).map_err(|_| AnalysisError::InvalidRange)?;
        let matches: Vec<_> =
            rows.windows(3)
                .filter_map(|window| {
                    let [call, after, cleanup] = window else {
                        return None;
                    };
                    let target = call
                        .operands
                        .strip_prefix("#0x")
                        .and_then(|hex| u64::from_str_radix(hex, 16).ok())?;
                    (call.operation == "bl"
                        && after.operation == "mov"
                        && after.operands == "x0,sp"
                        && cleanup.operation == "bl"
                        && verified.catalog.symbols.iter().any(|symbol| {
                            symbol.address == target && symbol.name == expected_reader
                        }))
                    .then_some((address, target, after.address))
                })
                .collect();
        let Some((load_entry, reader_entry, reader_return)) =
            matches.first().copied().filter(|_| matches.len() == 1)
        else {
            return Ok(None);
        };
        let owner = &selected.record.owner_candidate;
        let member_name = format!("{owner}::ReadMember(CReader&, int)");
        let mut members = verified
            .catalog
            .symbols
            .iter()
            .filter(|symbol| symbol.name == member_name);
        let (Some(member), None) = (members.next(), members.next()) else {
            return Ok(None);
        };
        let constructor_name = format!("{owner}::{owner}(int, CString const&)");
        let constructors: Vec<_> = verified
            .catalog
            .symbols
            .iter()
            .filter(|symbol| symbol.name == constructor_name)
            .map(|symbol| symbol.address)
            .collect();
        let Some(next_symbol) = verified
            .catalog
            .symbols
            .iter()
            .filter(|symbol| symbol.address > reader_entry)
            .map(|symbol| symbol.address)
            .min()
        else {
            return Ok(None);
        };
        let Some(length) = next_symbol.checked_sub(reader_entry) else {
            return Ok(None);
        };
        if length == 0 || length > 16 * 1024 {
            return Ok(None);
        }
        let reader_code = binary::code_range(&verified.executable, reader_entry, length)?;
        let reader_rows =
            decode_arm64(&reader_code, reader_entry).map_err(|_| AnalysisError::InvalidRange)?;
        let called: std::collections::BTreeSet<_> = reader_rows
            .iter()
            .filter(|row| row.operation == "bl")
            .filter_map(|row| row.operands.strip_prefix("#0x"))
            .filter_map(|hex| u64::from_str_radix(hex, 16).ok())
            .filter(|address| constructors.contains(address))
            .collect();
        let Some(&constructor_entry) = called.iter().next().filter(|_| called.len() == 1) else {
            return Ok(None);
        };
        Ok(Some(FixtureLoader {
            load_entry,
            reader_entry,
            reader_return,
            constructor_entry,
            member_entry: member.address,
        }))
    }

    /// Derive string storage from the root dispatch's proven reader arguments.
    pub(crate) fn fixture_string_fields(
        &self,
        registry: &str,
    ) -> Result<Vec<crate::protocol::observation::FixtureOutcomeFieldBinding>, AnalysisError> {
        use crate::engine::analysis::{
            directories::Directory,
            fields::{self, ReaderJoin, Value},
        };

        let verified = self.verified()?;
        let mut matching = verified
            .named_candidates()
            .iter()
            .filter(|candidate| candidate.directory == Directory::Named(registry.into()));
        let (Some(candidate), None) = (matching.next(), matching.next()) else {
            return Ok(Vec::new());
        };
        let input = verified.field_input(candidate.record.clone())?;
        let result = fields::analyze(&input).map_err(|_| AnalysisError::InvalidRange)?;
        Ok(result
            .fields
            .iter()
            .filter_map(|field| {
                if field.readers.len() != 1
                    || field
                        .paths
                        .iter()
                        .any(|&path| !result.paths[path].conditions.is_empty())
                {
                    return None;
                }
                let ReaderJoin::Joined { callee, arguments } = &field.readers[0] else {
                    return None;
                };
                if callee != "CReader::Read(CString&, bool)"
                    || arguments.get("x0") != Some(&Value::Reader(0))
                    || arguments.get("x8") != Some(&Value::Constant(field.token))
                {
                    return None;
                }
                let Some(Value::Owner(offset)) = arguments.get("x1") else {
                    return None;
                };
                let (Ok(token), Ok(storage_offset)) =
                    (u64::try_from(field.token), u64::try_from(*offset))
                else {
                    return None;
                };
                Some(crate::protocol::observation::FixtureOutcomeFieldBinding {
                    token,
                    name: field.name.clone(),
                    storage_offset,
                })
            })
            .collect())
    }

    pub(super) fn new(
        layout: SchedulerLayout,
        declarations: Option<&'static super::targets::DeclarationRecipe>,
        installation: Installation,
    ) -> Self {
        Self {
            layout,
            declarations,
            installation,
            invalidated: Mutex::new(None),
            catalog: OnceLock::new(),
        }
    }

    pub(crate) fn executable(&self) -> Result<Vec<u8>, AnalysisError> {
        let mut invalidated = self
            .invalidated
            .lock()
            .expect("static executable integrity lock");
        if let Some(reason) = &*invalidated {
            return Err(AnalysisError::Unavailable {
                reasons: vec![reason.clone()],
            });
        }
        // The full-file hash pins every byte of the selected slice identified at open.
        let bytes = self.installation.executable_bytes();
        let bytes = match bytes {
            Ok(bytes) => bytes,
            Err(reason) => {
                *invalidated = Some(reason.clone());
                return Err(AnalysisError::Unavailable {
                    reasons: vec![reason],
                });
            }
        };
        Ok(bytes)
    }

    pub(crate) fn verified(&self) -> Result<VerifiedAnalysis<'_>, AnalysisError> {
        let executable = self.executable()?;
        let catalog = self
            .catalog
            .get_or_init(|| self.build_catalog(&executable))
            .as_ref()
            .map_err(Clone::clone)?;
        Ok(VerifiedAnalysis {
            executable,
            catalog,
        })
    }
}

#[cfg(test)]
mod tests;

impl BoundAnalysis {
    /// Resolve public field readers from the same executable analysis used by
    /// `Native::registry_fields`.
    pub(crate) fn registry_fields(
        &self,
        registry: &str,
    ) -> Result<Option<Vec<crate::Field>>, AnalysisError> {
        use crate::engine::analysis::{directories::Directory, fields};

        let verified = self.verified()?;
        let mut matching = verified
            .named_candidates()
            .iter()
            .filter(|candidate| candidate.directory == Directory::Named(registry.into()));
        let (Some(candidate), None) = (matching.next(), matching.next()) else {
            return Ok(None);
        };
        let input = verified.field_input(candidate.record.clone())?;
        let result = fields::analyze(&input).map_err(|_| AnalysisError::InvalidRange)?;
        Ok(Some(crate::session::questions::normalized_fields(&result)))
    }
}

/// One template candidate and the content directory that its constructors establish.
#[derive(Debug, Clone)]
pub(crate) struct NamedCandidate {
    pub record: crate::engine::analysis::discovery::CandidateRecord,
    pub directory: crate::engine::analysis::directories::Directory,
}

impl BoundAnalysis {
    fn build_catalog(&self, bytes: &[u8]) -> Result<Catalog, AnalysisError> {
        use crate::engine::analysis::{directories, discovery};
        let input = binary::discovery::read(bytes, &self.layout)?;
        let records = discovery::candidates(&input.symbols);
        let constructors = binary::constructors::read(bytes, &input, &records)?;
        let anchors = binary::constructors::anchors(&input);
        let arguments: Vec<Vec<directories::Argument>> = records
            .iter()
            .map(|record| {
                constructors
                    .get(&record.database)
                    .into_iter()
                    .flatten()
                    .flat_map(|body| directories::arguments(body, &anchors, &input.strings))
                    .collect()
            })
            .collect();
        let needs_globals = arguments
            .iter()
            .flatten()
            .any(|a| matches!(a, directories::Argument::Global(_)));
        let globals = if needs_globals {
            let initializers = binary::constructors::initializers(bytes, &input)?;
            directories::globals(&initializers, &anchors, &input.strings)
        } else {
            Default::default()
        };
        let candidates = records
            .into_iter()
            .zip(arguments)
            .map(|(record, arguments)| NamedCandidate {
                directory: directories::directory(&arguments, &globals),
                record,
            })
            .collect();
        Ok(Catalog {
            candidates,
            symbols: input.symbols,
            strings: input.strings,
            pointers: input.pointers,
        })
    }

    pub(crate) fn has_declarations_method(&self) -> bool {
        self.declarations.is_some()
    }

    pub(crate) fn declaration_input(
        &self,
        kind: crate::DeclarationKind,
    ) -> Result<crate::engine::analysis::declarations::DeclarationInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.declaration_input(kind, recipe)
    }

    pub(crate) fn modifier_input(
        &self,
    ) -> Result<crate::engine::analysis::modifiers::ModifierInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.modifier_input(recipe)
    }

    pub(crate) fn scope_input(
        &self,
    ) -> Result<crate::engine::analysis::scopes::ScopeInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.scope_input(recipe)
    }
}
