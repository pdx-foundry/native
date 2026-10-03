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

### Policy and design

- [Development policy](docs/development-policy.md) Autonomy, knowledge ownership and preservation rules.
- [Native specification](docs/specs/native.md) Public operations, answers and behavior.
- [Technical design](docs/design/architecture.md) Module boundaries and target composition.
- [Simplification decision](docs/design/simplification.md) The compiler-need vision, the approved scope, removed mechanisms and the 2026-10-02 decisions.
- [Roadmap](docs/roadmap.md) Milestones toward full config coverage.
- [Development improvements](docs/design/native-dx.md) Open DX items for writing and adapting methods.

### Proposals

These are not accepted designs. Do not implement from them without an accepted decision.

- [Config checks](docs/design/config-checks.md) Atlas-owned oracle rules and the proposed vanilla corpus check.

### Methods and engine knowledge

- [Engine knowledge index](docs/engine-knowledge.md) Where to find engine facts and experiments.
- [Discovery methods](docs/native/discovery.md) Operations, source stamps, modules and method notes.
- [Method authoring](docs/native/method-authoring.md) Inspection, implementation, tests, parity and population runs.
- [Registry fields](docs/native/registry-fields.md) Field discovery, stops, compiler shapes and owner joins.
- [Reader kinds](docs/native/reader-kinds.md) Shared reader identities and broad value kinds.
- [Scoped numeric](docs/native/scoped-numeric.md) Operand proofs, routing forms, concrete storage and owner derivation.
- [Duration keys](docs/native/durations.md) Unit factors, combination rules and the flag-store countdown.
- [Registry items](docs/native/registry-items.md) Loaded collections and observation completeness.
- [Engine commands](docs/native/engine-commands.md) Commands, scopes, localization, on_actions and game rules.
- [References and dynamic names](docs/native/references.md) Reference lookups, flag namespaces, their shapes and the SDK-482 history.
- [Modifier blocks](docs/native/modifier-blocks.md) Shared fixed keys, entry forms and reader gaps.
- [Modifier families](docs/native/modifier-families.md) Generated names and the loaded modifier table.
- [Targets](docs/native/targets.md) Exact executable identities and build adaptation findings.

### Game processes and retained prototypes

- [Lifecycle](docs/native/lifecycle.md) Launch, isolation, process lifetime and cleanup.
- [Early observations](docs/native/early-observations.md) Injection and observation before content parsing.
- [Loader-entry worker](docs/native/loader-entry-worker.md) The debugger worker trial and its controls.
- [Engine calls and memory](docs/native/engine-calls.md) Calling conventions, layouts and live object identity.
- [Preservation](docs/native/preservation.md) Retained evidence and migration limits.
- [Prototype retrieval](docs/native/retrieval.md) Verify and restore local prototype bundles.
- [Source inventory](docs/native/source-inventory.json) Bundle origins and archive identities.

### Measurements

- [Performance](docs/native/performance.md) Static analysis and live-test costs.
