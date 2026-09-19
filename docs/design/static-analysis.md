# Qualified static analysis

SDK-527 adds one game-free operation: decode the M45-observe planet-class getter. It returns
instructions, not a reader summary or a registry discovery claim. The control begins at
`0x101156518` and is 44 bytes / 11 ARM64 instructions. The exact executable and selected slice
must match the catalogue. Neither version labels nor symbols can select a nearby target.

```rust
use pdx_native::{CapabilityRequest, Native, OpenRequest};

# fn inspect() -> Result<(), Box<dyn std::error::Error>> {
let native = Native::open(OpenRequest { installation_hint: "/path/to/stellaris".into() })?;
let capability = native.capability(&CapabilityRequest::StaticDecode);
let analysis = native.analysis()?;
let result = analysis.decode_control()?;
# Ok(())
# }
```

`cargo run --locked --example analysis -- /path/to/stellaris` prints a result. No production
feature, content files, debugger, supervisor, or process is needed. The same qualified ARM64
bytes can be analyzed on macOS, Linux, and Windows; this does not qualify a Windows game image.

`CapabilityRequest::Registry { registry }` replaces the old registry-only request struct.
`CapabilityBounds::{Registry, StaticDecode}` identifies the requested operation family.
The default request remains the traditions registry. Each report's `context` identifies its
operation composition; the static composition is independent of `Native::identity()`'s live
composition. Unknown or malformed executables fail `Native::open` with an `OpenError` before
operation admission. Missing, withdrawn, or changed qualification receives a capability reason.

The analysis context rereads and verifies the executable and slice before each decode. It decodes
that same immutable byte buffer. An observed change or unreadable executable permanently
invalidates the shared static context. Reopen Native to use restored bytes. Content integrity is
separate, so a failed live capability check cannot invalidate static analysis.

## Implementation boundary

Composition alone resolves the private target recipe, executable reader, and machine decoder.
`engine/analysis` receives those bound interfaces. Compile-fail controls inserted into that actual
module reject imports of target records, platform leaves, and machine leaves.

The existing `object` integration owns slice selection and range mapping. Capstone decodes ARM64
bytes through the evidence package's pure normalization function, shared by execution and replay.
The stable representation carries address, four raw bytes, lowercase mnemonic, and pinned
Capstone operand text without whitespace. Register width, signs, immediate values, addressing
modes, writeback, conditions, aliases, and absolute branch destinations remain visible. This is
bounded disassembly, not symbolic interpretation. Invalid instructions or partial decoding fail.

`object =0.37.3` and `capstone =0.14.0` are pinned; Cargo.lock pins the C decoder and build tools.
Only Capstone's ARM64, full-detail, and standard-library features are enabled. The evidence
package's dependency review permits the decoder and its C build tools while continuing to
reject Native dependencies and runtime process creation.

## Evidence and replay

`AnalysisResult.descriptor` records the input byte range and original provenance, including
method, decoder, static implementation, qualification records, and immutable evidence references.
`Engine.replay_analysis(ReplayRequest)` validates descriptor and byte hashes, then decodes again.
Replay needs only retained artifacts; it never binds an installation or grants current
qualification. `AnalysisOrigin::Replay` and the original `capture_origin` remain explicit.

Raw game bytes and candidate artifacts stay under `.local/evidence/`. The authored CI sequence
uses different registers, immediates, condition, and addresses, and retains synthetic origin.
The independently authored expected getter instructions refer to the verified SDK-482
`planet-getter.txt` in the `typed-extraction` bundle. No raw game executable is checked in.

Run private qualification after the workspace checks pass:

```sh
python3 tools/qualify-static-analysis.py /path/to/stellaris .local/evidence/sdk-527-qualification-NEW --promote
```

The tool verifies the historical bundle, writes a new candidate capture, checks the retained
control and replay, promotes the exact static composition, and runs the ordinary public-interface
control on an isolated copy without content. It never launches Stellaris. Previous evidence is
retained. The tracked report is `docs/native/static-analysis-qualification.json`.

Static source fingerprints use portable paths and LF source bytes and exclude qualification
records, host tool installations, and profile settings. Relevant source or dependency changes
invalidate static acceptance. CI checks the accepted composition and authored decoding on all
three hosts; full executable qualification is a local private control.

This ticket changes shared build inputs and therefore invalidates older live-session qualification.
Those records remain historical. Live admission must fail until a separate live requalification;
static acceptance cannot grant live authority.
