# Bounded live observations

The production API is implemented. Admission remains unavailable until a maintainer accepts fresh
qualification and the reviewed source record is added. Candidate captures cannot grant admission.

The first operation requests the complete window: three initial registration call entries followed
by `tree_template` and `traditions` read entries in the retained category fixture. It does not prove
successful registration returns, stored values, validation, a complete registry, or game rules.
The fixture is `tests/fixtures/candidate/category.txt`; other fixture bytes are refused.

## Consumer flow

`examples/live.rs` uses only supported production exports. Build it with:

```sh
cargo build --locked --release --features production --example live
```

1. `Engine::open` takes an installation hint and selects the exact target automatically.
2. `context.capability(&CapabilityRequest::default())` reports qualification and current input/tool
   availability. No private historical bundle is needed for admission.
3. `context.prepare_observation(ObservationRequest { fixture, deadline_seconds }, CaptureOptions {
   output })` validates the complete bounded request. The deadline is 1–180 seconds; output must
   be a new absolute directory beneath an existing parent.
4. Start a dedicated **direct child** of the controller using the consumer's own executable and
   private stdin/stdout pipes. In that role call `supervisor::serve(stdin(), stdout())`.
5. Pass the pipes and opaque plan to `supervisor::connect`. Call `started`, optionally `cancel`,
   then `finish`. A false startup result still has a final report. The consumer never selects a
   debugger, adapter, architecture, native hook, launch flag, or timing workaround.
6. Retain the report's replay request. `Engine::replay` verifies its artifacts without an installed
   game. Artifact roots may be relocated; descriptor hashes and relative paths remain unchanged.

Both processes must link the same Native build, target, profile, and feature set. Leave stdout
exclusive to Native protocol and use stderr for logs. Keep the control pipe open; EOF requests
cleanup. Let Native create its process session. Do not install another child reaper, pre-create a
session/process group, close Native-owned descriptors, or terminate the supervisor before `serve`
returns. Exit the dedicated process after return. Readers must expose peer EOF.

`ObservationReport` separates the owner termination reason, game disposal, durable reservation
resolution, and normalized evidence. Within evidence, activation and completion are independent.
Evidence retention/access failure returns an error in that field without discarding owner cleanup
facts. A lost result channel proves neither success nor disposal. The retained owner report remains
available for inspection. Live evidence has `ResultOrigin::Live`; later replay has `Replay` and does
not re-establish present qualification. Both use the same recorded-data validator and observations.

## Admission and operation ownership

The exact recipe and composer bind machine mechanisms, binding declarations, content prerequisites,
and the host-resolved strategy once per context. Execution consumes these resolved values. The
strategy supplies its actual source/package bytes. Worker requests and provenance come from that
same binding. A synthetic recipe variation tests this boundary; it does not qualify another target.

Ordinary supervision rechecks the bundled acceptance, composition, current content and executable,
and exact debugger identity before allocation. It also rechecks Native/worker protocol and package
identities. Changed inputs, withdrawn acceptance, unknown revisions, unsupported hosts, missing tools,
missing reservation prerequisites, and conflicting games refuse execution; there is no fallback.
The selected debugger is pinned again between admission and preparation/start.

The capability query can probe LLDB but does not launch or attach to a game. Reservation acquisition,
current process conflicts, and debugger access to the suspended child are checked by the owner;
a successful capability query is not a launch permit or an access guarantee.

The [host reservation prerequisites](lifecycle.md#platform-boundary-and-reservation) still apply.
Native does not provision or clear that namespace. Unresolved ownership blocks new attempts. Setup,
worker handshake, observation, worker shutdown, and game disposal have separate existing budgets.
Cancellation or worker loss does not release resource ownership. OS termination is not graceful exit.

Public and maintainer entry points share observation, lifecycle, and capture code. Their protocol
modes and report authority stay separate. Production cannot enable `maintainer-tools` or
`test-support`; the public request has no control or qualification override.

## Qualification and verification

Acceptance is a reviewed change to `src/qualification/records/accepted.json`, after presentation of
fresh evidence. Neither capture nor the investigation binary writes this authority. Composition
includes selected package bytes, machine/binding/content declarations, and the shared operation
source identity, Rust compiler identity, target, profile, and build flags. The complete linked-build identity remains separate, so an acceptance-only edit
does not change the implementation under qualification. Debugger identity is an additional accepted
prerequisite. Maintainer/production build differences are recorded and production controls rerun.

The maintainer matrix is `tools/check-candidate-observations.py`. After acceptance, run:

```sh
python3 tools/check-live-observations.py "/path/to/Stellaris" "$PWD/.local/new-live-batch"
```

This launches real games through the production consumer. Its external harness drops one raw record
or kills the attempt's recorded LLDB worker for negative controls. Those controls are not public API
options. The matrix covers normal, unavailable fixture, incomplete stream, cancellation, caller loss,
timeout, and worker loss; it compares live and replay results and verifies independent disposal,
profile preservation, and an unrelated sentinel. Run it separately from process-isolation unit tests.

Scope is exact M45-observe ARM64 macOS with the retained 68-file content boundary and fixture.
Windows support, stable portability, complete tradition coverage, Atlas integration (SDK-519), and
clean pinned-build reproduction (SDK-520) remain separate work.
