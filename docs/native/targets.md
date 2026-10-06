# Targets and adaptation

## Exact identities

| ID used here | Historical executable SHA-256 | Applicability |
| --- | --- | --- |
| M45-old | `408a5700a202837f16041bf14b5da34ff4a9d939b98e62a8240dc68dd602ddf7` | Native ARM64 macOS 26.6.2 / 25G83, Apple Silicon; older Cygnus 4.5 beta ready-world experiments |
| M45-observe | `3d4c8a7046d87175ce7e3b513b1a2ce589050d654d332744518a49d13ac82216` | ARM64 macOS, Cygnus 4.5.0 (1434); reference, early observation, Atlas discovery/grammar experiments |
| M45-release | `07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd` | ARM64 macOS, Cygnus v4.5.0 (8697), the full 4.5 release from Steam; catalogued from 2026-09-22 until SDK-674 removed it |
| M451-hotfix | `29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38` | ARM64 macOS, Cygnus v4.5.1; catalogued from 2026-09-29 until SDK-714 replaced it |
| M452 | `c621723d9c8e0c1cd153319208d30a9dfbb9e63675be86f9d0ae7debeaa7fe1b` | ARM64 macOS, Cygnus v4.5.2 (9776); catalogued 2026-10-06; the only catalogued target |
| W45 | `bd86b8c8187bd23b793b6680cc979945e696f97c0a6aa89b5ca4199a5739535f` | Windows 11 Home 10.0.26200 x64, Cygnus 4.5.0 (9e73), Steam build 25085736 |
| W446 | `bc451c72d9654c8901f1bb0bee1dd78d76f415465c2fbf746e9f98ade333173a` | Same Windows host, Pegasus 4.4.6 (fdde), public build 24109497; 46,418,552-byte AMD64 PE |

M45-observe ARM64 slice SHA-256 is `1e0c9aec45650272fcaecba2eb47f8dce8f17bc08ef2b992be18c99ae098c623`. M45-release ARM64 slice SHA-256 is `a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9` (85,076,200 bytes). M451-hotfix ARM64 slice SHA-256 is `2aeb9e15241bb114fd9f35a2dd09b454a5df6a0b1948b229d9eb83123e665c21`. M452 ARM64 slice SHA-256 is `573e0f2511317e83f47a4d11f9c48f31361fe1cbb68900aae99ca945b2e47507` (85,110,152 bytes). Universal-image identity alone does not select process architecture; mixed image/slice identities are unsupported. Per-run manifests in the bundles keep OS, LLDB, fixture and content identities. Intel and ARM64 observations are not interchangeable.

M452 is the only catalogued build. Native keeps full-release targets only, and a patch is not
admitted automatically; SDK-557 is the update rehearsal on the next build. Native supports one
release at a time: SDK-674 removed the M45-release record and pins (in Git at `b9dd059`), and
SDK-714 replaced the M451-hotfix record, pins and expected output in place (in Git at `04e80d8`).
The ignored exact-build tests and the `tests/expected/*m452` directories check M452.

## Port from M45-observe to M45-release

Both slices keep their symbols, so each pinned entry moved by its mangled symbol name. Return
addresses inside a function (`reader_return`) and the end of the scheduler table in
`NNullObjAndDatabaseInitUtil::SetupDatabases` kept their offset from the function start. The
`CTraditionCategory::ReadMember` field tokens (16793 `tree_template`, 14263 `traditions`), the
declaration slots, the registry layout and the `CReader`/`CLexer` offsets did not change; the
M45-observe expected output stayed valid.

Pitfall found by the live run: when the worker-loss fault kills the LLDB worker, Darwin can give
`EPERM` for `kill` on the worker's process group while its only member is still exiting. Cleanup
waits for that exit within its one-second budget.

## Port from M45-release to M451-hotfix

Fresh symbol and disassembly inspection gave separate fixture, script-check and world pins; no
version fallback or uniform address slide is used. The shared declaration layouts stayed the M45
layouts. The special-project inline loader still calls the reader at `Init()+0x160`, reads its
root at `+0x1d0`, and ends the file at `+0xa8`. The logger's formatted and unformatted virtual
calls moved to `0x1025087e4` and `0x1025088b8`; the scripted-trigger stream call is `0x10212388c`.
The tradition reader return is `0x100ce3054`, immediately after its reader call. Trigger writes
keep the `CAndTrigger` address point `0x103095ee8` and the child-array address point
`0x103000458`. The world pins are on [ready-world observations](ready-world.md#world-pins-on-m451-hotfix).
Raw symbol tables, disassembly, vtable slots, the old-to-new address map and capture scripts are
in `.local/sdk-650/hotfix/`.

Pitfalls of this port:

- The shared scoped-body matcher broke on changed compiler source paths and relocated local `adr`
  bases; see [scoped numeric](scoped-numeric.md#transfer-to-the-451-hotfix).
- Relocated store IDs change row order: static parity compares dynamic-namespace rows without
  assuming order, and checks current source stamps apart from the reviewed build's provenance.
- The SDK-533 live storage record (`tests/expected/m45/field-storage-sdk533.json` at `b9dd059`)
  was observed on M45-release only. The live fixture cases for omitted and repeated
  `unlocks_agenda` check the same storage on M451-hotfix.
- Fixture-loader and inline-fixture addresses moved, but field tokens, owner offsets and storage
  offsets did not: `custom_tooltip` `+0x1c0`, `unlocks_agenda` `+0x5a0`, special-project
  `fleet_power` `+0x5d8` (fixed point, scale 32768) in requirements at `+0x5a0`.

## Port from M451-hotfix to M452

Every pinned function and global moved by its symbol name; repeated constructor names keep their
order. The 50 functions captured for the M451-hotfix port have the same instruction count on M452.
Their differences are relocated page offsets and strings, lexer token IDs, one diagnostic source
line, and `CCountry` field offsets (below). Interior pins keep their offset
from the function start: the tradition and relic reader returns at `LoadFile+0x70`, the
special-project inline loader at `Init()+0xa8`, `+0x160` and `+0x1d0`, the sourced log call at
`CScriptedEffect::OnError+0x40`, the stream log call at `CScriptedTrigger::PostValidate+0xa8`,
the logger's virtual calls at `CPdxLogFileAndLine::operator()+0xcc` and `+0x1a0`, and the vtable
address points at `+0x10`. Layouts, field storage offsets and recipe slots did not change. Symbol
tables, captures, the address map and its scripts are in `.local/sdk-714/`.

Pitfalls of this port:

- **Lexer token IDs move unevenly.** IDs up to at least `0x3414` stayed; IDs from `0x3a35` to
  `0x447a` moved up by 9, and IDs from `0x44fa` up by 8 (`yes` is `0x3ff8`, special-project
  `fleet_power` is 18433). Derive each token from the build; do not apply an offset. Tests that
  assert a token number and the scoped `GetValue` shapes, which pin the token switch, need the new
  values; see [scoped numeric](scoped-numeric.md#transfer-to-m452).
- **`CCountry` lost 0xc0 bytes** before its name and flags: `GetNameNoThe` adds `+0x13e0` (was
  `+0x14a0`), `CEventScope::GetFlags` reads country flags at `+0x19a0` (was `+0x1a60`). No recipe
  holds a `CCountry` offset, but the retired [world pins](ready-world.md#world-pins-on-m451-hotfix)
  are M451-hotfix only.
- **A store to a global through an `adrp` page** was a literal page offset in canonical shapes, so
  the prefix shape broke when `CPostInitVariableValueDatabase::_pInstance` moved. Canonical shapes
  now name the global of an `adrp` page store, as they did for `add` and `ldr`.
- **Engine changes, not port defects:** six AI platform effects, the triggers
  `is_preferred_platform_weapons` and `pop_ethics_divergence`, the define
  `DESIGNER_STARBASE_WEAPON_PREF_MUL`, the edict container `relay_network_modifier`, and one
  country modifier node instead of two ([modifier masks](modifier-masks.md#result-on-m452)).
  `STATION_BUILD_DISTANCE_MAX`, listed as new in the 4.5.2 notes, was already in the M451-hotfix
  executable.
  Civics lost `multiply_by_habitability_effect_modifier`, and installed content loads 45,583
  modifiers (5 more).
- **The engine logs more fixture errors.** A repeated definition key now logs `Object with key: …
  already exists` at each repeat, and a quoted variable with a space and a trigger in the wrong
  scope now log during the fixture load. The emitting code did not change. Live expectations
  that counted diagnostics needed these entries; stored values did not change.
- **Do not run a live case beside heavy static work.** With parity and population runs on the
  same host, one control-trigger fixture lost its loader return; alone, it passed.

## Windows adaptation (W45, W446)

The SDK-447 to SDK-449 prototypes ran one frozen ready-world scenario through separate Mac and
Windows adapters; it is not a supported API. Windows symbols were stripped and no PDB was
available. Ports used fresh disassembly, masked-pattern candidates, console registrations,
call-site argument reconstruction, live object checks and prologue pins. A matched pattern is only
a candidate: one apparent string-constructor match was an assignment routine. W446 changed
entrypoints, singleton addresses, country internals and the idler's save-manager location, and
needed a null module handle for the process-local message hook. Sources are in the `sdk-testing`
bundle under `prototype/compatibility-harness/{apple-silicon,windows,windows-446}/`; raw Windows
archives are `linear-records/assets/3abce4f4-ee3d-4a66-bb4f-5ef058a2fb66` (W45) and
`c7ff3152-650d-4148-bb86-a2b7ac72e306` (W446).
