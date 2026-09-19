# Registry queries

Native's consumer interface answers a concrete question: **which entries are in this registry?**
The first names are `traditions` and `tradition_categories`. Strings keep the interface open to future
registry names. Native owns their mapping to engine classes, memory layouts, loading, and capture.
The [fresh qualification report](../native/registry-qualification.md) awaits maintainer acceptance.
Production acceptance remains empty.

## Consumer flow

The consumer supplies its own executable's supervisor role once. Native creates a dedicated direct
child for each query, connects its private pipes, and reaps it. No separate Native executable is
shipped. `examples/live.rs` is a complete production-only consumer, including cancellation and replay.

Run that consumer with an installation and an existing absolute retention directory:

```sh
cargo run --release --features production --example live -- \
  /path/to/Stellaris /existing/captures traditions
```

Until the qualification record is accepted and added, this returns `QualificationMissing` before
launch. Once admitted, it calls `get_registry_items`, prints the full report as JSON, and verifies
the retained replay.

```rust,no_run
use pdx_native::{Engine, OpenRequest, RegistryOptions};
use std::process::Command;

# fn query() -> Result<(), Box<dyn std::error::Error>> {
let context = Engine::open(OpenRequest { installation_hint: "/path/to/Stellaris".into() })?;
let mut host = Command::new(std::env::current_exe()?);
host.arg("--native-supervisor");
// In that executable role, call supervisor::serve(stdin(), stdout()), then exit.
let mut native = context.with_supervisor(host, RegistryOptions {
    retention_directory: "/existing/captures".into(),
    deadline_seconds: None,
})?;
let support = native.capability("traditions");
let report = native.get_registry_items("traditions")?;
if let Ok(registry) = &report.result {
    for item in &registry.registered_items { println!("{}", item.key); }
    println!("{:?}", registry.completion);
}
if let Some(retained) = report.replay { let replay = Engine.replay_registry(retained)?; }
# Ok(()) }
```

`get_registry_items` blocks until the answer and independent cleanup report arrive. `start_registry_items(name)`
returns a job with `started`, `cancel`, and `finish` for consumers that need cancellation. Dropping a
job closes control, requests independent cleanup, and arranges supervisor reaping; only a final owner
report confirms game disposal. Caller loss is handled independently of the caller's process.

Unknown names return `RegistryError::Unsupported` before process or capture allocation. Declared
registries without current qualification or prerequisites return `Unavailable` with admission reasons.
Deadlines are optional, default to 180 seconds, and must be 1–180 seconds; cleanup has separate budgets.
The retention directory must exist. Native creates a unique immutable attempt beneath it.

Consumers supply no fixtures, hooks, registration counts, field lists, or executable plans. The public
API has no investigation controls or qualification overrides. Both processes must link the same Native
build. The supplied command must start a dedicated direct child that calls `supervisor::serve` on its
private stdin/stdout and exits after return. Leave stdout exclusive to that protocol; use stderr for logs.
Do not create a process session/group or install a competing child reaper in that supervisor role.

## Answer and content boundary

The answer contains engine collection keys and opaque, capture-scoped subject identities. Native
captures the complete pointer-array collection on return from its initial loader, before later
validation. `Complete` means every slot at that boundary was witnessed. An empty collection needs the
same activation, loader, count, and terminal witnesses as a nonempty one. Missing records retain valid
entries with incomplete status; unavailable access and worker loss remain explicit.

Native copies the pinned installed files in `common/traditions` and `common/tradition_categories` into
an isolated private mod, replacing those two virtual directories. The engine parses these copies.
Registry keys are read from engine objects, never inferred from file names or a text parser. All file
extensions are included in input integrity checks. Replay verifies the copied files against the
retained input manifest. User mods, DLC additions to these directories, later reloads, field values,
tradition-category relationships, parser schemas, and rule coverage are outside this first answer.

`RegistryReport` keeps termination, independent game disposal, reservation resolution, and the answer
separate. Evidence-finalization failures stay explicit in `result` without losing disposal facts.
`RegistryResult` contains completeness, activation, historical disposal, provenance, and limits.
Live answers carry `Live`; retained derivation carries `Replay`. Candidate reports never produce an
admitted live result. Historical `Engine::replay` and the SDK-483 exports/artifacts remain unchanged;
registry artifacts use their own contract and `Engine::replay_registry`.

### Inspect a retained real-game answer

The final candidate normal capture returned 234 tradition keys and 33 category keys. To inspect an
available registry capture without launching Stellaris, pass its evidence directory and descriptor
reference to the game-free example:

```sh
cargo run --quiet --example registry-replay -- \
  /path/to/normal/evidence /path/to/normal/evidence/descriptor.ref.json
```

It prints the complete `RegistryResult` as JSON. Each `registeredItems` member has a `key` and an opaque
`subject`; the result also has completion, activation, disposal, origin, provenance, and limits.
For the tradition capture, the first keys are `tr_adaptability_adopt`, `tr_adaptability_finish`, and
`tr_adaptability_recycling`. Its completion is `complete`, activation is `demonstrated`, and disposal
is `confirmed`. Replay has `origin: "replay"`; ordinary admitted capture has `origin: "live"`.
The surrounding live `RegistryReport` additionally carries operation termination, reservation
resolution, retained replay reference, and explicit evidence-finalization errors.

### Field discovery is a separate question

SDK-518 exposes `get_registry_items(name)` to return registered items. `get_registry(name)` is reserved
for a follow-up that describes the registry, including fields accepted by its reader. That work
requires a separate operation and qualification effort. The retained
[discovery prototypes](../native/discovery.md#members-and-shared-readers) provide starting evidence;
they do not establish complete field schemas for these registries. Native should own the engine
method and qualified field facts. Atlas should consume those facts for authoring rules without
managing addresses, hooks, or extraction fixtures.

## Authority and verification

Composition selects the target, registry bindings, machine, strategy, package, and content once.
The supervisor independently rechecks current inputs, composition, helpers, and bundled qualification
before allocation. Serialized requests carry intent, not authority. Changed inputs invalidate the
context; re-opening does not grant qualification for different content. Admission needs no historical
bundle. Replay requires every referenced artifact and refuses absent or changed bytes.

Maintainer controls use the same execution, capture, and replay implementation through a separate,
feature-gated authority entry point. `tools/check-registry-observations.py --registry NAME` repeats the
ten candidate controls. `tools/check-candidate-observations.py` preserves the early-observation controls.
After maintainer acceptance, `tools/check-live-observations.py --registry NAME` exercises the ordinary
production consumer with external process/stream controls. No investigation command writes acceptance.

A relevant implementation change invalidates the previous qualification report. Review new evidence
before adding a tracked acceptance. Windows, other targets/content, registry discovery, Atlas
integration, and clean pinned-build reproduction remain outside this slice.
