# Development policy

Native is in rapid development, has no consumers, and has no stable API commitment. This policy records
the maintainer's standing authorization from SDK-521. It replaces earlier routine approval gates.

## Work autonomously

Make the code, API, documentation, tests, and qualification-record changes needed for the task. Breaking
API changes are expected as Atlas develops. Run relevant checks, report the result, and continue through
the authorized delivery workflow without asking for another approval of reversible project changes.

Generated captures, logs, build outputs, private development profiles, and Native's Application Support
state are disposable. Agents may repair, replace, or remove them when needed. Stellaris on this machine
is a development installation; agents may modify or restore its files as part of the task. Keep changes
scoped to development resources and preserve unrelated applications, personal data, and host security.

For a stale reservation, inspect the recorded process identities and current process state, deal with
any still-running owned process, then clear only the affected record under the namespace lock. This is
agent-operated development recovery, not automatic recovery by the library. Keep disposal reporting
truthful: administrative clearance does not establish that Native reaped the original process. A short
recovery note is enough; a new evidence archive and another permission request are not required.

**Amendment, 2026-09-20:** the [simplification decision](design/simplification.md) removes
qualification records and promotion. Until the work order removes that code, the paragraph below
applies to it. Afterwards, a build is supported when it is in the target catalogue and its tests pass.

Agents may qualify and promote a changed implementation after the required checks pass, then run its
ordinary production controls. Record what was tested and its limits. Separate human acceptance is not
required during this development phase; do not describe agent verification as a new human review.
The runtime's target, content, ownership, and admission checks still apply until deliberately changed
and verified as part of a task. A failed check is a reason to fix or narrow a claim, not to relabel it.

## Preserve acquired knowledge

The valuable asset is the hard-won knowledge from prototypes and probes, including untracked source,
notes, raw observations, and dependencies needed to understand or reproduce a finding. The
[knowledge inventory](native/source-inventory.json) and [preservation guide](native/preservation.md)
locate historical copies. Git does not protect these private files.

Before deleting or replacing a source that contains unique knowledge, verify a usable retained copy
or preserve its necessary contents first. Protect that copy from the same cleanup. Do not bulk-delete
`.local/evidence`, historical prototype trees, or their external backups as if they were build caches.
When an artifact contains a new finding, preserve the finding and the material needed to support it;
there is no requirement to retain every routine run or duplicate capture forever.

Ask only when necessary work could lose the last useful copy of acquired knowledge, affect unrelated
data or system stability, or exceed the user's authorized task. Routine project cleanup, unstable API
changes, and verified development promotion have standing authorization.
