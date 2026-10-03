# PDX Native

Native is a new library with no consumers and an unstable API. Agents may make necessary development
changes without routine approval. Preserve the untracked knowledge acquired by prototypes and probes.
The [development policy](docs/development-policy.md) says how to work autonomously, where each
engine fact lives, and how to preserve acquired knowledge before a cleanup.

Native is a simple engine API, not an evidence archive. Read the [simplification decision](docs/design/simplification.md)
before you add an operation. Do not add replay paths, evidence descriptors, artifact hashes, qualification
records, or Cargo features. Public names prefer clarity to brevity (`registry_fields`, not `fields`).

Before you work on game launch, cleanup, injection, engine calls, memory layouts or discovery, read the
[engine knowledge index](docs/engine-knowledge.md). Match the exact build before you reuse a finding.

`STELLARIS_PATH` is set in the development environment and names the installed game. Use it in
commands for `cargo parity`, the ignored M45 tests and `examples/inspect`; do not write the path.

Native owns the platform and build methods. Atlas owns extraction fixtures, rule conclusions and coverage.
The code and its tests are the authority for supported operations; the knowledge pages keep the experiments.

Authored ARM64 in tests (`arm64!` in `src/engine/analysis/assembler.rs`) may carry inline
comments that say what an instruction means to the test, such as `mov w1, #7 // token 7`. Assembly
has no names to carry intent. This overrides the general rule against inline comments; do not
restate what the instruction does.

The RustRover MCP is available and can be used for rename refactoring, searching, and viewing code
inspections (such as warnings and errors).

## Documentation

- [Development policy](docs/development-policy.md) Autonomy, knowledge ownership and preservation rules.
- [Native specification](docs/specs/native.md) Public operations, answers and behavior.
- [Technical design](docs/design/architecture.md) Module boundaries and target composition.
- [Simplification decision](docs/design/simplification.md) The compiler-need vision, the approved scope and the 2026-10-02 decisions.
- [Roadmap](docs/roadmap.md) Milestones toward full config coverage.
- [Development improvements](docs/design/native-dx.md) Open DX items for writing and adapting methods.
- [Config checks](docs/design/config-checks.md) Atlas-owned proposal, not an accepted design.
- [Engine knowledge index](docs/engine-knowledge.md) The one index of the knowledge pages in `docs/native/`: methods, engine facts, processes, targets and private bundles.
