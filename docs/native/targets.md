# Targets, adaptation, and qualification

## Exact identities

| ID used here | Historical executable SHA-256 | Applicability |
| --- | --- | --- |
| M45-old | `408a5700a202837f16041bf14b5da34ff4a9d939b98e62a8240dc68dd602ddf7` | Native ARM64 macOS 26.6.2 / 25G83, Apple Silicon; older Cygnus 4.5 beta ready-world experiments |
| M45-observe | `3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216` | ARM64 macOS, Cygnus 4.5.0 (1434); reference, early observation, Atlas discovery/grammar experiments |
| M45-release | `07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd` | ARM64 macOS, Cygnus v4.5.0 (8697), the full 4.5 release from Steam; the catalogued target since 2026-09-22 |
| M451-hotfix | `29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38` | ARM64 macOS, Cygnus v4.5.1; exact hotfix target added 2026-09-29 |
| W45 | `bd86b8c8187bd23b793b6680cc979945e696f97c0a6aa89b5ca4199a5739535f` | Windows 11 Home 10.0.26200 x64, Cygnus 4.5.0 (9e73), Steam build 25085736 |
| W446 | `bc451c72d9654c8901f1bb0bee1dd78d76f415465c2fbf746e9f98ade333173a` | Same Windows host, Pegasus 4.4.6 (fdde), public build 24109497; 46,418,552-byte AMD64 PE |

M45-observe ARM64 slice SHA-256 is `1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623`. M45-release ARM64 slice SHA-256 is `a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9` (85,076,200 bytes). Universal-image identity alone does not select process architecture. Original per-run manifests retain OS, LLDB/compiler, fixture/settings, source, and content identities; use these for the particular experiment rather than inferring them from another row.

The historical Intel 4.4.6 extraction baseline is separate: `typed-extraction/typed-extraction/reference-observation-prototype/baseline/` and the spike's `evidence/native-findings.md` retain its provenance. Intel and ARM64 observations are not interchangeable. No fresh Intel, Linux, or Windows early-parsing qualification was performed by this migration. Historical executables are not presumed retained merely because their hashes are recorded.

## What adaptation established

SDK-447, SDK-448 and SDK-449 demonstrate one frozen ready-world scenario through distinct real adapters. Shared source at `b822716a950dbaa1a17a6ccc5b4c5c93e24bb4d1` has combined SHA-256 `1042c9ec4ab83ef6ccde8bc367cde31ba88116591c5069a4231a36f94c629819`. The Mac baseline publication is `59fd8ba463a8778f5ede8f6d67f4fb12cb89e81b`. No common behavioral amendment was required. The literal protocol remains `sdk-446/v1-draft`; this is a frozen experiment, not a supported API.

Windows symbols were stripped and no PDB was available. Ports used fresh disassembly, masked-pattern candidates, console registrations, call-site argument reconstruction, live object checks and prologue pins. A matched pattern is a candidate. One apparent string-constructor match was an assignment routine and was corrected before use.

W446 changed entrypoints, singleton addresses, country internals and the idler's save-manager location. It also exposed a reused Windows host bug: the process-local message hook needed a null module handle. A fixture used `capital_scope.planet` after the engine rejected a colony-valued direct planet flag. These are distinct native-plumbing and fixture changes, not automatic proof of changed shared rules.

W45 uses a game-produced fixture hash `919df894628dcd1e21f636eb97eb8f20d9bb40b59527c2ef92cb9e2d297aeed7`, nonzero player 16777218, disabled DLC. W446 uses `a3373a32c4197eb863ad784ad2d538e90f9e6d46170a9b614bcf5e6255eeb2b5`, player zero and 28 required DLCs. These satisfy scenario roles; they are not identical worlds or a controlled content-equivalence comparison. The W446 fixture lacks the extra nonzero-player anti-hardcoding control.

## Support and remaining gates

Current support follows the target catalogue and Cargo tests, as specified by the
[simplification decision](../design/simplification.md). M45-release and M451-hotfix are catalogued. M45-release replaced
M45-observe on 2026-09-22; the hotfix was added on 2026-09-29. Steam does not offer old open betas for download, so Native keeps
full-release targets only. Windows
work is deferred under the [roadmap](../roadmap.md); the decisions below describe the historical
experiments and do not admit another build or platform.

SDK-476 originally started qualification with a pinned Apple Silicon 4.5 beta and required Mac
and Windows stable targets. The simplification decision and roadmap supersede that platform gate:
Windows is deferred, and support is now the exact catalogue entry plus its tests. Linux and Intel
Mac remain outside the initial scope. A patch is not admitted automatically.

SDK-445 is marked Done in Linear, but its text and result documents retain an open overall
maintenance comparison. Do not derive economical maintenance from its status. SDK-557 is the
deferred update rehearsal on a second distinct Apple Silicon executable. It replaces SDK-485, which
was closed on 2026-09-24; the earlier Mac/Windows criterion is superseded. The retained SDK-476 records did not establish a second ARM64 target; the 4.5.1 port below
is a separate current adaptation, not that frozen rehearsal.
Partial wall intervals and run counts are not active human/agent labor measurements.

Source: `sdk-testing` bundle, `sdk-testing/prototype/compatibility-harness/{apple-silicon,windows,windows-446}/`; Mac raw archive in `apple-silicon-baseline`; Windows raw archives in `linear-records/assets/3abce4f4-ee3d-4a66-bb4f-5ef058a2fb66` and `c7ff3152-650d-4148-bb86-a2b7ac72e306`. Local exported issue/comment records include SDK-476, SDK-485, SDK-445 and SDK-447–449. [Retrieval instructions](retrieval.md) explain nested archives.

## Port from M45-observe to M45-release (2026-09-22)

Both slices keep their symbols, so each pinned entry was moved by its mangled symbol name. Return
addresses inside a function (`reader_return`) and the end of the scheduler table in
`NNullObjAndDatabaseInitUtil::SetupDatabases` kept the same offset from the function start, and
were checked in the disassembly. The scheduler table kept 198 rows, offset 96 and stride 48. The
`CTraditionCategory::ReadMember` field tokens (16793 `tree_template`, 14263 `traditions`) and the
declaration slots did not change. Every static test passed with the M45-observe expected output
unchanged: 164 registries, the same fields, storage offsets and declarations. The structure offsets
of the registry layout and of `CReader`/`CLexer` did not change: all 43 live cases passed on the
release build.

The live run also found a supervisor race that is not specific to the build. When the worker-loss
fault kills the LLDB worker, Darwin can give `EPERM` for `kill` on the worker's process group while
its only member is still exiting. Cleanup now waits for that exit within its one-second budget.

## Port from M45-release to M451-hotfix (2026-09-29)

The installed Stellaris 4.5.1 universal image is
`29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`; its ARM64 slice is
`2aeb9e15241bb114fd9f35a2dd09b454a5df6a0b1948b229d9eb83123e665c21`.
The catalogue keeps the 4.5.0 release and adds this exact hotfix identity. World bindings were
added only to the hotfix, because the 4.5.0 route had disassembly inspection but no successful live
world check. The world route was retired on 2026-10-02; its bindings are in Git at `d8f9d8a`.
Mixed image/slice
identities remain unsupported. No version fallback or uniform address slide is used.

Fresh symbol and disassembly inspection established separate fixture, script-check and world
pins. The shared declaration layouts remain the M45 layouts. The special-project inline loader
still calls the reader at `Init()+0x160`, reads its root at `+0x1d0`, and ends the file at `+0xa8`.
The logger's formatted and unformatted virtual calls are now `0x1025087e4` and `0x1025088b8`;
the scripted-trigger stream call is `0x10212388c`. The tradition reader return is
`0x100ce3054`, immediately after its reader call. Trigger writes retain the `CAndTrigger`
address point `0x103095ee8` and the child-array address point `0x103000458`.

The world pins below belong to the [retired world route](ready-world.md). For SDK-650 world
preparation, `CInGameIdler::UpdateInternal(bool)` is `0x10086de60`.
The pause is at `+4`, after its first `sub sp, sp, #0xe0`. A pause at the entry carried
a branch-type status bit that the debugger could not restore; one ordinary instruction
clears that transient state. The full register check stays in force.
The expected caller chain remains `CGameIdler::Idle(bool)`, then
`CApplication::UpdateOneFrame(bool)`. `GetGameDateIfPossible()` is `0x100707da4` and reads
`g_CurrentGameState` at `0x1032e9450`, readiness at `+0x98`, and date at `+0xb8`.
The in-game idler global remains `0x1032e9438`; `JumpToNextDay` reads its pause byte at `+0x584`.
`CHuman::AccessSelectedCountry` reads the selected country ID at `+0x54` and checks country IDs
at `+0x20`. `SetCountry` writes scope type 4 at `+8`, ID at `+0x10`, and clears the cached
object at `+0x1c`. The scope constructor still fits the allocated `0x180` bytes.
Flag lookup uses 16-bit IDs at array `+0x10`, count `+0x1c`; signed counts use array `+0x40`
and count `+0x4c`. These are static layout checks, not a live world result.

For SDK-647 variable reads, `GetVariablePointer(CEventScope const&, CString const&)` is
`0x100d0d704`, `CVariables::VariableIsSet(CString const&) const` is `0x100d1e7b8` and
`CVariables::GetVariable(CString const&) const` is `0x100d1e784`. `GetVariable` looks up the map at
`+8` and loads the 64-bit value at entry `+0x30`, or the engine's zero constant for a missing
entry. The stored scale is 100000. The retired [live cases](ready-world.md#variables) verified
these reads.

Raw symbol tables, every pinned function's disassembly, vtable slots, the old-to-new address map,
and the capture scripts are retained under `.local/sdk-650/hotfix/`. No game process was launched
for this adaptation. The coordinator owns the live checks.

Default `cargo test` passed: 702 library tests and all default integration and example checks.
The current target-selection tests also pass after restricting world bindings to the hotfix.
Static `cargo parity` passed 27 cases and failed four. The SDK-533 storage comparison rejects
the old recorded build hash. Three recording comparisons report changed
`fields-council_agendas.json`, `fields-megastructures.json`, and `command-grammars.json`.
The actual method regression is unresolved scoped-number forms and selection for `agenda_cost`,
`cycle_length_in_days`, `overclock_cooldown`, and related command operands. The command recording
also carries the new build hash and additional gaps. Candidate recordings and structural diffs
are retained under `.local/sdk-650/hotfix/`; tracked expectations were not changed.
The shared scoped-number analysis needs further adaptation. This port does not establish full
static parity or successful live world behavior.

The shared scoped-body matcher was repaired for changed compiler source paths and relocated
local ADR bases; its complete instruction checks remain. See [scoped numeric](scoped-numeric.md).
Static parity compares full dynamic-namespace rows without assuming order from relocated store
IDs, and checks current source stamps separately from the reviewed build's provenance.
Historical SDK-533 storage remains applicable only to its exact 4.5.0 build. Fresh 4.5.1 live
checks matched all 22 stored-duration cases and the script argument matrix. The
[ready-world baseline, expiry matrix and failure controls](ready-world.md) of the retired world
route passed on 4.5.1.
