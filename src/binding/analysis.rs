//! The static methods bound to one installation: every read checks that the executable is
//! still the one that was opened.
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};

use super::binary::families::FamilyIndex;
use super::{binary, installation::Installation};
use crate::engine::analysis::decode::decode_arm64;
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::families::DatabaseLayout;
use crate::{AnalysisError, UnavailableReason};

pub(crate) struct BoundAnalysis {
    declarations: Option<&'static super::targets::DeclarationRecipe>,
    /// Where a template database holds its items, when the build has a template layout.
    database: Option<DatabaseLayout>,
    installation: Installation,
    /// The first change that a read saw. It stays, even when the original bytes come back.
    invalidated: Mutex<Option<UnavailableReason>>,
    catalog: OnceLock<Result<Catalog, AnalysisError>>,
    /// Derived from the catalog's executable; every read checks the executable first.
    families: OnceLock<Result<FamilyIndex, AnalysisError>>,
}

struct Catalog {
    candidates: Vec<NamedCandidate>,
    symbols: Vec<Symbol>,
    strings: BTreeMap<u64, String>,
    pointers: BTreeMap<u64, u64>,
    bound_slots: std::collections::BTreeSet<u64>,
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
    persistent: Option<&'static super::targets::recipes::PersistentRecipe>,
}

impl VerifiedAnalysis<'_> {
    /// The address of the one symbol with this demangled name.
    pub(in crate::binding) fn symbol(&self, name: &str) -> Result<u64, String> {
        let addresses: std::collections::BTreeSet<_> = self
            .catalog
            .symbols
            .iter()
            .filter(|symbol| symbol.name == name)
            .map(|symbol| symbol.address)
            .collect();
        match (addresses.first(), addresses.len()) {
            (Some(address), 1) => Ok(*address),
            (_, 0) => Err(format!("no symbol {name}")),
            _ => Err(format!("more than one symbol {name}")),
        }
    }

    /// The one call to the registry database's `LoadFromReader` in the first 256 bytes of its
    /// loader, followed by `mov x0, sp` and a second call. Returns the reader's entry and the
    /// loader address where the reader returns.
    fn loader_reader_call(
        &self,
        load_entry: u64,
        record: &crate::engine::analysis::discovery::CandidateRecord,
    ) -> Result<Option<(u64, u64)>, AnalysisError> {
        let expected_reader = format!(
            "TSingleObjectGameDatabase<{}, {}, false>::LoadFromReader(CReader&, bool)",
            record.database, record.owner_candidate
        );
        let code = binary::code_range(&self.executable, load_entry, 256)?;
        let rows = decode_arm64(&code, load_entry).map_err(|_| AnalysisError::InvalidRange)?;
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
                        && self.catalog.symbols.iter().any(|symbol| {
                            symbol.address == target && symbol.name == expected_reader
                        }))
                    .then_some((target, after.address))
                })
                .collect();

        Ok(matches.first().copied().filter(|_| matches.len() == 1))
    }

    /// The one `owner(int, CString const&)` constructor that the file reader calls directly.
    /// The scan runs from the reader's entry to the next symbol, which must be at most 16 KiB
    /// away.
    fn reader_constructor_call(
        &self,
        reader_entry: u64,
        owner: &str,
    ) -> Result<Option<u64>, AnalysisError> {
        let constructor_name = format!("{owner}::{owner}(int, CString const&)");
        let constructors: Vec<_> = self
            .catalog
            .symbols
            .iter()
            .filter(|symbol| symbol.name == constructor_name)
            .map(|symbol| symbol.address)
            .collect();
        let Some(next_symbol) = self
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

        let reader_code = binary::code_range(&self.executable, reader_entry, length)?;
        let reader_rows =
            decode_arm64(&reader_code, reader_entry).map_err(|_| AnalysisError::InvalidRange)?;
        let called: std::collections::BTreeSet<_> = reader_rows
            .iter()
            .filter(|row| row.operation == "bl")
            .filter_map(|row| row.operands.strip_prefix("#0x"))
            .filter_map(|hex| u64::from_str_radix(hex, 16).ok())
            .filter(|address| constructors.contains(address))
            .collect();

        Ok(called.first().copied().filter(|_| called.len() == 1))
    }

    /// Derive the loaded arrays from the executable readers before authorizing live access.
    /// `string_tag_offset` is the bound CString length/long-form flag byte offset.
    pub(in crate::binding) fn modifier_table_layout(
        &self,
        string_tag_offset: u64,
    ) -> Result<crate::engine::analysis::modifier_table::Layout, String> {
        let input = binary::modifier_table::read(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.pointers,
            string_tag_offset,
        )
        .map_err(|error| format!("modifier table input: {error}"))?;
        crate::engine::analysis::modifier_table::derive(&input).map_err(|reason| {
            format!(
                "modifier table layout was not established: {}",
                reason.reason
            )
        })
    }

    /// Establish the key's item-relative offset from the selected registry's constructor.
    pub(in crate::binding) fn registry_key_offset(
        &self,
        candidate: &NamedCandidate,
        string_tag_offset: u64,
    ) -> Result<u64, String> {
        let input = binary::families::key_storage(
            &self.executable,
            &self.catalog.symbols,
            &candidate.record.owner_candidate,
            string_tag_offset,
        )
        .map_err(|error| format!("item key storage analysis failed: {error}"))?;
        crate::engine::analysis::families::item_key_offset(&input)
            .map_err(|reason| format!("item key storage was not established at {}", reason.reason))
    }

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

    /// Every generation call, its joins to the named registries, and each registry's code.
    fn family_index(
        &self,
        recipe: &super::targets::DeclarationRecipe,
        database: DatabaseLayout,
    ) -> Result<FamilyIndex, AnalysisError> {
        use crate::engine::analysis::{directories::Directory, modifiers};

        let registries: Vec<_> = self
            .named_candidates()
            .iter()
            .filter_map(|candidate| match &candidate.directory {
                Directory::Named(name) => Some(binary::families::NamedRegistry {
                    name,
                    database: &candidate.record.database,
                    owner: &candidate.record.owner_candidate,
                }),
                _ => None,
            })
            .collect();

        let modifier_input = self.modifier_input(recipe)?;
        let declarations = modifiers::analyze(&modifier_input).map_err(AnalysisError::Input)?;
        let runtime_definitions: Vec<u64> = declarations
            .sites
            .iter()
            .zip(&modifier_input.definition_sites)
            .filter(|(site, _)| **site == modifiers::DefinitionSite::RuntimeToken)
            .filter_map(|(_, rows)| rows.last().map(|call| call.address))
            .collect();
        let table = self
            .modifier_table_layout(recipe.short_string_length_offset)
            .ok();

        binary::families::index(
            &self.executable,
            &self.catalog.symbols,
            &registries,
            binary::families::KnownFacts {
                runtime_definitions: &runtime_definitions,
                type_masks: declarations.type_masks,
                table,
                pointers: &self.catalog.pointers,
                bound_slots: &self.catalog.bound_slots,
            },
            recipe,
            database,
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

    fn localization_input(
        &self,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<crate::engine::analysis::localization::LocalizationInput, AnalysisError> {
        binary::language::localization(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            &self.catalog.pointers,
            &self.catalog.bound_slots,
            recipe,
        )
    }

    fn callbacks_input(
        &self,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<crate::engine::analysis::callbacks::CallbacksInput, AnalysisError> {
        binary::callbacks::callbacks(
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
            &self.catalog.pointers,
            &self.catalog.bound_slots,
            selection,
            self.persistent,
        )
    }
}

impl BoundAnalysis {
    pub(crate) fn defines_input(
        &self,
    ) -> Result<crate::engine::analysis::defines::DefineInput, AnalysisError> {
        if self.declarations.is_none() {
            return Err(AnalysisError::InvalidRange);
        }
        let verified = self.verified()?;
        binary::defines::read(
            &verified.executable,
            &verified.catalog.symbols,
            &verified.catalog.strings,
        )
    }

    /// Locate the file reader return and the matching owner's constructor and member reader.
    pub(crate) fn fixture_loader(
        &self,
        registry: &str,
    ) -> Result<Option<FixtureLoader>, AnalysisError> {
        let verified = self.verified()?;
        let Some(selected) = unique_named_candidate(verified.named_candidates(), registry) else {
            return Ok(None);
        };
        let Some(load_entry) = selected
            .record
            .address
            .strip_prefix("0x")
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        else {
            return Ok(None);
        };
        let Some((reader_entry, reader_return)) =
            verified.loader_reader_call(load_entry, &selected.record)?
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
        let Some(constructor_entry) = verified.reader_constructor_call(reader_entry, owner)? else {
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

    /// Bind root field tokens, with string storage only when reader arguments prove it.
    pub(crate) fn fixture_fields(
        &self,
        registry: &str,
    ) -> Result<Vec<crate::protocol::observation::FixtureOutcomeFieldBinding>, AnalysisError> {
        let verified = self.verified()?;
        let Some(candidate) = unique_named_candidate(verified.named_candidates(), registry) else {
            return Ok(Vec::new());
        };
        let input = verified.field_input(candidate.record.clone())?;
        let result = crate::engine::analysis::fields::analyze(&input)
            .map_err(|_| AnalysisError::InvalidRange)?;

        Ok(result
            .fields
            .iter()
            .filter_map(|field| {
                Some(crate::protocol::observation::FixtureOutcomeFieldBinding {
                    token: u64::try_from(field.token).ok()?,
                    name: field.name.clone(),
                    storage_offset: string_field_binding(field, &result.paths)
                        .and_then(|binding| binding.storage_offset),
                })
            })
            .collect())
    }

    pub(super) fn new(
        declarations: Option<&'static super::targets::DeclarationRecipe>,
        database: Option<DatabaseLayout>,
        installation: Installation,
    ) -> Self {
        Self {
            declarations,
            database,
            installation,
            invalidated: Mutex::new(None),
            catalog: OnceLock::new(),
            families: OnceLock::new(),
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
            persistent: self.declarations.map(|recipe| &recipe.persistent),
            executable,
            catalog,
        })
    }
}

/// The storage binding of a field that the root reader always reads as one `CString`: a single
/// reader join with no path condition, a direct `CReader::Read(CString&, bool)` of the root
/// reader with the field's token, into owner storage.
fn string_field_binding(
    field: &crate::engine::analysis::fields::RootField,
    paths: &[crate::engine::analysis::fields::TokenPath],
) -> Option<crate::protocol::observation::FixtureOutcomeFieldBinding> {
    use crate::engine::analysis::fields::{ReaderJoin, Value};

    let [
        ReaderJoin::Joined {
            callee, arguments, ..
        },
    ] = field.readers.as_slice()
    else {
        return None;
    };
    let unconditional = field
        .paths
        .iter()
        .all(|&path| paths[path].conditions.is_empty());
    if !unconditional
        || callee != "CReader::Read(CString&, bool)"
        || arguments.get("x0") != Some(&Value::Reader(0))
        || arguments.get("x8") != Some(&Value::Constant(field.token))
    {
        return None;
    }
    let Some(Value::Owner(offset)) = arguments.get("x1") else {
        return None;
    };
    let (Ok(token), Ok(storage_offset)) = (u64::try_from(field.token), u64::try_from(*offset))
    else {
        return None;
    };

    Some(crate::protocol::observation::FixtureOutcomeFieldBinding {
        token,
        name: field.name.clone(),
        storage_offset: Some(storage_offset),
    })
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
        use crate::engine::analysis::fields;

        let verified = self.verified()?;
        let Some(candidate) = unique_named_candidate(verified.named_candidates(), registry) else {
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

/// The one candidate whose constructors establish the content directory `directory`, or `None`
/// when no candidate or more than one does.
pub(crate) fn unique_named_candidate<'a>(
    candidates: &'a [NamedCandidate],
    directory: &str,
) -> Option<&'a NamedCandidate> {
    use crate::engine::analysis::directories::Directory;

    let mut matching = candidates.iter().filter(
        |candidate| matches!(&candidate.directory, Directory::Named(name) if name == directory),
    );
    let (Some(candidate), None) = (matching.next(), matching.next()) else {
        return None;
    };

    Some(candidate)
}

impl BoundAnalysis {
    fn build_catalog(&self, bytes: &[u8]) -> Result<Catalog, AnalysisError> {
        use crate::engine::analysis::{directories, discovery};
        let input = binary::discovery::read(bytes)?;
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
            bound_slots: input.bound_slots,
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

    pub(crate) fn grammar_input(
        &self,
        kind: crate::DeclarationKind,
    ) -> Result<crate::engine::analysis::grammar::GrammarInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        let verified = self.verified()?;
        binary::grammar::read(
            &verified.executable,
            &verified.catalog.symbols,
            &verified.catalog.strings,
            &verified.catalog.pointers,
            &verified.catalog.bound_slots,
            kind,
            recipe,
        )
    }

    pub(crate) fn modifier_input(
        &self,
    ) -> Result<crate::engine::analysis::modifiers::ModifierInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.modifier_input(recipe)
    }

    /// Every generation call, its joins to the named registries, and each registry's code.
    pub(crate) fn family_index(&self) -> Result<&FamilyIndex, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        let database = self.database.ok_or(AnalysisError::InvalidRange)?;
        let verified = self.verified()?;
        self.families
            .get_or_init(|| verified.family_index(recipe, database))
            .as_ref()
            .map_err(Clone::clone)
    }

    pub(crate) fn scope_input(
        &self,
    ) -> Result<crate::engine::analysis::scopes::ScopeInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.scope_input(recipe)
    }

    pub(crate) fn localization_input(
        &self,
    ) -> Result<crate::engine::analysis::localization::LocalizationInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.localization_input(recipe)
    }

    pub(crate) fn callbacks_input(
        &self,
    ) -> Result<crate::engine::analysis::callbacks::CallbacksInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.callbacks_input(recipe)
    }
}
