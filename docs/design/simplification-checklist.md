# Simplification implementation checklist

Tracks the agreed [2026-10-02 review](simplification-review.md). A checked item means its
change and relevant verification are complete. Preserve engine knowledge before removing code.

## Preparation

- [x] Read the review, development policy, and simplification decision.
- [x] Record existing work: `AGENTS.md` is modified and the review is untracked; preserve both.
- [x] Locate Native removal paths, Atlas callers, and verification commands.
- [x] Inventory Linear descriptions, blockers, and milestones before changing them.

## Linear

- [x] Cancel SDK-506, SDK-598, SDK-599, and SDK-628 with the agreed explanation.
- [x] Append dated amendments to every ticket in the review's amendment table; retain original text.
- [x] Create the Milestone 4 registry entry-context ticket; SDK-608 blocks it, and it blocks SDK-600.
- [x] Move SDK-608 to Milestone 4 and remove superseded blockers.
- [x] Update SDK-673 and SDK-676 references to SDK-547; retain SDK-641 and SDK-564 scope.
- [x] Address SDK-573 and the missing ship-size modifier-template ownership.

## Documents

- [x] Amend the specification, roadmap, README, architecture, and discovery index for compiler facts.
- [x] Remove runtime promises, policy overlay, and demo-consumer release requirements.
- [x] Add SDK-608 and the new entry-context ticket to Milestone 4.
- [x] Correct stale modifier totals and target-support statements.

## Native cuts (agreed order)

- [x] Cut 4: remove duration consumption; retain keys, factors, combination, and omitted count.
- [x] Cut 5: remove scoped operand selection and reference-state preservation property.
- [x] Cut 3: remove fixture runtime questions; retain parsing, storage, diagnostics, and validation.
- [x] Cut 7: remove numeric clamp and FieldDefault; retain numeric properties and FieldDomain.
- [x] Cut 9: remove public readiness/cancel and configurable idle/fixture deadlines; retain cleanup.
- [x] Cut 1: preserve the world route and fixture retrieval commit, remove it, retain script checks.
- [x] Cut 2: retain the category-reader field-outcome check, then remove entries/category reads.
- [x] Cut 6: move key-layout tests, retain internal loader-rule checks, remove public item selection.
- [ ] Cut 8: require recorded answer properties, together with the Atlas pin and new recordings.
- [x] Repair recorded `Native::supports` to reflect the available operations.

## Atlas

- [x] Remove `with_runtime`, the unused `category_reads` session, and the `RegistryItems` filter.
- [ ] Apply SDK-625 static claim rules, repeat behavior scoring, and SDK-626 triage rules.
- [x] Assign entry-scope gaps to SDK-608 and remove runtime promises from `CONTEXT.md`.
- [ ] Move the Native pin and record answers again on M451-hotfix.

## Verification and delivery

- [x] Run focused checks during each cut and retain meaningful live controls.
- [x] Run Native formatting, complete offline tests, and required parity/live checks.
- [ ] Run Atlas verification with the new recordings.
- [ ] Review the final changes, repair findings, and record actual results below.
- [ ] Commit the implementation without including unrelated changes.

## Results and remaining work

Implementation started 2026-10-02. Linear changes independently verified: all original descriptions
remain; 25 amendment-table tickets and SDK-541/673/676 have dated amendments. SDK-677 owns registry
entry contexts; SDK-678 owns ship-size templates. SDK-573 was already Done and stays historical.

All nine Native code cuts are implemented; final checks and Atlas recording migration remain.
The category control now uses field-outcome parsing for both fields. The key-layout controls use
loaded modifier keys, including the bypass +0x18 layout. One internal single-registry loader
control remains for SDK-552. World code, save and evaluated cases are retrievable at `d8f9d8a`
(and the earlier complete fixture tree at `2d930e4`).

Verified so far: all Rust targets compile before the final recording-shape edits; 19 fixture
reducer tests and 4 fixture request/recording tests pass; 13 Python protocol and 41 Python worker
tests pass. A read-only review found stale parity properties and weak migrated fault assertions;
both are repaired. Final checks will verify the changed recording shape and source stamps.

Atlas caller edits remove runtime requests, category_reads and the RegistryItems filter. Claim
scoring, current recordings, pin, and Atlas verification remain. Its existing Cargo.lock change
and untracked .DS_Store files are preserved.

The 17 recording tests pass after removing answer defaults. Rust all-target compilation passes.
The private script wire reply now has its own raw observation type, so public recordings require
stored-duration properties without requiring the worker to invent them.

The complete Native library run passed (804 tests; 26 ignored), and all integration groups before
the examples passed. Two example fixtures still depended on the removed answer defaults; they
now name each property explicitly, and the full example suite passes. Atlas all-target compilation
passes after migrating storage and new error/gap variants. Repeat scoring and its regression
tests are implemented; these tests await fresh recordings. Native parity/live verification and
fresh Atlas capture are queued next. Independent standards and spec reviews are running.

Follow-up verification: all 12 documentation tests pass. Removed the orphan Python world-only
test with its retired module; all 97 observation and 14 population Python tests pass. Native
Clippy passes with warnings denied. The standards review's trivial duration-input wrapper and
the spec review's obsolete profiling caller are repaired. The current live profiling workloads
use the loader fixture and loaded-modifier key layouts; historical results retain their retrieval
commit. The Atlas code review found no further issues. Fresh recordings and pin remain pending.

The full Native workspace suite now passes, including examples and doc tests; documentation also
builds with warnings denied. Static parity passed 30 of 33 tests. All three failures reported the
same 12 old method stamps and one retired duration-consumer gap; those exact expectations are
updated. The dedicated duration baseline also lost 33 retired consumer-only gaps. Layout checks
pass. Only the failed parity checks and the changed numeric/duration controls will be rerun before
live controls and Atlas capture. Both Atlas review axes report no actionable findings.

Native verification is complete: 33 static parity checks pass across the initial run and focused
rerun; the historical SDK-533 storage observation is explicitly skipped on the different exact
build. All seven dedicated duration/numeric/scoped static controls pass. The six retained live
controls pass: category field parsing, dropped record, internal loader fixture, modifier key
layouts, 48 script operands plus five rejection cases, and stored durations. Fresh Atlas capture
exited 0 with 28,187 rules and 14,027 gaps on M451-hotfix. Atlas coverage is 16,341 / 58,032;
the same 19 source diagnostics remain. Baseline review, pin and offline Atlas tests remain.
