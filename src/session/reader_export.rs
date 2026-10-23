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

use super::questions::{
    CONDITION_OPEN, KEYWORD_DOMAIN_OPEN, READER_JOIN_OPEN, TOKEN_PATH_OPEN, UNNAMED_PATHS,
    VALUE_FORM_OPEN, storage_gap,
};
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
const BRANCHES_OPEN: &str = "The key's path tests the value's token: a block and a bare value are \
                             read in different ways, which the reader export does not pair.";
const STORE_OPEN: &str = "The key's path stores a value through no shared reader.";
const READER_OPEN: &str = "The reader export does not establish the field's reader.";
const PATHS_OPEN: &str = "The reader export did not classify every path of the key.";
const REPEAT_OPEN: &str = "The reader export does not establish repeat behavior.";
const UNPROVED_MEMBERS: &str =
    "The reader export lists these nested fields, but its readers do not prove the block's keys.";
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
    kind: RowKind,
    value: Option<ValueShape>,
    reader_kind: Option<ReaderKind>,
    repeat: RepeatBehavior,
    conditional: bool,
    family: Option<BlockFamily>,
    items: Option<ReaderKind>,
    branches: Option<Vec<Branch>>,
    nested: Option<Nested>,
    scope_mask: Option<u64>,
    #[serde(rename = "enum")]
    keywords: Option<KeywordTable>,
    paths: Paths,
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
    Delegates,
    Helper,
    Logged,
    Unclassified,
}

#[derive(Debug, Deserialize)]
struct Branch {
    nested: Option<Nested>,
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

    /// Fill `answer`, Native's registry field answer for `registry`, from the export, and stamp
    /// the merged method. The answer stays unchanged, and the result is `false`, when the export
    /// has no root reader for the registry.
    pub(crate) fn merge(
        &self,
        registry: &str,
        layout: RootLayout,
        answer: &mut Answer<Vec<Field>>,
    ) -> bool {
        let Some(roots) = self.roots(registry) else {
            return false;
        };
        let native_complete = answer.completeness == Completeness::Complete;
        let native_gaps = std::mem::take(&mut answer.gaps);
        let mut merge = Merge::new(self, &native_gaps);

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
                Some(entry) => self.walk_tree(entry, path, &mut seen, &mut failures),
                None => failures.push(Failure::not_walked(path)),
            }
        }

        failures
    }

    fn walk_tree<'a>(
        &'a self,
        entry: &'a str,
        path: Vec<String>,
        seen: &mut BTreeSet<&'a str>,
        failures: &mut Vec<Failure>,
    ) {
        if !seen.insert(entry) {
            return;
        }

        let Some(reader) = self.readers.get(entry) else {
            failures.push(Failure::not_walked(path));
            return;
        };
        let conditions = reader.contract_failures();

        if !conditions.is_empty() {
            failures.push(Failure {
                path: path.clone(),
                conditions,
            });
        }

        for (next, next_path) in reader.subtree(&path) {
            self.walk_tree(next, next_path, seen, failures);
        }
    }

    /// Whether the whole tree under the reader at `entry` proves its keys.
    fn proves_tree<'a>(&'a self, entry: &'a str) -> bool {
        let mut failures = Vec::new();

        self.walk_tree(entry, Vec::new(), &mut BTreeSet::new(), &mut failures);

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
    /// `path`: its base reader and delegates at `path`, and each row's and branch's nested reader
    /// at the row's key.
    fn subtree(&self, path: &[String]) -> Vec<(&str, Vec<String>)> {
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

        readers.extend(at_path.map(|entry| (entry, path.to_vec())));

        for row in &self.rows {
            let nested = row.nested.iter().chain(
                row.branches
                    .iter()
                    .flatten()
                    .filter_map(|branch| branch.nested.as_ref()),
            );
            let key = row.key.clone().unwrap_or_default();

            for entry in nested.filter_map(|nested| nested.reader.as_deref()) {
                readers.push((entry, [path, std::slice::from_ref(&key)].concat()));
            }
        }

        readers
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
    /// The field's storage changed, so its storage gap is computed again.
    storage: bool,
}

/// One merge of the export into an answer.
struct Merge<'a> {
    export: &'a ReaderExport,
    native_gaps: &'a [Gap],
    /// What the export answers, by field path.
    answered: BTreeMap<Vec<String>, Answered>,
    /// The gaps of added fields and the storage gaps computed again.
    gaps: Vec<Gap>,
    /// The readers whose rows are being merged, outermost first: a nested reader that is one of
    /// them is not expanded again.
    ancestors: Vec<&'a str>,
}

impl<'a> Merge<'a> {
    fn new(export: &'a ReaderExport, native_gaps: &'a [Gap]) -> Self {
        Self {
            export,
            native_gaps,
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

    /// The walked reader that `row` reads the keys of its block with, unless it is being merged
    /// already. A reader that lists no key gives no members: a serializer whose member reader is
    /// `CPersistent::ReadMember` reads its values through its own `Read`, which the export does
    /// not walk.
    fn nested_entry(&self, row: &'a Row) -> Option<&'a str> {
        let entry = row.nested.as_ref()?.reader.as_deref()?;
        let reader = self.export.readers.get(entry)?;

        let lists_keys = reader.rows.iter().any(|row| row.key.is_some());

        (!self.ancestors.contains(&entry) && reader.reads_keys() && lists_keys).then_some(entry)
    }

    /// The gap of a field whose nested fields come from a reader tree that does not prove its
    /// keys: the list can miss a key.
    fn unproved_members_gap(&self, field: &Field, row: &Row, path: &[String]) -> Option<Gap> {
        let entry = row.nested.as_ref()?.reader.as_deref()?;

        (matches!(field.members, FieldMembers::Fields(_)) && !self.export.proves_tree(entry)).then(
            || Gap {
                kind: GapKind::UnresolvedStorage,
                subject: Some(field_subject(path)),
                detail: UNPROVED_MEMBERS.into(),
            },
        )
    }

    fn members(&mut self, row: &'a Row, path: &[String]) -> FieldMembers {
        if let Some(entry) = self.nested_entry(row) {
            let mut children = Vec::new();

            self.level(&mut children, entry, path);

            return FieldMembers::Fields(children);
        }

        match (row.items, row.reader_kind) {
            (Some(items), _) => FieldMembers::Items {
                reader: plain_reader(items, BlockFamily::NotApplicable),
            },
            (None, Some(kind)) if is_scalar(kind) => FieldMembers::None,
            _ => FieldMembers::Unresolved,
        }
    }

    /// Whether the export proves `row` completely: one unconditional reading, every path
    /// classified, and the keys of its nested reader's tree.
    fn proves(&self, row: &Row) -> bool {
        let nested_proved = row.nested.as_ref().is_none_or(|nested| {
            nested
                .reader
                .as_deref()
                .is_some_and(|entry| self.export.proves_tree(entry))
        });

        !row.conditional
            && row.paths.unknown == 0
            && !row.paths.truncated
            && reading(row).is_ok()
            && nested_proved
    }

    /// Fill the unresolved properties of Native's `field`, at `path`, from `row` of `owner`.
    fn fill(&mut self, field: &mut Field, owner: &ExportReader, row: &'a Row, path: &[String]) {
        let unconditional = !row.conditional
            && matches!(field.read.as_slice(), [alternative] if alternative.condition == FieldCondition::Always);

        // A conditional read can give each alternative its own reader and members; the export
        // pairs none of them.
        if !unconditional {
            return;
        }

        let kind_before = field.reader.kind;
        let proved = self.proves(row);
        let mut answered = Answered {
            proved,
            ..Answered::default()
        };

        if let Ok((reader, shape)) = reading(row) {
            answered.storage |= fill_read(field, &reader, shape);
        }
        if field.domain == FieldDomain::Unknown
            && field.reader.kind == ReaderKind::Keyword
            && !self.native_gap_at(path, UNKNOWN_KEPT)
            && let Some(domain) = listed_domain(row)
        {
            field.domain = FieldDomain::Listed(domain);
            answered.domain = true;
        }
        if proved
            && field.read_scope == GrammarProperty::Unresolved
            && let Ok(scope) = self
                .export
                .read_scope(owner, row, field.reader.kind, &field.members)
        {
            field.read_scope = scope;
            answered.read_scope = true;
        }

        match &mut field.members {
            FieldMembers::Fields(children) => {
                if let Some(entry) = self.nested_entry(row) {
                    self.level(children, entry, path);
                }
            }
            FieldMembers::Unresolved => {
                field.members = self.members(row, path);

                if field.members != FieldMembers::Unresolved {
                    answered.storage = true;
                    self.gaps
                        .extend(self.unproved_members_gap(field, row, path));
                }
            }
            _ => {}
        }

        let kind_filled = kind_before == ReaderKind::Unknown && field.reader.kind != kind_before;

        if answered.storage {
            let repeat_reason = super::fields::unknown_repeat_reason(&field.reader);

            self.gaps
                .extend(storage_gap(field, &path.join("."), repeat_reason));
        }
        if kind_filled && let Some((kind, detail)) = semantics_gap(field) {
            self.gaps.push(Gap {
                kind,
                subject: Some(field_subject(path)),
                detail: detail.into(),
            });
        }

        self.answered.insert(path.to_vec(), answered);
    }

    /// The field that `row` of `owner` reads at `path`, which Native does not list, with a gap
    /// for each property that the export leaves unresolved.
    fn added_field(&mut self, owner: &ExportReader, row: &'a Row, path: &[String]) -> Field {
        let reading = reading(row);
        let (reader, shape, outcome, open) = match reading {
            Ok((reader, shape)) => (
                reader.clone(),
                shape,
                FieldReadOutcome::Read { reader, shape },
                None,
            ),
            Err(open) => (
                plain_reader(ReaderKind::Unknown, BlockFamily::Unknown),
                open.shape,
                FieldReadOutcome::Unresolved,
                Some(open.gap),
            ),
        };
        // The export names no reader identity, so an added field has none: Native's rule for a
        // field without one is an `UnresolvedReader` gap.
        let identity = match open {
            Some((GapKind::UnresolvedReader, _)) => None,
            _ => Some((GapKind::UnresolvedReader, IDENTITY_OPEN)),
        };
        let condition = if row.conditional {
            FieldCondition::Unresolved
        } else {
            FieldCondition::Always
        };
        let members = self.members(row, path);
        let read_scope = self.export.read_scope(owner, row, reader.kind, &members);
        let domain = match reader.kind {
            ReaderKind::Keyword => {
                listed_domain(row).map_or(FieldDomain::Unknown, FieldDomain::Listed)
            }
            _ => FieldDomain::Unknown,
        };
        let accepted_categories = if is_scalar(reader.kind) {
            AcceptedCategories::NotApplicable
        } else {
            AcceptedCategories::Unresolved
        };
        let field = Field {
            name: path.last().cloned().unwrap_or_default(),
            reader,
            shape,
            read: vec![FieldReadAlternative { condition, outcome }],
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
            .extend(self.unproved_members_gap(&field, row, path));

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
            GapKind::UnresolvedStorage => answered.storage,
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

/// What `row` establishes about reading its value. A row that tests the value's token
/// (`branches`) and a row that only stores (`store`) stay open; step 3b replaces those two arms
/// when the exporter proves their outcomes.
fn reading(row: &Row) -> Result<(Reader, FieldShape), OpenRead> {
    // A read into another engine type, such as a vector, can take a block.
    let value = match row.kind {
        RowKind::ScalarOther => ValueShape::Unknown,
        _ => row.value.unwrap_or(ValueShape::Unknown),
    };
    let shape = FieldShape {
        value,
        repeat: repeat(row),
    };

    if row.branches.is_some() {
        return Err(OpenRead {
            shape: FieldShape {
                value: ValueShape::Any,
                ..shape
            },
            gap: (GapKind::ReaderSemantics, BRANCHES_OPEN),
        });
    }

    match (row.kind, row.reader_kind) {
        (RowKind::Store, _) => Err(OpenRead {
            shape,
            gap: (GapKind::ReaderSemantics, STORE_OPEN),
        }),
        (_, Some(kind)) if kind != ReaderKind::Unknown => {
            Ok((plain_reader(kind, family(row, kind)), shape))
        }
        _ => Err(OpenRead {
            shape,
            gap: (GapKind::UnresolvedReader, READER_OPEN),
        }),
    }
}

/// A persistent read into the owner's own member, with no reset, keeps the object: a repeat
/// merges into it.
fn repeat(row: &Row) -> RepeatBehavior {
    let in_place = row.kind == RowKind::Persistent
        && row
            .nested
            .as_ref()
            .is_some_and(|nested| nested.reader.is_some());

    match row.repeat {
        RepeatBehavior::Unknown if in_place => RepeatBehavior::Merges,
        repeat => repeat,
    }
}

fn family(row: &Row, kind: ReaderKind) -> BlockFamily {
    match row.family {
        Some(family) => family,
        None if kind == ReaderKind::Block && row.items.is_none() => BlockFamily::Unknown,
        None => BlockFamily::NotApplicable,
    }
}

fn plain_reader(kind: ReaderKind, family: BlockFamily) -> Reader {
    Reader {
        numeric: GrammarProperty::Unresolved,
        scoped_operand: GrammarProperty::Unresolved,
        id: None,
        kind,
        family,
    }
}

/// The gap of what a field's reader kind promises and the export does not establish: a keyword
/// domain, numeric conversion, or the lookup of a key reference.
fn semantics_gap(field: &Field) -> Option<(GapKind, &'static str)> {
    match field.reader.kind {
        ReaderKind::Keyword if field.domain == FieldDomain::Unknown => {
            Some((GapKind::ReaderSemantics, KEYWORD_DOMAIN_OPEN))
        }
        ReaderKind::Integer | ReaderKind::FixedPoint | ReaderKind::Float
            if field.reader.numeric == GrammarProperty::Unresolved =>
        {
            Some((GapKind::NumericConversion, NUMERIC_OPEN))
        }
        _ if takes_lookup(field) && field.reference == FieldReference::NotEstablished => {
            Some((GapKind::ReaderSemantics, LOOKUP_OPEN))
        }
        _ => None,
    }
}

/// Whether the engine looks up the value of `field`, a key reference or a list of them.
fn takes_lookup(field: &Field) -> bool {
    let items = match &field.members {
        FieldMembers::Items { reader } => Some(reader.kind),
        _ => None,
    };

    field.reader.kind == ReaderKind::Reference || items == Some(ReaderKind::Reference)
}

/// Whether a field with reader `kind` and `members` reads its value in no scope: a scalar, or a
/// block of bare values.
fn reads_no_scope(kind: ReaderKind, members: &FieldMembers) -> bool {
    matches!(members, FieldMembers::Items { .. }) || is_scalar(kind)
}

/// Whether a reader of `kind` reads one scalar value.
fn is_scalar(kind: ReaderKind) -> bool {
    matches!(
        kind,
        ReaderKind::Boolean
            | ReaderKind::Integer
            | ReaderKind::FixedPoint
            | ReaderKind::Float
            | ReaderKind::String
            | ReaderKind::Reference
            | ReaderKind::Keyword
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

fn fill_reader(native: &mut Reader, export: &Reader) {
    if native.kind == ReaderKind::Unknown {
        native.kind = export.kind;
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

    if row.conditional {
        add(GapKind::UnresolvedCondition, CONDITION_OPEN);
    }
    if row.paths.unknown > 0 || row.paths.truncated {
        add(GapKind::UnresolvedPath, PATHS_OPEN);
    }

    if let Some((kind, detail)) = semantics_gap(field) {
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
