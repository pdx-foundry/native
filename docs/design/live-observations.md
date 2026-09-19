# Installation queries and paused Game sessions

`Native` pins an installation. `get_registry(name)` returns its declared registry metadata without
launching Stellaris, starting a supervisor, probing a debugger, or requiring the production feature.
Reader and field discovery remain explicitly unknown. The description is not an item inventory or
a complete schema. Unknown names return `Unsupported`; changed or unreadable pinned inputs
permanently invalidate the context, even if the original bytes are restored.

`Game` owns one supervised process. Its only current observation is `get_registry_items(name)`,
for `traditions` and `tradition_categories`. Startup captures their initial-loader collections
where independently available. Repeated reads return the same startup snapshots and never resume
the game. Entries retain `registered_items` in Rust and `registeredItems` in JSON.

SDK-518's accepted one-shot implementation is the historical baseline. The changed SDK-521 session
implementation requires fresh qualification and maintainer acceptance before ordinary admission.
The earlier acceptance does not authorize this source revision.

## Consumer flow

Consumers supply an executable with a dedicated supervisor role. That role calls
`supervisor::serve(stdin(), stdout())`, then exits. Native owns the private pipes, process identity,
reservation, game, worker, and cleanup. No Native runtime executable is distributed.

```rust,no_run
use pdx_native::{GameOptions, Native, OpenRequest};
use std::process::Command;

# async fn query() -> Result<(), Box<dyn std::error::Error>> {
let native = Native::open(OpenRequest {
    installation_hint: "/path/to/Stellaris".into(),
})?;
let description = native.get_registry("traditions")?;
let mut host = Command::new(std::env::current_exe()?);
host.arg("--supervisor");
let native = native.with_supervisor(
    host,
    GameOptions::new("/existing/captures".into()),
)?;
let mut game = native.start_game().await?;
println!("{:?}", game.readiness());
for (name, availability) in game.registry_availability() {
    println!("{name}: {availability:?}");
}
let answer = game.get_registry_items("traditions").await;
// Always close, including when an individual query is unavailable.
let report = game.close().await?;
println!("{answer:?}; disposal: {:?}", report.disposal);
# Ok(()) }
```

`examples/live.rs` is the production consumer. Run it with an existing absolute retention directory:

```sh
cargo run --release --features production --example live -- \
  /path/to/Stellaris /existing/captures normal
```

The supervisor and caller must link the same Native build. Supervisor stdout is exclusive to the
protocol; use stderr for logs. The supervisor must be a dedicated direct child, with no competing
child reaper or separate process group/session installed by the consumer.

## What readiness means

Neither readiness value implies a loaded world, a ready main menu, completed engine validation,
or available gameplay operations. The process remains stopped at the witnessed initialization frame.

| Readiness | Established boundary |
| --- | --- |
| `PausedAfterRegistryInitialization` | Both declared initial registry loaders returned and the bound debugger freshly confirmed its unchanged stopped frame. |
| `PausedDuringRegistryInitialization` | Only part of registry initialization was witnessed before a safe pause. Available registry snapshots remain usable. |

Admission, hook activation, access, and collection completeness are evaluated per registry. A missing
traditions hook can leave categories available. A failed item read can leave loader readiness
established while that registry is unavailable. Missing entries produce an incomplete result for
the affected registry, not a successful empty collection or a failure of the other collection.
Unknown or unreadable shared transport evidence remains explicit where its affected scope cannot
be established.

Startup fails when Native cannot confirm a safe owned pause. `GameError::StartupFailed` preserves
partial results and independent disposal facts. A connection failure is not disposal confirmation.

## Lifetime and cancellation

`GameOptions::new` defaults startup and idle budgets to 180 seconds each. Both must be 1–180 seconds.
Successful item reads reset the idle budget; metadata inspection and unavailable queries do not.
Cleanup has separate budgets. Native retains every attempt in a new directory.

| Event | Behavior |
| --- | --- |
| Startup future dropped | Close the control channel and independently clean up any allocation. |
| Item-read future dropped | Keep the session alive; an already accepted read can complete. |
| `cancel()` | Request cancellation; await `close()` for the final report. |
| `close().await` | Request normal termination and await independent disposal; repeated calls return the same report. |
| Close future dropped after polling | Cleanup continues; the handle can await close again. |
| `Game` dropped | Request cleanup without claiming success. |
| `Native` dropped | An existing Game retains its own pinned context and supervision. |
| Caller or async runtime lost | Native's process-management thread and independent supervisor continue cleanup. |
| Worker lost | End the session, retain established observations, and independently dispose the game. |
| Supervisor lost | No disposal claim; an unresolved durable reservation blocks new launch. |

Async waiting uses Tokio channels. An independent Native thread owns and reaps the consumer
supervisor, so process lifetime is not tied to an async task. The external supervisor owns and
reaps Stellaris. Only its owner report establishes game disposal. Concurrent games remain excluded.
On close, the supervisor gives the attached debugger a bounded chance to terminate its target and
finish pending exit handling before forcing worker shutdown. The supervisor must still reap its
original child. A debugger response or absent PID never substitutes for that disposal proof.

## Evidence and replay

Item keys come from engine collections, not file names or a text parser. Native copies the pinned
installed `common/traditions` and `common/tradition_categories` trees into an isolated private mod.
All extensions participate in integrity checks. DLC additions, user mods, later reloads, field values,
category relationships, and rule coverage remain outside this result.

Session artifacts use `pdx-native/session-registry-snapshot-v1`. Each registry descriptor pins the
shared event stream and its own requested name. Replay checks global sequence integrity and
registry-specific loader, owner, thread, slot, and terminal joins.

Startup snapshots live under `snapshots/<registry>` and retain unconfirmed disposal. Close writes
new evidence under `final/<registry>`; it never changes a returned snapshot. A final `GameReport`
separates termination, game disposal, durable reservation resolution, results, and retention failures.
One registry's retention failure does not erase the other's evidence or the owner cleanup report.
Opaque subject identities belong to their descriptor; do not equate identities across snapshots.

`Engine::replay_registry` reads both the existing SDK-518 format and the new session format.
`Engine::replay` and SDK-483 artifacts are unchanged. Replay requires no installation or live helper;
missing or changed artifacts fail explicitly. Ordinary admitted results carry `Live`; retained
results carry `Replay`. Maintainer sessions retain replay origin and cannot grant live admission.

## Migration from SDK-518

- Replace `Engine::open(...).with_supervisor(..., RegistryOptions)` with `Native::open(...)` and
  `GameOptions`. `Engine::open` remains available for installation/capability callers.
- Replace the old `RegistryClient::get_registry_items` or `RegistryJob` flow with
  `start_game().await`, independent queries, and `close().await`.
- Handle each registry separately. Inspect `Game::readiness()` rather than assuming a loaded world.
- Retain startup references from `Game::replay_references()` and final references from `GameReport::replay`.
- Read final results from `GameReport::registries`; the removed `RegistryReport` had one `result`.
- The old live client/job/options/report exports are removed. No deprecated live wrappers remain.

Atlas's consumer transition remains SDK-519. Save loading, explicit-empire resource reads, and UI
operations remain future API sketches. There is no public `load_save` placeholder or gameplay method.

The future interface may add `Native::load_save(save).await`, returning a Game only after a separately
defined world-readiness boundary. A resource query must take an explicit empire identity, for example
`Game::get_resource(empire, resource).await`; it must not infer the local player or an arbitrary empire.
UI queries need their own readiness and availability contract. These are design sketches, not exported
methods or promises that the current paused session can perform those operations.

## Qualification checks

```sh
python3 tools/check-game-sessions.py /path/to/Stellaris /new/candidate-output
python3 tools/check-game-sessions.py /path/to/Stellaris /new/production-output --production
```

The candidate controls exercise the shared session implementation. They cover full and partial
readiness, each registry's unavailable/incomplete cases, cancellation, caller/runtime loss, worker
loss, deadlines, repeated reads, and independent evidence-retention failures. Ordinary controls
require reviewed admission. Neither command installs an acceptance record.
