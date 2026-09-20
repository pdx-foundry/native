# PDX Native

Rust bridge to Stellaris.

`Native` pins an installation and answers `get_registry(name)` without launching a game.
An async `Game` owns one supervised Stellaris process and returns independent startup snapshots
for `traditions` and `tradition_categories`. The process stays paused at registry initialization;
startup does not imply a loaded world or available gameplay operations.
See [installation queries and Game sessions](docs/design/live-observations.md), including the
SDK-518 migration and qualification boundary. Reader and field discovery remain explicitly unknown.

`Native::analysis()` decodes one qualified M45-observe function without a game, content files,
or a debugger. [Static analysis and replay](docs/design/static-analysis.md) describes the public
interface and its bounded qualification.

The separate evidence library replays both historical and new registry artifacts without a game.
Consumers supply their executable's supervisor role; Native distributes no runtime executable.

```sh
cargo run --example replay -- tests/fixtures/synthetic tests/fixtures/synthetic/cases/normal.ref.json
cargo test --workspace
python3 tools/check-replay-boundary.py
cargo test --workspace --features test-support
python3 tools/check-admission-boundary.py
```

Synthetic fixture results retain `synthetic` origin. Private retained replay needs the restored
bundle described in [retrieval instructions](docs/native/retrieval.md).

To inspect an installation without launching it:

```rust
use pdx_native::{Native, OpenRequest};

fn inspect() -> Result<(), pdx_native::OpenError> {
    let native = Native::open(OpenRequest {
        installation_hint: "/path/to/Stellaris".into(),
    })?;
    println!("{:?}", native.get_registry("traditions"));
    Ok(())
}
```

`cargo run --release --features production --example capabilities -- /path/to/Stellaris` prints the report as JSON. Supply an
executable, `stellaris.app`, or installation directory; automatic installation discovery is not
implemented. The initial catalogue identifies only the exact M45-observe ARM64 image. Unknown
patches never inherit its recipe. Reports distinguish qualification from availability, and report
content/executable changes without rebinding. See [capability admission](docs/design/admission.md)
for limits and build checks.

- [Native specification](docs/specs/native.md)
- [Technical design and project layout](docs/design/architecture.md)
- [Bounded replay design](docs/design/replay.md)
- [Loader-entry worker decision and bounded trial](docs/design/debugger-worker.md)
- [Native evidence and qualification records](docs/native-evidence.md)

- [Consumer-hosted lifecycle and maintainer harness](docs/design/lifecycle.md)

Maintainer-only [candidate observations](docs/design/candidate-observations.md) capture the retained
registration/category window under an independent supervisor and emit replayable evidence.
Public live admission requires the production feature and a reviewed target, content, toolchain, implementation, and release profile. SDK-527 changes the shared implementation identity; live admission remains unavailable until a separate requalification. The SDK-518 acceptance is historical; the changed SDK-521 session implementation requires new acceptance. See the [production consumer contract](docs/design/live-observations.md) and `examples/live.rs`.
