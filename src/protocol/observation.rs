#![cfg_attr(
    not(all(target_os = "macos", target_arch = "aarch64")),
    allow(dead_code)
)]
//! The private wire between the supervisor and its debugger worker. These Rust types are the
//! authority; the worker's Python schemas (`protocol.py`) are generated from them.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(crate) const MAX_RECORD: usize = 64 * 1024;
pub(crate) const MAX_TRACE: usize = 4 * 1024 * 1024;
/// Bound of the loaded modifier table file.
pub(crate) const MAX_MODIFIER_TABLE: usize = 32 * 1024 * 1024;

/// Developer-only worker progress, overwritten atomically; never used for admission.
pub(crate) const MAX_WORKER_DIAGNOSTICS: usize = 16 * 1024;

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerDiagnostics {
    pub attempt: String,
    pub game: u32,
    pub phase: String,
    pub context: Option<String>,
    pub thread: Option<u64>,
    pub operation: String,
    pub details: BTreeMap<String, String>,
    /// Worker monotonic time since diagnostic initialization, not supervisor time.
    pub elapsed_milliseconds: u64,
    /// Deadline on the same clock; absent when the worker has no active deadline.
    pub deadline_milliseconds: Option<u64>,
    pub last_attempted_call: Option<DiagnosticCall>,
    pub last_completed_call: Option<DiagnosticCall>,
    pub hooks: Vec<DiagnosticHook>,
    pub failure: Option<DiagnosticFailure>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiagnosticCall {
    pub ordinal: u64,
    pub operation: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiagnosticHook {
    pub name: String,
    pub enabled: bool,
    pub locations: u32,
    pub resolved: u32,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiagnosticFailure {
    pub kind: String,
    pub reason: String,
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerRequest {
    pub attempt: String,
    pub game: u32,
    pub executable: String,
    pub target: String,
    /// SHA-256 of each file of the worker package, by file name.
    pub source_hashes: BTreeMap<String, String>,
    pub machine: Machine,
    /// The registries to observe, by content directory.
    pub registries: BTreeMap<String, RegistryBinding>,
    /// A deliberate fault, applied only to its named observation.
    pub fault: Option<crate::protocol::session::Fault>,
    pub fixture: Option<FixtureSetup>,
    /// Read the loaded modifier table at the documentation point, and pause there.
    pub modifiers: Option<ModifierTableBinding>,
    pub script_checks: Option<super::script_check::ScriptCheckBinding>,
    pub deadline_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkerHello {
    pub attempt: String,
    pub game: u32,
    pub worker: u32,
    pub target: String,
    pub source_hashes: BTreeMap<String, String>,
    pub python: String,
    pub lldb: String,
    pub module: String,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResumeGrant {
    pub attempt: String,
    pub game: u32,
    pub worker: u32,
}

/// The architecture of the game and the registers that its member function hooks read.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Machine {
    pub architecture: String,
    pub spawn_preference: i32,
    pub registers: EntryRegisters,
}

/// The register that holds each value at a member function's entry: the receiver (`owner`), the
/// return address (`return`) and the arguments that a hook reads (`file`, `reader` and
/// `field-token`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EntryRegisters {
    pub owner: String,
    #[serde(rename = "return")]
    pub return_address: String,
    pub file: String,
    pub reader: String,
    #[serde(rename = "field-token")]
    pub field_token: String,
}

/// Diagnostic producer stages. Strings remain open on the wire so unknown stages lose coverage.
pub(crate) mod diagnostic_stage {
    pub const READER_MALFORMED: &str = "reader-malformed-report";
    pub const READER_UNEXPECTED: &str = "reader-unexpected-report";
    pub const ENGINE_PARSER: &str = "engine-parser-log";
    pub const ENGINE_VALIDATION: &str = "engine-validation-log";

    pub(super) fn python_names() -> serde_json::Value {
        serde_json::json!({
            "reader_malformed": READER_MALFORMED,
            "reader_unexpected": READER_UNEXPECTED,
            "engine_parser": ENGINE_PARSER,
            "engine_validation": ENGINE_VALIDATION,
        })
    }
}

/// Each documented unit variant contributes its serde value and a Python key.
fn python_variants(schema: schemars::Schema) -> serde_json::Map<String, serde_json::Value> {
    let variants = schema
        .get("oneOf")
        .and_then(serde_json::Value::as_array)
        .expect("each wire enum is a documented unit variant");

    variants
        .iter()
        .map(|variant| {
            let wire = variant["const"]
                .as_str()
                .expect("each wire enum has a string wire value");

            (wire.replace('-', "_").to_ascii_lowercase(), wire.into())
        })
        .collect()
}

pub(crate) fn python_bindings() -> String {
    let controls = python_variants(schemars::schema_for!(
        crate::protocol::session::ObservationControl
    ));
    let readers = python_variants(schemars::schema_for!(crate::ReaderKind));
    let hooks = super::hooks::python_names();
    let schemas = serde_json::json!({
        "request": schemars::schema_for!(WorkerRequest),
        "reader_kind": schemars::schema_for!(crate::ReaderKind),
        "hello": schemars::schema_for!(WorkerHello),
        "grant": schemars::schema_for!(ResumeGrant),
        "record": schemars::schema_for!(WorkerRecord),
        "pause": schemars::schema_for!(PauseWitness),
        "pause_check": schemars::schema_for!(PauseCheck),
        "modifier_table": schemars::schema_for!(ModifierTable),
        "script_check": schemars::schema_for!(super::script_check::CheckRequest),
        "script_reply": schemars::schema_for!(super::script_check::CheckReply),
        "worker_diagnostics": schemars::schema_for!(WorkerDiagnostics),
    });
    format!(
        "# Generated by Native's private protocol authority. Do not edit.\nimport json\nMAX_RECORD = {MAX_RECORD}\nMAX_TRACE = {MAX_TRACE}\nMAX_WORKER_DIAGNOSTICS = {MAX_WORKER_DIAGNOSTICS}\nMAX_MODIFIER_TABLE = {MAX_MODIFIER_TABLE}\nCONTROL = json.loads({controls:?})\nREADER_KIND = json.loads({readers:?})\nHOOK = json.loads({hooks:?})\nDIAGNOSTIC_STAGE = json.loads({stages:?})\nSCRIPT_LIMITS = json.loads({limits:?})\nSCHEMAS = json.loads({schemas:?})\n{codec}",
        limits = serde_json::json!({"text": crate::script::MAX_TEXT_BYTES, "diagnostics": crate::script::MAX_DIAGNOSTICS,
            "seconds": crate::script::CHECK_SECONDS, "checks": crate::script::MAX_CHECKS,
            "durations": super::script_check::MAX_DURATION_RECEIVERS,
            "reply": super::script_check::MAX_REPLY}).to_string(),
        controls = serde_json::to_string(&controls).unwrap(),
        schemas = serde_json::to_string(&schemas).unwrap(),
        readers = serde_json::to_string(&readers).unwrap(),
        hooks = serde_json::to_string(&hooks).unwrap(),
        stages = serde_json::to_string(&diagnostic_stage::python_names()).unwrap(),
        codec = include_str!("codec.py"),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_reader_kind_uses_its_serde_value_on_both_sides() {
        for value in super::python_variants(schemars::schema_for!(crate::ReaderKind)).values() {
            let kind: crate::ReaderKind = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(kind).unwrap(), *value);
            let event = serde_json::json!({
                "kind": "field-authority", "question": 0, "reader_id": null,
                "reader_kind": value, "reader_family": "Unknown", "storage_decoder": null, "unavailable": "unsupported"
            });
            let typed: super::FixtureEvent = serde_json::from_value(event.clone()).unwrap();
            assert_eq!(serde_json::to_value(typed).unwrap(), event);
        }
    }

    #[test]
    fn every_block_family_round_trips_through_reader_authority() {
        for value in super::python_variants(schemars::schema_for!(crate::BlockFamily)).values() {
            let event = serde_json::json!({
                "kind": "field-authority", "question": 0, "reader_id": null,
                "reader_kind": "Block", "reader_family": value,
                "storage_decoder": null, "unavailable": "unsupported"
            });
            let typed: super::FixtureEvent = serde_json::from_value(event.clone()).unwrap();
            assert_eq!(serde_json::to_value(typed).unwrap(), event);
        }
    }

    #[test]
    fn generated_python_matches_wire_authority() {
        let generated = super::python_bindings();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/binding/platform/macos/observation/protocol.py");
        if std::env::var_os("PDX_NATIVE_GENERATE_PROTOCOL").is_some() {
            std::fs::write(&path, &generated).unwrap();
        }
        assert_eq!(std::fs::read_to_string(path).unwrap(), generated);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegistryBinding {
    pub name: String,
    pub directory: String,
    pub load_entry: u64,
    pub directory_offset: u64,
    pub data_offset: u64,
    pub count_offset: u64,
    pub key_offset: Option<u64>,
    pub key_unavailable: Option<String>,
    pub pointer_size: u64,
    pub string_tag_offset: u64,
}

/// Exact-build entry points and source-location layout for fixture outcomes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureBinding {
    pub reader_lexer_offset: u64,
    pub lexer_file_offset: u64,
    pub file_name_offset: u64,
    pub string_tag_offset: u64,
    pub file_line_offset: u64,
    /// `CReader::ReportMalformed(CString const&)`, shared by every registry reader.
    pub malformed_entry: u64,
    /// `CReader::ReportUnexpected(CString const&)`, shared by every registry reader.
    pub unexpected_entry: u64,
    pub validation: Option<FixtureValidationBinding>,
}

/// Formatted logger arguments and the terminal after content validation on one exact build.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureValidationBinding {
    pub log_entry: u64,
    pub log_text_register: String,
    pub unformatted_log_entry: u64,
    pub stream_log_entry: u64,
    pub stream_log_text_register: String,
    pub sourced_log_entry: u64,
    pub sourced_log_text_register: String,
    pub sourced_log_owner_register: String,
    pub sourced_log_source_offset: u64,
    /// Bytes below `x29` of the instance source that the engine error generated.
    pub sourced_log_generated_frame_offset: u64,
    pub source_file_prefix: String,
    pub source_line_prefix: String,
}

/// Exact-build loader hooks for the fixture's registry.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureOutcomeRegistryBinding {
    pub load_entry: u64,
    pub reader_entry: u64,
    pub reader_return: u64,
    pub constructor_entry: u64,
    pub member_entry: u64,
    pub inline: Option<FixtureInlineLoader>,
}

/// File and object boundaries within one exact-build inline loader.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureInlineLoader {
    pub root_return: u64,
    pub key_storage: FixtureStorageBinding,
}

/// Proven embedded receiver and the member routine installed by its constructor.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureNestedField {
    pub parent_token: u64,
    pub owner_offset: u64,
    pub member_entry: u64,
}

/// A decoder whose representation is established by the selected build binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) enum FixtureStorageDecoder {
    String,
    Integer,
    Float,
    Integer16,
    FixedPoint {
        scale: u64,
    },
    ScopedNumeric {
        literal: ScopedLiteralDecoder,
        layout: ScopedStorageLayout,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) enum ScopedLiteralDecoder {
    Integer,
    FixedPoint { scale: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScopedStorageLayout {
    pub literal: u64,
    pub location: u64,
    pub trigger: u64,
    pub script_value: u64,
    pub modifier: u64,
    pub modifier_unset: u64,
    pub variable: u64,
}

impl FixtureStorageDecoder {
    pub(crate) fn reader_kind(self) -> crate::ReaderKind {
        match self {
            Self::String => crate::ReaderKind::String,
            Self::Integer | Self::Integer16 => crate::ReaderKind::Integer,
            Self::Float => crate::ReaderKind::Float,
            Self::FixedPoint { .. } => crate::ReaderKind::FixedPoint,
            Self::ScopedNumeric { .. } => crate::ReaderKind::ScopedNumeric,
        }
    }

    pub(crate) fn accepts(self, value: &crate::FixtureValue) -> bool {
        match (self, value) {
            (Self::String, crate::FixtureValue::String(_))
            | (Self::Integer, crate::FixtureValue::Integer(_))
            | (Self::Float, crate::FixtureValue::Float { .. })
            | (Self::Integer16, crate::FixtureValue::Integer16 { .. }) => true,
            (Self::FixedPoint { scale }, crate::FixtureValue::FixedPoint { scale: actual, .. }) => {
                scale > 0 && scale == *actual
            }
            (Self::ScopedNumeric { literal, .. }, crate::FixtureValue::ScopedNumeric(value)) => {
                match (literal, &value.literal) {
                    (ScopedLiteralDecoder::Integer, crate::ScopedNumericLiteral::Integer(_)) => {
                        true
                    }
                    (
                        ScopedLiteralDecoder::FixedPoint { scale },
                        crate::ScopedNumericLiteral::FixedPoint { scale: actual, .. },
                    ) => scale > 0 && scale == *actual,
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

/// One field's exact token and optional proven owner storage.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureOutcomeFieldBinding {
    pub token: u64,
    pub name: String,
    pub storage: Option<FixtureStorageBinding>,
}

/// A proven owner-relative destination and its exact-build decoder.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureStorageBinding {
    pub offset: u64,
    pub decoder: FixtureStorageDecoder,
}

/// One public question resolved against static reader evidence and exact live bindings.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureQuestionSetup {
    pub index: u64,
    pub definition: String,
    pub field: String,
    pub nested: Option<FixtureNestedField>,
    pub parent_field: Option<String>,
    pub parsing: bool,
    pub diagnostics: bool,
    pub reader_id: Option<String>,
    pub reader_kind: crate::ReaderKind,
    pub reader_family: crate::BlockFamily,
    pub token: Option<u64>,
    pub storage: Option<FixtureStorageBinding>,
    pub storage_unavailable: Option<String>,
}

impl FixtureQuestionSetup {
    /// A question with no established reader, token or storage yet.
    pub(crate) fn unresolved(index: usize, question: &crate::FixtureFieldQuestion) -> Self {
        Self {
            index: index as u64,
            definition: question.definition.clone(),
            field: question.field.clone(),
            nested: None,
            parent_field: question.parent_field.clone(),
            parsing: question.parsing,
            diagnostics: question.diagnostics,

            reader_id: None,
            reader_kind: crate::ReaderKind::Unknown,
            reader_family: crate::BlockFamily::Unknown,
            token: None,
            storage: None,
            storage_unavailable: None,
        }
    }
}

/// The validated observation selection, resolved before starting the worker.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureSetup {
    pub file: String,
    /// The terminal after content validation, when the window includes validation.
    pub validation_complete_entry: Option<u64>,
    pub questions: Vec<FixtureQuestionSetup>,
    pub registry: FixtureOutcomeRegistryBinding,
    pub bindings: FixtureBinding,
}

/// Where the engine holds its loaded modifier table and the names of the table's tokens, and the
/// function whose return is the documentation point. Resolved for one exact build.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModifierTableBinding {
    pub documentation_entry: u64,
    pub definitions: u64,
    pub array_data_offset: u64,
    pub array_count_offset: u64,
    pub definition_stride: u64,
    pub token_offset: u64,
    pub mask_offset: u64,
    pub lookup: u64,
    pub lookup_size: u64,
    pub lookup_stride: u64,
    pub string_tag_offset: u64,
    /// Registries whose item keys the worker reads at the documentation point, by content
    /// directory.
    pub registries: BTreeMap<String, ModifierRegistryBinding>,
}

/// One registry's database instance and item layout.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModifierRegistryBinding {
    /// The global that points to the registry's database.
    pub instance: u64,
    pub directory_offset: u64,
    pub data_offset: u64,
    pub count_offset: u64,
    pub pointer_size: u64,
    pub key_offset: Option<u64>,
    pub key_unavailable: Option<String>,
}

/// What the worker read at the documentation point. It writes this once to
/// `loaded-modifiers.json`; its stream record carries the file's size and SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModifierTable {
    pub attempt: String,
    /// The table's entries, in table order.
    pub entries: Vec<ModifierEntry>,
    /// The item keys of each bound registry, or why they could not be read.
    pub registries: BTreeMap<String, RegistryKeys>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModifierEntry {
    pub name: String,
    pub mask: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RegistryKeys {
    Keys(Vec<String>),
    Unavailable(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PauseWitness {
    pub attempt: String,
    pub game: u32,
    pub worker: u32,
    pub thread: u64,
    pub returned: Vec<String>,
    pub generation: u64,
    pub state: PauseState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PauseState {
    Held,
    Checking(u64),
    Failed(String),
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PauseCheck {
    pub attempt: String,
    pub game: u32,
    pub generation: u64,
}

/// One record of the worker's stream.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct WorkerRecord {
    /// Position in the stream, from 1. A hole means that a record was lost.
    pub seq: u64,
    /// Game thread on which the event occurred, when the event has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<u64>,
    /// Attempt identity of the session that wrote the record.
    pub run: String,
    #[serde(flatten)]
    pub event: WorkerEvent,
}

/// State of one debugger hook immediately before the game resumes.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct Hook {
    pub enabled: bool,
    pub locations: u64,
    pub resolved: u64,
    pub hits: u64,
}
impl Hook {
    /// Whether the hook is enabled at exactly one resolved location and has not been hit.
    pub fn active_before_resume(&self) -> bool {
        self.enabled && self.locations == 1 && self.resolved == 1 && self.hits == 0
    }
}

/// The top stack frame of one game thread at the suspended launch.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub(crate) struct Frame {
    pub function: String,
}

/// What the worker saw. `owner` and `object` are engine addresses as hexadecimal text; they
/// join records to each other and never leave the supervisor.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum WorkerEvent {
    /// Events of the bounded consumer fixture window.
    Fixture { event: FixtureEvent },
    /// The worker set its hooks; the debugger has not attached yet. `hooks` names every hook the
    /// session requires, including one that a fault control leaves out.
    HooksRequested { hooks: Vec<String> },
    /// The debugger attached to the game while it was still suspended at its first instruction.
    LaunchStopped {
        error: String,
        pid: u64,
        triple: String,
        frames: Vec<Frame>,
    },
    /// Every hook in this map was in place before the game ran any code.
    HooksActiveBeforeResume { hooks: BTreeMap<String, Hook> },
    /// The worker let the game run.
    Resume { error: String },
    /// A registry's loader was entered. `owner` is the loader's receiver.
    RegistryLoadStart {
        name: String,
        owner: String,
        directory: String,
    },
    /// The same loader returned to its caller.
    RegistryLoadReturned { name: String, owner: String },
    /// The worker started to read the collection, which holds `count` slots.
    RegistrySnapshot {
        name: String,
        owner: String,
        directory: String,
        count: u64,
    },
    /// One slot of the collection.
    RegistryEntry {
        name: String,
        owner: String,
        index: u64,
        object: String,
        key: String,
    },
    /// The terminal of one registry: the worker's own count of entries, and the sequence number
    /// that the worker gave this record.
    RegistryEnd {
        name: String,
        owner: String,
        count: u64,
        #[serde(rename = "producerLastSequence")]
        producer_last_sequence: u64,
    },
    /// The worker could not observe this registry.
    RegistryUnavailable { name: String, reason: String },
    /// The collection was reached, but this binding cannot read its item keys.
    RegistryUnsupported { name: String, reason: String },
    /// The engine entered the function that documents its modifiers.
    ModifierDocumentationEntered,
    /// The worker read the modifier table when that function returned, and wrote it once to
    /// `loaded-modifiers.json`: `count` entries, in a file of `bytes` bytes with this SHA-256.
    ModifierTable {
        count: u64,
        bytes: u64,
        sha256: String,
    },
    /// The terminal of the modifier observation, and the sequence number that the worker gave
    /// this record.
    ModifierTableEnd {
        count: u64,
        #[serde(rename = "producerLastSequence")]
        producer_last_sequence: u64,
    },
    /// The worker could not read the modifier table.
    ModifierUnavailable { reason: String },
    /// The game is held at a safe pause, after these registries returned from their loaders.
    /// `cause` says what ended the observation and left the game held.
    SessionPaused {
        returned: Vec<String>,
        cause: PauseCause,
    },
    /// The debugger could not attach, a hook was missing or late, or the supervisor's permission
    /// to resume did not arrive.
    CapabilityUnavailable { reason: String },
    /// The game was not suspended at its first instruction when the debugger attached.
    EarlyActivationUnavailable { reason: String },
    /// A hook callback failed outside the observation of one registry.
    CallbackError {
        #[serde(default)]
        error: String,
    },
    /// The game stopped on a native exception.
    NativeException {
        #[serde(default)]
        reason: String,
    },
    /// The worker reached the point where the worker-loss fault stops it.
    WorkerLossReady,
    /// The worker ended in order.
    WorkerFinished,
}

impl WorkerEvent {
    /// The cause that the worker named when it failed: an unavailable capability or early
    /// activation, a callback error or a native exception. Any other event names none.
    pub(crate) fn failure_reason(&self) -> Option<&str> {
        match self {
            Self::CapabilityUnavailable { reason }
            | Self::EarlyActivationUnavailable { reason }
            | Self::NativeException { reason } => Some(reason),
            Self::CallbackError { error } => Some(error),
            _ => None,
        }
    }
}

/// Why the worker held the game where it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PauseCause {
    /// Every registry whose hook was active returned from its initial loader.
    LoadersReturned,
    /// The engine's modifier documentation returned, after all content loaded.
    ContentLoaded,
    /// The worker's deadline passed first, and the worker stopped the game where it was.
    Deadline,
}

/// Worker events inside one fixture window. Thread and sequence belong to the outer record.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum FixtureEvent {
    LoadStart {
        file: String,
    },
    Definition {
        file: String,
        line: u64,
        definition: String,
        owner: String,
    },
    FieldAuthority {
        question: u64,
        reader_id: Option<String>,
        reader_kind: crate::ReaderKind,
        #[serde(default)]
        reader_family: crate::BlockFamily,
        storage_decoder: Option<FixtureStorageDecoder>,
        unavailable: Option<String>,
    },
    FieldStorage {
        question: u64,
        file: String,
        line: u64,
        definition: String,
        field: String,
        owner: String,
        occurrence: u64,
        value: crate::FixtureValue,
    },
    FieldParse {
        question: u64,
        file: String,
        line: u64,
        definition: String,
        field: String,
        owner: String,
        occurrence: u64,
        returned: bool,
    },
    ParsingTerminal {
        question: u64,
        count: u64,
        unavailable: Option<String>,
    },
    Diagnostic {
        text: String,
        stage: String,
        file: Option<String>,
        line: Option<u64>,
        definition: Option<String>,
        field: Option<String>,
        occurrence: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_field: Option<String>,
    },
    FieldTerminal {
        question: u64,
        owner: Option<String>,
        definition_line: Option<u64>,
        reader_id: Option<String>,
        reader_kind: crate::ReaderKind,
        #[serde(default)]
        reader_family: crate::BlockFamily,
        final_value: Option<crate::FixtureValue>,
        unavailable: Option<String>,
    },
    DiagnosticsTerminal {
        count: u64,
    },
    DiagnosticsUnavailable {
        reason: String,
    },
    ValidationComplete {
        file: String,
    },
    LoadReturned {
        file: String,
    },
    End {
        #[serde(default)]
        field_outcomes: u64,
        #[serde(default)]
        diagnostics: u64,
        producer_last_sequence: u64,
    },
    Unavailable {
        reason: String,
    },
}
