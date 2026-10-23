//! Fill registry field answers from the build directory's reader export.
//!
//! `tools/ghidra/` writes `readers.json`, the keys that each member reader of a build reads, and
//! `tables.json` ([Ghidra front end](../../docs/design/ghidra-front-end.md)). The export only
//! adds: it fills a property that Native leaves unresolved and a field that Native does not list,
//! and it removes a Native gap that it answers. A property that Native establishes stays.
//!
//! A registry becomes `Complete` only under the whole-block contract: every reader in its tree
//! proves the keys that it accepts, so that a key that no reader lists is no field.
use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;

use super::fields::persistent_read_merges;
use super::questions::{
    CONDITION_OPEN, KEYWORD_DOMAIN_OPEN, READER_ALTERNATIVES_OPEN, READER_JOIN_OPEN,
    TOKEN_PATH_OPEN, UNNAMED_PATHS, VALUE_FORM_OPEN, storage_gap,
};
use crate::binding::MemberHolder;
use crate::engine::analysis::fields::{OPEN_DOMAIN_REASONS, UNKNOWN_KEPT};
use crate::{
    AcceptedCategories, Answer, BlockFamily, Completeness, Field, FieldCondition, FieldDomain,
    FieldMembers, FieldReadAlternative, FieldReadOutcome, FieldReference, FieldRequirement,
    FieldShape, Gap, GapKind, GapSubject, GrammarProperty, ReadScope, Reader, ReaderKind,
    RepeatBehavior, ScopeId, ScopeReference, ValueShape,
};

/// The schema revision of `readers.json` and `tables.json` that this release reads.
const SCHEMA: u32 = 1;

const READ_SCOPE_PREFIX: &str = "read-scope: ";
const BRANCHES_OPEN: &str = "The key's path tests the value's token, and the reader export does not \
                             prove how it reads a block and a bare value.";
const STORE_OPEN: &str = "The key's path stores a value through no shared reader.";
const READER_OPEN: &str = "The reader export does not establish the field's reader.";
const PATHS_OPEN: &str = "The reader export did not classify every path of the key.";
const REPEAT_OPEN: &str = "The reader export does not establish repeat behavior.";
const UNPROVED_MEMBERS: &str =
    "The reader export lists these nested fields, but its readers do not prove the block's keys.";
const ENTRY_CONTEXTS_OPEN: &str =
    "The reader export establishes this block, but not the contexts that evaluate it.";
const IDENTITY_OPEN: &str =
    "The reader export establishes the value form, not the shared reader's identity.";
const NUMERIC_OPEN: &str = "The reader export does not establish numeric conversion.";
const LOOKUP_OPEN: &str = "The reader export does not establish the lookup of the key.";

/// The compiled-in files of one build directory.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExportFiles {
    /// `readers.json`.
    pub readers: &'static str,
    /// `tables.json`.
    pub tables: &'static str,
}

/// The reader export of one build: each registry's root readers, and each walked reader's rows.
#[derive(Debug, Deserialize)]
pub(crate) struct ReaderExport {
    schema: u32,
    registries: BTreeMap<String, ExportRegistry>,
    readers: BTreeMap<String, ExportReader>,
    #[serde(skip)]
    tables: Tables,
}

#[derive(Debug, Default, Deserialize)]
struct Tables {
    schema: u32,
    scope_types: Vec<ScopeTypeBit>,
    manual_exceptions: ManualExceptions,
}

#[derive(Debug, Deserialize)]
struct ScopeTypeBit {
    bit: u32,
    id: ScopeId,
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct ManualExceptions {
    zero_mask_read_scope: Vec<ZeroMaskException>,
}

/// A reader whose literal scope mask 0 keeps `Unresolved` read scope (simplification D5(a)).
#[derive(Debug, Deserialize)]
struct ZeroMaskException {
    /// The member reader that passes the mask.
    reader: String,
    /// The keys of `reader` that keep the exception.
    keys: Vec<String>,
    /// Whether the block that `reader` reads keeps it too.
    block: bool,
}

#[derive(Debug, Deserialize)]
struct ExportRegistry {
    roots: Vec<ExportRoot>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExportRoot {
    /// The root key whose statements the reader reads; empty for the registry's owner.
    key: String,
    reader: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ExportReader {
    name: Option<String>,
    walked: bool,
    dispatch: Option<Dispatch>,
    complete: bool,
    incomplete_because: Vec<String>,
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize)]
struct Dispatch {
    form: DispatchForm,
    default: DefaultPath,
    delegates: Vec<Nested>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DispatchForm {
    TokenSwitch,
    CompareChain,
    Entry,
    KeyText,
    ValueToken,
}

#[derive(Debug, Deserialize)]
struct DefaultPath {
    kind: DefaultKind,
    reader: Option<String>,
}

/// What a reader does with a key that it does not enumerate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DefaultKind {
    Persistent,
    LogAndSkip,
    BaseReader,
    Entry,
    Return,
    Helper,
    Miss,
}

/// A reader that a row, a branch or a dispatch names: its entry when the export walked it.
#[derive(Debug, Deserialize)]
struct Nested {
    reader: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize)]
struct Row {
    key: Option<String>,
    /// What the key's paths read, taken together.
    #[serde(flatten)]
    outcome: Outcome,
    conditional: bool,
    branches: Option<Vec<Branch>>,
    scope_mask: Option<u64>,
    #[serde(rename = "enum")]
    keywords: Option<KeywordTable>,
    paths: Paths,
}

/// What a row's paths, or one of its value branches, read.
#[derive(Debug, Deserialize)]
struct Outcome {
    kind: RowKind,
    value: Option<ValueShape>,
    reader_kind: Option<ReaderKind>,
    repeat: RepeatBehavior,
    family: Option<BlockFamily>,
    items: Option<ReaderKind>,
    nested: Option<Nested>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RowKind {
    Scalar,
    ScalarOther,
    Enum,
    Keyref,
    Lookup,
    List,
    Trigger,
    Effect,
    ScriptValue,
    Persistent,
    Serializer,
    Object,
    ObjectList,
    Entry,
    Skipped,
    Store,
    Presence,
    Delegates,
    Helper,
    Logged,
    Unclassified,
}

impl RowKind {
    /// Whether a row of this kind reads its value as a block with its nested reader.
    fn reads_block(self) -> bool {
        matches!(
            self,
            Self::Persistent | Self::Serializer | Self::Object | Self::ObjectList | Self::Entry
        )
    }
}

/// One outcome of a row whose path tests the value's token.
#[derive(Debug, Deserialize)]
struct Branch {
    value_token: ValueToken,
    /// Whether the export proves what this branch reads.
    proved: bool,
    #[serde(flatten)]
    outcome: Outcome,
}

/// The value token of a value branch: a block's, or any other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ValueToken {
    Block,
    Other,
}

/// The `TokenToEnum<E>` table of a keyword row.
#[derive(Debug, Deserialize)]
struct KeywordTable {
    keys: Vec<Option<String>>,
    complete: bool,
}

#[derive(Debug, Deserialize)]
struct Paths {
    unknown: u32,
    truncated: bool,
}

/// How a registry answer holds the fields of its owner's reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootLayout {
    /// The answer's root fields are the owner's fields.
    Owner,
    /// Each root field is a loader statement, such as an event kind, whose members are the
    /// owner's fields.
    Statements,
}

impl ReaderExport {
    /// Read the export, refusing a schema revision that this release does not read.
    pub(crate) fn parse(files: ExportFiles) -> Result<Self, String> {
        let mut export: Self = serde_json::from_str(files.readers)
            .map_err(|error| format!("readers.json: {error}"))?;
        let tables: Tables =
            serde_json::from_str(files.tables).map_err(|error| format!("tables.json: {error}"))?;

        for (file, schema) in [
            ("readers.json", export.schema),
            ("tables.json", tables.schema),
        ] {
            if schema != SCHEMA {
                return Err(format!(
                    "{file} has schema {schema}; this release reads schema {SCHEMA}"
                ));
            }
        }

        export.tables = tables;

        Ok(export)
    }

    /// The member readers, such as `CSettings::ReadMember`, that a persistent row reads the
    /// owner's own member with. The `Read` and post-read hooks of every object with one of them
    /// decide whether a repeat merges.
    pub(crate) fn persistent_members(&self) -> BTreeSet<&str> {
        self.readers
            .values()
            .flat_map(|reader| &reader.rows)
            .flat_map(Row::outcomes)
            .filter(|outcome| outcome.kind == RowKind::Persistent)
            .filter_map(walked_member)
            .collect()
    }

    /// Fill `answer`, Native's registry field answer for `registry`, from the export, and stamp
    /// the merged method. `merging_members` are the [`Self::persistent_members`] whose objects
    /// all keep themselves on a persistent read, so that a repeat merges. The answer stays unchanged, and the result
    /// is `false`, when the export has no root reader for the registry.
    pub(crate) fn merge(
        &self,
        registry: &str,
        layout: RootLayout,
        merging_members: &BTreeSet<String>,
        answer: &mut Answer<Vec<Field>>,
    ) -> bool {
        let Some(roots) = self.roots(registry) else {
            return false;
        };
        let native_complete = answer.completeness == Completeness::Complete;
        let native_gaps = std::mem::take(&mut answer.gaps);
        let mut merge = Merge::new(self, &native_gaps, merging_members);

        for root in roots {
            let Some(entry) = root.reader.as_deref() else {
                continue;
            };

            merge.root(&mut answer.value, layout, &root.key, entry);
        }

        let failures = self.tree_failures(roots);
        let keys_proved = failures.is_empty();
        let mut gaps: Vec<Gap> = native_gaps
            .iter()
            .filter(|gap| !merge.answers(gap, keys_proved))
            .cloned()
            .collect();

        gaps.extend(merge.gaps);

        // The export makes no answer complete unless its tree proves the keys. Native's own
        // complete answer proved them itself, and a partial one says why already.
        if !native_complete && Completeness::from_gaps(&gaps) == Completeness::Complete {
            gaps.extend(failures.iter().map(|failure| failure.gap(registry)));
        }

        answer.completeness = Completeness::from_gaps(&gaps);
        answer.gaps = gaps;
        answer.source.method = format!("registry-fields/ghidra-v{SCHEMA}");

        true
    }

    /// The root readers of `registry`, or `None` when the export has none.
    fn roots(&self, registry: &str) -> Option<&[ExportRoot]> {
        let registry = self.registries.get(registry)?;

        (registry.error.is_none() && !registry.roots.is_empty()).then_some(&registry.roots[..])
    }

    /// The rows of the reader at `entry`, each with the reader that reads it: its own rows, then
    /// the rows of its base readers for keys that it does not read itself. A key that the base
    /// reads is a field: delegation is not rejection.
    fn table(&self, entry: &str) -> Vec<(&ExportReader, &Row)> {
        self.own_table(entry)
            .into_iter()
            .map(|(reader, row)| self.delegated_row(row).unwrap_or((reader, row)))
            .collect()
    }

    /// The row that reads `row`'s key in the reader that `row` delegates the key to, at the same
    /// level: the delegate reads the key's value itself, not a block of its own. Only a delegation
    /// on every path, with every path classified, is resolved: the delegate's row keeps neither
    /// the delegation's condition nor its open paths.
    fn delegated_row(&self, row: &Row) -> Option<(&ExportReader, &Row)> {
        let certain = !row.conditional && row.paths.unknown == 0 && !row.paths.truncated;

        if row.outcome.kind != RowKind::Delegates || !certain {
            return None;
        }

        let entry = row.outcome.nested.as_ref()?.reader.as_deref()?;

        self.own_table(entry).into_iter().find(|(_, delegated)| {
            delegated.key == row.key && delegated.outcome.kind != RowKind::Delegates
        })
    }

    /// [`Self::table`] before delegated rows are resolved.
    fn own_table(&self, entry: &str) -> Vec<(&ExportReader, &Row)> {
        let mut rows = Vec::new();
        let mut keys = BTreeSet::new();
        let mut seen = BTreeSet::new();
        let mut next = Some(entry);

        while let Some(entry) = next
            && seen.insert(entry)
            && let Some(reader) = self.readers.get(entry)
        {
            for row in &reader.rows {
                if keys.insert(&row.key) {
                    rows.push((reader, row));
                }
            }

            next = reader.base_reader();
        }

        rows
    }

    /// Each reader in the trees of `roots` that does not prove the keys it accepts, with the key
    /// path at which the tree reaches it.
    fn tree_failures(&self, roots: &[ExportRoot]) -> Vec<Failure> {
        let mut failures = Vec::new();
        let mut seen = BTreeSet::new();

        for root in roots {
            let path: Vec<String> = [root.key.clone()]
                .into_iter()
                .filter(|key| !key.is_empty())
                .collect();

            match root.reader.as_deref() {
                Some(entry) => self.walk_tree(entry, path, true, &mut seen, &mut failures),
                None => failures.push(Failure::not_walked(path)),
            }
        }

        failures
    }

    /// Add to `failures` each reader in the tree under the reader at `entry`, at `path`, that
    /// does not prove its keys. `reads_block` says that the reader reads a block's keys, as a root
    /// or a nested reader does, and not the keys that another reader passes on to it.
    fn walk_tree<'a>(
        &'a self,
        entry: &'a str,
        path: Vec<String>,
        reads_block: bool,
        seen: &mut BTreeSet<(&'a str, bool)>,
        failures: &mut Vec<Failure>,
    ) {
        if !seen.insert((entry, reads_block)) {
            return;
        }

        let Some(reader) = self.readers.get(entry) else {
            failures.push(Failure::not_walked(path));
            return;
        };
        let mut conditions = reader.contract_failures();

        // A block reader that lists no key proves nothing about the block: a serializer whose
        // member reader is `CPersistent::ReadMember` reads its block through its own `Read`.
        if reads_block && !reader.lists_keys() && !reader.is_entry_mode() {
            conditions.push("no-listed-key".into());
        }

        if !conditions.is_empty() {
            failures.push(Failure {
                path: path.clone(),
                conditions,
            });
        }

        for (next, next_path, reads_block) in reader.subtree(&path) {
            self.walk_tree(next, next_path, reads_block, seen, failures);
        }
    }

    /// Whether the whole tree under the reader at `entry` proves its keys.
    fn proves_tree<'a>(&'a self, entry: &'a str) -> bool {
        let mut failures = Vec::new();

        self.walk_tree(entry, Vec::new(), true, &mut BTreeSet::new(), &mut failures);

        failures.is_empty()
    }

    /// Whether `row` of `owner` keeps the D5(a) read-scope exception.
    fn zero_mask_exception(&self, owner: &ExportReader, row: &Row) -> bool {
        self.tables
            .manual_exceptions
            .zero_mask_read_scope
            .iter()
            .any(|exception| {
                let own_key = owner.name.as_deref() == Some(exception.reader.as_str())
                    && row
                        .key
                        .as_ref()
                        .is_some_and(|key| exception.keys.contains(key));
                let block = exception.block
                    && row
                        .outcome
                        .nested
                        .as_ref()
                        .is_some_and(|nested| nested.name == exception.reader);

                own_key || block
            })
    }

    /// The scope types of the bits of `mask`, or `None` when a bit names no scope type.
    fn scope_types(&self, mask: u64) -> Option<Vec<ScopeReference>> {
        (0..64)
            .filter(|bit| mask & (1u64 << bit) != 0)
            .map(|bit| {
                self.tables
                    .scope_types
                    .iter()
                    .find(|scope| scope.bit == bit)
                    .map(|scope| ScopeReference {
                        id: scope.id.clone(),
                        name: scope.name.clone(),
                    })
            })
            .collect()
    }

    /// The read scope that `row` of `owner` establishes for a field whose reader is `kind`, or
    /// the gap that leaves it unresolved. Mask 0 reads in no scope (SDK-808), except for the
    /// D5(a) readers.
    fn read_scope(
        &self,
        owner: &ExportReader,
        row: &Row,
        kind: ReaderKind,
        members: &FieldMembers,
    ) -> Result<GrammarProperty<Vec<ReadScope>>, (GapKind, String)> {
        if self.zero_mask_exception(owner, row) {
            return Err((
                GapKind::OutsideMethod,
                super::weight_blocks::ZERO_MASK.into(),
            ));
        }

        match row.scope_mask {
            Some(0) => Ok(GrammarProperty::Known(Vec::new())),
            Some(mask) => self
                .scope_types(mask)
                .map(|types| GrammarProperty::Known(vec![ReadScope::Types(types)]))
                .ok_or((
                    GapKind::UnresolvedPath,
                    format!("{READ_SCOPE_PREFIX}scope-name"),
                )),
            None if reads_no_scope(kind, members) => Ok(GrammarProperty::Known(Vec::new())),
            None => Err((
                GapKind::UnresolvedPath,
                format!("{READ_SCOPE_PREFIX}no-literal-mask"),
            )),
        }
    }
}

impl ExportReader {
    fn base_reader(&self) -> Option<&str> {
        let default = &self.dispatch.as_ref()?.default;

        (default.kind == DefaultKind::BaseReader)
            .then_some(default.reader.as_deref())
            .flatten()
    }

    /// Whether the reader dispatches on the key's token, so that its rows are the keys of a block.
    fn reads_keys(&self) -> bool {
        self.dispatch.as_ref().is_some_and(|dispatch| {
            matches!(
                dispatch.form,
                DispatchForm::TokenSwitch | DispatchForm::CompareChain
            )
        })
    }

    fn lists_keys(&self) -> bool {
        self.rows.iter().any(|row| row.key.is_some())
    }

    fn is_entry_mode(&self) -> bool {
        self.dispatch
            .as_ref()
            .is_some_and(|dispatch| dispatch.form == DispatchForm::Entry)
    }

    /// The whole-block conditions that this reader fails on its own: it must be complete, reject
    /// a key that it does not enumerate (directly or through its base reader), and not read any
    /// key in entry mode.
    fn contract_failures(&self) -> Vec<String> {
        let Some(dispatch) = self.dispatch.as_ref().filter(|_| self.walked) else {
            return vec!["not-walked".into()];
        };
        let mut failures = self.incomplete_because.clone();

        if !self.complete && failures.is_empty() {
            failures.push("incomplete".into());
        }
        if dispatch.form == DispatchForm::Entry {
            failures.push("entry-mode".into());
        } else if !matches!(
            dispatch.default.kind,
            DefaultKind::LogAndSkip | DefaultKind::Persistent | DefaultKind::BaseReader
        ) && !failures
            .iter()
            .any(|failure| failure == "default-unresolved")
        {
            failures.push("default-unresolved".into());
        }

        failures
    }

    /// The walked readers that this reader's key domain depends on, with their key paths below
    /// `path` and whether each reads a block of its own: its base reader and delegates at `path`,
    /// and each row's and branch's nested reader, a block reader, at the row's key.
    fn subtree(&self, path: &[String]) -> Vec<(&str, Vec<String>, bool)> {
        let mut readers = Vec::new();
        let at_path = self
            .base_reader()
            .into_iter()
            .chain(self.dispatch.iter().flat_map(|dispatch| {
                dispatch
                    .delegates
                    .iter()
                    .filter_map(|delegate| delegate.reader.as_deref())
            }));

        readers.extend(at_path.map(|entry| (entry, path.to_vec(), false)));

        for row in &self.rows {
            let key = row.key.clone().unwrap_or_default();
            let key_path = [path, std::slice::from_ref(&key)].concat();

            // A delegate reads the key at this level; any other nested reader, a value branch's
            // included, reads its block.
            for outcome in row.outcomes() {
                let Some(entry) = outcome
                    .nested
                    .as_ref()
                    .and_then(|nested| nested.reader.as_deref())
                else {
                    continue;
                };

                match outcome.kind {
                    RowKind::Delegates => readers.push((entry, path.to_vec(), false)),
                    _ => readers.push((entry, key_path.clone(), true)),
                }
            }
        }

        readers
    }
}

impl Row {
    /// The row's own outcome and the outcome of each of its value branches.
    fn outcomes(&self) -> impl Iterator<Item = &Outcome> {
        let branches = self.branches.iter().flatten();

        std::iter::once(&self.outcome).chain(branches.map(|branch| &branch.outcome))
    }

    /// The outcomes of the two value branches, the block's and then any other value's, when the
    /// export proves both.
    fn value_forms(&self) -> Option<(&Outcome, &Outcome)> {
        let [block, other] = self.branches.as_deref()? else {
            return None;
        };
        let proved = block.value_token == ValueToken::Block
            && other.value_token == ValueToken::Other
            && block.proved
            && other.proved;

        proved.then_some((&block.outcome, &other.outcome))
    }

    /// The outcomes that the key's reading comes from: its proved value branches, or its own.
    fn read_outcomes(&self) -> Vec<&Outcome> {
        match self.value_forms() {
            Some((block, other)) => vec![block, other],
            None => vec![&self.outcome],
        }
    }

    /// The outcome whose block gives the field's members: the proved block branch, or the row's
    /// own outcome.
    fn block_outcome(&self) -> &Outcome {
        self.value_forms().map_or(&self.outcome, |(block, _)| block)
    }

    /// Whether every path of the key reaches its reading. Proved value branches cover every path:
    /// each has one path, and the value test is the only test that they decide.
    fn unconditional(&self) -> bool {
        !self.conditional || self.value_forms().is_some()
    }

    /// Whether the export classified every path of the key.
    fn paths_classified(&self) -> bool {
        self.value_forms().is_some() || (self.paths.unknown == 0 && !self.paths.truncated)
    }
}

/// A reader in a registry's tree that does not prove its keys.
struct Failure {
    /// The key path at which the tree reaches the reader; empty for a root.
    path: Vec<String>,
    /// The whole-block conditions that it fails.
    conditions: Vec<String>,
}

impl Failure {
    fn not_walked(path: Vec<String>) -> Self {
        Self {
            path,
            conditions: vec!["not-walked".into()],
        }
    }

    fn gap(&self, registry: &str) -> Gap {
        let place = match self.path.as_slice() {
            [] => "a definition".to_owned(),
            path => format!("`{}`", path.join(".")),
        };

        Gap {
            kind: GapKind::UnresolvedPath,
            subject: Some(GapSubject::registry(registry)),
            detail: format!(
                "The reader export does not prove which keys {place} accepts: {}.",
                self.conditions.join(", ")
            ),
        }
    }
}

/// What the export answers at one field path, for removing Native's gaps there.
#[derive(Debug, Default)]
struct Answered {
    /// The key's one unconditional reading, with every path classified and every nested reader's
    /// keys proved.
    proved: bool,
    /// The keyword domain.
    domain: bool,
    /// The read scope.
    read_scope: bool,
    /// The walker's storage gap of the field, when the export filled a storage property: it is
    /// computed again. Another storage gap at the path stays.
    replaced_storage: Option<Gap>,
    /// The conditions of the field's read alternatives.
    conditions: bool,
}

/// One merge of the export into an answer.
struct Merge<'a> {
    export: &'a ReaderExport,
    native_gaps: &'a [Gap],
    /// The member readers whose objects all keep themselves on a persistent read.
    merging_members: &'a BTreeSet<String>,
    /// What the export answers, by field path.
    answered: BTreeMap<Vec<String>, Answered>,
    /// The gaps of added fields and the storage gaps computed again.
    gaps: Vec<Gap>,
    /// The readers whose rows are being merged, outermost first: a nested reader that is one of
    /// them is not expanded again.
    ancestors: Vec<&'a str>,
}

impl<'a> Merge<'a> {
    fn new(
        export: &'a ReaderExport,
        native_gaps: &'a [Gap],
        merging_members: &'a BTreeSet<String>,
    ) -> Self {
        Self {
            export,
            native_gaps,
            merging_members,
            answered: BTreeMap::new(),
            gaps: Vec::new(),
            ancestors: Vec::new(),
        }
    }

    /// Merge the reader at `entry`, the reader of root key `key`, into `fields`.
    fn root(&mut self, fields: &mut Vec<Field>, layout: RootLayout, key: &str, entry: &'a str) {
        match (layout, key) {
            (RootLayout::Owner, "") => self.level(fields, entry, &[]),
            (RootLayout::Statements, "") => {
                for statement in fields {
                    if let FieldMembers::Fields(members) = &mut statement.members {
                        self.level(members, entry, std::slice::from_ref(&statement.name));
                    }
                }
            }
            (_, key) => {
                if let Some(field) = fields.iter_mut().find(|field| field.name == key)
                    && let FieldMembers::Fields(members) = &mut field.members
                {
                    self.level(members, entry, &[key.to_owned()]);
                }
            }
        }
    }

    /// Fill `fields`, the fields at `path`, from the rows of the reader at `entry`.
    fn level(&mut self, fields: &mut Vec<Field>, entry: &'a str, path: &[String]) {
        self.ancestors.push(entry);

        for (owner, row) in self.export.table(entry) {
            // An entry reader's one row reads any key, which an answer names with an empty key.
            let Some(key) = row
                .key
                .clone()
                .or_else(|| owner.is_entry_mode().then(String::new))
            else {
                continue;
            };
            let field_path = [path, std::slice::from_ref(&key)].concat();

            match fields.iter_mut().find(|field| field.name == key) {
                Some(field) => self.fill(field, owner, row, &field_path),
                None => {
                    let field = self.added_field(owner, row, &field_path);

                    fields.push(field);
                }
            }
        }

        self.ancestors.pop();
    }

    /// The walked reader that `outcome` reads the keys of its block with, unless it is being
    /// merged already. Only an outcome that reads a block has one. A reader that lists no key
    /// gives no members: a serializer whose member reader is `CPersistent::ReadMember` reads its
    /// values through its own `Read`, which the export does not walk.
    fn nested_entry(&self, outcome: &'a Outcome) -> Option<&'a str> {
        if !outcome.kind.reads_block() {
            return None;
        }

        let entry = outcome.nested.as_ref()?.reader.as_deref()?;
        let reader = self.export.readers.get(entry)?;

        (!self.ancestors.contains(&entry) && reader.reads_keys() && reader.lists_keys())
            .then_some(entry)
    }

    /// The gap of a root trigger, effect or weight block whose family the export establishes: the
    /// walker's entry-context analysis ran before the merge, so its contexts are not established.
    /// `family` is the family of `field`'s block reader: its one reader's, or its block form's.
    fn entry_context_gap(
        &self,
        family: BlockFamily,
        field: &Field,
        path: &[String],
    ) -> Option<Gap> {
        let root = self.ancestors.len() == 1;

        (root
            && super::field_entries::takes_entry_contexts(family)
            && field.entry_contexts.is_empty())
        .then(|| Gap {
            kind: GapKind::UnresolvedPath,
            subject: Some(field_subject(path)),
            detail: ENTRY_CONTEXTS_OPEN.into(),
        })
    }

    /// The gap of a field whose nested fields come from a reader tree that does not prove its
    /// keys: the list can miss a key.
    fn unproved_members_gap(
        &self,
        field: &Field,
        outcome: &Outcome,
        path: &[String],
    ) -> Option<Gap> {
        let entry = outcome.nested.as_ref()?.reader.as_deref()?;

        (matches!(field.members, FieldMembers::Fields(_)) && !self.export.proves_tree(entry)).then(
            || Gap {
                kind: GapKind::UnresolvedStorage,
                subject: Some(field_subject(path)),
                detail: UNPROVED_MEMBERS.into(),
            },
        )
    }

    fn members(&mut self, outcome: &'a Outcome, path: &[String]) -> FieldMembers {
        if let Some(entry) = self.nested_entry(outcome) {
            let mut children = Vec::new();

            self.level(&mut children, entry, path);

            return FieldMembers::Fields(children);
        }

        match (outcome.items, outcome.reader_kind) {
            (Some(items), _) => FieldMembers::Items {
                reader: plain_reader(items, BlockFamily::NotApplicable),
            },
            (None, Some(kind)) if keeps_no_block(kind) => FieldMembers::None,
            _ => FieldMembers::Unresolved,
        }
    }

    /// What `row` establishes about reading its value. A row whose path tests the value's token
    /// is read only through its two proved value branches.
    fn reading(&self, row: &Row) -> Result<Reading, OpenRead> {
        if row.branches.is_none() {
            return self.outcome_reading(&row.outcome).map(Reading::One);
        }

        let forms = row
            .value_forms()
            .map(|(block, other)| (self.outcome_reading(block), self.outcome_reading(other)));

        match forms {
            Some((Ok(block), Ok(other))) => Ok(Reading::ByValueForm { block, other }),
            _ => Err(OpenRead {
                shape: FieldShape {
                    value: ValueShape::Any,
                    ..self.shape(&row.outcome)
                },
                gap: (GapKind::ReaderSemantics, BRANCHES_OPEN),
            }),
        }
    }

    /// What `outcome` establishes about reading a value.
    fn outcome_reading(&self, outcome: &Outcome) -> Result<(Reader, FieldShape), OpenRead> {
        let shape = self.shape(outcome);

        match (outcome.kind, outcome.reader_kind) {
            (RowKind::Store, _) => Err(OpenRead {
                shape,
                gap: (GapKind::ReaderSemantics, STORE_OPEN),
            }),
            (_, Some(kind)) if kind != ReaderKind::Unknown => {
                Ok((plain_reader(kind, family(outcome, kind)), shape))
            }
            _ => Err(OpenRead {
                shape,
                gap: (GapKind::UnresolvedReader, READER_OPEN),
            }),
        }
    }

    /// The shape that `outcome` reads, with `Merges` for a persistent read into the owner's own
    /// member whose objects keep themselves.
    fn shape(&self, outcome: &Outcome) -> FieldShape {
        let keeps_object = outcome.kind == RowKind::Persistent
            && walked_member(outcome).is_some_and(|member| self.merging_members.contains(member));

        outcome_shape(outcome, keeps_object)
    }

    /// Whether the export proves `row` completely: a reading on every path, every path
    /// classified, and the keys of the trees of its nested readers.
    fn proves(&self, row: &Row) -> bool {
        let nested_proved = row.read_outcomes().into_iter().all(|outcome| {
            outcome.nested.as_ref().is_none_or(|nested| {
                nested
                    .reader
                    .as_deref()
                    .is_some_and(|entry| self.export.proves_tree(entry))
            })
        });

        row.unconditional() && row.paths_classified() && self.reading(row).is_ok() && nested_proved
    }

    /// Fill the unresolved properties of Native's `field`, at `path`, from `row` of `owner`.
    fn fill(&mut self, field: &mut Field, owner: &ExportReader, row: &'a Row, path: &[String]) {
        let unconditional = row.unconditional()
            && matches!(field.read.as_slice(), [alternative] if alternative.condition == FieldCondition::Always);
        let kind_before = field.reader.kind;
        let family_before = field.reader.family;
        let name = path.join(".");
        let storage_before = storage_gap(
            field,
            &name,
            super::fields::unknown_repeat_reason(&field.reader),
        );
        let reading = self.reading(row);
        let forms_filled = match &reading {
            Ok(Reading::ByValueForm { block, other }) => fill_value_forms(field, block, other),
            _ => false,
        };

        // A conditional read can give each alternative its own reader and members; the export
        // pairs them only by the value's form.
        if !unconditional && !forms_filled {
            return;
        }

        let mut storage_changed = forms_filled;
        // A proved pair answers the field only where its alternatives were installed.
        let proved = self.proves(row) && (forms_filled || matches!(reading, Ok(Reading::One(_))));
        let mut answered = Answered {
            proved,
            conditions: forms_filled,
            ..Answered::default()
        };

        if let Ok(Reading::One((reader, shape))) = &reading {
            storage_changed |= fill_read(field, reader, *shape);
        }
        if field.domain == FieldDomain::Unknown
            && field.reader.kind == ReaderKind::Keyword
            && !self.native_gap_at(path, UNKNOWN_KEPT)
            && let Some(domain) = listed_domain(row)
        {
            field.domain = FieldDomain::Listed(domain);
            answered.domain = true;
        }
        // A pair's block reader decides its read scope and block family; the other form reads one
        // scalar.
        let block_reader = match &reading {
            Ok(reading) if forms_filled => reading.block_reader().clone(),
            _ => field.reader.clone(),
        };
        let (scope_kind, block_family) = (block_reader.kind, block_reader.family);

        if proved
            && field.read_scope == GrammarProperty::Unresolved
            && let Ok(scope) = self
                .export
                .read_scope(owner, row, scope_kind, &field.members)
        {
            field.read_scope = scope;
            answered.read_scope = true;
        }

        let block = row.block_outcome();

        match &mut field.members {
            FieldMembers::Fields(children) => {
                if let Some(entry) = self.nested_entry(block) {
                    self.level(children, entry, path);
                }
            }
            FieldMembers::Unresolved => {
                field.members = self.members(block, path);

                if field.members != FieldMembers::Unresolved {
                    storage_changed = true;
                    self.gaps
                        .extend(self.unproved_members_gap(field, block, path));
                }
            }
            _ => {}
        }

        let read_filled = forms_filled
            || (kind_before == ReaderKind::Unknown && field.reader.kind != kind_before);

        if storage_changed {
            let repeat_reason = super::fields::unknown_repeat_reason(&field.reader);

            self.gaps.extend(storage_gap(field, &name, repeat_reason));
            answered.replaced_storage = storage_before;
        }
        if read_filled {
            self.gaps
                .extend(semantics_gaps(field).into_iter().map(|(kind, detail)| Gap {
                    kind,
                    subject: Some(field_subject(path)),
                    detail: detail.into(),
                }));
        }
        if family_before == BlockFamily::Unknown {
            self.gaps
                .extend(self.entry_context_gap(block_family, field, path));
        }

        self.answered.insert(path.to_vec(), answered);
    }

    /// The field that `row` of `owner` reads at `path`, which Native does not list, with a gap
    /// for each property that the export leaves unresolved.
    fn added_field(&mut self, owner: &ExportReader, row: &'a Row, path: &[String]) -> Field {
        let condition = if row.unconditional() {
            FieldCondition::Always
        } else {
            FieldCondition::Unresolved
        };
        let reading = self.reading(row);
        let (reader, shape, read) = match &reading {
            Ok(reading) => (
                reading.reader(),
                reading.shape(),
                reading.alternatives(condition),
            ),
            Err(open) => (
                plain_reader(ReaderKind::Unknown, BlockFamily::Unknown),
                open.shape,
                vec![FieldReadAlternative {
                    condition,
                    outcome: FieldReadOutcome::Unresolved,
                }],
            ),
        };
        // The export names no reader identity, so an added field has none: Native's rule for a
        // field without one is an `UnresolvedReader` gap.
        let open = reading.as_ref().err().map(|open| open.gap);
        let identity = match (&reading, open) {
            (_, Some((GapKind::UnresolvedReader, _))) => None,
            (Ok(Reading::ByValueForm { .. }), _) => {
                Some((GapKind::UnresolvedReader, READER_ALTERNATIVES_OPEN))
            }
            _ => Some((GapKind::UnresolvedReader, IDENTITY_OPEN)),
        };
        let members = self.members(row.block_outcome(), path);
        let block_reader = reading.as_ref().map_or(&reader, Reading::block_reader);
        let (scope_kind, block_family) = (block_reader.kind, block_reader.family);
        let read_scope = self.export.read_scope(owner, row, scope_kind, &members);
        let domain = match reader.kind {
            ReaderKind::Keyword => {
                listed_domain(row).map_or(FieldDomain::Unknown, FieldDomain::Listed)
            }
            _ => FieldDomain::Unknown,
        };
        let accepted_categories = super::fields::accepted_categories(block_family);
        let field = Field {
            name: path.last().cloned().unwrap_or_default(),
            reader,
            shape,
            read,
            members,
            domain,
            uses: Vec::new(),
            reference: FieldReference::NotEstablished,
            entry_contexts: Vec::new(),
            read_scope: read_scope.clone().unwrap_or_default(),
            accepted_categories,
            requirement: FieldRequirement::Unknown,
        };
        let open = open
            .into_iter()
            .chain(identity)
            .map(|(kind, detail)| (kind, detail.to_owned()))
            .chain(read_scope.err());

        self.gaps.extend(added_field_gaps(&field, row, open, path));
        self.gaps
            .extend(storage_gap(&field, &path.join("."), REPEAT_OPEN));
        self.gaps
            .extend(self.unproved_members_gap(&field, row.block_outcome(), path));
        self.gaps
            .extend(self.entry_context_gap(block_family, &field, path));

        if field.accepted_categories == AcceptedCategories::Unresolved {
            self.gaps.push(Gap {
                kind: GapKind::UnresolvedPath,
                subject: Some(field_subject(path)),
                detail: super::container_masks::CONTAINER_OPEN.into(),
            });
        }

        field
    }

    /// Whether Native reports `detail` at `path`.
    fn native_gap_at(&self, path: &[String], detail: &str) -> bool {
        self.native_gaps
            .iter()
            .any(|gap| gap.detail == detail && gap_path(gap).as_deref() == Some(path))
    }

    /// Whether the export answers Native's `gap`. `keys_proved` says that the registry's whole
    /// reader tree proves its keys.
    fn answers(&self, gap: &Gap, keys_proved: bool) -> bool {
        let detail = gap.detail.as_str();
        let Some(path) = gap_path(gap) else {
            let registry = matches!(gap.subject, Some(GapSubject::Registry { .. }));

            return registry && keys_proved && answers_key_gap(gap);
        };
        let Some(answered) = self.answered.get(&path) else {
            return false;
        };

        match gap.kind {
            GapKind::UnresolvedReader | GapKind::UnnamedField => {
                answered.proved && answers_key_gap(gap)
            }
            GapKind::UnresolvedPath => {
                answered.proved && answers_key_gap(gap)
                    || answered.read_scope && detail.starts_with(READ_SCOPE_PREFIX)
            }
            GapKind::ReaderSemantics => {
                answered.proved && detail == VALUE_FORM_OPEN
                    || answered.domain
                        && (detail == KEYWORD_DOMAIN_OPEN || OPEN_DOMAIN_REASONS.contains(&detail))
            }
            GapKind::UnresolvedStorage => answered.replaced_storage.as_ref() == Some(gap),
            GapKind::UnresolvedCondition => answered.conditions && detail == CONDITION_OPEN,
            _ => false,
        }
    }
}

/// Whether `gap` says that a path, a reader join or a key name of the registry's walk is open,
/// which a proved reader tree answers.
fn answers_key_gap(gap: &Gap) -> bool {
    match gap.kind {
        GapKind::UnresolvedPath => gap.detail == TOKEN_PATH_OPEN,
        GapKind::UnresolvedReader => gap.detail == READER_JOIN_OPEN,
        GapKind::UnnamedField => gap.detail.ends_with(UNNAMED_PATHS),
        _ => false,
    }
}

/// The subject of a gap at the field at `path`, named as Native names a nested field:
/// `parent.child`.
fn field_subject(path: &[String]) -> GapSubject {
    GapSubject::field(path.join("."))
}

/// The field path that a gap names, or `None` for another subject. Native names a nested field
/// as `parent.child` or by a key path.
fn gap_path(gap: &Gap) -> Option<Vec<String>> {
    match gap.subject.as_ref()? {
        GapSubject::Field { name } => Some(name.split('.').map(Into::into).collect()),
        GapSubject::KeyPath { path } => Some(path.clone()),
        _ => None,
    }
}

/// A row's value reading that the export leaves open, with its shape and its gap.
struct OpenRead {
    shape: FieldShape,
    gap: (GapKind, &'static str),
}

/// What a row establishes about reading its value.
enum Reading {
    /// One reader of every value.
    One((Reader, FieldShape)),
    /// The path tests the value's token: a block and any other value are read in different ways.
    ByValueForm {
        block: (Reader, FieldShape),
        other: (Reader, FieldShape),
    },
}

impl Reading {
    /// The field's shared reader: none is established for two readers.
    fn reader(&self) -> Reader {
        match self {
            Self::One((reader, _)) => reader.clone(),
            Self::ByValueForm { .. } => plain_reader(ReaderKind::Unknown, BlockFamily::Unknown),
        }
    }

    /// The reader that reads the field's block: the one reader, or the block form's. It decides
    /// the read scope and block family; the other form reads one scalar, in no scope.
    fn block_reader(&self) -> &Reader {
        match self {
            Self::One((reader, _))
            | Self::ByValueForm {
                block: (reader, _), ..
            } => reader,
        }
    }

    /// The value form and repeat behavior that every alternative agrees on.
    fn shape(&self) -> FieldShape {
        match self {
            Self::One((_, shape)) => *shape,
            Self::ByValueForm { block, other } => FieldShape {
                value: ValueShape::Any,
                repeat: agreed_repeat(block.1, other.1),
            },
        }
    }

    /// The field's read alternatives; one reading holds where `condition` does.
    fn alternatives(&self, condition: FieldCondition) -> Vec<FieldReadAlternative> {
        let read = |(reader, shape): &(Reader, FieldShape)| FieldReadOutcome::Read {
            reader: reader.clone(),
            shape: *shape,
        };

        match self {
            Self::One(one) => vec![FieldReadAlternative {
                condition,
                outcome: read(one),
            }],
            Self::ByValueForm { block, other } => vec![
                FieldReadAlternative {
                    condition: FieldCondition::ValueForm { block: true },
                    outcome: read(block),
                },
                FieldReadAlternative {
                    condition: FieldCondition::ValueForm { block: false },
                    outcome: read(other),
                },
            ],
        }
    }
}

fn agreed_repeat(first: FieldShape, second: FieldShape) -> RepeatBehavior {
    if first.repeat == second.repeat {
        first.repeat
    } else {
        RepeatBehavior::Unknown
    }
}

/// The shape that `outcome` reads; `keeps_object` says that a repeat of its persistent read
/// merges into the object.
fn outcome_shape(outcome: &Outcome, keeps_object: bool) -> FieldShape {
    // A read into another engine type, such as a vector, can take a block.
    let value = match outcome.kind {
        RowKind::ScalarOther => ValueShape::Unknown,
        _ => outcome.value.unwrap_or(ValueShape::Unknown),
    };

    FieldShape {
        value,
        repeat: match outcome.repeat {
            RepeatBehavior::Unknown if keeps_object => RepeatBehavior::Merges,
            repeat => repeat,
        },
    }
}

/// The `members` whose objects all keep themselves on a persistent read, by the walker's rule
/// checked on each of `holders` (every vtable address point) that holds the member. A member
/// that no point holds, or a holder whose persistent read is not established, does not merge: the
/// check cannot run.
pub(crate) fn merging_members(
    members: BTreeSet<&str>,
    holders: &[MemberHolder],
) -> BTreeSet<String> {
    members
        .into_iter()
        .filter(|member| {
            let mut objects = holders
                .iter()
                .filter(|holder| holder.member.strip_suffix("(CReader&, int)") == Some(*member))
                .peekable();

            objects.peek().is_some()
                && objects.all(|holder| {
                    holder
                        .reader
                        .as_ref()
                        .is_some_and(|reader| persistent_read_merges(reader.family, Some(reader)))
                })
        })
        .map(str::to_owned)
        .collect()
}

/// The name of the walked member reader that `outcome` reads its value with.
fn walked_member(outcome: &Outcome) -> Option<&str> {
    let nested = outcome.nested.as_ref()?;

    nested.reader.is_some().then_some(nested.name.as_str())
}

fn family(outcome: &Outcome, kind: ReaderKind) -> BlockFamily {
    match outcome.family {
        Some(family) => family,
        None if kind == ReaderKind::Block && outcome.items.is_none() => BlockFamily::Unknown,
        None => BlockFamily::NotApplicable,
    }
}

/// A reader of `kind` with no identity. The merge runs after Native's numeric pass, so the
/// reader takes the facts that its kind alone establishes here.
fn plain_reader(kind: ReaderKind, family: BlockFamily) -> Reader {
    let mut reader = Reader {
        numeric: GrammarProperty::Unresolved,
        scoped_operand: GrammarProperty::Unresolved,
        id: None,
        kind,
        family,
    };

    super::numeric::attach_kind_facts(&mut reader);

    reader
}

/// The gaps of what the readers of `field`, its shared reader and the reader of each read
/// alternative, promise and the export does not establish.
fn semantics_gaps(field: &Field) -> Vec<(GapKind, &'static str)> {
    let alternatives = field
        .read
        .iter()
        .filter_map(|alternative| match &alternative.outcome {
            FieldReadOutcome::Read { reader, .. } => Some(reader),
            _ => None,
        });
    let mut gaps = Vec::new();

    for gap in std::iter::once(&field.reader)
        .chain(alternatives)
        .filter_map(|reader| semantics_gap(reader, field))
    {
        if !gaps.contains(&gap) {
            gaps.push(gap);
        }
    }

    gaps
}

/// The gap of what `reader`, a reader of `field`, promises and the export does not establish: a
/// keyword domain, numeric conversion, or the lookup of a key reference.
fn semantics_gap(reader: &Reader, field: &Field) -> Option<(GapKind, &'static str)> {
    match reader.kind {
        ReaderKind::Keyword if field.domain == FieldDomain::Unknown => {
            Some((GapKind::ReaderSemantics, KEYWORD_DOMAIN_OPEN))
        }
        ReaderKind::Integer | ReaderKind::FixedPoint | ReaderKind::Float
            if reader.numeric == GrammarProperty::Unresolved =>
        {
            Some((GapKind::NumericConversion, NUMERIC_OPEN))
        }
        _ if takes_lookup(reader, field) && field.reference == FieldReference::NotEstablished => {
            Some((GapKind::ReaderSemantics, LOOKUP_OPEN))
        }
        _ => None,
    }
}

/// Whether the engine looks up the value that `reader` reads for `field`: a key reference, or a
/// list of them.
fn takes_lookup(reader: &Reader, field: &Field) -> bool {
    let items = match &field.members {
        FieldMembers::Items { reader } => Some(reader.kind),
        _ => None,
    };

    reader.kind == ReaderKind::Reference || items == Some(ReaderKind::Reference)
}

/// Whether a field with reader `kind` and `members` reads its value in no scope: a scalar, a
/// value that it keeps nothing of, or a block of bare values.
fn reads_no_scope(kind: ReaderKind, members: &FieldMembers) -> bool {
    matches!(members, FieldMembers::Items { .. }) || keeps_no_block(kind)
}

/// Whether a reader of `kind` keeps no block: it reads one scalar value, or keeps nothing of the
/// value.
fn keeps_no_block(kind: ReaderKind) -> bool {
    matches!(
        kind,
        ReaderKind::Boolean
            | ReaderKind::Integer
            | ReaderKind::FixedPoint
            | ReaderKind::Float
            | ReaderKind::String
            | ReaderKind::Reference
            | ReaderKind::Keyword
            | ReaderKind::Skipped
            | ReaderKind::Presence
    )
}

/// The sorted names of a complete keyword table, when each value has one.
fn listed_domain(row: &Row) -> Option<Vec<String>> {
    let table = row.keywords.as_ref().filter(|table| table.complete)?;
    let mut names: Vec<String> = table.keys.iter().cloned().collect::<Option<_>>()?;

    names.sort();
    names.dedup();

    (!names.is_empty()).then_some(names)
}

/// Fill the unknown parts of Native's single unconditional reading of `field` from the export's
/// `reader` and `shape`. Returns whether the field's storage facts changed.
fn fill_read(field: &mut Field, reader: &Reader, shape: FieldShape) -> bool {
    let [alternative] = field.read.as_mut_slice() else {
        return false;
    };
    let before = (field.reader.clone(), field.shape);

    match &mut alternative.outcome {
        FieldReadOutcome::Unresolved
            if matches!(field.reader.kind, ReaderKind::Unknown)
                || field.reader.kind == reader.kind =>
        {
            fill_reader(&mut field.reader, reader);
            fill_shape(&mut field.shape, shape);
            alternative.outcome = FieldReadOutcome::Read {
                reader: field.reader.clone(),
                shape: field.shape,
            };
        }
        FieldReadOutcome::Read {
            reader: read,
            shape: read_shape,
        } => {
            fill_reader(read, reader);
            fill_reader(&mut field.reader, reader);
            fill_shape(read_shape, shape);
            fill_shape(&mut field.shape, shape);
        }
        _ => {}
    }

    before != (field.reader.clone(), field.shape)
}

/// Give the read alternatives of Native's `field` the export's proved value forms, which read
/// `block` and `other`. Native's one unconditional, unresolved alternative of a field without a
/// shared reader becomes the pair. Native's two alternatives under unresolved conditions take the value-form
/// conditions when their outcomes fit the pair one to one: an outcome fits a form when it is
/// unresolved or reads the form's reader kind and value shape. Returns whether the alternatives
/// changed.
fn fill_value_forms(
    field: &mut Field,
    block: &(Reader, FieldShape),
    other: &(Reader, FieldShape),
) -> bool {
    let shape = FieldShape {
        value: ValueShape::Any,
        repeat: agreed_repeat(block.1, other.1),
    };
    let filled = match field.read.as_mut_slice() {
        [alternative] => {
            let unresolved = alternative.condition == FieldCondition::Always
                && alternative.outcome == FieldReadOutcome::Unresolved
                && field.reader.id.is_none()
                && field.reader.kind == ReaderKind::Unknown;

            if unresolved {
                field.read = Reading::ByValueForm {
                    block: block.clone(),
                    other: other.clone(),
                }
                .alternatives(FieldCondition::Always);
            }

            unresolved
        }
        [first, second] => resolve_value_forms([first, second], block, other),
        _ => false,
    };

    if filled {
        fill_shape(&mut field.shape, shape);
    }

    filled
}

/// Give `alternatives`, two alternatives under unresolved conditions, the value-form conditions
/// of the forms that their outcomes fit, and fill their unresolved outcomes from the export.
fn resolve_value_forms(
    [first, second]: [&mut FieldReadAlternative; 2],
    block: &(Reader, FieldShape),
    other: &(Reader, FieldShape),
) -> bool {
    // The two forms read different value shapes, so a read outcome fits at most one of them.
    // Only two unresolved outcomes fit both orders, and those take the same answers either way.
    let fits = |alternative: &FieldReadAlternative, (reader, shape): &(Reader, FieldShape)| {
        alternative.condition == FieldCondition::Unresolved
            && match &alternative.outcome {
                FieldReadOutcome::Unresolved => true,
                FieldReadOutcome::Read {
                    reader: native,
                    shape: native_shape,
                } => native.kind == reader.kind && native_shape.value == shape.value,
                _ => false,
            }
    };
    let forms = if fits(first, block) && fits(second, other) {
        [(true, block), (false, other)]
    } else if fits(first, other) && fits(second, block) {
        [(false, other), (true, block)]
    } else {
        return false;
    };

    for (alternative, (block, (reader, shape))) in [first, second].into_iter().zip(forms) {
        alternative.condition = FieldCondition::ValueForm { block };

        match &mut alternative.outcome {
            FieldReadOutcome::Read {
                reader: native,
                shape: native_shape,
            } => {
                fill_reader(native, reader);
                fill_shape(native_shape, *shape);
            }
            outcome => {
                *outcome = FieldReadOutcome::Read {
                    reader: reader.clone(),
                    shape: *shape,
                }
            }
        }
    }

    true
}

/// Fill the unknown kind and family of Native's reader from the export's. A kind that the merge
/// establishes also gives the numeric and scoped-operand facts of that kind, where Native's
/// numeric pass left them unresolved.
fn fill_reader(native: &mut Reader, export: &Reader) {
    if native.kind == ReaderKind::Unknown {
        let mut kind_facts = export.clone();

        super::numeric::attach_kind_facts(&mut kind_facts);
        native.kind = export.kind;

        if native.numeric == GrammarProperty::Unresolved {
            native.numeric = kind_facts.numeric;
        }
        if native.scoped_operand == GrammarProperty::Unresolved {
            native.scoped_operand = kind_facts.scoped_operand;
        }
    }
    if native.family == BlockFamily::Unknown {
        native.family = export.family;
    }
}

fn fill_shape(native: &mut FieldShape, export: FieldShape) {
    if native.value == ValueShape::Unknown {
        native.value = export.value;
    }
    if native.repeat == RepeatBehavior::Unknown {
        native.repeat = export.repeat;
    }
}

/// The gaps of a field that the export adds at `path`: `open`, the gaps of its reading and its
/// read scope, and one for each other property that it leaves unresolved.
fn added_field_gaps(
    field: &Field,
    row: &Row,
    open: impl IntoIterator<Item = (GapKind, String)>,
    path: &[String],
) -> Vec<Gap> {
    let mut details: Vec<(GapKind, String)> = open.into_iter().collect();
    let mut add = |kind, detail: &str| details.push((kind, detail.to_owned()));

    if !row.unconditional() {
        add(GapKind::UnresolvedCondition, CONDITION_OPEN);
    }
    if !row.paths_classified() {
        add(GapKind::UnresolvedPath, PATHS_OPEN);
    }

    for (kind, detail) in semantics_gaps(field) {
        add(kind, detail);
    }

    details
        .into_iter()
        .map(|(kind, detail)| Gap {
            kind,
            subject: Some(field_subject(path)),
            detail,
        })
        .collect()
}

#[cfg(test)]
mod tests;
