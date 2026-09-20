//! The one result type, its source stamp, its typed gaps, and the one error type.
use serde::{Deserialize, Serialize};

/// An answer to one engine question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Answer<T> {
    /// What was established. A partial answer keeps every established part.
    pub value: T,
    /// Whether the stated search or window completed. Never complete game knowledge.
    pub completeness: Completeness,
    /// What is missing. Empty when the answer is complete.
    pub gaps: Vec<Gap>,
    /// Which build and method gave this answer.
    pub source: Source,
}

/// Whether the method's stated search completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Completeness {
    /// The search or window completed. An empty value then means that nothing was found.
    Complete,
    /// Part of the search did not complete; `gaps` says which part.
    Partial,
}

/// One missing part of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    /// Machine-readable reason.
    pub kind: GapKind,
    /// The registry, field, or other public name that the gap concerns, when one exists.
    pub subject: Option<String>,
    /// Human-readable detail. It holds no address, symbol, or other native detail.
    pub detail: String,
}

/// Why a part of an answer is missing. A gap never means that the game forbids a construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GapKind {
    /// Registries were found whose content directory is not established.
    UnnamedRegistries,
    /// The method does not cover some loaders, readers, or paths.
    OutsideMethod,
    /// The method could not read a required part of the executable.
    UnreadableInput,
    /// A field exists on a path whose name could not be recovered.
    UnnamedField,
    /// A path through the reader could not be followed to its end.
    UnresolvedPath,
    /// A field's reader could not be established.
    UnresolvedReader,
    /// The reader is identified, but its accepted values and behavior are not established.
    ReaderSemantics,
    /// A live observation window did not complete; the established part is kept.
    IncompleteObservation,
}

/// How an answer was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Basis {
    /// The engine declares it. Behavior is not demonstrated.
    Declared,
    /// Traced in the executable without a game.
    StaticAnalysis,
    /// Observed in a supervised game.
    LiveObservation,
    /// Read from recorded answers. Never evidence of the present installation.
    Recorded,
}

/// Opaque identity of the exact game build. Keep it and compare it; do not parse it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuildId(pub(crate) String);

/// The stamp on every answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Exact game build.
    pub build: BuildId,
    /// Native release that ran the method.
    pub native_version: String,
    /// Method name and revision.
    pub method: String,
    /// How the answer was obtained.
    pub basis: Basis,
}

impl Source {
    pub(crate) fn new(build: BuildId, method: &str, basis: Basis) -> Self {
        Self {
            build,
            native_version: env!("CARGO_PKG_VERSION").into(),
            method: method.into(),
            basis,
        }
    }
}

/// A question that Native can be asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    /// `Native::registries`
    Registries,
    /// `Native::registry_fields`
    RegistryFields,
    /// `Game::registry_items`
    RegistryItems,
}

/// Whether the game process that a session owned is gone. Only the independent supervisor can
/// confirm it; a lost connection or an exit code never does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disposal {
    /// The supervisor reaped the game process that it owned.
    Confirmed,
    /// Disposal is not established. A new game is refused until this is resolved.
    Unconfirmed(String),
    /// No game process was created: the start failed early, or the answers are recorded.
    NotApplicable,
}

/// Whether this build and host can answer an operation. Asking never starts a game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Support {
    /// The operation has an implementation here, and its inputs and tools can be read.
    Supported,
    /// The operation cannot run here, for this reason.
    Unsupported(String),
}

/// Why a question has no answer. Never used for a search that completed and found nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Error {
    /// This build or host has no implementation of the operation.
    Unsupported {
        /// The operation asked for.
        operation: Operation,
        /// Why it is not supported here.
        reason: String,
    },
    /// The executable changed after `open`. Open the installation again.
    BuildChanged,
    /// No registry with this name was found. `registries` lists the known names.
    UnknownRegistry {
        /// The name asked for.
        name: String,
    },
    /// The method failed on an input that it should handle.
    Method(String),
    /// The game session did not establish this observation. Other observations may be available.
    Observation {
        /// The operation asked for.
        operation: Operation,
        /// What was not established.
        reason: String,
    },
    /// The game did not reach a safe pause. The session is over.
    Startup {
        /// Why the start failed.
        reason: String,
        /// Whether the game process, if one was created, is gone.
        disposal: Disposal,
    },
    /// Final session cleanup failed. The work directory is kept for inspection.
    Cleanup {
        /// What failed during cleanup.
        reason: String,
        /// Whether the supervisor established that the game process is gone.
        disposal: Disposal,
    },
    /// The game session is closing or closed.
    Closed,
    /// Recorded answers hold no file for this question. Never an empty answer.
    NotRecorded {
        /// The question and its subject, such as `registry_items/common/traditions`.
        question: String,
    },
    /// A recorded answer could not be read or written.
    Recorded(String),
    /// The connection to the supervisor failed. Disposal of the game is not established.
    Supervisor(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { operation, reason } => {
                write!(f, "{operation:?} is not supported: {reason}")
            }
            Self::BuildChanged => f.write_str("the executable changed after it was opened"),
            Self::UnknownRegistry { name } => write!(f, "no registry is named {name}"),
            Self::Method(reason) => write!(f, "the method failed: {reason}"),
            Self::Observation { operation, reason } => {
                write!(f, "{operation:?} was not observed: {reason}")
            }
            Self::Startup { reason, disposal } => {
                write!(
                    f,
                    "the game did not start: {reason} (disposal: {disposal:?})"
                )
            }
            Self::Cleanup { reason, disposal } => {
                write!(
                    f,
                    "session cleanup failed: {reason} (disposal: {disposal:?})"
                )
            }
            Self::Closed => f.write_str("the game session is closed"),
            Self::NotRecorded { question } => write!(f, "no answer is recorded for {question}"),
            Self::Recorded(reason) => write!(f, "recorded answer failed: {reason}"),
            Self::Supervisor(reason) => write!(f, "the supervisor connection failed: {reason}"),
        }
    }
}
impl std::error::Error for Error {}

/// One engine registry, named by its content directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registry {
    /// Content directory relative to the game root, such as `common/traditions`.
    pub name: String,
}

/// One root field of a registry's definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    /// Field key as the engine spells it.
    pub name: String,
    /// The shared reader that handles the field's value.
    pub reader: Reader,
    /// The engine reads this field differently depending on state that the field key does not
    /// determine.
    pub conditional: bool,
}

/// The shared reader behind a field. Two fields with one reader report the same `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reader {
    /// Opaque reader identity within one build, or `None` when no reader is established.
    pub id: Option<ReaderId>,
    /// Value form that the reader accepts.
    pub kind: ReaderKind,
}

/// Opaque identity of a shared reader within one build.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReaderId(pub(crate) String);

/// Value form of a reader. Only `Unknown` exists until reader binding is implemented (SDK-531).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReaderKind {
    /// The value form is not established.
    Unknown,
}
