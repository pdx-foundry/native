# Report structure

The model is [milestone-2-review.md](../../../docs/design/milestone-2-review.md). Sections, in
order:

1. **Status line.** Recommendation or agreed recommendation; date; the commit reviewed; what was
   checked in the source and what was not run again.
2. **Finding.** The verdict on the exit gate, then the cracks that block transfer, each with its
   file. State the combination of signals that justifies action now.
3. **Acceptance target.** One sentence that a test can decide: what an unfamiliar case must do
   with no new handwritten knowledge.
4. **Rule for the cuts.** Use this text:

   > Keep code that serves current behavior, verifies a current guarantee, or preserves knowledge
   > that is not yet transferred. Remove duplicate implementations, obsolete paths, and
   > speculative machinery with no current responsibility.

   To preserve knowledge means small representative inputs, expected outcomes, failed cases and a
   short note. Then the implementation is deleted; Git history keeps it.
5. **Repairs.** The measurement first. Each repair states its reason and its limit.
6. **Cuts**, one table for each repository: cut, lines, reason and conditions. Each cut says what
   to preserve first. List what must stay.
7. **Changes to guarantees**, apart from the cuts. A change to a lifecycle, transport or test
   guarantee is a policy decision: state the guarantee that is lost, why that is acceptable, which
   specification section it amends, and the check that goes with it.
8. **Linear.** Duplicate or stale tickets.
9. **Order of work.** Evidence first: document exact limits, measure, then repair, then cut, then
   guarantee changes one at a time. Say what can proceed in parallel.
10. **Deferred.** What waits, and for what.

## Generalize or cut

When a feature holds handwritten knowledge, give it one **bounded experiment** with all four
parts:

- a limit in working time that includes verification;
- the method frozen before it meets unfamiliar cases, with no case-specific constants added to
  make them pass;
- success defined through the public API, with unsupported cases reported honestly;
- a binding failure outcome: what is removed, what findings are preserved, which observations
  stay, and where further work returns.

If the failure outcome can reduce supported output, the report says so. Promise "same public
behavior" only when each cut preserves it.
