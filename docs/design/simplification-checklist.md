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
- [x] Cut 8: require recorded answer properties, together with the Atlas pin and new recordings.
- [x] Repair recorded `Native::supports` to reflect the available operations.

## Atlas

- [x] Remove `with_runtime`, the unused `category_reads` session, and the `RegistryItems` filter.
- [x] Apply SDK-625 static claim rules, repeat behavior scoring, and SDK-626 triage rules.
- [x] Assign entry-scope gaps to SDK-608 and remove runtime promises from `CONTEXT.md`.
- [x] Move the Native pin and record answers again on M451-hotfix.

## Verification and delivery

- [x] Run focused checks during each cut and retain meaningful live controls.
- [x] Run Native formatting, complete offline tests, and required parity/live checks.
- [x] Run Atlas verification with the new recordings.
- [x] Review the final changes, repair findings, and record actual results below.
- [x] Commit the implementation without including unrelated changes.

## Results

Completed 2026-10-02. Native implementation: `92b27b3eaf2f7021ad9e8105ad9818edd0b7ec19`.
Atlas implementation: `46e8055a893d4f7d50716b8d12da09d423ec7cbf`. Both are published on their repository's
`codex/compiler-simplification` branch. Atlas pins Native's implementation commit; this final
checklist update changes no API or recordings.

All nine cuts and the recorded-support repair are complete. World code and its save/expected
cases remain retrievable at `d8f9d8a` (also the earlier complete fixture tree `2d930e4`). Knowledge
pages preserve removed facts. The category control uses both fields' parser outcomes; modifier
key-layout controls use loaded keys; one internal loader fixture remains for SDK-552.

Linear changes were independently verified: original ticket descriptions remain; all 25
amendment-table tickets plus SDK-541/673/676 carry dated amendments. The four agreed cancellations
and blocker changes are applied. SDK-677 owns registry entry contexts, SDK-678 owns ship-size
templates, and SDK-573 remains historically Done.

Native verification:

- Full workspace suite passes, including 804 library tests, integration tests, examples and 12
  documentation tests. Formatting, Clippy and documentation builds pass with warnings denied.
- All 97 observation and 14 population Python tests pass.
- All 33 static parity checks pass across the initial and focused runs. Historical SDK-533
  storage is explicitly skipped because its exact M45 build differs from M451-hotfix.
- Seven dedicated duration, numeric and scoped-operand static controls pass.
- Six live controls pass: category parsing, dropped record, internal loader fixture, modifier key
  layouts, 48 script operands plus five rejection cases, and stored durations.

Atlas verification:

- Fresh M451-hotfix capture exits 0 with confirmed game disposal: 28,187 rules and 14,027 gaps.
  All 343 fixture files come from that capture; only the documented loaded-modifier test sample
  is reduced. Older recordings remain in Atlas Git history.
- All 72 offline tests pass against the actual remote Native pin in an isolated source copy;
  formatting, Clippy, documentation and the full-config acceptance gate pass.
- Live and full recorded snapshots agree after basis, identity and supported-operation
  differences are normalized. Full recorded answers give zero current-engine coverage.
- Coverage is 16,341 / 58,032 (28.158602%): 362 gains, all repeat-behavior maximum questions,
  with no losses. Ledger bytes and the 19 source diagnostics are unchanged. Name-list,
  script-doc, define and modifier-tag comparisons are unchanged.
- The measured gap shapes, compiler/config triage rule and all report hashes are in Atlas
  `docs/coverage/simplification.md`. The baseline now pins the reviewed fresh reports.

Standards and specification reviews are complete. Their findings (a leftover duration wrapper,
obsolete profiling caller and fixture README order wording) are repaired. Test failures exposed
stale defaults, method stamps and schema variants; all are repaired and verified.

Existing Native `AGENTS.md` edits, Atlas's local Native patch and its corresponding working
lockfile change, and untracked `.DS_Store` files remain outside the commits. The committed Atlas
lockfile changes only Native's Git revision. Final remote-pin verification used a source copy
because workspace tooling returned the working lockfile to its local-patch form.

Final verification logs: Codex jobs `job-fth19c1_` (Native parity/live and fresh capture) and
`job-slw0mzgj` (Atlas remote-pin suite and coverage gate). Earlier offline Native success is in
`job-nglwlwwx`; that job's stale parity expectations were repaired in the focused run.
