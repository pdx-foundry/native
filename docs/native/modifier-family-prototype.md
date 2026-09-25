# Modifier families and shared modifier readers

Prototype result, 2026-09-23. Prepared for
[Report which registries generate which modifier families](https://linear.app/unnamed-system/issue/SDK-540/report-which-registries-generate-which-modifier-families)
from [Validate modifier grammars and generated modifier families](https://linear.app/unnamed-system/issue/SDK-498/validate-modifier-grammars-and-generated-modifier-families).

**Generation rules are recoverable for the five traced templates. This does not establish
complete family extraction or a complete shared modifier grammar.** No production API changed.

**2026-09-24:** SDK-498 is closed. SDK-607 owns the remaining shared modifier-block grammar,
and SDK-544 owns numeric conversion limits. SDK-486 owns the archive of the untracked
`.local/sdk-498/` prototype.

**Implemented 2026-09-23:** `Native::modifier_families` (SDK-540) recovers these templates and three
more registries from the executable. See [modifier
families](modifier-families.md#result-on-m45-release). `Game::loaded_modifiers` (SDK-564)
returns the loaded inventory and classifies each entry; see [modifier
families](modifier-families.md#the-loaded-modifier-table). SDK-566 adds the item
post-read code and the shared helpers of item 5 below; see [modifier
families](modifier-families.md#roots).

## Target and experiment

Fresh analysis and two live runs used **M45-release**, Cygnus 4.5.0 (8697), macOS ARM64:

- Executable SHA-256: `07988b4f1b865623becd7a61af1cae92e111be6515d341754af70f02107822cd`.
- ARM64 slice: `a4cb49ad17a84ef6bf438019a50d3a66362c80731f8359888ddbce47c0d0aab9`.
- Native source baseline: `20425ea`. See [target identities](targets.md).

Each run loaded installed content and enabled DLC, plus one additive private mod. No user mod,
save, or world was loaded. The mod added one building, bypass, and district. Run B renamed all
three keys from `sdk498_*_a` to `sdk498_*_b` and changed bypass `windup_time` from 0 to 5.
The manifests record the exact fixture text and installed content inputs; this is not a claim
to have traced every mounted physical file.

Hooks were installed at `_dyld_start`. At five registration call sites they read the item key,
generated name, and category mask. At `CModifier::LogDefinitions` they observed the table count
and the completed documentation string. Both runs had 684 contiguous observations, resolved
hooks, and confirmed game reaping. The executable, installed content, and protected ordinary
profile files were unchanged. The presentation guard reported installation.

## Demonstrated templates

`{key}` means the loaded definition's key, not its file name or display name. Category names
below are the engine's intended-use tags; they do not prove runtime application scopes.

| Registry | Template | Category | Matched in each run |
| --- | --- | --- | --- |
| `common/buildings` | `planet_{key}_build_speed_mult` | Colony | 499 / 499 |
| `common/districts` | `planet_{key}_build_speed_mult` | Colony | 148 / 148 |
| `common/bypass` | `{key}_empire_windup_mult` | Countries | 11 / 11 |
| `common/bypass` | `{key}_ship_windup_mult` | Ships | 11 / 11 |
| `common/bypass` | `{key}_megastructure_bypass_windup_mult` | Megastructures | 11 / 11 |

All **680 / 680** observed registration names and masks matched the templates and final loaded
names/tags in each run. The new keys were included; the old keys disappeared after renaming.
Changing bypass windup time did not change its name rule. In the inspected generator loops,
these five registrations apply to every loaded item without an item-field gate.

This is a rate for those five sites, not for all modifiers belonging to these registries.
Each loaded inventory contained **45,585** names: **571** matched Native's built-in declarations,
**680** matched the traced templates, and **44,334 remained unexplained by this prototype**.
The sets are disjoint. Two new district names, `sdk498_district_*_max_add` and
`sdk498_district_*_max_mult`, are in that unexplained set. Their existence proves that the
district build-speed result is incomplete; name resemblance is not a recovered generation rule.

Five built-in names acquired different loaded category tags, as already found in [declaration
discovery](engine-commands.md#modifiers-categories-scopes-and-links-sdk-536):
`terraforming_cost_mult`, the three `starbase_shipyard_*build_cost_mult` variants, and
`gdf_ship_alloys_cost_mult`. Keep executable declarations distinct from the loaded table after later
registrations.

## How to implement the static question

The production seams are `src/binding/binary/language.rs` (executable inputs),
`src/engine/analysis/modifiers.rs` (method), and the registry naming/ownership methods in
`src/binding/binary/declarations.rs` and `src/engine/analysis/directories.rs`.

1. **Start at registration calls.** The fresh scan found 61 direct calls to
   `TryAddDynamicModifier` and one to `AddDynamicModifier`. These are call sites, not 62 families:
   helper functions serve multiple callers. Also retain the 11 nonliteral-token `AddDefinition`
   gaps reported by the existing method. Counting only the dynamic helpers misses generation.
2. **Join the input to its owner.** The selected database constructors establish the content
   directories. Their generator loops read the database pointer array at `+0x48`, with count at
   `+0x54`. Buildings/districts take the key from item `+0x10`; bypasses use `+0x18`.
   These are private, exact-build findings, not public API fields.
3. **Recover an expression, not nearby literals.** Buildings/districts construct a `CString`
   prefix, append the item `CString`, move the string object, then append a literal suffix.
   Bypasses pass the item's character data to `PdxStrFmt<128>`, then wrap its result in `CString`.
   The key-to-argument and string-move joins were reviewed manually. The prototype's linear
   literal sampler alone is not safe evidence for arbitrary functions.
4. **Keep conditions and bounds.** At `TryAddDynamicModifier`, `x1` holds the name and `[sp]`
   holds the category mask: `0x40000000`, `0x100`, `0x407c`, or `0x10000` here. This differs from
   the direct `AddDefinition` category argument at `[sp+4]`. Bypass formatting calls
   `vsnprintf` with capacity 128: unrestricted concatenation is not equivalent for long keys.
   Results above cover names that fit 127 bytes; the offline demo reports longer names unresolved.
   Long-key engine behavior was not live-tested.
5. **Compose through helpers.** `CDatabaseModifierGenerator<T>::Generate`,
   `CModifierGeneratorBase::GenerateFrom`, and the economic-category matrix helpers need caller
   arguments, item-field conditions, and sometimes more than one registry input.
   `CEconomicCategory::GenerateModifiers` iterates strategic resources and passes category/settings
   fields to `FillModifierMatrix`. This relationship is located, not a qualified extracted rule.
   An unknown condition or helper must produce a gap, not an unconditional template or empty success.

For reproduction, the five release call sites are `0x1000df770` (building), `0x100431bdc`
(district), and `0x1000fefec`, `0x1000ff05c`, `0x1000ff0c4` (bypass).
The final string is available at `CModifier::LogDefinitions()+584`, before
`CLogStream::operator<<(CString const&)`. Resolve functions by symbols and check the instructions;
do not carry these addresses to another build.

A suitable normalized answer contains the generating registry, ordered literal/key parts,
category tags, known conditions and naming bounds, plus explicit gaps. Keep the ordinary
`Answer`/`Source` contract. Native exposes engine relationships; Atlas applies them to project
content and owns config comparison. A small expected result plus an ignored `STELLARIS_PATH`
parity test belongs in the implementation. Do not add capture/replay APIs or evidence descriptors.

## Shared-reader findings and coverage matrix

| Property or family | Evidence and status | Remaining owner |
| --- | --- | --- |
| Five templates above | Demonstrated static mechanism plus two fresh live mutations | Family implementation, SDK-540 |
| Other generation, including district maximums and conditional resource/job families | Precise method limit: caller/helper/condition recovery is absent; 44,334 loaded names unexplained | SDK-540; full loaded inventory SDK-564 |
| Graphical fixed fields | Release disassembly and literal token constructors identify `icon`, `custom_tooltip` → `CString`; `icon_frame` → integer; `show_only_custom_tooltip`, `important`, `hide_from_country_list` → boolean | Shared grammar, SDK-607 |
| Inherited special fields | `CPdxModifier::TryReadMember` handles token 27 `name` through a polymorphic name reader, and token 240 `data` through an integer reader | SDK-607 must qualify the name-reader variants |
| Numeric modifier entries | Release code searches the declaration table, reads `CFixedPoint`, stores the entry, then checks category overlap | Conversion limits remain with SDK-544; grammar/duplicate qualification with SDK-607 |
| Static/scripted modifier references | `CModifier::TryReadMember` tries loaded static modifiers, with immediate-add/deferred paths; `CScriptedModifier::PostReadInit` calls `AddDynamicModifier` | SDK-607 must test valid, absent, forward, and colliding keys through deferred completion |
| Repeated graphical blocks | Historical M45-observe agenda probe: numeric entries reset; omitted tooltip/flag metadata persists | Fresh release and second-use transfer remain SDK-607 |
| Post-read graphical validation | Release `InitPostRead` checks a nonempty icon: `GFX_` prefix bypasses the file check; other strings call `VFSExists` and may log a missing-icon diagnostic | SDK-607; sprite/localisation asset inventories are project/external inputs |
| Localisation and whole shared grammar | A `CString` read does not prove localisation-key existence. The old root walker returned 13 gaps here: unsupported load writeback or indirect call | SDK-607; no complete grammar claim |
| Runtime effect and propagation | Untested by startup registration or parser storage | SDK-497 / SDK-547 |
| Duplicate warnings, severity, recommended syntax | Consumer policy, separate from engine storage and diagnostics | Atlas/consumer |

Historical evidence is `atlas-discovery` bundle,
`prototype/council-agenda-reconstruction/review.md`, and corrected run `20260917-023136`:
21 agenda definitions and 207 ordered events on **M45-observe**, not this release. It already
shows that a blanket “last block wins” rule is wrong. No new live graphical-reader or second
modifier-block-use test was performed here. That part of the bounded experiment remains open.

## Controls, comparison, and maintenance cost

The same sampler handles the building/district concatenation and bypass formatting shapes.
Templates were fixed before the two live runs. All three functions were inspected beforehand;
this is unchanged-method transfer, not a blind held-out discovery test.

The offline consumer applies the rules to `future_mod_key` without starting a game. A corrupted
template fails comparison; missing/unresolved rules and overlong bypass output remain unresolved;
an unmounted key has no loaded declaration. Partial rules never become a complete registry claim.

Only after freezing the engine results was config compared, at commit
`85747602a614ad7daa8cc66453777ecb023463a8`. `config/modifiers.cwt:728–737` agrees with the five
templates/tags. `config/aliases.cwt` also asks about `description` and
`description_parameters`; this probe does not establish their reader contract. Config supplied
no extraction answers, and no config is read by the native probe.

Reusable work: literal sampling, call-site inventory, observation comparison and offline rule
application. Target adaptation: exact hash gate, symbol resolution, calling convention, string
and object layouts, and the five observation points. Manual interpretation: key/owner joins,
string moves, branch gates and formatter bounds. The prototype is roughly 500 lines plus reused
historical helpers; it is not evidence that extraction maintenance is automatic or cheap.

## Local reproduction

Untracked source and results are under `.local/sdk-498/`; no prototype archive was uploaded.
`analyze.py` re-reads the pinned executable. `live.py` creates the isolated fixtures, owns and
reaps the game, and writes per-run observations. `verify.py` checks the observations and runs
the offline demo. `static.rs` obtains the built-in baseline through Native's public API.
The reused helper source is retained under `historical/`; see [retrieval](retrieval.md) for its
original bundle. These scripts use Python's standard library, Xcode command-line tools and the
installed game; the unused local Python environment is not required.

```sh
python3 .local/sdk-498/analyze.py
python3 .local/sdk-498/live.py       # two game launches; use run-and-queue
python3 .local/sdk-498/verify.py     # no game; includes offline consumer demo
```

Fresh runs: `run_a_20260922_234435`, `run_b_20260922_234505` (local time).
`results.json` contains counts, controls, category differences and demo output;
`generation-sites.json` contains the unresolved-site starting inventory.
Re-run engine questions for verification. These development files are not a supported replay path.
