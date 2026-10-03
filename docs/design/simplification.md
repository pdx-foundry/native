# Decision: a simple engine API, scoped by compiler need

Status: approved by Jackson on 2026-09-19 and implemented on 2026-09-20. The compiler-need review
of 2026-10-02, agreed with Jackson and implemented the same day, narrowed the scope. The
[specification](../specs/native.md) holds the current API; the
[technical design](architecture.md) holds the layout. The earlier API sketch, work order and
review tables are in Git history.

## Purpose of Native

Native is a standard API to ask Stellaris questions, the same on each platform and game build.
It is not an evidence archive. A caller gets an answer, a statement of how complete the answer is,
and a small stamp that says which build and method gave it. To check an answer, run the question
again. Tests use small recorded answers when no game is available.

## Vision: scope by compiler need

Native gives Atlas the static engine facts that a PDXScript compiler and language service would
use to accept, reject, type or complete script, plus the smallest live check that shows a static
answer is true. Runtime values are out of scope. Proven facts that pass this test stay; new depth
is built only when the config replacement needs it.

- **Customer:** the Atlas developer. Atlas is the only consumer.
- **The test for a fact:** it changes what a compiler accepts, rejects, types or completes. "A
  `.cwt` rule cannot express it" is not a reason to cut; the product is the JSON snapshot.
- **Static facts only.** Evaluated values, game state and consumer meaning do not pass the test.
- **Keeping and building differ.** A proven fact that passes the test stays. New depth needs a
  config-replacement need or a low cost.
- **Later projects do not justify a feature now.** The typed PDXScript language consumes Atlas
  output only. The end-to-end test framework will need its own design.

## What stays

- **Exact build identification.** `open` hashes the executable and refuses an unknown build. There
  is no nearest-version fallback.
- **Target composition** (`binding`): target records, recipes, platform and machine leaves. A new
  build adds a record, not a copy of an adapter.
- **Independent process ownership.** The consumer-hosted supervisor owns the game and cleans up
  when the caller or the worker fails. The ordinary game profile is never touched.
- **Honest partial answers.** Complete, partial with typed gaps, or an error. An empty answer is
  never used for "could not look".
- **No native details in the public types.** No addresses, tokens, symbols or instructions.

### Kept because of the vision

A first pass proposed these as cuts under a "replace the `.cwt`" test. A compiler can use each
one, so they stay:

- Reference lookup `stage`, `key_match`, `empty_key` and `on_missing`: they decide if a bad
  reference is an error, a load-order error or a silent null.
- Numeric `width_bits`, `signedness`, `literal_syntax` and `accepted_range`: a type checker needs
  literal forms, overflow limits and fixed-point precision.
- `CommandGrammar.ordering`: "`else` must follow `if`" is a compiler diagnostic.
- Duration factor, combination rule and omitted count.
- `TargetArgument.stage`: keep the field. Build no more proof for it now; all 275 target arguments
  on M45-release are unresolved.
- `ModifierFamily::name_limit`: low cost; it supports a "generated name too long" diagnostic.
- The loaded-modifier explanation join (`declared`, `generated_by`, the unexplained count). It is
  the only live check of the static family answer, and Native's item keys depend on it.
- `Native::supports`: the check before launch.

## What goes

| Removed | Replaced by |
| --- | --- |
| Replay as a feature; the `native-evidence` package; `Engine::replay*`; descriptors and artifact hashes on results | Run the question again. Analysis methods move to `engine/analysis`. |
| Bundled qualification records, admission, promotion, withdrawal | A build is supported when it is in the catalogue. Its tests prove it. |
| `production`, `maintainer-tools`, `test-support` features; the `investigation` module; `build.rs` guards | One build. Experiments are ordinary examples or tests. |
| `capture.rs`, retention directories, startup and final snapshots | A temporary work directory, deleted on close. Kept only on failure, for debugging. |
| `CapabilityRequest`, `CapabilityBounds`, `CapabilityReport` | `native.supports(operation)` |
| Multi-gigabyte captures as test input | Small tracked test inputs and recorded answers |
| `tools/check-*.py` (11 files), qualification pages in `docs/native/` | Cargo tests. Knowledge pages stay. |
| Cut on 2026-10-02: the world route, fixture registration entries, category reads and runtime outcomes, the public item query and registry selection, the duration consumer, scoped operand selection, the numeric clamp, `FieldDefault`, defaults on recorded answer properties, public readiness and cancel, configurable idle and fixture deadlines | Static compiler facts and the parser, storage and diagnostic checks of the fixture method. The removed code is in Git at `d8f9d8a`; the knowledge pages keep its findings. |

## Decisions of 2026-10-02

These decisions came with the review and still govern the open work. They keep the review's
numbers, which the Linear amendments cite; decisions 1 and 6 to 8 are in the specification or in
Linear.

- **Decision 2, modifier categories.** The category names are engine facts; the `supported_scopes`
  lists of `modifier_categories.cwt` are written by hand and have errors. A static probe on
  M451-hotfix (`.local/modifier-node-probe/REPORT.md`) found a fixed graph of 35 modifier nodes with
  constant category masks, except the ship, and restricted parse-time containers. SDK-547 builds the
  node table and the container masks, and keeps a typed gap for where a category takes effect and
  for the three categories with no node. Atlas derives candidate scopes through a stated
  node-to-scope mapping; a disagreement with the config is a case to review. The gap text is: "Where
  a modifier takes effect is decided at application by each receiver's category mask and the include
  and exclude masks of each propagation edge. The engine reports no diagnostic for a filtered entry.
  These masks are not established; supported scopes stay outside this method." The mask evidence is
  from the M45-observe beta capture; match it to the supported build before it is reused.
- **Decision 3, `root`, `from` and `prev` are in scope.** A language service needs them for
  completion and typing. SDK-549 keeps `this`, with a read-time fixture check. SDK-677 establishes
  `root`, `from` and `prev` for registry field blocks from their evaluation call sites. SDK-608
  blocks SDK-677, because both need the self-link rule, and SDK-677 blocks SDK-600.
- **Decision 4, entry scopes with no live check.** SDK-608 and SDK-677 state the self-link rule as
  an assumption. The checks are: 10 to 15 hand-read call sites across on_actions, game rules and
  field blocks; every on_action answer compared with the scope comments in the vanilla on_action
  files; game rules and field blocks compared with the config's `replace_scopes`. The comments and
  the config are test expectations only; Native never reads them to make an answer. Agreement counts
  go on the knowledge page. A disagreement is a typed gap with a hand-read of its call site. An
  answer with no independent source keeps a gap that says so. Keep [ready-world
  observations](../native/ready-world.md) complete, so that the world pause can be restored if the
  rule fails.
- **Decision 5, repeat behavior, not occurrence counts.** The engine fact is `RepeatBehavior`
  (`Replace`, `Accumulate`, `Unknown`), not a maximum count. A compiler warns on a repeat of a
  `Replace` field and allows an `Accumulate` field. Atlas credits a config maximum of 1 on `Replace`
  and an unbounded maximum on `Accumulate`; it publishes no engine limit. A field is required only
  where validation reports its absence.
- **Decision 9, one game per host.** Keep the host-wide lock and its one-time setup. A per-account
  lock loses only the case of two accounts that launch in the same instant. Revisit only if a second
  account becomes a real need.
