# Atlas caller migration

The SDK-519 prototype at `pdx-atlas/prototypes/native-registry` now uses the simplified Native API.
It remains a local, ignored Atlas prototype. Its previous source and freeze are preserved privately;
see [preservation](../native/preservation.md). Atlas's published claim ledger is unchanged.

## API replacements

| Earlier caller | Current caller |
| --- | --- |
| `Native::open(OpenRequest { installation_hint })` | `Native::open(path)` |
| `get_registry(name)` | `registries()` for discovery; `registry_fields(name)` for fields |
| `traditions`, `tradition_categories` | `common/traditions`, `common/tradition_categories` |
| `with_supervisor(command, options)` then `start_game()` | `start_game(GameOptions::new(command))` |
| Caller-supplied retention directory | Native-owned temporary work directory |
| `get_registry_items(name)` and `RegistryResult` | `registry_items(name)` and `Answer<Vec<String>>` |
| `registry_availability()` | The result of each question: complete, partial, or `Error` |
| `close()` returning `GameReport` | `close()` returning `Result<Disposal, Error>` |
| `Engine::replay_registry`, descriptors and final snapshots | `Native::from_recorded_answers(directory)?` and the same question flow |
| `production` Cargo feature | No features |

`Native::open` still returns `OpenError`. Question and session errors use `Error`; a failed start
can carry `Error::Startup { reason, disposal }`. A lost supervisor connection never confirms disposal.
Always await `close`, even when one or all questions fail. Each registry result is independent.
Failed final cleanup returns `Error::Cleanup { reason, disposal }` and keeps the work directory.
A confirmed process disposal can accompany a cleanup error, such as an unresolved host reservation.

## Keep the answer whole

Atlas retains `value`, `completeness`, typed `gaps` and `Source`. The source contains the exact
build, Native version, method and basis. This is the accepted amendment to SDK-473, recorded in
[the Atlas map](https://linear.app/unnamed-system/issue/SDK-470/specify-pdx-atlas-and-its-engine-derived-rule-database).
Claims no longer require Native capture hashes or artifact references. Re-run the question to check it.

A partial list keeps its established items; a missing item in that list proves no absence.
`Basis::Recorded` stays recorded after Atlas processing and proves nothing about the current game.
Recorded directories require `build.json` with the original serialized `BuildId`, such as
`"authored"` for a hand-written test. `Native::build()` retains that identity, and answers from
another build are refused. The recorder writes this metadata even when the question fails.
Disposal belongs to the session, separately from the answers. Item names establish no fixture
execution, field storage, validation, schema completeness or rule coverage.

## Local caller and checks

The caller has `describe`, `live` and `recorded` commands. Both live and recorded modes call
`collect(&Native, GameOptions)`. It reads both registries, retains each result and closes before
serializing. Startup failure yields no fabricated game or answers. Failed questions or unconfirmed
disposal give the CLI a nonzero exit status after it writes the structured results.

From the caller directory:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo run --release -- describe /path/to/Stellaris
cargo run --release -- live /path/to/Stellaris /path/to/answers
cargo run --release -- recorded /path/to/answers
```

The four tests use small authored `Result<Answer<Vec<String>>, Error>` records. They check
preservation of partial values and gaps, source stamps, independent failures, missing and corrupt
records, and the recorded CLI with an empty `PATH`. A recorded query error tests the consumer's
error handling; it does not reproduce a failed process start. Native's live suite checks that path.

The migrated prototype uses the adjacent Native checkout while this branch is reviewed. Before
freezing a release, replace that path dependency with the merged Native Git revision and update
its lockfile. Do not reuse the old freeze. Supply the authored category script through
`GameOptions::fixture(request)` before launch (SDK-532); enumeration alone does not inject it.
Then `game.observe_fixture()` returns separate registration entries and field reads from that
session. The first implementation covers one category file and the bounded initial read window.
