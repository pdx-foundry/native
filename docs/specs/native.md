# PDX Native specification

Rewritten 2026-09-20 and narrowed 2026-10-02 by the
[simplification decision](../design/simplification.md).

## Problem and solution

Atlas asks the Stellaris engine questions to derive scripting rules. The answers depend on
executable addresses, memory layouts, compiler patterns, launch workarounds and process
control; if Atlas depended on them, each game update would be an Atlas port. Native is a Rust
library with one API, the same on each platform and build, that owns all of that knowledge.
Its goal is parity with the cwtools config, plus additional static compiler facts that pass the
compiler-need test and the overbuild check ([vision](../design/simplification.md#vision)). Atlas
is the first consumer and, today, the only one.

A caller gets an answer, how complete it is, and a stamp that says which build and method gave it;
to check an answer, run the question again. **Atlas has no platform or game-build knowledge**: it
does not select adapters, decode native data, compare versions or choose a platform path, and it
decides what an answer establishes about a rule.

## Implementation Decisions

### 1. Authority and module boundaries

Dependencies run from Atlas extraction into Native's public API. Native does not import Atlas's
rule model. Atlas does not import adapters, analysis helpers, or process control.
Atlas's caller is checked by `tests/consumer_boundary.rs` against this public boundary.

Native establishes what was read or observed. Atlas decides what that establishes about a rule.

### 2. Public API


| Operation | Availability | Atlas supplies | Native returns |
| --- | --- | --- | --- |
| `Native::open` | Implemented | Installation location | A pinned installation, or a precise `OpenError` |
| `supports` | Implemented | An `Operation` | `Support::Supported`, or `Support::Unsupported` with a reason |
| `registries`, `registry_fields` | Implemented | A registry name for fields | Registries; fields with reader identity, broad kind, block family, conditional read alternatives and reference lookups (target registry by content directory, stage, key match, missing-key result), and, for each root trigger and effect block, the contexts of `this`, `root`, the `from` chain and the `prev` chain that the engine's direct evaluation calls supply (a self-link stays `SelfLink`), with explicit unknowns |
| `start_game` | Implemented | Supervisor command, startup budget and optional fixture or loaded-modifier request | A `Game` paused at the internal boundary for its questions |
| `Game::loaded_modifiers` | Implemented for M451-hotfix | `GameOptions::loaded_modifiers` before launch | The modifier table after all content loads, read where the engine documents its modifiers: each name with its loaded category tags, whether the executable declares it, and each `modifier_families` family and loaded item that gives it; the loaded keys of each family registry; the loaded content. Unexplained names and unjoined generation sites are gaps. No config or log file is read. |
| `Game::close` | Implemented | — | A disposal result; dropping the session or an active call also starts cleanup |
| `from_recorded_answers`, `record_answers_to` | Implemented | A directory | Recorded answers in place of a game; a record of real questions |
| `declarations` | Implemented for effects and triggers | A declaration kind | Engine name, description, usage, and declared scopes from every registration call and tail call in executable text, including registry helper constructors and names composed at run time through up to two callers; each chain of callers is one declaration. Registrations that cannot be followed, unreadable documentation, and scope getters that cannot be followed make the answer partial. Target arguments and their accepted scopes are in `command_grammar`. |
| `command_grammar` | Implemented for effects and triggers on M451-hotfix | A declaration kind and registered command name | The accepted forms (value alternatives with their reader kind, a block, or both); target arguments with the scope types they accept and the stage that checks them; concrete shared reader identity, child families, fixed keys with their reference lookups (including the receiver initializer's lookup of a stored key), nested members, numeric child grammar, conditional reader ordering and duration key groups. The answer is `Complete` only when every property is established at every depth. Every property keeps unresolved evidence explicit; this is not runtime meaning. A missing registered command gives `UnknownCommand`. |
| `modifiers`, `modifier_categories` | Implemented | — | Built-in modifiers with their declared category tags, from every direct definition call; category names from the engine's category switch, each with the single categories that it covers (`Ships` covers six). Generated modifier families are gaps. Tags are intended-use tags, not application contexts. |
| `modifier_category_keys` | Implemented for M451-hotfix | — | Each value that script writes for a modifier category (`category` in `common/scripted_modifiers`, `modifier_category` in `common/economic_categories`) with the single categories that the engine parses it as; `none` has none. A value that the engine reads as no category and reports as invalid is not listed. A category that a later step adds, such as AI Economy for generated economic-category modifiers, is an `OutsideMethod` gap. |
| `modifier_nodes` | Implemented for M451-hotfix | — | The engine's modifier node graph: each node's source nodes, the engine types that construct it (such as `CShip`), and the categories that it keeps, named as the engine names a mask. A node whose calculation can store another mask gives each mask (the ship). Where a modifier takes effect is a gap, and the answer has no supported scopes; a category that only all-bits masks keep is a gap; a node with no followed construction (node 0) is a gap. |
| `modifier_families` | Implemented for database generators, post-read code and shared helpers | A registry name | Name templates that the registry's code registers for each item, with the item-key position, category tags, whether every item generates the family, and a name-length limit. Code that generates modifiers and is not joined to a registry is a gap, with its reason. |
| `derived_names` | Implemented for M451-hotfix | A registry name | Names that the registry's own `const` methods and post-read initialization compose from the item key or a string field and then check or look up: the name's parts (literal, item key, field path), what it is looked up in (localisation, sprites, files), the stage (when used, or owner initialization), what a missing name gives (shows the key, silent, a diagnostic, a fallback to another derived name, or unresolved) and the field condition. Names that interface code composes and the run-time choice behind a selection are outside it; unfollowed paths, unresolved name parts and unresolved conditions are gaps. |
| `script_expansions` | Implemented for M451-hotfix | — | For inline scripts, scripted effects, scripted triggers, script values and scripted variables: where script can use each (registry roots, command families, object blocks, trigger references, scoped operands, statements), where its names are defined, when the engine expands a use (`Lex`, `Read` or `Compile`), its call forms and parameter forms, what an absent parameter yields, and load-time checks. Call and parameter forms are a stated per-build rule (a manual exception), checked by fixtures. Unjoined readers, the variable lookup order, cycles and forward definitions are gaps. Scripted modifiers are modifier names, in `modifier_families`. |
| `scopes`, `scope_links` | Implemented | — | Scope types with the keywords that the engine maps to each, and keywords that match several types (`carrier`); documented links and the link prefixes that take data, each with declared input and output scopes |
| `localization_declarations` | Implemented | — | Localization contexts from the engine's text tables, each context's commands and links, each link's output context, and the scope types that select each context; a missing join keeps its commands; links that the method cannot follow are gaps |
| `on_actions` | Implemented | — | On_action names that engine call sites fire, from every direct call to the firing functions, the deferred command and the checked forwarders, with the cached pulse lists; for each name, each distinct context of `this`, `root`, the `from` chain and the `prev` chain that a followed call site supplies. A self-link stays `SelfLink`; its documentation states how script reads it, as an assumption. Names that script content fires, names built at run time and call sites that the method cannot follow are gaps. |
| `game_rules` | Implemented | — | Game rules from the engine's rule declarations, scripted and weighted, with each distinct context that the rule set's call sites supply. A declared rule with no followed call site is a gap. |
| `dynamic_names` | Implemented for integer flags on M451-hotfix | — | One namespace for each flag store that commands reach: its owner (a scope type, or one global store), the effects and triggers that define, remove and read names in it, and whether they accept `name@target`. Two commands share a namespace only when both reach the same store. Saved event targets and variables are outside it; commands and stores that the method cannot follow are gaps. |
| `defines` | Implemented for M451-hotfix | — | Define namespace, name and engine read type from compiled read helpers; unresolved helpers are gaps. No shipped define or config file is read. |
| `Game::check_script` | M451-hotfix at the loaded-modifier pause | Trigger or effect text and a scope ID from `Native::scopes` | Whether reading returned, top-level child count, current diagnostics with stage and optional line, prior-check diagnostics, unjoined messages, capture bounds, and stored duration counts of top-level children. No trigger evaluation or effect execution. |
| `Game::observe_fixture`: field outcomes | M451-hotfix; initial file load, optionally through bounded deferred validation, for a registry with verified boundaries | At most 32 named definition and field questions in one bounded relative text file | Separate parser entry/return occurrences, source-correlated diagnostics and typed string, integer or fixed-point storage where bound; other dimensions report unavailable. Lost observations cannot establish acceptance. |

Rules for the API:

- **Names prefer clarity to brevity.** A method name says its subject (`registry_fields`).
- **A registry is named by a string** in every operation: the full content directory
  (`common/traditions`, `map/galaxy`). A discovered candidate with no established name is a
  gap, not a registry.
- **No native details in public types.** No addresses, offsets, tokens, symbols, instructions,
  debugger commands, launch switches, or timing workarounds appear in a request or a result.
  Missing behavior needs a Native extension or an explicit gap. There is no raw-native escape hatch.
- **The build is fixed at `open`.** Native checks that the executable is unchanged before each
  operation. A caller cannot force an adapter or ask for the nearest supported version.
- Native selects the observed registries, including the fixture registry. Loaded item keys serve
  the modifier-family check; a language service reads user item names from files. SDK-552 owns
  file selection and duplicate rules, alongside definition-name rules (`skip_root_key`, `name_field`).
- `check_script` stays public as the smallest grammar control. New features require an accepted
  Atlas corpus comparison or a ticket naming a specific control.

### 3. Answers

```rust
pub struct Answer<T> {
    pub value: T,
    pub completeness: Completeness,   // Complete | Partial
    pub gaps: Vec<Gap>,               // typed; OutsideMethod can accompany Complete
    pub source: Source,               // build id, Native version, method name, basis
}
pub enum Basis { Declared, StaticAnalysis, LiveObservation, Recorded }
```

`Gap.subject` is `Option<GapSubject>`, not a free-text name. The subject kind identifies a
registry, field, named answer item, localization context, localization link, scope type, or
fixture file. Context and scope subjects include `LocalizationContextId` or `ScopeId` and a
human-readable name; names alone do not identify them. A gap without an identifiable subject
has `None`. Recorded JSON writes named subjects as objects with `kind` and `name`, plus `id`
for contexts and scope types.

- `Complete` means the stated search or window completed. It is not complete game knowledge.
  `OutsideMethod` describes an explicit boundary; other gaps make the bounded answer partial.
- An empty `Complete` answer needs a completed search that found nothing. An access failure, a
  missing hook, or a lost record gives `Partial` with a gap, or an `Error`.
- An unsupported operation does not mean that the game forbids a construct.
- `Basis` keeps a declaration, a traced static relationship, and an observed behavior distinct.
- Parsing, parser storage and engine validation are separate values. Runtime outcomes and
  scope availability are out of scope. A candidate reference class does not establish lookup semantics.
- Two fields that use one shared reader report the same reader identity.
- `Reader.numeric` supplies independently known numeric representation, width, signedness, scale,
  partial literal forms and accepted range. `Known(None)` establishes a
  nonnumeric reader; unresolved or partial properties must not be completed from storage limits
  or finite fixture observations. Caller post-processing is outside the shared conversion.
  `accepted_range` means faithful storage; an endpoint is `Known` only under the rule in
  [numeric conversion](../native/numeric-conversion.md#faithful-storage-and-endpoints), and a range
  with one established endpoint is `Partial`. A known range does not close `numeric-overflow`.
- `ReaderKind::ScopedNumeric` identifies a shared reader whose destination can store an integer
  or fixed-point literal and scoped references. `Reader.numeric` describes its concrete literal
  storage. `Reader.scoped_operand` reports partial routing forms. These facts do not claim a
  successful lookup or an evaluated number.
- `Field.read_scope` gives the read-time `this` scope alternatives for a block. `Types` is a set
  from one reader path; multiple alternatives retain different read-time entry paths. `Enclosing`
  means equality with the parent's scope, never `Any`. A known empty list means the field does
  not read a block. Zero masks and unknown arguments remain unresolved. This is separate from
  evaluation `entry_contexts` and does not prove runtime availability.
- Atlas uses `read_scope` for a registry block's `replace_scopes.this` comparison, and
  `child_scopes` or a named child's `read_scope` for command `push_scope`. It uses evaluation
  `entry_contexts` for `root`, `from` and `prev` under the self-link rule. If read-time and
  evaluation `this` differ, retain both and report the difference; do not merge them or replace
  the read-time answer. An unresolved read-time scope stays unresolved even when evaluation
  `this` is known. Config expectations are validation inputs only, never inputs to Native.
- `CommandGrammar.child_scopes` associates read-time scope alternatives with each dispatched
  command family. Named keys keep their own `Field.read_scope`; for example, an iterator's
  `limit` can differ from its other child keys. Scope IDs join to `scopes` and matching
  `scope_links` outputs without interpreting display names.
- `CommandGrammar.durations` groups child keys that set one duration count by their reader code.
  Each group gives the keys, factors, combination rule (`ScaledAtRead` or `SharedFactor`), and
  omitted count. Consumers, expiry dates and update frequency are outside the API.
- All recorded answer properties must be present. Missing properties are a malformed recording,
  never a `Complete` answer with silently unresolved properties. Reads validate the build identity,
  not the method revision.
- The build id in `Source` is opaque to Atlas. Atlas may keep it and compare it for equality.
- Constructor-bound root modifier fields expose `FieldMembers::ModifierBlock`: fixed keys with
  reader kinds and numeric or static-modifier-reference entry forms. Each property may be partial
  or unresolved. Numeric entries share the ordinary conversion facts; scripted modifier names
  come from the registry's existing `modifier_families` answer. String reads do not establish
  localisation-key existence. Runtime effects, repeated-block behavior, deferred completion and
  nested modifier fields remain outside this grammar. See [modifier blocks](../native/modifier-blocks.md).
- Constructor-bound root weight fields have family `Weight` and expose `FieldMembers::WeightBlock`:
  the bare-value reader, fixed keys, operation keys with their operand readers, whether a further
  operation accumulates or replaces, and whether other keys are rejected or read as trigger
  conditions. Nested `modifier`, `scaled_modifier` and `complex_trigger_modifier` entries carry
  their own `WeightBlock`. A key or condition read in the block's own stored scope reports
  `Enclosing`; the field's `read_scope` names that scope. When the readers of a key differ but all
  read one value kind, the key reports that kind with no reader identity. Weight evaluation and
  operation semantics remain outside this grammar. See [weight blocks](../native/weight-blocks.md).
- Root fields whose collected objects have a triggered modifier clause reader have family
  `TriggeredModifier` and expose `FieldMembers::TriggeredModifier`: the clause's own fixed keys,
  and `other_keys`, the modifier block that reads every key the clause does not name. A `modifier`
  key carries `FieldMembers::ModifierBlock`; it and `other_keys` name the reader identity of the
  shared modifier grammar. `Unresolved` other keys mean that their disposition is unknown, not that
  the engine rejects them. Condition timing, the multiplier's effect and where the modifier takes
  effect remain outside this grammar. See [triggered modifiers](../native/triggered-modifiers.md).
- `Field.accepted_categories` gives the single categories of modifier entries that a modifier
  container field accepts at parse time; for an entry with none of them the engine logs `Modifier
  has entry not allowed by category` and keeps the entry. `Listed` comes only from positive
  evidence (a category constructor's argument, a default construction, or the default word that an
  owner stores inline); an unknown argument, an unreached construction, disagreeing paths or
  destinations and a missing join are `Unresolved` with a gap, never every category. A key of a
  triggered modifier clause that reads into the clause's own container is `Enclosing`. A field with
  an established non-modifier reader is `NotApplicable`; one whose reader family is unknown is
  `Unresolved`. Atlas expands an entry's `category_tags` through `modifier_categories` and accepts
  the entry when the sets meet. Where an accepted entry takes effect stays outside this answer.
  See [modifier masks](../native/modifier-masks.md#container-masks).
- `DerivedName.name` uses `NamePart`; a `Field` part is a string field's path from the registry's
  definition, such as `["tradition_swap", "name"]`. A fixed key with no key or field part is not a
  derived name. `on_missing: ShowsKey` comes from the lookup function's stated rule when no check of
  the name comes first; [derived names](../native/derived-names.md) records the rule's checks.
  `Fallback` names the derived name that the same check and lookup use when the name is missing.
  A name with a field part has an `Unresolved` miss behavior, with a gap: the method plants one
  state for every string field and does not explore a miss that depends on another field.
  A condition is never `Always` for a name with a field part: it keeps an `Unresolved` term and
  `FieldZero { zero: false }` for each field part, with a gap. A flag term comes only from runs
  that wrote the flag. The `Unresolved` term beside a flag term is the run-time choice of the
  selected object, which the `OutsideMethod` gap states.
- `ReaderKind::Keyword` identifies a value from a fixed set of engine names that the reader stores
  as the engine's own value, such as `calc` or `mode` in a weight modifier. Its `numeric` is
  `Known(None)`. `Field.domain` gives the accepted names when it is established; until then a
  `ReaderSemantics` gap says the domain is unknown.
- `ReferenceTarget::Triggers` is the collection of trigger commands that `declarations` lists for
  `DeclarationKind::Trigger`. A lookup in it can name its other facts as unresolved; a gap then says
  what a name that is no trigger command yields.
- Repeated modifier names combine all registrations. Unresolved or conflicting category tags
  remain `DeclaredTags::Unresolved` with a gap; an earlier known registration cannot hide them.
- `ScriptExpansion.stage` says whether a tool checks a use's written text (`Lex` and `Read` expand
  it as the file is read) or the generated text (`Compile` expands it after all content loads). The
  stated forms of the [script expansion](../native/script-expansion.md#stated-forms-recorded-manual-exception)
  page are a recorded manual exception: claim, conditions, obstacle and removal route are there,
  and each form has a fixture row.
- Atlas's published gaps carry a reason and owner category. Repair-ticket mappings live in docs,
  so changing the work plan does not change the published engine knowledge (agreed G1).

Question, session, and recorded-answer failures use one `Error` type. Opening an installation uses
`OpenError`, because no `Native` exists yet. `Answer<T>` and `Error` are serializable.

### 4. Game lifecycle and isolation

Native is a library. The consumer supplies a dedicated supervisor executable that calls
`supervisor::serve`. The supervisor owns the game independently of the caller and of the
observation worker.

- Native prepares an isolated profile. It never changes the ordinary profile and never kills a
  process that it does not own.
- Hooks must be active before the engine phase that they observe, with an ordering witness. A
  fixed delay is not a witness. If activation is not established, the answer is an error or
  partial; it is never complete.
- `close` returns disposal as confirmed, unconfirmed, or not applicable. Worker failure, caller
  loss, timeout, cancellation, and partial launch all end with a bounded cleanup attempt.
  Failed final cleanup returns `Error::Cleanup` with the witnessed disposal and retains the work
  directory. A report-write failure is separate from whether the game was reaped.
- One Native-owned game runs on a host at a time. A conflicting instance is refused with a reason.
  An OS lock excludes concurrent Native sessions; a process inventory checks for other Stellaris
  instances. Old session files do not block a new launch after these checks pass.
- A `Game` uses a temporary work directory. `close` deletes it only after a clean, confirmed
  session that the caller ended with no read error; otherwise it is kept. The error from
  `start_game` or `close` names the kept directory, and `Game::work_directory` gives it after a
  read error. A failed deletion is a cleanup error with confirmed disposal; a later `close` tries
  it again.
- Startup is configurable from 1 to 180 seconds. The idle timeout is 180 seconds. Readiness and
  cancellation stay internal.
- No blind retry of an operation whose completion is uncertain.

**Agreed G2.** Native selects the registries a session observes and always includes the fixture
registry. The selection is validated against the bound build's discovery, without a fixed count;
duplicate and unknown selections are invalid. Build-specific counts are test expectations.

### 5. Shared native methods

Decoding, value provenance, bounded control-flow analysis, owner joins, and reader patterns stay
inside Native. Do not build a handwritten answer table for each command or field.

An unfamiliar instruction shape, unresolved callee, clobbered value, or unproved owner narrows or
stops an answer and becomes a gap. Engine-only discovery runs without config or a field list. The
[development policy](../development-policy.md#keep-engine-knowledge-in-its-home) states where each
engine fact lives and how method transfer is measured. A manual exception records its claim,
conditions, obstacle, and removal route; it is never presented as automatic extraction.

### 6. Supported builds and maintenance

A build is supported when its exact executable identity is in the target catalogue and its tests
pass; there are no separate qualification records, and a new patch does not inherit support.
[Targets](../native/targets.md) lists the catalogued build: one Apple Silicon full release, 4.5.1.
Native keeps full-release targets only, because Steam offers old full releases for download but not
old open betas. Native supports one release at a time. Windows x64 is
deferred: one platform is enough for platform-independent snapshots, and target composition keeps
platform knowledge in its own leaves.

The update rehearsal freezes Atlas extraction logic for a second distinct Apple Silicon executable;
routine port work stays in Native. Measure tooling, routine updates and exceptional repairs
separately. No numeric maintenance guarantee is accepted.

### 7. Recorded answers and provenance

Recorded answers are JSON files of `Result<Answer<T>, Error>`. `record_answers_to` writes them
during a real run. `from_recorded_answers` serves them for static and live questions and starts no
process. A question with no recorded answer returns `Error::NotRecorded`. Each recorded answer
carries `Basis::Recorded`. Failure cases can be written by hand. Recorded `supports` checks
whether the directory contains an answer for the operation; it does not claim support for
operations with no recording.

Each directory has a `build.json` containing the original serialized `BuildId` (a JSON string).
Opening a recorded directory returns `Result<Native, Error>` and requires valid build metadata.
`Native::build()` and all successful answers use that original identity. Reads and recording
into an existing directory reject a different build. Errors can be recorded without a successful
answer and still keep the build identity. Each write uses a temporary name of its own and is then
renamed, so an interrupted or concurrent recording leaves no partial file. One recorder writes to a directory at
a time.

Atlas claims keep provenance through `Source`; they do not reference retained captures. Atlas pins
an exact Native commit.

## Testing Decisions

Test behavior through the public API. Internal tests are justified for unsafe decoding and for
supervision failures that the public API cannot cause safely.

1. **Boundary:** the Atlas caller has no platform or build branches, native constants or adapter
   imports (`tests/consumer_boundary.rs`).
2. **Build check:** a changed executable, an unknown build and an unsupported operation each give
   the declared error and execute nothing (`tests/installation.rs`).
3. **Answer integrity:** wrong owners, clobbered values, unresolved calls and missing joins give
   typed gaps; no partial answer becomes complete.
4. **Static methods:** authored inputs test method logic; ignored parity tests read the exact
   executable and compare with small tracked expected answers (`tests/static_questions.rs`).
5. **Live operations:** ignored by default, run with `STELLARIS_PATH` (`tests/live.rs`): normal,
   missing-hook, dropped-record, worker-loss, timeout and cancel cases; the ordinary profile and
   unrelated processes stay unchanged.
6. **Supervisor without a game:** unit tests and a fake-worker session test cover reservations,
   worker cleanup, pause witnesses, cancellation, the final report and reaping.
7. **Recorded answers:** a recorded run gives the same answers apart from `Basis`; a missing record
   gives `NotRecorded`; no process starts (`tests/recorded_answers.rs`).
8. **Shared-method transfer:** the population run and the locality gate of the
   [development policy](../development-policy.md#measuring-method-transfer).
9. **Atlas integration:** a tradition field passes through Native answers, Atlas claims, a
   schema-valid snapshot and an end-to-end offline test that reads the snapshot, including invalid
   input and an absent answer.
10. **Update portability:** the unchanged Atlas flow runs on the next patch after 4.5.1; the live
    run credits no recorded answer.

### Milestone 4 shared-reader acceptance

The [roadmap](../roadmap.md#milestone-4-acceptance) owns the Milestone 4
acceptance contract: the SDK-600 council agenda test, the method fixture criteria, method
transfer and Atlas integration.

## Out of Scope

- Runtime values, scope availability, weight evaluation and modifier application.
- Atlas rule extraction, snapshot assembly, or consumer diagnostic policy inside Native.
- Replay of retained captures, evidence archives, and qualification records.
- Game or mod catalogues, and config-derived fallback answers.
- A general decompiler, an arbitrary native-code API, or a promise that every property can be
  extracted automatically.
- The author-facing real-game testing framework.
- Other games, Windows, Linux, and Intel Mac in the first release.
- Concurrent games, supervisor-loss recovery, and guaranteed invisible launch.
- Installation discovery without a location. It does not block config coverage.

## Paused script checks

Start with `GameOptions::loaded_modifiers`, then call `Game::check_script` serially; its
documentation and `src/script.rs` give the bounds. Native adds trailing whitespace so the final
token can be read.

Results are observations against the session's loaded content and retained command databases.
Each message retains the raw signed engine log level; Native neither filters levels nor assigns
severity. Foreign messages without line numbers do not reduce the current check's completeness.
Each request has a unique source identity. Validation can report errors from earlier checks;
these are separate `foreign` diagnostics. Source-free or ambiguous messages stay `unjoined`.
Missing hooks, unreadable messages, missing current-source lines, unjoined messages or reached bounds
make the answer partial. Repeated messages remain separate occurrences. A complete quiet
answer records capture coverage; it does not establish script acceptance.

A check temporarily replaces the idle deadline. A failed call, timeout, worker loss, register
mismatch or cancellation ends the session; call `close` to observe disposal. Engine allocations and
deferred command objects remain until the session ends: checks are not isolated fresh games.

`stored_durations` lists, for each top-level child whose receiver has a static `Duration` group,
the stored count after reading and before validation. A scaled-at-read group gives its `Integer`
count; a shared-factor group gives its `ScopedNumeric` operand and the signed 32-bit factor.
Children are classified by receiver; command names in the text only nominate receivers, at most
64 per check. `Known` means every child was classified, its receiver's static duration list is
`Known`, and every group was read; otherwise the property is `Partial` with an
`IncompleteObservation` gap. Recordings without this property are rejected.
Stored values are parser storage, not evaluated or executed durations.

Checks do not alter static answers, resolve silent properties, check registry fields, evaluate
triggers or execute effects.
