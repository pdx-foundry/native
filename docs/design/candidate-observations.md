# Candidate registration and category read capture

`investigation::prepare_observation` accepts the retained category fixture and a bounded deadline.
The consumer starts its own dedicated supervisor and uses the same `connect`, `started`, `cancel`,
and `finish` flow as [candidate lifecycle](lifecycle.md). Enable `maintainer-tools`; production
builds reject that feature. Neither a candidate plan nor its report grants live admission.

The initial implementation accepts only M45-observe on ARM64 macOS, the retained 68-file producer
content boundary, and the two-field fixture. It captures three registration call entries and the
`tree_template` and `traditions` read entries. These are not successful registration returns,
stored values, a complete registry, validation rules, or runtime semantics. No world is loaded.

## Ownership and wire meaning

The Rust supervisor remains the game's direct parent and holds the existing host reservation.
Its selected binding starts an Xcode LLDB subprocess with embedded Python. LLDB is discovered and
probed before game allocation, pinned for the attempt, and started with `-b -x`, without inherited
Python paths or injected libraries. Requests cannot select another debugger.

Binding groups own the hook addresses, token mappings, and reader/source layouts. The selected
strategy owns ARM64 argument use, source/owner/thread joins, hook timing, and the presentation guard.
The guard is built with Xcode for the maintainer binary; its actual bytes and source are retained.
The strategy package and linked Native build have separate recorded identities. Composition hashes
include the strategy sources and generated bindings. The candidate manifest records the actual
executable/slice, content, fixture, composition, method, strategy, build, tool, and package identities.

Native's private protocol types define request, hello, and resume acknowledgement meaning.
Schemars derives their Python validation schemas, including recorded events from the evidence crate.
The shared codec rejects unsupported schema constraints and oversized messages. Regenerate with:

```sh
PDX_NATIVE_GENERATE_PROTOCOL=1 cargo test --features maintainer-tools \
  protocol::observation::tests::generated_python_matches_wire_authority -- --exact
```

The same test checks generated-file drift in ordinary CI. Historical replay records remain readable.
The evidence crate contains only recorded data, schema generation, and pure replay; the dependency
allowlist includes the schema library and its proc-macro dependencies, with no path back to Native.

The owner durably records the child before launching LLDB and records the worker identity before
resume acknowledgement. The worker independently requires `_dyld_start`, ARM64, and all required
hooks resolved, enabled, and unhit. The owner validates attempt, child, worker, target, package,
LLDB, Python, and module identities before acknowledging. Missing or late hooks refuse resume.

Owner setup has a 30-second budget. Hello/acknowledgement is bounded to 15 seconds; observation
accepts 1–180 seconds; worker shutdown and game disposal have separate five- and ten-second budgets.
Cancellation, caller loss, callback failure, and worker loss all reach owner cleanup. The owner
observes worker exit without reaping, preserving its process-group identity until group cleanup.
An exited worker with no live group members is reaped without signaling an empty group. An
unresolved worker or game keeps the reservation blocking. There is no automatic orphan recovery.

## Retained artifacts and result meaning

Each new output directory retains its private working profile, source package, raw trace, worker
handshake, diagnostics, durable owner events, and lifecycle report. `evidence/` contains independent
snapshots of the pre-launch profile and replay artifacts. The game can rewrite its working profile
without changing the retained input bytes. Writes create new files and never replace prior captures.

`InvestigationReport.replay`, when present, is a descriptor reference relative to `output/evidence`.
The candidate report envelope is version 2; controller/supervisor wire version 2 rejects older peers.
The existing replay format and observation contract are unchanged. To replay a fresh candidate:

```sh
cargo run --example replay -- /absolute/attempt/evidence \
  /absolute/attempt/evidence/descriptor.ref.json
```

Capture uses the evidence crate's serializable records and runs its existing replay validator.
Replay returns captured origin and replay provenance, not current qualification. Candidate execution
ending normally does not mean the observation window completed: consult replay's activation,
completion, gaps, and independent disposal fields. Owner-reported access failures can establish
unavailable even when no worker record arrived. Worker loss preserves earlier facts.

Transport records are limited to 64 KiB and traces to 4 MiB. Diagnostic streams have the same 4 MiB
per-file limit. A malformed or truncated raw tail is retained with a transport diagnostic; only the
valid prefix is normalized, and no terminal survives transport corruption. Sequence gaps are never
renumbered. Storage failure cannot become empty success. Finalization errors remain diagnostics in
the candidate report; independent disposal is still reported.

## Verification

This command **launches real games**, requires the provisioned host reservation store, and refuses
missing or mismatched prerequisites:

```sh
python3 tools/check-candidate-observations.py "/path/to/Stellaris" \
  "$PWD/.local/new-observation-batch"
```

The harness retains normal, missing-hook, late-hook, dropped-record, missing-terminal, failed native
memory-read, and actual LLDB-loss controls, plus cancellation, caller loss, and timeout. It checks
ordinary-profile hashes and an unrelated sentinel after every attempt and exercises `Engine::replay`
on each finalized descriptor. Deliberate controls are confined to the maintainer surface.

Run the default, test-support, and maintainer-tools workspace suites; format and Clippy checks;
`python3 -m unittest discover -s tools/observation`; the replay/admission boundary checks; and private
SDK-483 replay. Unit tests cover generated wire constraints, handshake identity changes, worker
cleanup, immutable writes, storage bounds, damaged tails, source snapshots, and thread joins.

See the [fresh evidence record](../native/candidate-observations.md) for executed attempts and limits.
