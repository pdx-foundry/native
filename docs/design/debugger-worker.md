# Loader-entry debugger worker decision (SDK-515)

Select an **LLDB subprocess with embedded Python callbacks** for the
`MacSuspendedChildLoaderEntry` strategy. The independent Native owner remains the game's
direct parent. The strategy revision fixes this backend when released; neither target recipes
nor Atlas requests gain a debugger selector. Changing the backend requires a new implementation
revision and affected qualification.

This is a bounded integration decision supported by a fresh candidate trial on M45-observe.
It does not qualify the Rust owner, a production worker, or a public live operation. The library's
admission result remains implementation-unavailable and unaccepted.

## Alternatives and evidence

| Choice | Assessment |
| --- | --- |
| LLDB executable with embedded Python | Selected. Reuses the accepted attach/callback mechanism. The selected Xcode supplies a working Python/LLDB pair; no Rust LLDB linking or separately configured Python binding is needed. All four fresh controls passed. |
| External Python importing LLDB | Viable with a matched interpreter, framework loader paths and module paths. The host's ordinary Python 3.13.5 returned `ModuleNotFoundError: No module named 'lldb'`; LLDB itself loaded its Python 3.9.6 module. Introducing a second interpreter setup provides no required capability for this window. This is a local discovery result, not a claim that external Python cannot work. |
| Rust LLDB binding | Requires selecting, distributing and linking a compatible binding/framework and translating the callbacks. No required benefit warrants that additional qualification work in this trial. Not built or benchmarked. |
| Replace LLDB with a custom debugger/in-process observer | Would reopen ordering, calling conventions, access and failure qualification. The retained dylib fallback did not establish parsing. Out of scope. |

The prior evidence was verified with `tools/evidence.py typed-extraction` and
`tools/check-observation-evidence.py`. The archive SHA-256 is
`70fce0ce8dae5dbb473937b08ef77e711218e34a1e2d6406519ad3b574dbfe79`.
The trial copies the executed source from retained run `20260917-002215-none-d8a910`, verifies
it against the sealed bundle, and applies exact, single-match edits. No native offsets or
breakpoint semantics are reimplemented in Rust or copied into tracked source.

## Process and transport

```text
trial runner (Python; maintainer-only orchestration)
  └─ independent owner (retained Python parent for this experiment)
       ├─ Stellaris (posix_spawn START_SUSPENDED, private profile, ARM64 guard)
       └─ selected Xcode lldb -b -x (new session/process group)
            └─ Xcode debugserver as needed for attach
            embedded Python: worker.py → retained debugger_attempt.py callbacks
```

In the production implementation the Native Rust owner takes the parent role. An additional
Rust worker shim is unnecessary: the strategy can spawn LLDB directly. This trial proves the
concrete external worker boundary; it does not prove a Rust port of the parent.

The selected local transport is a private per-attempt directory, with:

- `hello.json`: atomically renamed handshake containing protocol revision, attempt identity,
  game and worker PIDs, executable identity, source/guard hashes, LLDB version, Python version
  and module location. The parent checks every field before granting resume.
- `resume-granted`: parent acknowledgement after the handshake. The callback independently
  requires loader entry, ARM64, and enabled, resolved, zero-hit hooks before continuing.
- `trace.jsonl`: append-only UTF-8 JSON records, flushed and fsynced by one producer. Each record
  has attempt identity and a producer sequence. The terminal includes last sequence and field
  count. Thread IDs join loader entry, field reads and loader return; field reads also share an
  owner and source file/line. Registration is bounded to three call entries.
- `owner.json`: independent parent journal, including child ownership, accepted handshake,
  worker exit, disposal and exact child reaping. Its clock is never compared with worker time.
- `worker.stdout` and `worker.stderr`: separate diagnostics, never parsed as observation data.

This is a bounded file journal, not a general streaming service. Missing sequence/count records
prevent completion. Missing terminal data after worker death preserves partial facts as worker
lost. Partial JSON lines cause verification failure; the trial does not silently repair them.
Production must add bounded record sizes, storage failure handling, protected directory creation,
and partial-tail recovery that cannot grant completion. The existing replay format is unchanged.

The retained missing-hook control resumed with the field hook omitted. The fresh trial tightens
that control: it emits unavailable and exits at loader entry without any resume record. The
parent then disposes of the suspended child. This behavior is checked from raw records.

## Protocol authority and packaging

`tools/loader-entry-trial/protocol.py` owns the experiment's `sdk-515-trial/1` handshake;
both Python processes import that one module. The sealed SDK-483 callback owns the retained
event vocabulary. The trial adds only callback thread correlation and the fail-closed hook gate.
`verify.py` checks evidence; producer status labels do not establish success.

For the implementation, **Native's private `src/protocol` is the single owner of wire meaning**.
Generate the Python message encoder/decoder and schema version from that authority, including
the request/hello/acknowledgement and event envelope. Check generated files for drift in CI and
include their hashes in the package identity. Do not hand-maintain matching Rust and Python
schemas. This ticket adds no production schema or second runtime. The all-Python trial needs
no foreign binding generation; generated Python bindings are a required step in the Rust port.
Target-specific argument and source joins remain Native binding knowledge, not protocol policy.

The strategy package must contain the callback/bootstrap scripts, generated protocol module,
guard source and ARM64 guard binary, and an immutable manifest that pins them with the strategy
and binding revisions. The manifest also pins the owner/helper build identities. Discover the
host tool through `xcrun --find lldb`, resolve the absolute path once, and probe embedded Python
before allocating a game. Start LLDB with `-x` to suppress user initialization, remove inherited
`PYTHONHOME`/`PYTHONPATH`, and keep the selected executable fixed for that attempt. No fallback
to another debugger is permitted on a failed probe or handshake.

The candidate environment was macOS 26.6.2 (25G83), ARM64, LLDB
`lldb-2100.0.17.203`, embedded Python 3.9.6, and Apple clang 21.0.0. LLDB was
`/Applications/Xcode.app/Contents/Developer/usr/bin/lldb`, SHA-256
`0035650adb4c8278122f70771e2e052a2b6e6d644a76745ffecf8c3a0bd686ca`.
The module came from Xcode's `LLDB.framework/Resources/Python/lldb/__init__.py`.
Full version strings, absolute discovery paths and executed script/guard hashes are in each
trial's `preflight.json`, `expected.json`, run manifest and accepted hello. This establishes
one tested combination, not an accepted version range. Xcode/LLDB and debugger access are
external host prerequisites; this decision does not redistribute Apple's toolchain.

## Cancellation and disposal

The parent holds the direct-child identity until `waitpid` reaps it, so worker failure does not
remove the parent's ability to kill the game. The normal callback may request game termination;
only the parent's independent check establishes disposal. A dead worker, missing hook, rejected
handshake, timeout or handled cancellation must still enter the parent's cleanup path. The
parent stops/waits for LLDB, kills/reaps the game when necessary, then kills the worker process
group. The retained limits are a 15-second handshake/acknowledgement, 180-second owner observation
budget, five-second worker wait and ten-second game disposal budget.

The worker-loss control sends SIGKILL while the callback is stopped after registration. Raw
records show the parent's subsequent kill and reaping, with no terminal observation record.
The trial also routes owner SIGTERM to its cleanup block. SIGTERM delivery, parent SIGKILL,
controller loss, debugserver descendant accounting and concurrent owners are **not** qualified
by these four controls. Production still requires the specified independent supervisor, host-wide
reservation and resource journal. Do not copy the trial's process-list conflict check as a lock.

## Fresh evidence and reproduction

The exact universal executable and ARM64 slice match [M45-observe](../native/targets.md).
Preflight checks the complete filename set and bytes of the retained 68-file producer content
boundary (launcher settings, tradition categories and traditions). Fixture bytes match the
retained two-field category. The parent records private-profile hashes and checks the executable,
content files and four protected ordinary-profile files after each attempt. This is not a full
DLC/installation equivalence claim. No world or save is loaded.

From the repository root, **the following launches real games**:

```sh
python3 tools/evidence.py typed-extraction --restore # only if not already restored
python3 tools/loader-entry-trial/run.py .local/evidence/sdk-515/new-trial
```

The output directory must not exist. Missing installations, mismatched content/source, another
Stellaris process, missing embedded Python or debugger access are failures, not a synthetic
replacement. The native worker uses only pinned retained source. The trial copies its own
sources into `trial-source/` and snapshots executed producer scripts per run.

Offline verification and game-free tests:

```sh
python3 tools/loader-entry-trial/verify.py .local/evidence/sdk-515/trial-02
python3 -m unittest discover -s tools/loader-entry-trial -p 'test_*.py'
```

See the [fresh result record](../native/loader-entry-worker.md) for the four attempt identities,
timings, preservation hashes and interventions. Review checks covered startup ordering, exact
identity rejection, field/source/owner/thread joins, terminal counts, dropped records and
independent reaping. The repository's full default/test-support suites, format/lint checks and
architecture boundary checks remain required before publication.
