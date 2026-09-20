# Registry discovery

SDK-528 provides `Native::analysis()?.discover_registries()` without config seeds, a supplied
name list, content files, debugger access, or a game launch. Admission uses
`CapabilityRequest::RegistryDiscovery`, independently of `StaticDecode` and live registry reads.
Only the exact M45-observe executable and ARM64 slice have a qualified recipe.

The operation enumerates exact template loader symbols and reconstructs a bounded startup table
from raw instructions, literal strings and Mach-O chained fixups. `object` reads the image;
`cpp_demangle =0.5.1` normalizes C++ names. The target adaptation supports offset-format 64-bit
chained pointers and addend64 imports. Same-image weak bindings remain static candidates:
replay of a live table can confirm a historical resolution, but cannot establish current interposition.
Unknown calls invalidate volatile register values. Unsupported instructions, missing pointers and
clobbered or missing table owners produce explicit gaps.

Candidates, scheduling witnesses and relationships are separate. Each has a stated basis and
hashed evidence references. Native addresses and class names stay in retained input artifacts.
`RegistrySubject` handles have private construction and cannot be deserialized. A result accepts
only its own handles (and their clones); another result rejects them even on the same executable.
`relationships_for(handle)` resolves candidate, loader and root-owner relationships; loader and owner handles preserve observed object reuse only within the same retained run. Handles are not stable rule identities. Serialized ordinals are local to their containing result.

Static discovery returns 164 candidates and 198 scheduling witnesses on the retained target;
35 witnesses are outside the template method. It reports every candidate as unobserved until
historical trace evidence is supplied through replay. These counts do not establish all registries.
`get_registry(name)` remains the existing declared-metadata query. Reader and field schemas are
still unknown where no separate method qualifies them.

## Retain and replay

```sh
cargo run --locked --release --example discover-registries -- /path/to/stellaris .local/evidence/discovery-NEW
cargo run --locked --release --example replay-discovery -- .local/evidence/discovery-NEW .local/evidence/discovery-NEW/descriptor.ref.json
```

The static result exposes `input_bytes()` for retaining its input artifact at the descriptor's
archive-relative path. Descriptors contain input references, not extracted answers. The separate
evidence crate verifies each artifact and runs the same bounded reducer without an installation.
Unsupported contracts, damaged bytes, missing artifacts and unsafe paths fail explicitly.

The accepted SDK-489 capsule can supply historical ownership evidence. The preparation tool verifies
the archive and restored capsule before copying the two retained traces, tables, result records and
manifests into the new output. It does not run a capture script or launch a game.

```sh
python3 tools/prepare-registry-discovery.py .local/evidence/discovery-NEW
cargo run --locked --release --example replay-discovery -- .local/evidence/discovery-NEW .local/evidence/discovery-NEW/historical.ref.json
```

Historical replay preserves the original exact target and content boundary. It validates activation,
sequence and completion, compares static and observed scheduler slots, and joins root keys with
loader files, receiver directories, direct enumeration callers, concrete vtables, offset-to-top and
member-dispatch pointers. The accepted custom static-modifier observations retain their two-phase
boundary. Incidental reader occurrences and incomplete joins remain gaps.

The retained result has 162 observed template candidates and two unobserved candidates. Six
AI economic-plan roots are established among 249 reader occurrences. Replay never upgrades these
historical observations into current live ownership. Custom, nested, late and shared-reader search
limits remain explicit; item enumeration, field schemas and cross-build correspondence are separate work.

## Qualification

```sh
python3 tools/qualify-registry-discovery.py /path/to/stellaris .local/evidence/sdk-528-qualification-NEW --promote
python3 tools/qualify-static-analysis.py /path/to/stellaris .local/evidence/sdk-527-requalification-NEW --promote
```

The first command compares every candidate and scheduler slot against retained evidence and runs
41 Rust controls, including capsule/freeze integrity and the held-out ownership transfer. It promotes
only the exact operation composition, then checks ordinary public discovery and replay; failed public
checks restore the previous authority. The second requalifies the decode control after shared changes.
Qualification records do not change their own implementation fingerprint. Shared changes do invalidate
older live-session qualification; this task does not requalify or launch live sessions.

Run the full workspace CI checks before publishing. Synthetic omission, wrong-owner, foreign-handle,
artifact-failure and replay controls run on clean macOS, Linux and Windows CI hosts. Private tests are
explicitly ignored unless the exact executable and prepared evidence directory are provided.
