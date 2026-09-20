use crate::{
    ArtifactReference, CaptureOrigin, EvidenceReference,
    analysis::{AnalysisOrigin, AnalysisProvenance},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};

/// Recorded symbol; native names are evidence locators, never public subject identities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Symbol {
    /// Demangled symbol spelling.
    pub name: String,
    /// File virtual address.
    pub address: u64,
}
/// Target-adapted scheduler layout recorded with the input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchedulerLayout {
    /// Start of the literal initialization range.
    pub start: u64,
    /// Exclusive end before scheduling begins.
    pub end: u64,
    /// Table offset from the receiver register x19.
    pub offset: u64,
    /// Row stride in bytes.
    pub stride: u64,
    /// Number of bounded scheduling rows, not the number of registries.
    pub count: usize,
}
/// Immutable inputs read from one verified executable buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticInput {
    /// Symbol inventory, without a supplied registry list.
    pub symbols: Vec<Symbol>,
    /// Raw ARM64 initialization bytes.
    pub code: Vec<u8>,
    /// Qualified scheduler bounds.
    pub layout: SchedulerLayout,
    /// Resolved pointer locations and target-local values.
    pub pointers: BTreeMap<u64, u64>,
    /// Literal strings keyed by their file addresses.
    pub strings: BTreeMap<u64, String>,
    /// Vtable address points with executable-derived owner adjustments and dispatch slots.
    pub vtables: BTreeMap<u64, VtableWitness>,
}
/// One retained observation run. Every artifact is verified before derivation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryRun {
    /// Startup event stream.
    pub trace: ArtifactReference,
    /// Independently observed scheduling table.
    pub table: ArtifactReference,
    /// Original activation/completion/disposal checks.
    pub result: ArtifactReference,
    /// Exact target and content boundary of this historical capture.
    pub manifest: ArtifactReference,
}
/// Replay descriptor; contains input references rather than extracted answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryDescriptor {
    /// Must match the supported discovery format.
    pub format: String,
    /// Original captured or synthetic input origin.
    pub capture_origin: CaptureOrigin,
    /// Executable and method provenance.
    pub provenance: AnalysisProvenance,
    /// Executable-derived inputs.
    pub input: ArtifactReference,
    /// Historical runs; empty for executable-only discovery.
    pub runs: Vec<DiscoveryRun>,
}
/// Context-owned identity. Cloning retains ownership; deserialization and construction are private.
#[derive(Debug, Clone, Serialize)]
pub struct RegistrySubject {
    #[serde(skip)]
    pub(super) scope: Arc<()>,
    pub(super) ordinal: usize,
}
impl PartialEq for RegistrySubject {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.scope, &other.scope) && self.ordinal == other.ordinal
    }
}
impl Eq for RegistrySubject {}
/// Basis for a relationship; static candidates are distinct from historical observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DiscoveryBasis {
    /// Exact template loader symbol, including its owner type argument.
    TemplateSymbol,
    /// Literal initialization and a local weak symbol resolution; runtime interposition is untested.
    StaticScheduling,
    /// Loader receiver directory observed during the retained run.
    HistoricalLoader,
    /// Constructor key, concrete owner, persistent base and member dispatch joined in a retained run.
    HistoricalOwner,
    /// Custom loader's key/read phases and concrete owner witnessed in a retained run.
    HistoricalCustomOwner,
}
/// A discovered template loader candidate. No complete-registry claim is implied.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryCandidate {
    /// Opaque identity for related observations in this result.
    pub subject: RegistrySubject,
    /// Whether the symbol inventory contains a separately named member reader; not reader qualification.
    pub has_named_member_reader: bool,
    /// Static discovery basis.
    pub basis: DiscoveryBasis,
    /// Symbol inventory row supporting this candidate.
    pub evidence: EvidenceReference,
}
/// A scheduling record remains visible even when no template candidate matches it.
#[derive(Debug, Clone, Serialize)]
pub struct SchedulingWitness {
    /// Position in the bounded startup table.
    pub index: usize,
    /// Candidate handles joined through function-slot symbols.
    pub candidates: Vec<RegistrySubject>,
    /// All literal slots and the name were recovered.
    pub recovered: bool,
    /// Static basis; observed tables provide a separate historical check.
    pub basis: DiscoveryBasis,
    /// Input artifact and row.
    pub evidence: EvidenceReference,
}
/// One bounded loader or owner relationship.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryRelationship {
    /// Observed loader receiver, when the trace establishes it.
    pub loader: Option<RegistrySubject>,
    /// Concrete root owner, scoped to its historical run.
    pub owner: Option<RegistrySubject>,
    /// Template candidate when established; custom paths can lack one.
    pub subject: Option<RegistrySubject>,
    /// Observed content directory when established.
    pub directory: Option<String>,
    /// Definition key only when joined to a root owner.
    pub key: Option<String>,
    /// What establishes this relationship.
    pub basis: DiscoveryBasis,
    /// All contributing retained records.
    pub evidence: Vec<EvidenceReference>,
}
/// A missing or unresolved part of the bounded search.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveryGap {
    /// Machine-readable reason.
    pub kind: DiscoveryGapKind,
    /// Related candidate, when known.
    pub subject: Option<RegistrySubject>,
    /// Precise missing obligation.
    pub reason: String,
    /// Retained location exposing the gap.
    pub evidence: EvidenceReference,
}
/// Discovery gaps never imply the game has no corresponding registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum DiscoveryGapKind {
    /// No loader observation for a static candidate.
    UnobservedCandidate,
    /// Scheduling witness is outside the template method.
    OutsideTemplate,
    /// Missing literal, pointer, name, or slot.
    Scheduler,
    /// Unknown call or unsupported instruction invalidates tracked values.
    UnknownInstruction,
    /// Root, receiver, directory, or virtual dispatch join is incomplete.
    OwnerJoin,
    /// Shared/custom/nested/late paths exceed this method.
    UnresolvedHelper,
    /// Activation, sequence, target, or completion check failed.
    HistoricalIntegrity,
}
/// Registry discovery with explicit bounds and per-fact evidence.
#[derive(Debug, Clone, Serialize)]
pub struct RegistryDiscoveryResult {
    /// Executable analysis or historical replay.
    pub origin: AnalysisOrigin,
    /// Original provenance and exact inputs.
    pub descriptor: DiscoveryDescriptor,
    /// Template candidates; never implicitly qualified owners.
    pub candidates: Vec<RegistryCandidate>,
    /// Every scheduling row, including unresolved rows.
    pub scheduling: Vec<SchedulingWitness>,
    /// Established bounded relationships.
    pub relationships: Vec<RegistryRelationship>,
    /// Unobserved and unresolved obligations.
    pub gaps: Vec<DiscoveryGap>,
    /// Search limits; this method never establishes complete registry coverage.
    pub limits: Vec<String>,
    #[serde(skip)]
    pub(super) scope: Arc<()>,
    #[serde(skip)]
    pub(super) input_bytes: Vec<u8>,
    #[serde(skip)]
    pub(super) subject_count: usize,
}
/// A subject came from another result/context or is not a candidate in this result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignRegistrySubject;
impl std::fmt::Display for ForeignRegistrySubject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("registry subject belongs to another discovery context")
    }
}
impl std::error::Error for ForeignRegistrySubject {}
impl RegistryDiscoveryResult {
    /// Exact executable-input artifact bytes for retaining this result for offline replay.
    pub fn input_bytes(&self) -> &[u8] {
        &self.input_bytes
    }
    /// Find relationships for a candidate, loader or owner issued by this result.
    /// Foreign contexts are rejected even when their executable and evidence match.
    pub fn relationships_for(
        &self,
        handle: &RegistrySubject,
    ) -> Result<Vec<&RegistryRelationship>, ForeignRegistrySubject> {
        if !Arc::ptr_eq(&self.scope, &handle.scope) || handle.ordinal >= self.subject_count {
            return Err(ForeignRegistrySubject);
        }
        Ok(self
            .relationships
            .iter()
            .filter(|r| {
                r.subject.as_ref() == Some(handle)
                    || r.loader.as_ref() == Some(handle)
                    || r.owner.as_ref() == Some(handle)
            })
            .collect())
    }
    /// Resolve candidate handles issued by this discovery result. Loader/owner handles use relationships_for.
    pub fn subject(
        &self,
        handle: &RegistrySubject,
    ) -> Result<&RegistryCandidate, ForeignRegistrySubject> {
        if !Arc::ptr_eq(&self.scope, &handle.scope) {
            return Err(ForeignRegistrySubject);
        }
        self.candidates
            .get(handle.ordinal)
            .ok_or(ForeignRegistrySubject)
    }
}
/// Target-local candidate record used for retained parity checks, not a public subject identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRecord {
    /// Database type argument.
    pub database: String,
    /// Owner type argument; unqualified until joined.
    pub owner_candidate: String,
    /// Loader symbol.
    pub loader: String,
    /// File address in hexadecimal, matching the original evidence convention.
    pub address: String,
    /// Named reader symbol exists.
    pub has_named_member_reader: bool,
}
/// Literal scheduling row retained for independent comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SchedulerRow {
    /// Bounded table index.
    pub index: usize,
    /// Literal scheduling name, not a public registry identity.
    pub name: Option<String>,
    /// Name pointer and five function slots.
    pub values: Vec<Option<u64>>,
    /// Instruction addresses that wrote each slot.
    pub sources: Vec<Option<String>>,
    /// Recovered or gap.
    pub status: String,
}

/// Executable-derived persistent-base adjustment and shared reader slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VtableWitness {
    /// Concrete owner named by the executable vtable symbol.
    pub owner: String,
    /// Signed offset from this base to the concrete object.
    pub offset_to_top: i64,
    /// Function pointer at the qualified shared member-dispatch offset.
    pub member: u64,
}
