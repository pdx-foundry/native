# Bounded early-observation replay

SDK-513 implements the retained registration/category-read window through `Engine::replay`.
The [specification](../specs/native.md) governs result meaning and the
[architecture](architecture.md) governs ownership. This change introduces no live target support.

The consumer supplies a read-only artifact root and a hash/size-pinned descriptor reference.
The descriptor has an explicit format and contract, the original attempt identity, capture or
synthetic origin, and archive-relative artifact references. Moving the root does not change identity.
The evidence package owns these records, normalized observations, stream rules, artifact reads,
and retained derivation. Native owns only the consumer request and its replay adapter.
No context discovery, admission, process helper, or callback is needed for replay.

The initial input format wraps the retained SDK-483 JSON/JSONL representations. It requires the
original manifest, request, trace, and independent owner journal; additional pinned artifacts retain
sources, fixture files, producer-content manifest, and original summary. Native details remain in
the manifest reference. Original summaries and fault labels do not determine replay outcomes.
Source/content/fixture hash references in the original manifest must resolve to verified artifacts.
The prototype game rewrote `profile/settings.txt` in three retained attempts. Replay verifies the
archived post-run settings against the bundle identity and reports the missing pre-launch settings
bytes as a provenance gap. This does not invalidate the independent bounded trace derivation, and
does not establish that the original launch profile can be restored. Other input mismatches fail.
Historical installed content and the executable itself are provenance, not replay prerequisites.

Activation requires the loader-entry witness, resolved/enabled zero-hit required hooks, successful
resume, registration entries before the fixture parse, and the bounded parse-end/terminal relation.
Completion also requires continuous producer sequence, one registration-window marker, three
registration entries, the two ordered fixture fields, a single owner, and matching terminal totals.
The producer manifest must pin the requested fixture bytes. Reported field locations are checked
against the retained fixture's unquoted field keys; a mismatch preserves facts with a source-join gap.
The terminal covers observations; trailing worker-finished/dispose records are allowed, but further
observations or another terminal prevent completion. Missing or failed trailing control records remain
gaps without downgrading a verified completed window. An abnormal worker exit likewise remains visible;
without a verified complete window it produces worker-lost. Worker-loss evidence comes from the independent journal.
Disposal requires a final checked record that confirms reaping the same owned child with no remaining
identity. Clocks from worker and owner are never compared.

Missing bytes, size/hash mismatch, malformed/truncated JSON, unsupported format/contract, unsafe
paths, and inconsistent provenance are errors. Readable but incomplete attempts return observations
with specific gaps. Replay and original synthetic origin are always explicit; replay does not confer
current qualification or rule completeness. Handles are scoped to the descriptor identity.

Tracked authored synthetic fixtures exercise the public interface on a clean checkout. Tracked
private descriptors contain references only; their tests explicitly require a restored bundle.
Compile-fail controls, a transitive dependency allowlist, unsafe-code prohibition, and semantic
Clippy checks against process creation enforce the evidence-package boundary.

Verification: `python3 tools/check-replay-boundary.py`, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace`.
Private verification: `PDX_NATIVE_PRIVATE_EVIDENCE=<prepared private replay root>
cargo test --test private_replay -- --ignored`.

Synthetic admission factories, live helper binaries, target composition, and release feature guards
belong to later live work; replay does not need an admission bypass or synthetic live engine.
