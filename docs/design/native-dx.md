# Development improvements: open items

These are the open items of the development-experience proposal. They keep their DX numbers,
which other documents cite; Linear holds their status.

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
  covers them: it is a conservative second tier, not duplication. `decode::general_register` is
  the one register-name reader (SDK-727): `x0`–`x30` and `w0`–`w30`, never `sp`, `xzr`, `fp` or
  `lr`, because the pinned decoder writes `x29` and `x30`. The `adrp`, load and store operand
  readers in `callbacks/instances.rs` wait for this step.
- Step 3: fold the ad hoc register trackers onto `Machine` only where their semantics are shown
  equal ([SDK-595](https://linear.app/unnamed-system/issue/SDK-595)). The instance-pointer pass
  has one register flow, `register_flow` (SDK-727). It stays apart from `Machine` on purpose:
  it must cover every path, so it has no search bound, joins by union and follows no value
  through memory. `clears_moved_source` (binding `fields.rs`) asks the opposite question, a
  clear on every return, so it joins by intersection; a shared worklist would be a trait for
  two lattices.

## DX 8. Entry-context diagnostics

SDK-728 added the per-site view of block entry contexts (`inspect --entry-contexts`), the block
counts in the field sweep's `entry_contexts` section, the on_action and game rule counts
(`tools/population/callbacks.py`) and name-keyed comparison of the two callback parity files
([method authoring](../native/method-authoring.md)). Two parts stay open; add them when a ticket
needs them:

- A per-site view of the callback context pass (`callbacks/contexts.rs`, `Runner::contexts`) for
  on_actions and game rules. It combines reasons across paths and keeps only the reason word.
- The wrapper and call behind a reason charged while climbing to an entry (`no-caller`,
  `caller-depth`, `caller-not-decoded`). These belong to no entry run.
