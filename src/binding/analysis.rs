//! The static methods bound to one installation: every read checks that the executable is
//! still the one that was opened.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

use super::binary::families::FamilyIndex;
use super::{binary, installation::Installation};
use crate::engine::analysis::decode::decode_arm64;
use crate::engine::analysis::discovery::Symbol;
use crate::engine::analysis::families::DatabaseLayout;
use crate::engine::analysis::references::{self, ReferenceFacts};
use crate::engine::analysis::{declarations::DeclarationResult, grammar::GrammarInput};
use crate::{AnalysisError, UnavailableReason};
mod fixtures;

pub(crate) struct BoundAnalysis {
    declarations: Option<&'static super::targets::DeclarationRecipe>,
    /// Where a template database holds its items, when the build has a template layout.
    database: Option<DatabaseLayout>,
    installation: Installation,
    /// The first change that a read or the binding's integrity check saw. It stays, even when
    /// the original bytes come back.
    invalidated: Arc<Mutex<Option<UnavailableReason>>>,
    catalog: OnceLock<Result<Catalog, AnalysisError>>,
    /// Derived from the catalog's executable; every read checks the executable first.
    families: OnceLock<Result<FamilyIndex, AnalysisError>>,
    /// Derived from the catalog's executable; every read checks the executable first.
    references: OnceLock<Result<ReferenceFacts, AnalysisError>>,
    numeric: OnceLock<Result<crate::engine::analysis::numeric::NumericFacts, AnalysisError>>,
    modifier_blocks: OnceLock<
        Result<crate::engine::analysis::modifier_blocks::ModifierBlockFacts, AnalysisError>,
    >,
    weight_blocks:
        OnceLock<Result<crate::engine::analysis::weight_blocks::WeightBlockFacts, AnalysisError>>,
    triggered_modifiers: OnceLock<
        Result<crate::engine::analysis::modifier_blocks::triggered::TriggeredFacts, AnalysisError>,
    >,
    scoped_numeric: OnceLock<Result<crate::engine::analysis::scoped_numeric::Facts, AnalysisError>>,
    block_facts: OnceLock<Result<BlockFacts, AnalysisError>>,
    modifier_nodes: OnceLock<
        Result<crate::engine::analysis::modifier_nodes::ModifierNodeResult, AnalysisError>,
    >,
    category_input:
        OnceLock<Result<crate::engine::analysis::modifiers::CategoryInput, AnalysisError>>,
    names: OnceLock<crate::engine::analysis::names::NameInput>,
    /// One immutable input per family; callers verify the executable before each access.
    grammar: [OnceLock<Result<(GrammarInput, DeclarationResult), AnalysisError>>; 2],
}

struct Catalog {
    candidates: Vec<NamedCandidate>,
    symbols: Vec<Symbol>,
    strings: BTreeMap<u64, String>,
    pointers: BTreeMap<u64, u64>,
    bound_slots: std::collections::BTreeSet<u64>,
    imports: BTreeMap<u64, String>,
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
/// Verified static facts that a live session reads for its script-check duration tables.
pub(crate) struct PreparedDurations<'a> {
    pub input: &'a GrammarInput,
    pub declarations: &'a DeclarationResult,
    pub numeric: &'a crate::engine::analysis::numeric::NumericFacts,
    pub scoped: &'a crate::engine::analysis::scoped_numeric::Facts,
}

pub(crate) struct VerifiedAnalysis<'a> {
    executable: Arc<[u8]>,
    catalog: &'a Catalog,
    persistent: Option<&'static super::targets::PersistentRecipe>,
}

impl VerifiedAnalysis<'_> {
    fn scoped_numeric_input(
        &self,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<crate::engine::analysis::scoped_numeric::Input, AnalysisError> {
        let image = binary::references::Image {
            bytes: &self.executable,
            symbols: &self.catalog.symbols,
            strings: &self.catalog.strings,
            pointers: &self.catalog.pointers,
            imports: &self.catalog.imports,
        };
        binary::scoped_numeric::read(
            &image,
            &self.catalog.bound_slots,
            recipe.reader_value_token_offset,
        )
    }

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
        let code = binary::code_range(&self.executable, load_entry, 256)?;
        fixture_reader_boundary(&code, load_entry, &record.loader, &self.catalog.symbols)
    }

    /// Join the owner's constructor directly, or through the matching template's new-entry reader.
    fn reader_constructor_call(
        &self,
        reader_entry: u64,
        owner: &str,
    ) -> Result<Option<u64>, AnalysisError> {
        let mut entry = reader_entry;
        for depth in 0..2 {
            let Some(end) = self
                .catalog
                .symbols
                .iter()
                .filter(|symbol| symbol.address > entry)
                .map(|symbol| symbol.address)
                .min()
            else {
                return Ok(None);
            };
            let length = end - entry;
            if length == 0 || length > 16 * 1024 {
                return Ok(None);
            }
            let bytes = binary::code_range(&self.executable, entry, length)?;
            let rows = decode_arm64(&bytes, entry).map_err(|_| AnalysisError::InvalidRange)?;
            match fixture_constructor_route(&rows, entry, owner, &self.catalog.symbols) {
                Some(FixtureConstructorRoute::Constructor(address)) => return Ok(Some(address)),
                Some(FixtureConstructorRoute::NewEntry(address)) if depth == 0 => entry = address,
                _ => return Ok(None),
            }
        }
        Ok(None)
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

    fn grammar_input(
        &self,
        kind: crate::DeclarationKind,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<
        (
            crate::engine::analysis::grammar::GrammarInput,
            crate::engine::analysis::declarations::DeclarationResult,
        ),
        AnalysisError,
    > {
        let declarations = self.declaration_input(kind, recipe)?;
        let inventory = crate::engine::analysis::declarations::analyze(&declarations)
            .map_err(AnalysisError::Input)?;
        let mut input = binary::grammar::read(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            &self.catalog.bound_slots,
            declarations,
            &inventory,
            recipe,
            kind,
            binary::references::names(
                &self.catalog.symbols,
                &self.catalog.pointers,
                &self.catalog.imports,
                &self.catalog.strings,
            ),
        )?;
        let reference_input = binary::references::read(
            &binary::references::Image {
                bytes: &self.executable,
                symbols: &self.catalog.symbols,
                strings: &self.catalog.strings,
                pointers: &self.catalog.pointers,
                imports: &self.catalog.imports,
            },
            &self.catalog.candidates,
        )?;
        input.forms.initializers = binary::grammar::initializers(
            &reference_input,
            &self.catalog.symbols,
            &mut input.forms.qualified_references,
        )?;
        let scoped_input = self.scoped_numeric_input(recipe)?;
        let scoped = crate::engine::analysis::scoped_numeric::analyze(&scoped_input);
        let image = binary::references::Image {
            bytes: &self.executable,
            symbols: &self.catalog.symbols,
            strings: &self.catalog.strings,
            pointers: &self.catalog.pointers,
            imports: &self.catalog.imports,
        };
        let numeric_input = binary::numeric::read(&image, recipe)?;
        let numeric = crate::engine::analysis::numeric::analyze(&numeric_input);
        input.durations.scoped_storage =
            crate::engine::analysis::durations::scoped_storage(&scoped, &numeric);
        Ok((input, inventory))
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

    fn expansion_input(
        &self,
    ) -> Result<crate::engine::analysis::expansions::ExpansionInput, AnalysisError> {
        binary::expansions::read(
            &self.executable,
            &self.catalog.symbols,
            &self.catalog.strings,
            &self.catalog.candidates,
        )
    }

    fn category_key_input(
        &self,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<crate::engine::analysis::category_keys::CategoryKeyInput, AnalysisError> {
        binary::language::category_keys(
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
        let image = binary::references::Image {
            bytes: &self.executable,
            symbols: &self.catalog.symbols,
            strings: &self.catalog.strings,
            pointers: &self.catalog.pointers,
            imports: &self.catalog.imports,
        };
        binary::callbacks::callbacks(&image, &self.catalog.bound_slots, recipe)
    }

    fn block_input(
        &self,
        recipe: &super::targets::DeclarationRecipe,
    ) -> Result<crate::engine::analysis::callbacks::blocks::BlockInput, AnalysisError> {
        let owners = self
            .catalog
            .candidates
            .iter()
            .map(|candidate| candidate.record.owner_candidate.as_str())
            .collect();
        let image = binary::references::Image {
            bytes: &self.executable,
            symbols: &self.catalog.symbols,
            strings: &self.catalog.strings,
            pointers: &self.catalog.pointers,
            imports: &self.catalog.imports,
        };
        binary::callbacks::block_evaluations(&image, &self.catalog.bound_slots, &owners, recipe)
    }

    fn reference_facts(&self) -> Result<ReferenceFacts, AnalysisError> {
        let image = binary::references::Image {
            bytes: &self.executable,
            symbols: &self.catalog.symbols,
            strings: &self.catalog.strings,
            pointers: &self.catalog.pointers,
            imports: &self.catalog.imports,
        };
        let input = binary::references::read(&image, &self.catalog.candidates)?;

        Ok(references::analyze(&input))
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

    /// Bind root field tokens, with storage only when reader arguments prove it.
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

        let scoped = self.scoped_numeric_facts()?;
        let numeric = self.numeric_facts()?;
        Ok(result
            .fields
            .iter()
            .filter_map(|field| {
                Some(crate::protocol::observation::FixtureOutcomeFieldBinding {
                    token: u64::try_from(field.token).ok()?,
                    name: field.name.clone(),
                    storage: fixture_storage_binding(field, &result.paths)
                        .or_else(|| scoped_fixture_storage(field, &result, scoped, numeric)),
                })
            })
            .collect())
    }

    pub(super) fn new(
        declarations: Option<&'static super::targets::DeclarationRecipe>,
        database: Option<DatabaseLayout>,
        installation: Installation,
        invalidated: Arc<Mutex<Option<UnavailableReason>>>,
    ) -> Self {
        Self {
            declarations,
            database,
            installation,
            invalidated,
            catalog: OnceLock::new(),
            families: OnceLock::new(),
            references: OnceLock::new(),
            numeric: OnceLock::new(),
            modifier_blocks: OnceLock::new(),
            weight_blocks: OnceLock::new(),
            triggered_modifiers: OnceLock::new(),
            scoped_numeric: OnceLock::new(),
            block_facts: OnceLock::new(),
            modifier_nodes: OnceLock::new(),
            category_input: OnceLock::new(),
            names: OnceLock::new(),
            grammar: std::array::from_fn(|_| OnceLock::new()),
        }
    }

    pub(crate) fn executable(&self) -> Result<Arc<[u8]>, AnalysisError> {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FixtureConstructorRoute {
    Constructor(u64),
    NewEntry(u64),
}

/// Select one proven direct constructor or one matching new-entry reader, without chasing other calls.
fn fixture_constructor_route(
    rows: &[crate::engine::analysis::decode::Instruction],
    entry: u64,
    owner: &str,
    symbols: &[Symbol],
) -> Option<FixtureConstructorRoute> {
    let constructor = format!("{owner}::{owner}(int, CString const&)");
    let helper_names: std::collections::BTreeSet<_> = symbols
        .iter()
        .filter(|symbol| symbol.address == entry)
        .filter_map(|symbol| symbol.name.strip_suffix("::LoadFromReader(CReader&, bool)"))
        .map(|database| format!("{database}::ReadNewEntry(CReader&, CString const&)"))
        .collect();
    let mut routes = BTreeMap::new();
    for row in rows.iter().filter(|row| row.operation == "bl") {
        let Some(target) = row
            .operands
            .strip_prefix("#0x")
            .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        else {
            continue;
        };
        for symbol in symbols.iter().filter(|symbol| symbol.address == target) {
            if symbol.name == constructor {
                routes.insert(target, FixtureConstructorRoute::Constructor(target));
            } else if helper_names.contains(&symbol.name) {
                routes.insert(target, FixtureConstructorRoute::NewEntry(target));
            }
        }
    }
    let mut routes = routes.values();
    match (routes.next(), routes.next()) {
        (Some(route), None) => Some(*route),
        _ => None,
    }
}

/// Join the loader's exact template specialization to its reader and cleanup boundary.
fn fixture_reader_boundary(
    code: &[u8],
    load_entry: u64,
    loader: &str,
    symbols: &[Symbol],
) -> Result<Option<(u64, u64)>, AnalysisError> {
    let Some(database) = loader.strip_suffix("::LoadFile(char const*, bool)") else {
        return Ok(None);
    };
    let expected_reader = format!("{database}::LoadFromReader(CReader&, bool)");
    let rows = decode_arm64(code, load_entry).map_err(|_| AnalysisError::InvalidRange)?;
    let matches: Vec<_> = rows
        .windows(3)
        .filter_map(|window| {
            let [call, after, cleanup] = window else {
                return None;
            };
            let target = call
                .operands
                .strip_prefix("#0x")
                .and_then(|hex| u64::from_str_radix(hex, 16).ok())?;
            let cleanup_target = cleanup
                .operands
                .strip_prefix("#0x")
                .and_then(|hex| u64::from_str_radix(hex, 16).ok())?;
            (call.operation == "bl"
                && after.operation == "mov"
                && after.operands == "x0,sp"
                && cleanup.operation == "bl"
                && symbols
                    .iter()
                    .any(|symbol| symbol.address == target && symbol.name == expected_reader)
                && symbols.iter().any(|symbol| {
                    symbol.address == cleanup_target && symbol.name == "CReader::~CReader()"
                }))
            .then_some((target, after.address))
        })
        .collect();
    Ok(matches.first().copied().filter(|_| matches.len() == 1))
}

/// Bind one unconditional direct read of the root reader into owner storage.
fn fixture_storage_binding(
    field: &crate::engine::analysis::fields::RootField,
    paths: &[crate::engine::analysis::fields::TokenPath],
) -> Option<crate::protocol::observation::FixtureStorageBinding> {
    use crate::engine::analysis::fields::{ReaderJoin, Value};
    use crate::protocol::observation::FixtureStorageBinding;

    let [
        ReaderJoin::Joined {
            callee, arguments, ..
        },
    ] = field.readers.as_slice()
    else {
        return None;
    };
    if field.paths.is_empty()
        || !field.paths.iter().all(|&path| {
            paths[path].conditions.is_empty() && paths[path].domain == [field.token, field.token]
        })
        || arguments.get("x0") != Some(&Value::Reader(0))
    {
        return None;
    }
    let Some(Value::Owner(offset)) = arguments.get("x1") else {
        return None;
    };
    // Reader identity selects storage only after the exact-build installation is verified.
    let decoder = fixture_decoder(callee)?;
    Some(FixtureStorageBinding {
        offset: u64::try_from(*offset).ok()?,
        decoder,
    })
}

/// Use the same constructor, subtype and code-layout proofs as the static reader answer.
fn scoped_fixture_storage(
    field: &crate::engine::analysis::fields::RootField,
    result: &crate::engine::analysis::fields::RegistryFieldResult,
    facts: &crate::engine::analysis::scoped_numeric::Facts,
    numeric: &crate::engine::analysis::numeric::NumericFacts,
) -> Option<crate::protocol::observation::FixtureStorageBinding> {
    use crate::engine::analysis::fields::{ReaderJoin, Value};
    use crate::protocol::observation::FixtureStorageBinding;
    let [
        ReaderJoin::Joined {
            callee, arguments, ..
        },
    ] = field.readers.as_slice()
    else {
        return None;
    };
    if callee != "CVariableValue::Read(CReader&, EScopeType)"
        || arguments.get("x1") != Some(&Value::Reader(0))
        || field.paths.is_empty()
        || !field.paths.iter().all(|&path| {
            result.paths[path].conditions.is_empty()
                && result.paths[path].domain == [field.token, field.token]
        })
    {
        return None;
    }
    let Some(Value::Owner(destination)) = arguments.get("x0") else {
        return None;
    };
    let point = result.scoped_destinations.get(destination)?;
    Some(FixtureStorageBinding {
        offset: u64::try_from(*destination).ok()?,
        decoder: scoped_operand_decoder(*point, facts, numeric)?,
    })
}

/// The storage decoder of the scoped operand at a factory-agreed vtable point, when its literal
/// is a proven signed 32-bit integer or a signed 64-bit fixed point in the selection layout.
pub(crate) fn scoped_operand_decoder(
    point: u64,
    facts: &crate::engine::analysis::scoped_numeric::Facts,
    numeric: &crate::engine::analysis::numeric::NumericFacts,
) -> Option<crate::protocol::observation::FixtureStorageDecoder> {
    use crate::engine::analysis::scoped_numeric::Subtype;
    use crate::protocol::observation::{
        FixtureStorageDecoder, ScopedLiteralDecoder, ScopedStorageLayout,
    };
    use crate::{GrammarProperty, NumericRepresentation, NumericSignedness};
    let Subtype::Numeric {
        token_reader,
        literal,
    } = facts.subtypes.get(&point)?.as_ref().ok()?
    else {
        return None;
    };
    let layout = facts.shared.selection.as_ref().ok()?;
    if *literal != layout.literal {
        return None;
    }
    let (GrammarProperty::Known(Some(conversion)) | GrammarProperty::Partial(Some(conversion))) =
        &numeric.token_readers.get(token_reader)?.conversion
    else {
        return None;
    };
    if conversion.representation != GrammarProperty::Known(NumericRepresentation::Integer)
        || conversion.signedness != GrammarProperty::Known(NumericSignedness::Signed)
    {
        return None;
    }
    let literal = match (&conversion.width_bits, &conversion.scale) {
        (GrammarProperty::Known(32), GrammarProperty::Known(Some(1))) => {
            ScopedLiteralDecoder::Integer
        }
        (GrammarProperty::Known(64), GrammarProperty::Known(Some(scale))) if *scale > 0 => {
            ScopedLiteralDecoder::FixedPoint { scale: *scale }
        }
        _ => return None,
    };
    Some(FixtureStorageDecoder::ScopedNumeric {
        literal,
        layout: ScopedStorageLayout {
            literal: layout.literal,
            location: layout.location,
            trigger: layout.trigger,
            script_value: layout.script_value,
            modifier: layout.modifier,
            modifier_unset: layout.modifier_unset,
            variable: layout.variable,
        },
    })
}

fn fixture_decoder(callee: &str) -> Option<crate::protocol::observation::FixtureStorageDecoder> {
    use crate::protocol::observation::FixtureStorageDecoder;
    Some(match callee {
        "CReader::Read(CString&, bool)" => FixtureStorageDecoder::String,
        "CReader::Read(int&)" => FixtureStorageDecoder::Integer,
        "CReader::Read(float&)" => FixtureStorageDecoder::Float,
        "CReader::Read(short&)" => FixtureStorageDecoder::Integer16,
        "CReader::Read(CFixedPoint&)" => FixtureStorageDecoder::FixedPoint { scale: 100_000 },
        "CReader::Read(fpml::fixed_point<long long, (unsigned char)48, (unsigned char)15>&)" => {
            FixtureStorageDecoder::FixedPoint { scale: 32_768 }
        }
        _ => return None,
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
        let references = self.reference_facts()?;
        Ok(Some(crate::session::questions::normalized_fields(
            &result, references,
        )))
    }
}

/// The contexts of registry field blocks, with the scope names that name their types.
pub(crate) struct BlockFacts {
    pub entries: crate::engine::analysis::callbacks::blocks::BlockEntries,
    pub scope_names: Option<Vec<String>>,
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
        let anchors = binary::constructors::anchors(&input.symbols);
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
            imports: input.imports,
        })
    }

    /// Numeric conversions derived once per installation. Every access verifies the image.
    pub(crate) fn numeric_facts(
        &self,
    ) -> Result<&crate::engine::analysis::numeric::NumericFacts, AnalysisError> {
        let verified = self.verified()?;
        self.numeric
            .get_or_init(|| {
                let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
                let image = binary::references::Image {
                    bytes: &verified.executable,
                    symbols: &verified.catalog.symbols,
                    strings: &verified.catalog.strings,
                    pointers: &verified.catalog.pointers,
                    imports: &verified.catalog.imports,
                };
                let input = binary::numeric::read(&image, recipe)?;
                Ok(crate::engine::analysis::numeric::analyze(&input))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Shared modifier grammar at each executable-bound address point.
    pub(crate) fn modifier_block_facts(
        &self,
    ) -> Result<&crate::engine::analysis::modifier_blocks::ModifierBlockFacts, AnalysisError> {
        let verified = self.verified()?;
        self.modifier_blocks
            .get_or_init(|| {
                let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
                let image = binary::references::Image {
                    bytes: &verified.executable,
                    symbols: &verified.catalog.symbols,
                    strings: &verified.catalog.strings,
                    pointers: &verified.catalog.pointers,
                    imports: &verified.catalog.imports,
                };
                let input =
                    binary::modifier_blocks::read(&image, &verified.catalog.candidates, recipe)?;
                Ok(crate::engine::analysis::modifier_blocks::analyze(
                    &input,
                    self.numeric_facts()?,
                ))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Shared weight grammar at each executable-bound address point.
    pub(crate) fn weight_block_facts(
        &self,
    ) -> Result<&crate::engine::analysis::weight_blocks::WeightBlockFacts, AnalysisError> {
        let verified = self.verified()?;
        self.weight_blocks
            .get_or_init(|| {
                let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
                let image = binary::references::Image {
                    bytes: &verified.executable,
                    symbols: &verified.catalog.symbols,
                    strings: &verified.catalog.strings,
                    pointers: &verified.catalog.pointers,
                    imports: &verified.catalog.imports,
                };
                let input =
                    binary::weight_blocks::read(&image, &verified.catalog.bound_slots, recipe)?;
                let text = binary::declarations::Text::read(
                    &verified.executable,
                    &verified.catalog.symbols,
                )?;
                let bodies = |address| binary::weight_blocks::body(&text, address);
                Ok(crate::engine::analysis::weight_blocks::analyze(
                    &input, &bodies,
                ))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Triggered modifier clause grammar at each executable-bound address point.
    pub(crate) fn triggered_modifier_facts(
        &self,
    ) -> Result<&crate::engine::analysis::modifier_blocks::triggered::TriggeredFacts, AnalysisError>
    {
        let verified = self.verified()?;
        self.triggered_modifiers
            .get_or_init(|| {
                let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
                let image = binary::references::Image {
                    bytes: &verified.executable,
                    symbols: &verified.catalog.symbols,
                    strings: &verified.catalog.strings,
                    pointers: &verified.catalog.pointers,
                    imports: &verified.catalog.imports,
                };
                let input = binary::triggered_modifiers::read(
                    &image,
                    &verified.catalog.bound_slots,
                    recipe,
                )?;
                Ok(crate::engine::analysis::modifier_blocks::triggered::analyze(&input))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Scoped numeric facts derived once from the verified executable.
    pub(crate) fn scoped_numeric_facts(
        &self,
    ) -> Result<&crate::engine::analysis::scoped_numeric::Facts, AnalysisError> {
        let verified = self.verified()?;
        self.scoped_numeric
            .get_or_init(|| {
                let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
                let input = verified.scoped_numeric_input(recipe)?;
                Ok(crate::engine::analysis::scoped_numeric::analyze(&input))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// The contexts of registry field blocks, derived once from the verified executable.
    pub(crate) fn block_facts(&self) -> Result<&BlockFacts, AnalysisError> {
        let verified = self.verified()?;
        self.block_facts
            .get_or_init(|| {
                let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
                let input = verified.block_input(recipe)?;
                Ok(BlockFacts {
                    entries: crate::engine::analysis::callbacks::blocks::analyze_blocks(&input),
                    scope_names: input.scope_names,
                })
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    #[cfg(test)]
    pub(crate) fn scoped_numeric_input_for_test(
        &self,
    ) -> Result<crate::engine::analysis::scoped_numeric::Input, AnalysisError> {
        let verified = self.verified()?;
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        verified.scoped_numeric_input(recipe)
    }

    /// The timed-flag execute body, decoded.
    #[cfg(test)]
    pub(crate) fn duration_execute_for_test(
        &self,
    ) -> Result<Vec<crate::engine::analysis::decode::Instruction>, AnalysisError> {
        let verified = self.verified()?;
        binary::durations::execute_body(&verified.executable, &verified.catalog.symbols)
            .ok_or(AnalysisError::InvalidRange)
    }

    /// The lookup of every reference reader in the executable.
    pub(crate) fn reference_facts(&self) -> Result<&ReferenceFacts, AnalysisError> {
        let verified = self.verified()?;
        self.references
            .get_or_init(|| verified.reference_facts())
            .as_ref()
            .map_err(Clone::clone)
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
    ) -> Result<&(GrammarInput, DeclarationResult), AnalysisError> {
        let verified = self.verified()?;
        self.cached_grammar_input(kind, &verified)
    }

    fn cached_grammar_input(
        &self,
        kind: crate::DeclarationKind,
        verified: &VerifiedAnalysis<'_>,
    ) -> Result<&(GrammarInput, DeclarationResult), AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        let index = match kind {
            crate::DeclarationKind::Effect => 0,
            crate::DeclarationKind::Trigger => 1,
        };
        self.grammar[index]
            .get_or_init(|| verified.grammar_input(kind, recipe))
            .as_ref()
            .map_err(Clone::clone)
    }

    /// Verify the executable once and compute every fact that a script check's duration table
    /// reads. A live session calls this before launch, so no check rereads the executable inside
    /// its idle window.
    pub(crate) fn prepare_script_durations(&self) -> Result<(), AnalysisError> {
        let verified = self.verified()?;

        for kind in [
            crate::DeclarationKind::Effect,
            crate::DeclarationKind::Trigger,
        ] {
            self.cached_grammar_input(kind, &verified)?;
        }

        self.numeric_facts()?;
        self.scoped_numeric_facts()?;

        Ok(())
    }

    /// The facts that [`Self::prepare_script_durations`] computed, without verifying again; `None`
    /// before it succeeded. Only a session that verified them at its start may use these.
    pub(crate) fn prepared_script_durations(
        &self,
        kind: crate::DeclarationKind,
    ) -> Option<PreparedDurations<'_>> {
        let index = match kind {
            crate::DeclarationKind::Effect => 0,
            crate::DeclarationKind::Trigger => 1,
        };
        let (input, declarations) = self.grammar[index].get()?.as_ref().ok()?;

        Some(PreparedDurations {
            input,
            declarations,
            numeric: self.numeric.get()?.as_ref().ok()?,
            scoped: self.scoped_numeric.get()?.as_ref().ok()?,
        })
    }

    /// Both command families with their receivers' code, and the flag functions.
    pub(crate) fn dynamic_name_input(
        &self,
    ) -> Result<crate::engine::analysis::dynamic_names::DynamicNameInput<'_>, AnalysisError> {
        use crate::DeclarationKind;
        use crate::engine::analysis::dynamic_names::{CommandFamily, DynamicNameInput};

        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        let verified = self.verified()?;
        let catalog = verified.catalog;
        let families = [
            (DeclarationKind::Effect, recipe.effect_names),
            (DeclarationKind::Trigger, recipe.trigger_names),
        ]
        .into_iter()
        .map(|(kind, slots)| {
            let (grammar, inventory) = self.cached_grammar_input(kind, &verified)?;
            Ok(CommandFamily {
                kind,
                declarations: &grammar.declarations,
                inventory,
                slots,
            })
        })
        .collect::<Result<_, AnalysisError>>()?;

        Ok(DynamicNameInput {
            families,
            functions: binary::dynamic_names::flag_functions(&catalog.symbols)?,
            scope_type_offset: recipe.callbacks.scope_type_offset,
            names: binary::references::names(
                &catalog.symbols,
                &catalog.pointers,
                &catalog.imports,
                &catalog.strings,
            ),
        })
    }

    pub(crate) fn modifier_input(
        &self,
    ) -> Result<crate::engine::analysis::modifiers::ModifierInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.modifier_input(recipe)
    }

    /// The category switch, the reader checks after each call of it, and the token names.
    pub(crate) fn category_key_input(
        &self,
    ) -> Result<crate::engine::analysis::category_keys::CategoryKeyInput, AnalysisError> {
        let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.category_key_input(recipe)
    }

    /// The engine functions, classified callers and message sites of each script expansion
    /// mechanism. The stated forms in the input belong to builds with a declaration recipe.
    pub(crate) fn expansion_input(
        &self,
    ) -> Result<crate::engine::analysis::expansions::ExpansionInput, AnalysisError> {
        self.declarations.ok_or(AnalysisError::InvalidRange)?;
        self.verified()?.expansion_input()
    }

    /// The modifier node graph, read once from the node type symbols, node constructor calls and
    /// static initializers.
    pub(crate) fn modifier_node_result(
        &self,
    ) -> Result<&crate::engine::analysis::modifier_nodes::ModifierNodeResult, AnalysisError> {
        let verified = self.verified()?;
        self.modifier_nodes
            .get_or_init(|| {
                let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
                let input = binary::modifier_nodes::read(
                    &verified.executable,
                    &verified.catalog.symbols,
                    &verified.catalog.pointers,
                    &verified.catalog.bound_slots,
                    recipe,
                )?;
                Ok(crate::engine::analysis::modifier_nodes::analyze(&input))
            })
            .as_ref()
            .map_err(Clone::clone)
    }

    /// The category-name switch and what running it needs.
    pub(crate) fn category_input(
        &self,
    ) -> Result<&crate::engine::analysis::modifiers::CategoryInput, AnalysisError> {
        let verified = self.verified()?;
        self.category_input
            .get_or_init(|| {
                let recipe = self.declarations.ok_or(AnalysisError::InvalidRange)?;
                binary::language::category_input(
                    &verified.executable,
                    &verified.catalog.symbols,
                    recipe,
                )
            })
            .as_ref()
            .map_err(Clone::clone)
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

    /// The lookup, check and diagnostic functions of the derived-name method.
    pub(crate) fn name_input(
        &self,
    ) -> Result<&crate::engine::analysis::names::NameInput, AnalysisError> {
        let verified = self.verified()?;
        Ok(self
            .names
            .get_or_init(|| binary::names::input(&verified.catalog.symbols)))
    }

    /// The code of one named registry's item class that composes derived names, or `None` when
    /// the name is not a registry's content directory.
    pub(crate) fn registry_names(
        &self,
        registry: &str,
    ) -> Result<Option<crate::engine::analysis::names::RegistryInput>, AnalysisError> {
        let index = self.family_index()?;
        let input = self.name_input()?;
        let verified = self.verified()?;
        let Some(candidate) = unique_named_candidate(verified.named_candidates(), registry) else {
            return Ok(None);
        };
        let named = binary::families::NamedRegistry {
            name: registry,
            database: &candidate.record.database,
            owner: &candidate.record.owner_candidate,
        };
        binary::names::registry(
            &verified.executable,
            &verified.catalog.symbols,
            &index.input.data,
            &index.input.strings,
            input,
            &named,
        )
        .map(Some)
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
