# Simplification review: scope by compiler need

Status: review of 2026-10-02 at commit `d8f9d8a`. The vision, the nine cuts, the ticket changes
and the nine decisions are agreed with Jackson (2026-10-02). No code, ticket or specification was
changed by the review itself; the section "Work to do" lists the changes that follow. This
document follows the [simplification decision](simplification.md) and does not replace it.

## Vision

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

This vision supersedes the runtime-meaning promises in the [specification](../specs/native.md):
user story 8 ("and runtime outcomes"), the section 6 row on scope availability, and the fixture
criteria named under "Ticket changes".

## Evidence limit

Atlas pins Native at `c33a3fc`, 43 commits behind `d8f9d8a`. Atlas has no caller for
`command_grammar`, `dynamic_names`, `check_script`, `observe_world` or the numeric properties.
For those, the review judged by the vision's test and not by Atlas usage.

## Cuts

| # | Cut | What stays | Needs first | Status |
| --- | --- | --- | --- | --- |
| 1 | `GameOptions::world`, `Game::observe_world`, `WorldRequest` and its results, `GameReadiness::PausedInWorld`, `Operation::ObserveWorld`, the M451-hotfix world recipe | The M451-hotfix target; it also binds fixtures and script checks. The non-world operand and duration checks in `tests/live/world_numeric.rs` (through `check_script`) | The SDK-547 amendment below. State on `docs/native/ready-world.md` that the route is retired, and name the commit that holds the save fixture and expected cases. | Agreed |
| 2 | Fixture registration entries and category reads: `FixtureRequest::new`, `FixtureObservationKind`, `FixtureWindow::InitialCategoryLoad`, `RegistrationEntry`, `FieldRead` | Fixture field outcomes. The live check that `tree_template` and `traditions` reach the category reader (`tests/live.rs`, the category-read case) moves into the field-outcome method or is replaced before the hooks go. | Atlas removes its `category_reads` session, whose result it does not read. Keep the findings in `docs/native/early-observations.md`. This also removes the only per-registry exception in the locality gate. | Agreed |
| 3 | `FixtureRuntime`, `with_runtime()`, `FixtureFieldQuestion.runtime` | Parsing, storage, diagnostics and the validation window | The SDK-598, SDK-599, SDK-545 and SDK-549 changes below | Agreed |
| 4 | Duration consumer: `Duration.consumption`, `DurationConsumption`, `FlagCountdown` | Unit keys, factor, combination rule, omitted count | Nothing. The facts are on `docs/native/durations.md`. | Agreed |
| 5 | Scoped operand `selection` and `literal_assignment_preserves_reference_state` | `ReaderKind::ScopedNumeric` and `ScopedOperand.forms` | Nothing. The facts are on `docs/native/scoped-numeric.md`. | Agreed |
| 6 | `Game::registry_items` and `GameOptions::registries` | Native selects the observed registries itself, always including the fixture registry. Item keys stay in `LoadedModifiers.registry_items`. A language service reads item names from the user's files; it needs the file rules of SDK-552 and the definition-name rules (`skip_root_key`, `name_field`), which are dependencies of this cut, not replacements. Keep a bounded internal way to check the SDK-552 loader rules. | Confirm that this supersedes the [Milestone 2 review](milestone-2-review.md) line "do not cut … `registry_items`". Move the two live key-offset cases (`nonstandard_key`, `generator_registries`) to a loaded-modifier test. Keep `docs/native/registry-items.md`. | Agreed |
| 7 | `Reader.numeric.clamp` and `FieldDefault` | The other numeric properties; `FieldDomain` (SDK-627) | Nothing. `clamp` is `Known(None)` for every complete reader shape (no explicit reader clamp); `FieldDefault` has only `Unknown`. This is not a ban on a proven clamp or default later, if one changes diagnostics or completion. | Agreed |
| 8 | `#[serde(default)]` on answer properties, which lets an older recording load as `Complete` with unresolved properties | Recordings whose properties are all present. The reader still checks only the build identity, not the method revision. | Do this when Atlas moves its pin and records again | Agreed |
| 9 | `Game::readiness` (public), `Game::cancel`, `GameOptions::idle_seconds`, `FixtureRequest.deadline_seconds` | `close`, cleanup on drop, `startup_seconds`, the 180-second idle timeout; internal readiness and cancellation. `GapSubject::FixtureFile` stays: the field-outcome method uses it for window and diagnostic-coverage gaps. | Amend user story 12 for `cancel` | Agreed |

Each cut must keep its engine knowledge on the knowledge pages before the code goes; see the
[development policy](../development-policy.md).

A second opinion (Codex, 2026-10-02) judged cuts 1, 2, 3, 6, 7, 8 and 9 under the compiler test
after the vision changed. It held 1, 3, 7 and 8 and narrowed 2, 6 and 9 as the table now states.

## Kept because of the vision

A first pass proposed these as cuts under a "replace the `.cwt`" test. A compiler can use each
one, so they stay:

- Reference lookup `stage`, `key_match`, `empty_key` and `on_missing`: they decide if a bad
  reference is an error, a load-order error or a silent null.
- Numeric `width_bits`, `signedness`, `literal_syntax` and `accepted_range`: a type checker needs
  literal forms, overflow limits and fixed-point precision.
- `CommandGrammar.ordering`: "`else` must follow `if`" is a compiler diagnostic.
- Duration factor, combination rule and omitted count. This agrees with the SDK-544 AC3 amendment.
- `TargetArgument.stage`: keep the field. Build no more proof for it now; all 275 target arguments
  on M45-release are unresolved.
- `ModifierFamily::name_limit`: low cost; it supports a "generated name too long" diagnostic.

## Not recommended now

- **The loaded-modifier explanation join** (`declared`, `generated_by`, the unexplained count). It
  is the only live check of the static family answer, and cut 6 relies on its item keys.
- **`Native::supports`.** Keep the check before launch. Repair one fault: with recorded answers it
  returns `Supported` for every operation.

## Ticket changes

A second review on 2026-10-02 read the 32 open product tickets of the Atlas project against the
vision. Internal tickets (performance, tooling, technical debt) were not reviewed. Jackson agreed
to every row on 2026-10-02; Linear is changed in the work below.

### Cancel

| Ticket | Reason | Follow-up |
| --- | --- | --- |
| SDK-598 (runtime weights) | Every criterion asks for a runtime result. How `add` and `mult` combine does not change what a compiler accepts. | Remove it from the blockers and the "Runtime fixture dependency" section of SDK-545 |
| SDK-599 (scope availability) | "Observed availability" is runtime meaning. The compiler needs the scope that the engine reads the block in; SDK-549 takes a smaller check for that. | Remove it from the blockers of SDK-549. Keep the SDK-488 parser-scope findings on `docs/native/discovery.md`. |
| SDK-506 (reconstruction verdict) | Prototype-era qualification: demo, runtime outcomes, freeze, held-out transfer and replay are all out of scope. | SDK-511 takes the verdict (decision 8). SDK-545, 547, 549 and 551 lose a dependent. |
| SDK-628 (enclosing conditions for swap selections) | The remainder is when a swap is selected at run time, with a heavy receiver proof. SDK-541 already gives the flag-to-field relation that the config's `inherit_name` subtype needs. | Add a note to the residual obligations of SDK-541. If Atlas cannot credit the inheritance claims, state one rule and check it on the three established selections. |

### Amend

| Ticket | Change |
| --- | --- |
| SDK-547 (modifier application) | Retitle to modifier nodes and the accepted categories of modifier containers. Build the node table (owner, kept categories, sources, both ship masks) and the container masks per script key (decision 2). The category check runs in `TryReadMember` during file read. Remove criteria 1, 2, 4, 6 and the negative control carried from SDK-497. Criterion 3: the naval-capacity modifiers are accepted by the tradition modifier container, shown by its category mask and one fixture check; remove it if no container restricts categories. Criterion 5: declared category tags and each container's accepted-category mask are separate fields. The application rule is a typed gap (text below). |
| SDK-545 (weight rules) | Criterion 5: one fixture parses a weight block with one additive and one multiplicative entry, with complete parsing and no diagnostic; one control gets a source-located diagnostic. Nested weight storage cannot be decoded today, so the check is parse and diagnostic only. Criterion 3: remove "applies to both a tradition and its swaps". Criterion 4: the read scope comes from SDK-549's method, or is unresolved. Carried list: remove "operation order"; keep the repeated-operation rule; keep "omitted base" only as accepted without a diagnostic; keep the mean-time reader, which tradition `ai_weight` uses. |
| SDK-549 (scope context) | Criterion 4: one fixture diagnostic confirms a tradition entry scope; the existing wrong-scope case in `tests/live.rs` also checks that the message names the supplied scope. Remove the separate "observed" field. Define entry scope as the scope type that the engine reads the block in (`this`). `root`, `from` and `prev` move to a new ticket (decision 3). Criterion 5 covers read-time differences only. Remove the runtime items carried from SDK-495. `check_script` cannot serve as this check: its caller supplies the scope. |
| SDK-600 (acceptance test) | Its acceptance criteria stay as written; all required facts are static. Blocked by SDK-549 and the new entry-context ticket (below). The fixture criteria become the parse and diagnostic checks of SDK-545 and SDK-549. Change the SDK-547 exclusion to "the application rule is a typed gap by design". |
| SDK-546 (localisation and asset names) | Criterion 2: miss behavior comes from the lookup code path or a load-time diagnostic, or is not established. Criterion 4: a use-time stage is reported statically or as a gap. |
| SDK-550 (script expansion) | Criterion 4: "as observed" becomes "established statically or by one fixture". Keep recursion only if a load-time diagnostic exists. |
| SDK-551 (late registries) | Static discovery only: no item query, no observation phase, no readiness boundary. AC1 resolves the 35 out-of-template scheduler rows and `CGameScenarioDatabase`, or keeps a gap. AC4 adds no live case. Narrow the carried root/nested item to two authored cases. Keep AC6, the strategic-resource and planet-class modifier-family joins. |
| SDK-552 (file selection and duplicates) | Retitle to "State file selection and duplicate-definition rules". State each registry's folder, extension and recursion, and the duplicate rule of each loader shape, from the loader. State the file-order and mod-over-vanilla rules once. One fixture checks each rule shape. Remove loader phases, reload, late registration, the held-out transfer and the static-modifier four-way split. |
| SDK-553 (registry sweep) | Add to AC2: each relationship counts only the static answer of its method; no runtime weight, scope-availability or modifier-application relationship. |
| SDK-556 (map and descriptors) | Remove "defaults". Replace "override semantics" with the duplicate rule of SDK-552. Exclude `map/galaxy` and `map/other`, which the sweep covers. |
| SDK-554, SDK-555 (other formats) | Exclude the directories that `registries()` already names. SDK-554: state which format and field declares content names; Atlas reads the names. |
| SDK-609 (localisation in a live game) | Static only. Retitle to argument forms and the remaining link output contexts. Keep criterion 4. Resolve the 27 link rows without an output context, or keep a narrower gap. Delete the live rendering criteria; they need a world. |
| SDK-610 (define defaults and bounds) | Retitle to "Resolve the custom define read helpers". Keep the 80 unresolved helpers and the population run. Delete defaults, bounds traced to run-time use, and the second-namespace transfer. Report a range only where the helper rejects a value. |
| SDK-627 (defaults, enum domains, occurrence limits) | Defaults are out of scope (cut 7). Enum domains stay, from reader string tables. Replace occurrence limits with repeat behavior for the fields where it is unknown, worked by reader because blocks can replace, add or merge, and with required fields where validation reports absence. Check the assumption that the engine never rejects a repeat with one fixture case for a `Replace` field and one for an `Accumulate` field. Remove the per-field positive and negative controls (decision 5). |
| SDK-608 (event and callback entry contexts) | Keep events, `push_scope`, pre_triggers and the `prev` chain. State the self-link rule for `root` and `from` as an assumption and check it on the three hand-checked call sites. Report argument bindings, not call-site availability. Run-time names stay typed gaps. Delete the live fixture criterion. Move it into Milestone 4 (decision 3). |
| SDK-642 (target-getter conversion routes) | Accepted scopes are required; the stage may stay unresolved. Try a stated rule first, with three checks. Use the SDK-568 masks as the agreement check. |
| SDK-635 (reference lookup shapes) | Each group resolves, keeps a narrower gap, or closes by a stated rule checked on three members. Do the groups with config claims first. |
| SDK-675 (modifier fixed-key readers) | One positive and one negative authored control per new shape |
| SDK-557 (update rehearsal) | "Every operation" becomes every operation that remains after these cuts. The target is the next patch after 4.5.1. Replace the recorded-answer rejection criterion with "the live run credits no recorded answer". |
| SDK-674 (drop 4.5.0) | Do not port tests of cut items. Land it after cuts 1, 2 and 6, or delete those tests with their cut. |
| SDK-625 (Atlas Milestone 4 coverage) | Fixture conclusions are parser outcomes; no runtime claim. Numeric claims do not use the duration consumer, operand selection, `clamp` or `FieldDefault`. Scope claims are declared entry scopes. The pin move records again on M451-hotfix and removes `with_runtime`, the `category_reads` session and the `RegistryItems` filter. |
| SDK-626 (gap triage) | Add the triage rule: a failure shape gets a ticket only if closing it changes what a compiler accepts, rejects, types or completes, and a config claim depends on it. Other shapes get an "out of scope, vision 2026-10-02" note. Measure the shapes after the pin move. |
| SDK-511 (first Atlas release) | Add the composition verdict of the tradition and category slice, from SDK-600 and SDK-553: established, typed gap with owner, or out of scope. Item 2: the end-to-end offline test replaces the experimental consumer (decision 7). Remove SDK-506 as a blocker. The release covers one catalogued build with passing tests. The update rehearsal is not a release gate. A correction is a new snapshot version. A guarantee that needs a runtime value does not block the release. Remove the stale SDK-485 blocker. |
| SDK-470 (Atlas map) | Append amendments dated 2026-10-02 and keep the old text: runtime behavior is out of scope; no retained evidence or requalification; one Apple Silicon build at a time, Windows deferred; severity, subtype naming and alias factoring are not Atlas products (decision 6); the SDK-478 experimental consumer decision is superseded by decision 7. |

### Keep unchanged

SDK-673 (triggered modifier clause), SDK-676 (modifier grammar for unjoined and nested fields) and
SDK-641 (raw-string Assign readers). SDK-673 and SDK-676 need only their SDK-547 reference
updated. SDK-564 does not change while the explanation join stays.

Gap text for SDK-547: "Where a modifier takes effect is decided at application by each receiver's
category mask and the include and exclude masks of each propagation edge. The engine reports no
diagnostic for a filtered entry. These masks are not established; supported scopes stay outside
this method."

The SDK-547 mask evidence is from the M45-observe beta capture. Match it to the supported build
before it is reused.

## Work to do

In this order. Each item is a separate task; none was started by the review.

1. **Linear.** Cancel SDK-506, SDK-598, SDK-599 and SDK-628 with a comment that names this
   document. Append each amendment in "Ticket changes" to its ticket as a dated amendment
   (2026-10-02); do not rewrite the original text. Create the entry-context ticket (`root`,
   `from` and `prev` for registry field blocks, from evaluation call sites; blocked by SDK-608;
   blocks SDK-600; Milestone 4). Move SDK-608 into Milestone 4. Remove the blockers named in the
   tables. Append the dated amendments to SDK-470.
2. **Specification, roadmap and README.** Apply the amendments listed under "Documents to amend"
   and the decisions above. The roadmap loses the policy overlay from "Not yet ticketed" and
   gains SDK-608 and the new ticket in Milestone 4.
3. **Cuts, in this order:** 4 and 5 (no dependency); 3; 7; 9; 1 (after the SDK-547 amendment,
   keeping the non-world checks); 2 (after its live check moves); 6 (after the key-layout tests
   move); 8 (with the Atlas pin move). Before each cut, confirm that its engine knowledge is on
   a knowledge page.
4. **Atlas.** Remove `with_runtime()`, the `category_reads` session and the `RegistryItems`
   filter; move the pin; record again on M451-hotfix; add the SDK-626 triage rule; point the
   entry-scope gap owner at SDK-608; apply SDK-625's claim rules.
5. **Stale items** under "Found on the way".

## Documents to amend when a cut lands

- [Specification](../specs/native.md): section 2 rows for `observe_world`, `observe_fixture`
  entries and reads, `Game::registry_items`, `start_game`, `Game::close`/`Game::cancel` and
  `command_grammar` (duration consumer); section 3 bullets for `ReaderKind::ScopedNumeric`,
  `CommandGrammar.durations` and the older-recording defaults; the section 4 world paragraph and
  agreed G2; the section 7 amendment of 2026-09-29; user stories 8 and 12; the "Milestone 4
  shared-reader acceptance" lines that name SDK-598 and SDK-599.
- [Roadmap](../roadmap.md): Milestone 4 acceptance item 2 and the ticket list.
- `README.md`: "Live questions", "Prepared fixtures" and the recorded layout.
- [Architecture](architecture.md) and `docs/native/discovery.md`: the world route.

## Open decisions

1. **`check_script`. Decided, 2026-10-02:** it stays public. It is the smallest live check of
   command grammar, and the proposed Atlas corpus comparison needs it. No new `check_script`
   feature is built until Atlas accepts that comparison or a ticket names a specific control.
2. **`modifier_categories.cwt`. Decided, 2026-10-02:** the category names are engine facts; the
   `supported_scopes` lists are written by hand and have errors. A static probe on M451-hotfix
   (`.local/modifier-node-probe/REPORT.md`) found a fixed graph of 35 modifier nodes with constant
   category masks, except the ship, and restricted parse-time containers. SDK-547 builds the node
   table and the container masks, and keeps a typed gap for where a category takes effect and for
   the three categories with no node. Atlas derives candidate scopes through a stated
   node-to-scope mapping; a disagreement with the config is a case to review.
3. **`root`, `from` and `prev`. Decided, 2026-10-02:** they are in scope. A language service
   needs them for completion and typing. SDK-549 keeps `this`, with a read-time fixture check. A
   new ticket establishes `root`, `from` and `prev` for registry field blocks from their
   evaluation call sites. SDK-608 moves into Milestone 4 and blocks the new ticket, because both
   need the self-link rule. The new ticket blocks SDK-600.
4. **Entry scopes with no live check. Decided, 2026-10-02:** SDK-608 and the new entry-context
   ticket state the self-link rule as an assumption. The checks are: 10 to 15 hand-read call
   sites across on_actions, game rules and field blocks; every on_action answer compared with the
   scope comments in the vanilla on_action files; game rules and field blocks compared with the
   config's `replace_scopes`. The comments and the config are test expectations only; Native
   never reads them to make an answer. Agreement counts go on the knowledge page. A disagreement
   is a typed gap with a hand-read of its call site. An answer with no independent source keeps a
   gap that says so. Keep `docs/native/ready-world.md` complete, so that the world pause can be
   restored if the rule fails.
5. **Occurrence scoring. Decided, 2026-10-02:** the engine fact is `RepeatBehavior` (`Replace`,
   `Accumulate`, `Unknown`), not a maximum count. A compiler warns on a repeat of a `Replace`
   field and allows an `Accumulate` field. Atlas credits a config maximum of 1 on `Replace` and an
   unbounded maximum on `Accumulate`; it publishes no engine limit. A field is required only
   where validation reports its absence.
6. **Atlas policy overlay. Decided, 2026-10-02:** cut from the product. Severity belongs to the
   compiler; subtype names and alias factoring serve only the `.cwt` emitter, which keeps its own
   mapping as test code. Remove the overlay from the roadmap's "Not yet ticketed" list and add
   the statement to SDK-470.
7. **Consumer demo. Decided, 2026-10-02:** no demo consumer. The compiler is the real consumer
   and comes later. The release gate is a schema-valid snapshot for one catalogued build and the
   end-to-end offline test of specification testing decision 9, where the test itself reads the
   snapshot. SDK-511 item 2 and the specification change accordingly; SDK-506 loses its demo.
8. **SDK-506. Decided, 2026-10-02:** cancel. SDK-511 takes the composition verdict from SDK-600
   and SDK-553 and loses SDK-506 as a blocker.
9. **One game per host. Decided, 2026-10-02:** keep the host-wide lock and its one-time setup.
   A per-account lock loses only the case of two accounts that launch in the same instant.
   Revisit only if a second account becomes a real need.

## Found on the way

- `README.md` gives 987 generated and 44,020 unexplained loaded modifiers.
  `docs/native/modifier-families.md` and Atlas give 5,432 and 39,576. One is stale.
- Specification section 2 says fixtures and `check_script` are "M45-release only". The
  M451-hotfix target binds both.
- No open ticket gives the config's `<ship_size>` modifier templates (99 lines of
  `modifiers.cwt`). SDK-551 AC6 owns the `<resource>` templates.
- Atlas names the closed SDK-496 as owner of the 517 entry-scope gaps. The owner is SDK-608.
- SDK-573 (generator registries at the registry pause) has no purpose after cut 6.
- Specification section 6 ("actual scope type and availability") and Atlas `CONTEXT.md` still
  promise runtime meaning.
