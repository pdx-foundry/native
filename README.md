# PDX Native

Rust bridge to Stellaris.

The library identifies exact installations, reports capability admission, and replays the bounded
historical registration/category-read window. The live consumer API asks `get_registry_items("traditions")` or `get_registry_items("tradition_categories")`; the [fresh qualification report](docs/native/registry-qualification.md) awaits maintainer acceptance. Native owns supervision, private content, and capture; callers configure their supervisor command and retention directory once. See [registry queries](docs/design/live-observations.md).
The optional maintainer API runs suspended candidate lifecycle attempts inside a consumer-supplied
supervisor process; Native distributes no runtime executable. An Atlas-style caller uses `Engine::replay` with a relocatable artifact
root and a pinned descriptor reference.

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
use pdx_native::{CapabilityRequest, Engine, OpenRequest};

fn inspect() -> Result<(), pdx_native::OpenError> {
    let context = Engine::open(OpenRequest {
        installation_hint: "/path/to/Stellaris".into(),
    })?;
    let report = context.capability(&CapabilityRequest::default());
    println!("{:?}: {:?}", report.qualification, report.reasons);
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
Public live operations remain unavailable pending reviewed qualification. See the [production consumer contract](docs/design/live-observations.md) and `examples/live.rs`.
