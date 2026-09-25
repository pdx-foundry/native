# Proposal v3: make Native development smoother for Milestones 4 to 7

## Scope and sources

Native (Rust crate `pdx-native`) answers questions about the Stellaris executable. Atlas is its only consumer. Milestone 3 closed on 2026-09-24 (SDK-570). The open Atlas project work that shapes the next months: Milestone 4 shared readers (SDK-541 to SDK-550 and SDK-569, each a new ARM64 static method), Milestone 5 registry sweep (SDK-551 to SDK-553 and the SDK-563 bug), Milestone 6 other formats (SDK-554 to SDK-556), Milestone 7 update rehearsal (SDK-557), the Milestone 3 leftovers SDK-574 and SDK-577, and SDK-571. The 18 open prototype and grilling children of SDK-470 add no new engineering work. The target stays the catalogued M45-release ARM64 build; Windows is deferred.

The cost that matters is writing and repairing one shared method, and porting the method set to a new build.

Sources: the Native and Atlas repos read directly; the full text of SDK-543, 553, 557, 563, 569, 571, 574 and 577; a digest of all 100 Atlas issues. Every repo finding below was verified in source.

## Findings

### How a method is written today

- A throwaway Python prototype in `.local/sdk-NNN/` with text disassembly dumps. Addresses are hand-checked and written into `discovery.md`. Then a Rust port, authored tests with hand-encoded hex words (`analysis_support.rs:6-67`), a parity file, a sweep, and counts.
- No tracked inspection tool. The primitives exist but are crate-private: `Text::direct_calls`, `branches_to`, `calls_into` (`binding/binary/declarations.rs:319-346`), `VerifiedAnalysis::symbol` (`binding/analysis.rs:52`), the string map (`binary/discovery.rs:307-320`). `lib.rs` exposes only the public answer types plus a hidden `internals` module holding `ObservationControl`, which `tests/consumer_boundary.rs` rejects in Atlas.
- **The image reader is build-bound.** `binary::discovery::read(bytes, layout: &SchedulerLayout)` (`discovery.rs:275-332`) reads the layout's address range, and its `fixups` step (`discovery.rs:65-125`) fails the whole read unless the chained-fixup header matches M45's values and format 6. Symbols, strings, fixups and the scheduler window are all read in that one function. Nothing in the crate reads an image without a target record.
- Analysis stops with bare reasons: `Unresolved(&'static str)` (`evaluate.rs:60`), `FieldGap { kind, reason }` (`fields/records.rs:114`), and `AnalysisError` prints `{self:?}`. Per-path `instructions` and `terminal` addresses exist internally (`fields/records.rs:86-97`) but are surfaced nowhere. Locating an obstruction means adding prints.
- Evaluator coverage is the repeated cause of repair rounds (`engine-commands.md:209-215, :243, :338-342`, `modifier-families.md:189`). SDK-563 is the sharpest case: 32 megastructure fields missed behind `ldrh …; br` jump tables, and five others found on the release build only because the beta compiler chose a table. SDK-543 names an indirect jump table in the Army reader; whether it is the same dispatch shape is not established.

### Decoder duplication (a fact, not a prerequisite)

- `decode_arm64` (`decode.rs:34-85`) caps at 4096 bytes, builds a new Capstone per call, and returns string operands. The chunking loop is repeated at about ten sites, and eight private text parsers re-parse operands. `families.rs:443-498` deliberately keeps a raw-word `adrp`/`add` fallback for functions that do not decode; that is a conservative second tier, not duplication, and must survive any consolidation.

### Parity tests and build identity

- `tests/static_questions.rs:823` hard-codes `tests/expected/m45/`. SDK-557 needs expected output for a second build.
- `OpenError::UnknownTarget` and `Ambiguous` carry nothing (`src/api.rs:6-19`). `BuildId` is opaque (`answer.rs:174-176`), so no public hash record exists to reuse.
- Per-build data is scattered: `targets/recipes.rs:62-111`; hard-coded unslid hook addresses and tokens in `groups.rs:7-45`; `installation.rs:39` uses `M45_DEFAULT_REGISTRIES` for every build. The last port was by hand (`targets.md:49-59`).

### Consumer seam

- Atlas pins Native by git rev (`atlas/Cargo.toml:7`). Every Native change needs a manual bump and a fetch from GitHub; there is no local override and no root workspace.
- Atlas hand-maps every `Operation` to a name (`atlas/src/extraction.rs:158-174`); `Operation` has no `name()`.
- `modifier_families(registry)` rebuilds its input per call (`performance.md:3`); 164 calls per snapshot.
- `examples/registry_field_sweep.rs:94` counts partial answers as `successful_queries`, so it cannot serve as a completeness measure.

### Live worker, CI, diagnostics, docs

- SDK-574: three observers share pause flags in `worker.py`, which caused two SDK-564 defects. SDK-571: a hang whose cause is unconfirmed.
- CI runs no parity or live tests and no `cargo doc`; no toolchain pin. The SDK-569 gate is not implemented; `tests/consumer_boundary.rs` has a `syn` scanner to reuse.
- The session work directory is removed on confirmed disposal; one failure's cause was lost that way (`registry-items.md:66-70`).
- Stale docs: `simplification.md:169,309,342` says the fake-worker test is missing, but it exists (`supervisor.rs:566`); `roadmap.md:40`; `retrieval.md:3` and `.claude/skills/milestone-review/SKILL.md:14` use the old checkout path; `architecture.md:80-89` omits four directories; `AGENTS.md:5` says "reservation recovery". `discovery.md` has no method index.
- `.local/sdk-559`, `sdk-560-*`, `sdk-561-*` hold instrumented source copies and raw logs that `performance.md:39` explicitly retains.

## Recommendations, in order

### 1. One inspection-and-diagnostics slice, driven by SDK-563 (first)

- **Split image inventory from registry discovery** in `src/binding/binary/`:
    - `inventory::read(bytes) -> Inventory`: identity (`binary::identify`), the selected slice, sections, symbols (demangled, plus indirect stubs), read-only strings, and `code_range` access. No `SchedulerLayout`, no target record, no fixups.
    - `fixups::read(slice, &file) -> Result<Fixups, FixupDiagnostic>`: the existing chained-fixup parser, moved. An unsupported header or format returns a diagnostic that names the form seen. It does not fail the inventory.
    - `discovery::read(bytes, layout)` becomes a composition of `Inventory`, `Fixups` (required there, as today), the verified layout's code window, and the vtable witnesses. Supported operations are unchanged and parity output stays byte-identical.
- **A hidden developer entry, `pdx_native::internals::inspect`.** Same status as the existing `internals::ObservationControl`: `#[doc(hidden)]`, not a consumer API, already rejected by the consumer boundary test. Thin wrappers only: open a raw image by path through `inventory::read` (never `Native::open`; no M45 addresses and no dummy layout are supplied for an unknown image); symbol search; bounded disassembly of a function by symbol or address with symbolized branches and inline strings; direct callers; string references. Pointer resolution (adrp/add targets through fixups, vtable slots) is enabled only when `fixups::read` succeeded; otherwise the output states that pointer resolution is unavailable and gives the fixup diagnostic. Unresolved indirect calls and uncertain function boundaries are labelled as such.
- **`examples/inspect.rs`** over that entry. Addresses appear in developer output only, never in public answer types.
- **Internal stop diagnostics.** Replace `Unresolved(&'static str)` with a struct carrying the instruction address, enclosing function, the unknown value or the exhausted bound. Carry it into `FieldGap` internally and print it through the inspector. Public `Gap` stays normalized. Success means a developer locates the obstruction from one run without adding prints.
- **SDK-563 as specified:** bounded jump-table dispatch in `fields/` (dispatch and control flow), authored `ldrb` and `ldrh` tests, a typed gap that names the reader and the table. Then check whether SDK-543's Army reader can reuse it; do not assume it.

### 2. Sweep report for method tickets (Native's share only)

- Extend `examples/registry_field_sweep.rs` (renamed `registry-field-sweep`): explicit complete, partial and failed totals per operation, per registry and per reader identity; failure shapes grouped by the internal stop diagnostic (function, instruction kind, bound), not only by `GapKind` and subject; a diff of normalized answers between two runs. This serves each method ticket's required inventory run. Atlas owns the SDK-553 run driver, rule composition, config comparison and coverage calculation. Operation completeness and claim coverage stay distinct.

### 3. Parity tests per build (SDK-557 prerequisite)

- `tests/expected/<build-name>/`, keyed by the target record. An optional generator writes a candidate tree for review as a diff; it is not verification.
- Anchors as symbol plus checked offset in the target record; the `groups.rs` addresses and `M45_DEFAULT_REGISTRIES` move there. `examples/rebind.rs`, over the inspection entry and a raw image path, lists candidate anchors on a new executable as moved, missing or unchanged. A candidate is confirmed only by checking the instructions and behavior. The SDK-557 cost record (shared-method work, adaptation, fixture changes, Atlas changes, human attention, agent effort) is kept separately; the tool does not supply it.

### 4. Consumer seam (small)

- `Operation::name(self) -> &'static str`, `impl Display`, `Operation::ALL`. Atlas deletes its match.
- Local development without a workspace: an untracked `.cargo/config.toml` in Atlas with `[patch."https://github.com/pdx-foundry/native.git"] pdx-native = { path = "../native" }`. The pinned rev stays and is validated by Atlas's build and contract tests.
- `Native::modifier_families_all()` or a cached modifier input, only after the 164-call cost is measured again.
- Identity on `UnknownTarget` is a separate public-contract decision. The inspector prints the identity of any image, which covers the SDK-557 need without that change.

### 5. Live worker (SDK-574, SDK-571)

- SDK-574 as written. Test the Python pause decision directly, in `tools/observation` beside the codec tests, with the fake-worker path (`supervisor.rs:566`) covering supervisor integration.
- SDK-571 stays an investigation: reproduce first; choose warnings or a display assertion only if the result shows they are feasible and needed.
- Keep the session work directory on any `Error`, not only on unconfirmed disposal.

### 6. Gate, CI, test loop

- SDK-569 as written, reusing the `syn` scanner; it runs in `cargo test`. It prevents per-registry shortcuts; it does not prove semantic correctness, so each ported reader still carries its prototype controls (SDK-543's 27, SDK-544's numeric controls) as authored tests and parity checks.
- `.cargo/config.toml` aliases `cargo parity` and `cargo live`; `cargo doc --no-deps` in CI; `rust-toolchain.toml`.
- A manual Mac workflow for parity is worthwhile after SDK-571 reports; it is not a prerequisite.

### 7. Staged decoder consolidation (follow-ups, each with an equivalence test)

- Step 1: move the 4096-byte chunking into `decode_arm64` and reuse one Capstone per `BoundAnalysis`. Check: byte-identical parity output.
- Step 2: typed operands, migrating one consumer at a time with authored equivalence tests. Keep the `families.rs` raw-word fallback unless a tested replacement covers non-decodable functions.
- Step 3: fold ad hoc trackers onto `Machine` only where semantics are shown equal. None of this blocks a Milestone 4 ticket.
- A small assembler helper for authored tests when step 2 starts.

### 8. Docs and hygiene

- A method index at the top of `discovery.md` and a `docs/native/method-authoring.md` recipe.
- Fix the stale docs. For `.local/sdk-559`, `sdk-560-*` and `sdk-561-*`: verify a retained copy of any unique source, measurements and dependencies, as the development policy and `performance.md:39` require, before removing anything; otherwise keep them.
- SDK-577 stays Atlas-side; Native needs no change.

## Verification

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (with the SDK-569 gate).
- Two new unit cases in `src/binding/binary/` on authored images (`tests/support/mod.rs` and `analysis_support.rs` already build minimal Mach-O files):
    - An uncatalogued authored ARM64 Mach-O is identified and one function disassembled through `internals::inspect` with no target record, while `Native::open` on the same file returns `OpenError::UnknownTarget`.
    - An image whose chained-fixup header is not format 6 still lists symbols and strings and disassembles, produces no resolved pointers, and reports the fixup diagnostic.
- `STELLARIS_PATH=... cargo test --release --test static_questions -- --ignored`: byte-identical after the inventory split; after SDK-563, `registry_fields("common/megastructures")` includes the 32 named fields, other registries may legitimately change, and the count of changed registries is recorded in `registry-fields.md`.
- `cargo run --release --example inspect -- --function 'CMegaStructureType::ReadMember'` shows both jump tables that SDK-563 describes.
- Atlas builds with the local patch and `tests/snapshot.rs` passes with recorded answers.