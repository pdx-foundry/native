# Consumer-hosted candidate lifecycle

Native is a library. Atlas or another consumer supplies a dedicated supervisor process, private
input/output pipes, scheduling, and user presentation. Inside that process Native owns target
validation, the host reservation, private profile, direct child, deadlines, and disposal journal.
There is no installed Native owner executable or helper search path.

## Integration

The ordinary `supervisor::serve` entry point now uses the shared owner after ordinary admission.
The public registry client starts and reaps the consumer-supplied supervisor command and manages its private pipes. The bounded registry acceptance is bundled; see [registry queries](live-observations.md). `investigation::{connect, serve}` is a separate `maintainer-tools` surface producing
only `InvestigationReport` and `CandidateCapture`. It cannot construct a supported replay result or
change qualification authority. Lifecycle-only captures have their own versioned report format. The separate
[candidate observation path](candidate-observations.md) additionally emits evidence descriptors
that the public replay API can read; this does not admit a public live operation.

The consumer first calls `investigation::prepare` to validate and pin the request, then starts a **direct child** running its own supervisor role with piped stdin/stdout.
That child calls `investigation::serve`; the controller calls `investigation::connect` with those
pipes and the prepared `CandidatePlan`. The library validates protocol and linked Native build identities,
then independently resolves and verifies the candidate composition. Build identity includes Native
source, dependency lockfile, target, profile, and build-mode features. Consumer executable paths may
differ; linked Native identities must agree. No serialized claim grants admission.

The caller retains `CandidateJob`, awaits `started`, and then awaits `finish`. `started` returning
`None` means a report is already available from `finish`, including disposal for failed startup.
Cancellation and worker-loss notifications request owner cleanup. A broken result channel is not
proof of disposal; inspect the retained report. The consumer is responsible for detecting its
observation worker's exit and notifying the owner. The lifecycle-only request starts no observation worker. Observation requests use the selected
strategy under the same owner, which directly monitors its worker.

Use a dedicated process, not a thread inside the main application. Native creates a new macOS
session to separate terminal lifetime. Let Native establish the session; do not pre-create a process group/session. Do not install another SIGCHLD reaper, ignore SIGCHLD, close
Native-owned descriptors, or terminate this process before `serve` returns. Exit the dedicated
process after return; its input thread may still be waiting for controller EOF. Supplied readers
must return EOF when their peer closes. Logs must not share the protocol output.

Native's reader/writer threads own no game resources. Output backpressure cannot stall the owner.
Handshake reads have a 15-second bound. Setup and the suspended hold each have a separate 30-second owner bound, followed by a separate
10-second disposal budget. Setup time does not consume the requested suspended hold. Candidate requests select a suspended hold of 1–30,000 milliseconds;
a full 30,000-millisecond request terminates as timed out. The child is never resumed. OS process
termination is not graceful in-game exit. Supervisor termination leaves unresolved ownership;
there is no automatic orphan recovery or in-process lifetime guarantee.

## Platform boundary and reservation

Shared `execution::instances` owns journal interpretation and launch exclusion policy.
`execution::supervisor` owns resource lifetime and cleanup decisions. The compiled binding platform
provides storage/lock access, process inventory and identity, suspended spawn, and reaping.
Consumers cannot change the namespace. Windows, Linux, and Intel Mac return `HostUnavailable`
before allocation. Adding another platform requires implementing and qualifying these services,
not copying the macOS filesystem path into shared code.

On Apple Silicon macOS the namespace is `/Library/Application Support/PDX Native/instances`.
Provision it once for the designated local account; commands must be run from that account:

```sh
sudo install -d -o root -g wheel -m 755 "/Library/Application Support/PDX Native"
sudo install -d -o "$(id -un)" -g "$(id -gn)" -m 700 "/Library/Application Support/PDX Native/instances"
```

The parent must be root-owned and not writable by group/others. The namespace must belong to the
running account with no group/other access. Other accounts fail closed; cross-user execution is
not supported. Do not run these commands to take over an existing namespace owned by another
account. Inspect its ownership and unresolved reservations first. Native never provisions,
changes permissions, relocates, or silently clears the namespace itself.

The lock inode is retained across releases. A competing supervisor returns busy. Every other
entry must be a recognized version-1 JSON journal whose filename matches its attempt and whose
state is disposed. Unknown versions/states, extra fields, unreadable/truncated records, symlinks,
non-regular files, pending writes, and unresolved reservations block launch without rewriting
prior evidence. Writes synchronize the file and namespace directory. A reservation is durable
before profile allocation or spawn; child incarnation is added immediately after spawn.

A free OS lock, absent PID, or elapsed time never clears a reservation. Owner death between spawn
and child-record persistence still leaves the pre-spawn reservation blocking. Disposal is marked
only after direct-child reaping, or when no game was launched. Failed journal commits leave
`reservation_resolved` false even when the game was reaped. Record-retention failures are returned
in `diagnostics`, separately from disposal. Operator-assisted inspection/clearance is outside this
API; do not delete a reservation to retry a failed attempt.

The lock covers Native owners only. A bounded `/bin/ps` inventory checks ordinary Stellaris
instances before launch and periodically during the hold. Inspection failure or a conflict ends
the attempt; Native signals only its unreaped direct child. These checks do not guarantee absence
of an external launch between samples. The game uses Darwin `START_SUSPENDED`, ARM64 preference,
closed inherited descriptors, a private profile/HOME, and a restricted environment.

## Development and verification

`examples/investigate.rs` is a maintainer development harness demonstrating both consumer roles.
It is not a distributed runtime executable. After provisioning, this command launches a real game:

```sh
cargo run --release --features maintainer-tools --example investigate -- \
  "/path/to/Stellaris" "$PWD/.local/new-attempt" normal
```

The installation must be the exact M45-observe executable/content snapshot identified by Native.
The output parent must exist and the output directory must be new. Other scenarios are `long-hold` (29,999 milliseconds), `cancel`,
`caller-loss`, `worker-loss`, `worker-loss-before-launch`, and `timeout`. The worker-loss scenario injects the consumer's loss
notification; no debugger qualification follows from it. Each successful allocation retains the
request, private profile, owner journal snapshot, report, capture, and game stdout/stderr.

Run `cargo test --workspace --features maintainer-tools` for journal and lifecycle controls.
Mac tests use suspended `/bin/sleep` children, a real terminated worker, and a separate owner process
killed before game allocation to prove unresolved-reservation refusal without orphaning a game.
These tests use a private test-only store constructor, inaccessible to consumer builds. Real-game
checks are separate and must verify profile preservation and raw child reaping. Unsupported-host
CI runs shared schema/protocol tests and compile checks; it does not qualify a live platform.

The complete configured real-game control command is:

```sh
python3 tools/check-candidate-lifecycle.py "/path/to/Stellaris" "$PWD/.local/new-lifecycle-batch"
```

It builds the optimized maintainer example, hashes the ordinary profile before and after every
attempt, and keeps an unrelated sentinel process alive throughout. Missing prerequisites fail;
this command never substitutes a synthetic target.

The retained [SDK-516 controls](../native/candidate-lifecycle.md) record the initial implementation results and limits.
