# Development policy

Native is in rapid development. It has no stable API. This policy records the
maintainer's standing authorization from SDK-521. It replaces the earlier routine approval gates.

## Work autonomously

Make the code, API, documentation and test changes that the task needs. Breaking API changes are
expected while Atlas develops. Run the relevant checks, report the result, and continue through
the authorized delivery workflow. Do not ask for another approval of a reversible project change.

Generated captures, logs, build outputs, private development profiles, and Native's Application
Support state are disposable. Agents may repair, replace or remove them. Stellaris on this machine
is a development installation; agents may modify or restore its files as part of the task. Keep
changes scoped to development resources. Preserve unrelated applications, personal data and host
security.

A build is supported when it is in the target catalogue and its tests pass. A failed check is a
reason to fix or narrow a claim, not to relabel it. Do not describe agent verification as a human
review.

## Preserve acquired knowledge

The valuable asset is the knowledge from prototypes and probes. It includes untracked source,
notes, raw observations, and the dependencies needed to understand or reproduce a finding. The
[knowledge inventory](native/source-inventory.json) and [preservation guide](native/preservation.md)
locate the copies. Git does not protect these private files.

Before you delete or replace a source that holds unique knowledge, verify a usable retained copy
or preserve the necessary contents first. Protect that copy from the same cleanup. Do not
bulk-delete `.local/evidence/bundles`, historical prototype trees, or their external backups as if
they were build caches. When an artifact holds a new finding, preserve the finding and the
material that supports it. It is not necessary to keep each routine run or duplicate capture.

Ask only when necessary work could lose the last useful copy of acquired knowledge, affect
unrelated data or system stability, or exceed the user's authorized task.
