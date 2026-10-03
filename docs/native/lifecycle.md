# Launch, isolation, process lifetime, and cleanup

The current macOS launch is `src/binding/platform/macos/lifecycle.rs`. The retired 4.5.1 world
route is in [ready-world observations](ready-world.md). The experiments below keep their original
build limits.

## Supervisor and worker pitfalls (macOS)

- **Debugger shutdown.** A forced debugger shutdown while the game is stopped under the debugger
  leaves the game defunct. The parent's `waitpid` then returns `ECHILD`, and disposal cannot be
  confirmed. A retry does not help, and LLDB cannot detach and keep the game stopped. When the
  debugger ends the game in an orderly way first, the original parent can reap it. The supervisor
  gives that path two seconds, then forces the worker to stop.
- **Exited process groups.** macOS refuses a signal to a process group whose members have all
  exited. Check that the group is not empty before you signal it, and keep the direct child
  identity until it is reaped.
- **Harmless test processes.** macOS kills a copied Apple system binary before a test can inspect
  it. A test that needs a harmless process named `stellaris` must compile its own.
- **Ordinary game conflict.** Any process named `stellaris` makes a live start refuse, and makes a
  running game report lost isolation. Run the live tests apart from the process tests that start
  such a process.
- **Debugger authorization.** On macOS 27.0, debugger attaches can wait for authentication even
  with Developer mode on. While approval is pending, live cases time out at the startup deadline
  with `worker-start` as the last completed phase and `hooks-requested` as the worker's last
  record. A plain `lldb` attach to a freshly compiled program hangs the same way, which separates
  this from a Native fault. Approve one attach in a terminal of the same login session before a
  live run. Attach to a small compiled looping program: macOS always refuses an attach to
  `/bin/sleep`. How long the approval lasts is not established.
- **A saved credential is not attach permission.** A non-interactive `AuthorizationCreate` request
  for `system.privilege.taskport` tests only for a saved credential. It returned `NO (-60007)`
  while a real `lldb` attach from the same shell succeeded, so a check before the start would
  refuse games that can attach. The bounded attach is the authority.
- **Bounded attach and debugger cleanup.** A watchdog gives `target.Attach` 15 seconds. The attach
  stays on LLDB's script thread, which owns its API locks. On expiry the watchdog flushes
  `capability-unavailable` with the attach reason and exits without waiting for LLDB shutdown.
  LLDB normally starts `debugserver` in a separate process group with `--setsid`, so it survived
  worker cleanup. Native's launcher in `LLDB_DEBUGSERVER_PATH` joins the worker's group, removes
  `--setsid` and executes the selected LLDB's stub. No process-name kill is used.
- **Session transport.** Session files are temporary and are not recovered, so the supervisor
  does not `fsync` them. Atomic publication, bounded reads, session identity, stream continuity and
  the hashes of the copied worker files protect answer integrity; keep them. The worker still calls
  `fsync` for its atomic control messages and appended trace records.
- **Worker-loss cases.** Worker loss before hook activation and after the first registry entry
  test different cleanup guarantees; keep both live cases.
- **Process inventory.** The one-second process inventory deadline expired once after
  activation, for an unknown reason; later runs took 0.03 seconds. The deadline was not relaxed.
- **First-case crash.** A first live case once stopped on `EXC_BAD_ACCESS` at address zero after
  an OpenGL context failure in the engine log; disposal was confirmed and the retry passed. A
  live case must select only the work directories that it created.

## Worker handshake and fault controls

The supervisor stays the game's direct parent and spawns LLDB with embedded Python as the worker
(an external Python cannot import `lldb` without Xcode's interpreter setup). Before the game
resumes, worker and supervisor exchange a handshake: protocol revision, attempt identity, process
identities, executable identity, and LLDB and Python versions. The trace is an append-only
sequence with a terminal record.

`worker.py::decide_pause` makes the pause decision from observed progress. A fixture never owns
the pause. The private request carries one fault with an observation target and a control kind;
the hidden `GameOptions::fault` selects a registry, the fixture or the modifier table, and
modifier faults are restricted to worker loss. The worker holds a worker-loss stop for the
supervisor to kill it, without publishing a safe pause. `src/protocol/hooks.rs` owns the shared
hook names. Game-free checks are `tools/observation/test_worker.py` and `test_protocol.py`, plus
the fake-worker supervisor test.

## Early parsing requires a different owner

An independent process is the game's real parent and starts the debugger worker. Darwin
`posix_spawn` with `START_SUSPENDED` permits attachment at `_dyld_start`. The parent keeps the
child identity and uses `waitpid` to confirm reaping after worker loss. An earlier
debugger-owned attempt left a defunct entry owned by PID 1 with unconfirmed disposal. Owner loss,
concurrent supervisors and orphan recovery are not established.

## Background launch and visibility (M45-old)

The M45-old bridge prototype launched with `open -n -g -j -W`, `SDL_MAC_BACKGROUND_APP=1` and an
AppKit guard that hides windows. Two failures matter for display and visibility work (SDK-571):
copying ordinary borderless settings made the game visible despite a "not frontmost" report, and
`open -j` alone had a visibility race. Keep independent on-screen window observations; sampling
proves the sampled runs, not a guarantee. The guard needed correct Objective-C `BOOL` and
no-argument signatures. "Process started" or "library loaded" never implies readiness. Sources and
records are in the `sdk-testing` and `apple-silicon-baseline` bundles.

## Windows (deferred)

The W45 and W446 prototypes use a detached Python owner, a separate inactive desktop, DX11 (DX9
failed there) and a suspended primary thread, and assign the game to a kill-on-close Job Object.
The game's command-line parser splits `-userdir` at hyphens: use a hyphen-free directory junction,
forward slashes and a trailing slash. Sources are under
`sdk-testing/prototype/compatibility-harness/{windows,windows-446}/`; [targets](targets.md) names the
raw archives.

## Live-run summary

Every session that owns a work directory ends with `session/run-summary.json`, written by the
supervisor after `owner.json` and `report.json`, so it states the final outcome.
`src/execution/run_summary.rs` defines its content: phase times on one supervisor monotonic
clock, hook states, stream holes, the reducers' results at the pause and the worker's last
diagnostic checkpoint. The worker replaces `session/worker-diagnostics.json` atomically before
blocking operations and after verified calls; a lost worker without a failure report has an
unavailable cause and a last witnessed operation, never an inferred debugger failure. Neither file
changes an answer, an outcome or cleanup. How the live harness keeps and reports the directory is
in [method authoring](method-authoring.md#live-cases).
