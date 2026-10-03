# Atlas caller migration

The SDK-519 prototype caller, first at `pdx-atlas/prototypes/native-registry`, moved into the
Atlas crate at `/Users/jackson/Developer/pdx-foundry/atlas`, which depends on a pinned Native
commit. Its previous source and freeze are preserved privately;
see [preservation](../native/preservation.md). Atlas's published claim ledger is unchanged.

## API replacements

| Earlier caller | Current caller |
| --- | --- |
| `Native::open(OpenRequest { installation_hint })` | `Native::open(path)` |
| `get_registry(name)` | `registries()` for discovery; `registry_fields(name)` for fields |
| `traditions`, `tradition_categories` | `common/traditions`, `common/tradition_categories` |
| `with_supervisor(command, options)` then `start_game()` | `start_game(GameOptions::new(command))` |
| Caller-supplied retention directory | Native-owned temporary work directory |
| `get_registry_items(name)` and `RegistryResult` | Item names come from user files; modifier join keys remain in `LoadedModifiers.registry_items` |
| A fixed live registry list | Native selects registries and includes the fixture registry |
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

`Native::declarations(DeclarationKind::{Effect, Trigger})` supplies declared command text and scopes for Atlas's command existence, declared scopes, and documentation routes.

Atlas retains `value`, `completeness`, typed `gaps` and `Source`. The source contains the exact
build, Native version, method and basis. This is the accepted amendment to SDK-473, recorded in
[the Atlas map](https://linear.app/unnamed-system/issue/SDK-470/specify-pdx-atlas-and-its-engine-derived-rule-database).
Each named gap has a `GapSubject` kind. Atlas attaches it by that kind and uses the context or
scope ID where supplied; it does not infer the kind from a name. Recorded answers use objects
such as `{"kind":"localization_link","name":"Planet"}` in `subject`, or `null` for a
gap without an identifiable subject. Context and scope subjects also carry `id`.
Claims no longer require Native capture hashes or artifact references. Re-run the question to check it.

A partial list keeps its established items; a missing item in that list proves no absence.
`Basis::Recorded` stays recorded after Atlas processing and proves nothing about the current game.
Recorded directories require `build.json` with the original serialized `BuildId`, such as
`"authored"` for a hand-written test. `Native::build()` retains that identity, and answers from
another build are refused. The recorder writes this metadata even when the question fails.
Disposal belongs to the session, separately from the answers. Item names establish no fixture
execution, field storage, validation, schema completeness or rule coverage.

## Field answer migration (SDK-541 / SDK-597)

`registry_fields` still returns `Answer<Vec<Field>>`, with method `registry-fields/v12`.
The former `Field.conditional` Boolean is replaced by paired `read` alternatives. Each
alternative retains its `condition` and `outcome` (`Read`, `Rejected`, or `Unresolved`).
Never combine the condition from one alternative with another's reader or shape.

Keep `shape.value`, `shape.repeat`, `members`, `domain`, and `uses` in snapshots.
Replacement and accumulation describe storage, not allowed occurrence counts. `Unknown`
is an unanswered fact. `members: Fields` can still have gaps; an empty child inventory does
not prove an empty grammar.

`uses` describes local selection of stored data in an engine method, separately from parser
acceptance. Its condition can be `All([Unresolved, FieldZero { path, zero }])`; retain both
terms and the registry-relative nested path. A use ID identifies the containing method on
this build, so independent selections may share it. An empty use list does not prove absence
of runtime conditions. SDK-597 owns this snapshot migration and SDK-546 consumes the naming
relationships. SDK-627 owns domains and unknown repeat behavior. SDK-628 is cancelled; runtime selection is
out of scope. Atlas credits maximum 1 for Replace and an unbounded maximum for Accumulate,
without publishing either as an engine limit. Required fields need validation evidence.

## Block and command grammar migration (SDK-542 / SDK-597 / SDK-625)

Keep `Reader.family` on both field summaries and read alternatives. `Unknown` is unresolved;
`NotApplicable` belongs to a scalar reader. Concrete persistent receivers can refine reader IDs.
Nested collection IDs also now include their concrete member reader, so their IDs change from v5.
Unjoined generic persistent destinations have no ID. IDs remain opaque within a build. Do not convert a conditional family into an unconditional claim.

`Native::command_grammar(kind, name)` returns independent `GrammarProperty` values. Preserve
partial fixed keys, nested numeric-child grammar, ordering conditions, and unresolved siblings.
A routing rule does not impose runtime order. `limit` is a child key, not a registered command.
`forms` lists each accepted form: `CommandForm::Value` with its reader kind and reference, and
`CommandForm::Block`. A value alternative is listed only when its whole stage chain accepts it.
`targets` lists each target argument (the command's own value or a named key path) with its
accepted scope types and `TargetCheckStage`. A known empty list means the command takes no target.
All answer properties must be present; move the pin and record again on M451-hotfix. Use
`completeness == Complete` to decide whether a grammar can drive a validation rule.

For fixture conclusions, request `.with_parsing()` on each question and `.through_validation()`
on the field-outcome request when deferred errors matter. Require witnessed complete parsing and
complete relevant diagnostic coverage for acceptance. Keep parsing, storage and diagnostics separate; runtime claims are out of scope. Recorded
answers give no new live coverage credit. The [method contract](../native/command-grammar.md#consumer-boundary)
names the SDK-597, SDK-625 and SDK-600 responsibilities and remaining gates.

## Current caller checks

Atlas owns its extraction fixtures and offline snapshot tests. Its `snapshot` command records
current answers on M451-hotfix. Remove `with_runtime`, the unused `category_reads` session and the
`RegistryItems` support filter when moving the Native pin. Follow Atlas's README for its snapshot,
ledger and coverage checks. SDK-608 owns entry-scope gaps. SDK-626 creates a failure-shape ticket
only when a config claim depends on a fact that changes compiler acceptance, rejection, typing
or completion; other shapes are marked out of scope under the 2026-10-02 vision.
