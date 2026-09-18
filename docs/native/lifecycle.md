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
