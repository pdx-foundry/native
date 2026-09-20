# Registry root fields

SDK-530 adds `AnalysisContext::analyze_subject(&discovery, &subject)` and
`CapabilityRequest::RegistryFields`. Open one static context, call its `discover_registries()`,
and pass a candidate handle from that result. The context rejects handles issued by another
context and handles paired with another discovery result, even on the same executable.
Cloning a handle or its discovery result retains its identity. Replay handles cannot authorize
an executable analysis operation. Loader/owner observation handles are not candidate handles.

Only the exact M45-observe executable and ARM64 slice have a qualified composition. Each operation
checks executable integrity again; a detected change permanently invalidates that binding.
Field admission is independent of decode and discovery admission, although obtaining a candidate
for the public operation requires admitted discovery. No game process or debugger is used.

## What the result means

`RegistryFieldResult` contains root token names, token identities, constructor evidence, a ledger
of token paths, and `Joined` or `Missing` reader relationships. A join proves argument routing to
a named callee. It does not establish the reader's grammar, accepted types, scope contract, or
runtime behavior. Native callee names and addresses are target-local evidence locators; subject
handles remain opaque. The owner is linked through the exact template loader symbol, which does
not establish current live ownership.

The reducer reads raw instructions, the executable symbol inventory, and engine string literals.
It first finds reachable token-constructor calls, joining only equal constants across control-flow
merges, then recovers their literal arguments and partitions signed 32-bit root-token branches.
Known zero tests take only their feasible branch; unknown state tests retain both alternatives.
Unknown instructions, unsupported addressing,
missing symbols/names, conflicting token names, cycles and clobbered values become explicit gaps.
A singleton reaching a delegate remains visible even when its reader join is missing. A bare
comparison pivot or an unsupported path does not establish a field. A gap on another alternative
of an established field remains attached to that field.

This revision stops at the first external delegate. Owner helpers, inherited readers beyond the
verified base rejection, indirect calls, dynamic names and nested grammars remain explicit
boundaries. It does not carry argument provenance through unknown calls or assume that a stack
restore recovers provenance. Root traversal is limited to 500 instructions per path and 4,096
states. Token-construction reachability permits at most 40 visits per decoded instruction in
aggregate; unknown control-flow shapes stop token recovery. Symbol aliases are indexed once by
address, and conflicting names stay unresolved. Input descriptors are limited to 1 MiB; recorded
inputs to 64 MiB, 128 functions and 4 MiB
of aggregate code, with a 1 MiB per-function decode bound. Input ranges use the existing 4 KiB decoder.

`partition_accounted` means the ledger accounts for the signed token intervals, including gaps;
it never means every path was resolved. `complete_registry` is always false for this method.
Council agenda retains five SDK-487 shared-reader blockers: scoped integer values, triggers,
effects, graphical modifiers and AI weight. These labels preserve the failed experiment's
obligations; they are not field-name seeds or qualified reader-kind classifications.
Their source is the verified `atlas-discovery` bundle's
`prototype/council-agenda-reconstruction/registry.json`, described in
[discovery evidence](../native/discovery.md).

The M45 checks find 10 council-agenda fields over 21 paths, 11 tradition fields over 22 paths,
and 7 tradition-category fields over 14 paths. Counts describe this bounded method, not exhaustive
member inventories. Agenda parity includes names, tokens, branch conditions, instruction paths,
reader arguments and the raw bytes of all three retained functions.

## Capture and replay

Candidate ordinals are local to a discovery result; they are not stable registry identifiers.
The example selects an ordinal from a fresh discovery and passes its actual opaque handle.

```sh
cargo run --locked --release --example analyze-subject -- /path/to/stellaris CANDIDATE_ORDINAL .local/evidence/fields-NEW
cargo run --locked --release --example replay-fields -- .local/evidence/fields-NEW .local/evidence/fields-NEW/descriptor.ref.json
```

`Engine::replay_registry_fields` verifies input artifacts and uses the same reducer without an
installation. Its descriptor stores input references and method/decoder/implementation identities,
not extracted answers. Synthetic evidence remains synthetic, replay remains historical, and replay
does not grant qualification. Unsafe paths, missing/damaged artifacts and unsupported revisions
fail explicitly. `get_registry(name)` continues to return declared metadata; it does not run analysis.

## Qualification

After implementation changes, first requalify discovery, then fields, then the decode control:

```sh
python3 tools/qualify-registry-discovery.py /path/to/stellaris .local/evidence/sdk-530-discovery-NEW --promote
python3 tools/qualify-registry-fields.py /path/to/stellaris .local/evidence/sdk-530-fields-NEW --promote
python3 tools/qualify-static-analysis.py /path/to/stellaris .local/evidence/sdk-530-decode-NEW --promote
```

Field qualification freezes executable-only Rust results before the preparation tool loads the
verified SDK-487 comparison. It then checks exact parity, omission, clobber and unsupported-path
controls. Promotion also requires ordinary public analysis and replay for all three registries
from a temporary directory containing only the executable. Foreign-context/result handles and a
changed executable are rejected. After these controls pass, promotion stores a final immutable
evidence report that binds their
artifact hashes; temporary admission evidence remains retained separately. A further check reads
the accepted evidence through the public capability and verifies the public-control references.
Failure restores the previous field authority.

The [qualification record](../native/registry-fields-qualification.json) records the exact checks
and their private evidence. Run all workspace CI checks before publishing. Clean CI runs synthetic
controls; private target checks require explicit evidence and are otherwise ignored. Shared source
changes invalidate prior live-session qualification; this operation does not requalify live sessions.
