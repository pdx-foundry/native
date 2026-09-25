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

## Keep engine knowledge in its home

A shared method reads the executable and has no branch on a registry, a command or a build. Each
engine fact has one home:

- **The executable states it.** The method derives it. A branch that shortcuts the derivation is
  wrong on the next build.
- **It is per build**, such as an offset, a recipe or a table location. It lives in the binding
  authority: target records and recipes.
- **No method reaches it and a person supplied it.** It is a recorded manual exception in the
  specification's format: claim, conditions, obstacle, removal route. It is never a branch.

An unfamiliar shape becomes a typed gap. A repair lands in the shared module that owns the shape.
Review checks this. The locality gate, `tests/locality.rs`, enforces it in `cargo test`. It scans
all production code except the binding authority, and it fails on the following:

- a content directory;
- an engine class, registry, field or command selected by a comparison;
- the registry count of a build;
- a build or version test.

Test code may name build-specific registries, fields and counts, because they are regression
expectations. Each recorded manual exception is one entry in the gate's list, with its reason and
removal route.

### Measuring method transfer

A method ticket ends with one run of the method over every discovered registry, or over the whole
command inventory when the method reads commands. Record on the method's page in `docs/native/`
(the [discovery index](native/discovery.md) lists them) the counts of complete, partial and failed
answers, each failure shape, and any distinct finding the run produced. Repairs that the run
prompts land in the shared module; the next ticket's run reflects them. Do not record routine run
chronology.

There is no freeze commit, no commit-per-repair rule and no held-out selection. A method with no
per-case branch treats every registry the same, so running it on a registry it has not seen is not
a special event. The registry sweep (SDK-553) is the one gated number: it runs the method set
unchanged at a recorded commit and reports the automatic rate per relationship, registry and
reader identity. Failures in the sweep become follow-up tickets, never fixes inside the sweep.

This replaces the per-ticket freeze and held-out tests of the Milestone 2 and 3 tickets
(2026-09-23). Each method page keeps the failed shapes of those runs as pitfalls, not the runs.

### Write a method in one task

Do not write a separate throwaway prototype first. In the method's own task, explore the executable
with the developer inspector (`examples/inspect.rs`), record each finding and each failed shape on
the method's page in `docs/native/`, and deliver the method with its authored tests and parity
output. This replaces the prototype-then-port split (2026-09-24). Untracked work in `.local/` still
holds acquired knowledge until its findings are recorded; preserve it as the next section says.

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
