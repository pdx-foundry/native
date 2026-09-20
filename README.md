# PDX Native

A standard Rust API to ask Stellaris questions, the same on each platform and game build.
Atlas is its first consumer.

> **Simplification in progress (2026-09-20).** The [simplification decision](docs/design/simplification.md)
> defines the target API (`Native`, `Game`, `Answer<T>`) and removes replay, the evidence package,
> qualification records and the Cargo features. The code still has the earlier API until the
> decision's work order is complete. The source and its doc comments describe the present code.

- [Specification](docs/specs/native.md): what Native does.
- [Technical design](docs/design/architecture.md): project layout and target composition.
- [Roadmap](docs/roadmap.md): order of work.
- [Engine knowledge index](docs/native-evidence.md): findings from prototypes and probes.

The target catalogue has one build: the exact M45-observe ARM64 executable (Stellaris 4.5 beta,
Apple Silicon). An unknown patch is refused; it never inherits another build's recipe.

## Checks

```sh
cargo test --workspace
cargo test --workspace --features test-support
```

## One-time setup for live games (Apple Silicon macOS)

Native permits one Native-owned game per host. Its reservation directory must exist before the
first live game. Run these commands from the account that runs Native:

```sh
sudo install -d -o root -g wheel -m 755 "/Library/Application Support/PDX Native"
sudo install -d -o "$(id -un)" -g "$(id -gn)" -m 700 "/Library/Application Support/PDX Native/instances"
```

The parent directory must be owned by root and not writable by group or others. The `instances`
directory must belong to the running account with no group or other access. Do not run these
commands to take over a directory that another account owns; inspect its ownership and its
unresolved reservations first. Native never creates, moves, or clears this directory itself.

A consumer supplies the supervisor process: a dedicated direct child that calls
`pdx_native::supervisor::serve(stdin, stdout)`. See `examples/live.rs`.
