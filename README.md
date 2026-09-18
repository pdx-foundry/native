# PDX Native

Rust bridge to Stellaris.

The current library replays the bounded historical registration/category-read window. It does not
launch games or qualify a current installation. An Atlas-style caller uses `Engine::replay` with a
relocatable artifact root and a pinned descriptor reference.

```sh
cargo run --example replay -- tests/fixtures/synthetic tests/fixtures/synthetic/cases/normal.ref.json
cargo test --workspace
python3 tools/check-replay-boundary.py
```

Synthetic fixture results retain `synthetic` origin. Private retained replay needs the restored
bundle described in [retrieval instructions](docs/native/retrieval.md).

- [Native specification](docs/specs/native.md)
- [Technical design and project layout](docs/design/architecture.md)
- [Bounded replay design](docs/design/replay.md)
- [Native evidence and qualification records](docs/native-evidence.md)
