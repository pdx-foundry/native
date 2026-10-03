# Development improvements: open items

The 2026-09-24 proposal to make Native development smoother is mostly delivered: the inspector
and stop diagnostics (DX 1), the sweep report (DX 2), `Operation::name` and the Atlas local patch
(DX 4), the pause owner (DX 5), the locality gate, cargo aliases, toolchain and `cargo doc` in CI
(DX 6), decoder step 1 and the test assembler (DX 7), and the method index and authoring guide
(DX 8). The items below are still open. They keep their DX numbers, because other documents cite
them. Linear holds their status.

## DX 3. Parity tests per build (prerequisite of SDK-557)

- Expected output under `tests/expected/<build-name>/`, keyed by the target record
  ([SDK-636](https://linear.app/unnamed-system/issue/SDK-636)). An optional generator writes a
  candidate tree for review as a diff; it is not verification.
- Anchors as symbol plus checked offset in the target record. `examples/rebind.rs`, over the
  inspection entry and a raw image path, lists candidate anchors on a new executable as moved,
  missing or unchanged ([SDK-582](https://linear.app/unnamed-system/issue/SDK-582)). A candidate
  is confirmed only by checking the instructions and behavior.
- The SDK-557 cost record (shared-method work, adaptation, fixture changes, Atlas changes, human
  attention, agent effort) is kept separately; the tool does not supply it.

## DX 4. Batched modifier families

`Native::modifier_families_all()` or a cached modifier input, only after the per-registry cost is
measured again ([SDK-591](https://linear.app/unnamed-system/issue/SDK-591)). Each call rebuilds
its input; a snapshot makes one call for each registry.

## DX 5. Live sessions and display sleep

[SDK-571](https://linear.app/unnamed-system/issue/SDK-571) stays an investigation: reproduce
first, and choose warnings or a display assertion only if the result shows that they are
feasible and needed.

## DX 6. Manual parity workflow

A manual Mac workflow for the parity tests
([SDK-592](https://linear.app/unnamed-system/issue/SDK-592)) is worthwhile after SDK-571 reports.
It is not a prerequisite.

## DX 7. Decoder consolidation, steps 2 and 3

Each step has an equivalence test. None of them blocks a method ticket.

- Step 2: typed operands, one consumer at a time, with authored equivalence tests
  ([SDK-594](https://linear.app/unnamed-system/issue/SDK-594)). The decoder now gives string
  operands, and several private text parsers read them again. Keep the raw-word `adrp`/`add`
  fallback of the family method for functions that do not decode, unless a tested replacement
  covers them: it is a conservative second tier, not duplication.
- Step 3: fold the ad hoc register trackers onto `Machine` only where their semantics are shown
  equal ([SDK-595](https://linear.app/unnamed-system/issue/SDK-595)).
