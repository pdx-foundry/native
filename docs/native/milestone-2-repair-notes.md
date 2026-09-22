# Milestone 2 repair notes

## Shared decisions from PRs #15 and #18

PR #15 encoded the M45 CString tag offset for fixture field reads. PR #18 added the same
offset to the shared-template registry layout. Both describe the same representation. The
fixture binding now reads `M45_TEMPLATE_LAYOUT.string_tag_offset`; the layout has one owner in
`src/binding/groups.rs`. The fixture field-token entries remain category-specific read-entry
controls. Outcome field tokens and storage offsets come from verified reader analysis, and the
worker receives those resolved bindings. Public fixture validation bounds relative path syntax,
file size and question shape; the exact-build binding selects the supported loader and owner
hooks. The category read-entry names and tokens remain a recorded manual exception in
`early-observations.md`.

## Transport and lifecycle

Session files are temporary. Atomic publication, bounded reads, session identity, stream
continuity and worker-source hashes remain. Supervisor-side file and directory `fsync` calls
were removed; the Python worker still calls `fsync` for atomic control messages and appended
trace records.
The worker-source hashes still verify that the copied Python package is the one the supervisor
prepared. The generated Python protocol remains the sole wire-schema projection from Rust.

The live suite uses one shared-template registry for each registry fault instead of repeating
the same fault matrix across three registries. Fixture fault cases exercise the second path.
Worker loss is checked before hook activation and after the first registry entry; those are
different cleanup guarantees. Reducer, handshake and transport checks run as fast crate tests.
A fake worker now also drives one paused registry answer through the supervisor observation
and cleanup path without Stellaris. It checks the result, final report, worker reap, game
disposal and reservation release.

The OS lock excludes concurrent Native supervisors, and the process inventory refuses an
ordinary Stellaris instance. A prior session report is no longer an admission gate. Cleanup
still reports game disposal separately from failure to write the owner or session report.

## Static completeness

`registries` searches the shared database-template candidates. It is complete when every
candidate inside that boundary has one content directory; custom, nested and late loaders are
outside the search. `registry_fields` searches root reader paths for one such registry. It is
complete when every path, field name and promised reader classification is resolved. An
`OutsideMethod` gap documents the boundary and can accompany `Complete`. Unnamed candidates,
unresolved paths, unknown reader classifications and unreadable required input make the bounded
answer partial. These semantics are revisions `registry-directories/v3` and
`registry-fields/v3`. The frozen milestone sweep retains the earlier v2 answers.
