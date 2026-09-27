# Launch, isolation, process lifetime, and cleanup

## macOS ready-world operations

On M45-old, final bridge-only isolation records 18 fresh profiles: ten normal runs, four interruptions and one recovery after each. Independent checks recorded 13,761 samples with zero visible game windows, active samples or foreground samples. Nine AppKit lookups were unavailable; independent window/focus samplers still supplied evidence. Sampling establishes these runs, not a continuous non-interference guarantee.

Use a private profile copied from identified inputs; hash the source fixture, ordinary settings/selections/logs/saves and executable before and after. Final settings are windowed 1280×900, fullscreen/borderless disabled, 30 FPS cap, with empty `pdx_settings.txt`. The older path uses `open -n -g -j -W` and `SDL_MAC_BACKGROUND_APP=1`. A launch-loaded AppKit guard hides windows and uses accessory policy on the main queue.

Two failures matter: copying ordinary borderless settings made the game visible despite an older observer's “not frontmost” report; `open -j` alone also had a visibility race. Retain independent on-screen window observations. The old guard later needed correct Objective-C BOOL and no-argument signatures in M45-observe experiments; reuse its earlier source only with that correction and target-specific verification.

Readiness is a separate gate: bridge process/architecture/symbol checks, main-thread normal-input stack, ready loaded world, paused idler, date, actual human country, and correlated fixture markers. “Process started” or “library loaded” cannot imply readiness or completed action.

Interruption records separate pre-readiness, action accepted without completion, caller/host loss, and deliberately stopped game. The owner can SIGTERM then SIGKILL under a bound and retain partial records. No graceful in-game shutdown result follows from OS process exit or the `open` wrapper's return code.

Evidence: `linear-records/linear/doc-macos-bridge-only-isolation-and-recovery-evidence-3e3c53d31447.json`; `sdk-testing/sdk-testing/scratch/repeatability-isolation-probe/REPORT.md`; `apple-silicon-baseline/apple-silicon-native-evidence.tar.gz` contains the later common scenario's direct cleanup and observer records. Those final SDK-447 controls independently disposed 16 owned games after ordinary operation, partial launch, worker loss and timeout. Profiles were deliberately retained with paths/reasons rather than silently removed.

## Early parsing requires a different owner

M45-observe's final early-observation method makes an independent process the game's real parent, then starts the debugger worker. Darwin `posix_spawn` with `START_SUSPENDED` permits attachment at `_dyld_start`. The parent retains child identity and uses `waitpid` to confirm reaping after worker loss. The four final scenarios each establish disposal separately from observation completion. See [early observations](early-observations.md).

The earlier debugger-owned attempt left PID 61187 as a non-running defunct entry owned by PID 1. Its disposal remains unconfirmed in the historical result. Later successful parent-owned runs do not repair that record. Owner loss, concurrent supervisors and production orphan recovery remain unqualified.

## Windows ownership and isolation

W45/W446 use a detached Python owner, a separate inactive desktop, DX11 and a suspended primary thread. The owner assigns the game to a kill-on-close Job Object before resuming it. Cleanup uses a retained process handle and a separate request channel after execution-worker death; a 600-second owner bound remains if the controller disappears. Desktop/focus/window observations are retained. No keyboard/mouse input or desktop switch is used.

The game's command-line parser split at hyphens inside `-userdir`; a hyphenated checkout path crashed with a null write-directory path. Use a fresh **hyphen-free directory junction**, forward slashes and a trailing slash for the profile argument. Cleanup verifies the resolved target and unlinks only that junction. Profiles and evidence remain. DX9 failed on the inactive desktop; DX11 established the bounded path.

Prerequisites: the recorded unlocked single-user graphical session, exact installation, compatible game-produced fixture, recorded DLC/content, Python Windows APIs, Node and the recorded toolchain. The adapter refuses a different executable or an already-running instance. These results do not establish concurrency, cross-user ownership, owner-loss recovery or a universal invisible launch.

Authoritative experimental source: `sdk-testing/sdk-testing/prototype/compatibility-harness/apple-silicon/lifecycle.ts`, `windows/host.py`, `windows-446/host.py`; corresponding README files provide fresh-run commands. Raw Windows files are in the retained Linear assets named in [targets](targets.md). Historical helpers still carry original machine paths; [preservation](preservation.md) lists fresh-run limits.

## Findings from the Rust supervisor (M45-observe, macOS)

- **Debugger shutdown.** A forced debugger shutdown while the game is stopped under the debugger leaves the game defunct. The parent's `waitpid` then returns `ECHILD`, and disposal cannot be confirmed. A retry does not help, and LLDB cannot detach and keep the game stopped. When the debugger ends the game in an orderly way first, the original parent can reap it. The supervisor gives that path two seconds, then forces the worker to stop.
- **Exited process groups.** macOS refuses a signal to a process group whose members have all exited. Check that the group is not empty before you signal it, and keep the direct child identity until it is reaped.
- **Harmless test processes.** macOS kills a copied Apple system binary before a test can inspect it. A test that needs a harmless process named `stellaris` must compile its own.
- **Ordinary game conflict.** Any process named `stellaris` makes a live start refuse, and makes a running game report lost isolation. Run the live tests apart from the process tests that start such a process.
- **Debugger authorization.** SDK-630 observed debugger attaches waiting for authentication on
  macOS 27.0, even with Developer mode on. The earlier explanation was a per-login approval for
  `system.privilege.taskport`, shared for 10 hours; the fresh-login result below does not confirm
  that login boundary. While approval was pending, the live cases timed out at the startup
  deadline with `worker-start` as the last completed phase and `hooks-requested` as the worker's
  last record.
  A plain `lldb` attach to a freshly compiled program hangs the same way, which separates this
  from a Native fault. Approve one attach in a terminal of the same login session before a live
  run. SDK-633 adds a non-interactive Security framework request for this right in supervisor
  admission, before starting the game or worker. A refusal tells the developer to approve one
  attach in the same login session and retry. The request extends rights without allowing a
  dialog; it neither destroys cached rights nor changes the host's authorization policy.
- **Bounded attach and debugger cleanup.** A watchdog gives `target.Attach` 15 seconds. The
  attach stays on LLDB's script thread, which owns its API locks. On expiry the watchdog
  flushes `capability-unavailable` with the attach reason, then exits without waiting for LLDB
  shutdown. The supervisor carries that reason
  into the failed session report. LLDB normally starts `debugserver` in a separate process
  group and passes `--setsid`, which allowed it to survive worker cleanup. Native supplies a
  small launcher through `LLDB_DEBUGSERVER_PATH`: it joins the worker's group, removes
  `--setsid`, and executes the selected LLDB installation's stub. The unreaped worker reserves
  that group identity until cleanup confirms no live members remain. No process-name kill is
  used. Pause ownership, hooks and answers are unchanged.
- **Process inventory.** The one-second process inventory deadline expired one time after activation. Twenty later runs of the same command took 0.03 seconds each. The cause is not known; the deadline was not relaxed.

SDK-633 checks on 2026-09-26: the non-interactive `security authorize
system.privilege.taskport` returned `YES (0)` in the current approved login session. A real LLDB
attach against a deliberately blocked stub reached its test deadline, recorded the
attach failure, and left the stub for supervisor cleanup; cleanup removed it. This also checks
that the installed LLDB releases Python's interpreter lock during attach. Tests cover the
approval refusal message and Security error statuses. The unapproved state has not yet been
observed with the new admission check.

After the user logged out of macOS and back in, the non-interactive authorization check still
returned `YES (0)`. Both `normal` and `fixture_normal` passed in 24 seconds each. A process
inventory after the run contained no game, LLDB worker or `debugserver`. This verifies the
approved path after a fresh login, not the refusal path. Logging out is not a demonstrated way
to clear this approval on this host; its actual lifetime remains unestablished.

The standard suite and 40 Python tests pass. All 53 live cases passed across the full run and
affected-case reruns. The first `normal` case attached and activated its hooks, then stopped on
`EXC_BAD_ACCESS` at address zero after engine logs reported an OpenGL context failure; disposal
was confirmed and the retry passed. Its retained modifier log exposed a harness bug: the next
case searched earlier failed sessions too. The modifier comparison now selects only directories
created by its own case, keeping failed-run evidence intact.

## Live-run summary

Every session that owns a work directory ends with `session/run-summary.json`, written by the
supervisor after `owner.json` and `report.json`, so it states the final outcome. It is a developer aid; its failure is
printed on the supervisor's standard error and never changes the outcome or `close`. The live
harness (`tests/live.rs`) sets the hidden `GameOptions::keep_work_directory`, so `close` keeps
the directory. A passing case removes it; a failing case keeps it and prints
`kept <dir>; run summary <dir>/session/run-summary.json: outcome …, last completed phase …`,
also when a check fails after a clean `close`. The same directory holds `raw-trace.jsonl`,
`owner-events.jsonl`, worker and game output, and the private profile's engine logs.

What the summary says, and what it does not:

- **Timing.** One supervisor monotonic clock, from the host reservation to the end of cleanup.
  It excludes plan admission, the caller's handshake and the worker's own time. The session
  phases are `setup`, `worker-start`, `awaiting-pause` and `paused`; each is `completed`,
  `interrupted` (running when the session ended) or `not-reached`. Cleanup is timed apart. Worker
  progress has no clock: `worker.last_record` gives the last record the worker wrote.
- **Hooks.** `requested` comes from the worker's `hooks-requested` record, the states from
  `hooks-active-before-resume`. A missing hook is `absent`, `disabled` (the late-hook control),
  `unresolved` or `hit-before-resume`. When no requested hook is active, the worker stops before
  its hook state record, so the states are `unavailable` and only `requested` is listed.
- **Stream.** Holes are read from the stream as written, before a damaged stream loses its
  terminals. A hole gives the expected and found sequence numbers, never a count of lost
  records.
- **Observations.** The reducers' own results at the pause, projected: each registry's observed
  state, item count and diagnostics; the fixture's gaps, diagnostic window, and for each question
  its parsing, storage, validation (diagnostics by engine stage) and runtime; the modifier table's
  entry count. Before the pause they are `unavailable`; an observation the session did not
  request is `not-requested`. The summary decides nothing that an answer does not.

Lists keep eight samples with a count of the rest, and each text keeps 240 characters.
