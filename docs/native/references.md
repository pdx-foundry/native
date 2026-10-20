# References and dynamic names

A reference is a field or command argument whose value the engine looks up by key in a loaded
collection. A dynamic name is a name that script both defines and reads, such as a country flag.

`Field.reference` in `Native::registry_fields` and in the fixed keys of `Native::command_grammar`
gives each lookup of a field's value: the target
registry by content directory, the stage, the key match, whether an empty key is looked up, and
what a missing key yields. The method is `engine/analysis/references.rs`, with owner
initializers in `references/initialization.rs` and the shapes in `references/shapes/`;
`binding/binary/references.rs` reads the functions and joins each database to its directory. `cargo run --release --example inspect -- --lookup-lines NAME` prints a function
as the canonical lines that the shapes match; use it to author a shape from a new body.

## Engine facts on M45-release

All facts below are for the exact executable in [targets](targets.md).

### Reference readers

- `NParserUtil::ReadKeyReferenceDeferred<D>(CGlobalDeferredDatabaseObject const&, CReader&,
  D::ValueType const**)` has 86 instantiations. Each reads the token text at `CReader+0x288`, then
  calls `CGlobalDeferredDatabaseObjectResolver::Register` with the key, the file location and a
  `std::function` lambda that captures the destination.
- `Register_Internal` appends the entry to a pending array while the resolver's byte at `+0x18` is
  set; otherwise it resolves at once. `Run()` clears that byte and resolves every pending entry.
  `Run()` has one caller, `NNullObjAndDatabaseInitUtil::SetupDatabases`, after the loop that
  initializes every database. `ResolveReference` calls the lambda and, when it returns false, logs
  `Failed to deferred read key reference %s from database %s` with the key and location.
- On M452, `Register_Internal` (`0x1025b2a90`) inserts at the current count and compares no
  earlier entry, and `Run()` (`0x1025b2f20`) resolves the entries in array order. Two
  registrations for one destination both resolve, and the later one writes last; this is the
  `Replace` rule of [repeat behavior](registry-fields.md#repeat-behavior). Read both functions
  again on a new build.
- `NParserUtil::ReadKeyReference<D>(CReader&, D const&, bool)` has 87 instantiations. It looks the
  token up at read time, returns the item or the typed null object, and logs
  `Failed to read key reference '%s' from database '%s' at '%s'` for an invalid result unless the
  `bool` argument is true.
- `ReadKeyReferenceUniform<D, CPdxArray<…>>` has 13 instantiations,
  `ReadKeyReferenceDeferredUniform<CScriptedActionDatabase>` and
  `ReadIndexReferenceDeferred<CTechnologyTierDatabase>` one each. The [list
  readers](#list-readers) section gives their lookups.
- `CDatabaseObjectEffect<D>::Read` (45) and `CDatabaseObjectTrigger<D>::Read` (75) tail-call the
  deferred reader with the command object as the owner.

### Lookup shapes

A read-only census grouped function bodies after renaming registers by first use and replacing
addresses, template arguments and, for initializers, field offsets:

| Population | Functions | Shapes | Largest groups |
| --- | ---: | ---: | --- |
| `ReadKeyReference<D>` | 87 | 7 | 77 linear scans; 5 out-of-line `CPdxRobinHoodTable::Find`; 5 singletons |
| `ReadKeyReferenceDeferred<D>` | 86 | 1 | all register with the resolver |
| Deferred lambda `__func::operator()` | 88 | 11 | 73 forward to `__invoke_void_return_wrapper::__call`; 6 inline map finds; 9 singletons |
| Forwarded `__call` bodies | 74 | 5 | 70 linear scans |
| `CPdxRobinHoodTable<CString, …>::Find` | 37 | 10 | 20 share one shape |
| `{Owner}::PostInit()` with a database lookup | 151 of 309 | 59 | 38, 9, 8 and 8 linear scans; 6 and 4 getter wrappers; 3 inline map finds |

- **Linear scan.** Load `TGameDatabase<D>::_pInstance`; iterate its item array (`+0x48`, count
  `+0x54`); compare the item key length, then its bytes with an inline loop for short strings or
  `memcmp` for long ones; the first equal item wins; an empty collection or no match selects
  `TPdxNullObject<C>::_pInstance`. The comparison is byte for byte: no case folding.
- **Map find.** Call `CPdxRobinHoodTable<…>::Find(CString const&)` on `TGameDatabase<D>::_pInstance`
  plus `0x58`; a returned iterator whose state byte is `0xff` selects the null object. The 20-body
  `Find` shape hashes with `_PMurHash32` and then compares length and bytes. Its signature names
  `SPdxHash<CString>` and `std::equal_to<CString>`.
- **Getter.** The static modifier lambda calls
  `CStaticModifierDatabase::GetStaticModifier(CString const&) const` on
  `CStaticModifierDatabase::_pInstance`, stores the result and tail-calls the item's validity
  check, vtable slot `0x98`. The getter scans its own items (`this+0x3d0`, count `this+0x3dc`, key
  at `item+0x158`) with the same comparison and returns the null object on a miss.
  `CPlanetClassDatabase::GetPlanetClass(CString const&)` wraps a `CHashTable::Find` and substitutes
  the null object for a null result. The lambdas of traits (`GetTrait`), planet classes,
  strategic resources (`GetResource`) and special projects (`FindProjectType`) have the same
  form with the validity check at slot `0x40` or `0x48`; the slot only selects the resolver's log.
- **Immediate variants.** `ReadKeyReference<CBypassTypeDatabase>` is the immediate scan over items
  whose key string is at `item+0x18` and whose validity check is called through the base at
  `item+0x8`. `ReadKeyReference<CPlanetClassDatabase>` calls `GetPlanetClass` on the database
  argument and returns its result. In both, as in every immediate reader, the validity check only
  selects the log; the returned value is the item or the null object.

### List readers

- **Immediate list.** `ReadKeyReferenceUniform<D, CPdxArray<C const*>>(CReader&, D const&,
  CPdxArray<C const*>&)` reads every token of a block (`CLexer::Tok()`; a token that starts with `@`
  first goes through `CReader::ParseAdvancedStatement`). For each it searches the database
  argument: an inline linear scan (11 instantiations) or `Find<CString>` on the map at `+0x58`
  (ship sizes and star classes). A miss selects `TPdxNullObject<C>::_pInstance`. The reader then
  calls the selected object's vtable slot `0x10`, its validity check: true appends the object with
  `InsertAtEmplace` at the array's count, false logs `Failed to read key reference '%s' from
  database '%s' at '%s'` and appends nothing. An empty key is compared like any other.
- **Validity.** On M452 slot `0x10` of each of the 13 item classes returns the constant 1
  (`CDatabaseObjectBase::IsValid()` or the class's own override), and slot `0x10` of
  `vtable for TPdxNullObject<C>` is `TPdxNullObject<C>::IsValid() const`, which returns 0. So a
  found item is appended and a key that selects nothing is omitted (`Missing::Omitted`). The
  method reads both checks from the vtables; a check of another body leaves the lookup unresolved
  (`reference-list-validity`).
- **Stated rule: the null object's class.** The object at `TPdxNullObject<C>::_pInstance` is an
  instance of `TPdxNullObject<C>`, so its validity check is that class's slot. Checked on M452 in
  `TPdxNullObject<CJobTag>`, `<CEthic>` and `<CShipSize>::Initialize()`: each constructs the
  object in `_Memory` and stores the address point of `vtable for TPdxNullObject<C>` in it.
- **Five bodies.** The 13 instantiations are five canonical bodies once their type names are
  placeholders: one scan for DistrictZoneSlot, JobTag, OverclockType, ScriptedAction and TraitTag;
  one in another frame for ColonyAutomationCategory, CountryFocusCard, ShipCategory,
  SpeciesArchetype and SpeciesClass; one for Ethic; one map search for ShipSize and one for
  StarClass. The differences are instruction order and frame size, so each is its own shape
  (`list_scan`, `list_scan_frame`, `list_scan_ethic`, `list_map`, `list_map_star`). The method
  also checks that the append is `InsertAtEmplace` of `C const*` with `C` the null object's type,
  and that the log names `D`.
- **Deferred list.** `ReadKeyReferenceDeferredUniform<CScriptedActionDatabase>` appends the null
  object for each token, then registers the token with a lambda that captures the array and the
  new element's index. The lambda forwards to the linear scan of a forwarded deferred lambda,
  whose result it stores in that element. So a key that selects nothing keeps the null object in
  its place (`Missing::NullObject`).
- **Index.** `ReadIndexReferenceDeferred<CTechnologyTierDatabase>` reads an index, not a key; the
  method does not read it (`reference-index-form`). No registry field or command key reads it.

### Effect initializers

A command's `{Receiver}::PostInit()` runs after the command is read. The receiver's vtable holds
it at the address point plus `0x90` for effects and `0x70` for triggers; the binding recipe
records both slots. The executable has 309 `PostInit()` functions. 155 name no global instance.
154 name one or more: 151 a database, and 3 the event manager (`CEventManager::_pInstance`).

| Effect | Key storage | Lookup in `PostInit()` | Miss |
| --- | --- | --- | --- |
| `create_ship` | `CReader::Read(CString&, bool)` into `+0x600` | inline map find in `TGameDatabase<CShipSizeDatabase>`; skipped for an empty key | null object at `+0x120`; `PostValidate` logs |
| `add_district` | `CReader::Read(CString&, bool)` into `+0xa8` | linear scan of `TGameDatabase<CDistrictTypeDatabase>` | null object at `+0xd0`; `PostValidate` logs |
| `add_relic` | token text copied inline (`strlen`, no reader call) into `+0xa8` | linear scan of `TGameDatabase<CRelicsDatabase>` | null object at `+0xd0`; `PostValidate` logs |
| `change_pc` | token text at `+0x238` | `CPlanetClassDatabase::GetPlanetClass` | null object at `+0x260`; `PostValidate` also accepts random planet lists and scope keywords |
| `create_army` | `ReadKeyReference<CArmyTypeDatabase>` in `CCreateOrModifyArmyParentEffect::ReadMember`, at read time | none | the reader's null object and log |

The initializer shapes cover these groups. Offsets differ between owners, so each shape binds
the key string, its length and flag byte (`+0x8`, `+0x17`), the item key layout and the output:

| Shape | Initializers | Lookup |
| --- | ---: | --- |
| `initializer_scan` | 38 | linear scan of `TGameDatabase<D>`; first equal key; empty key looked up |
| `initializer_scan_nonempty` | 5 | returns for an empty key, then the same scan |
| `initializer_map_nonempty` | 3 | skips an empty key, then `Find` on the map at `+0x58` |
| `initializer_map` | 2 | `Find` on the map at `+0x58`, empty key included |
| `initializer_getter` | 20 | `D::getter(CString const&) const` on `D::_pInstance` |
| `initializer_scan_valid` | 8 | the scan, then the selected object's validity check (slot `0x10`): `item+0x38` when it passes, a null pointer otherwise |
| `initializer_scan_valid_flag` | 8 | the same scan and store; when the check fails, also stores whether `StringToToken` of the key is token `0x3ff8` |
| `initializer_getter_scan_nonempty` | 4 | two lookups: a getter call, then a scan of another database that returns early for an empty key |

The getter's own body gives the lookup. `GetResource` and `FindProjectType` (10 initializers)
scan their own items (`getter_scan`). `GetModifier` and `GetPlanetClass` (5) call
`CHashTable<CString, E, …>::Find` and substitute `TPdxNullObject<E>` for a null result
(`null_getter`); `Find` hashes with `_PMurHash32`, walks the bucket chain and compares length
and bytes (`hash_find`), so the key match is `Equal`. The null object's type must equal the
element type that the map or hash table states. A scan does not state its element type.
`COnActionDatabase::GetOnActionList` tail-calls a qualified `Find` on its own table and returns
its result, so a miss stores a null pointer (`hash_getter`). `CEventManager::GetEvent`
(`event_getter`) calls `GetIDInNameSpace` on the key, splits the id into a namespace index
(`id / 100000`) and an event index (`id % 100000`), and returns the event pointer or null; an id
below `-100000` returns null. It never compares the key's bytes.

The two validity scans are the species-rights commands (`CSetSpeciesRightsEffect<…>`,
`CHasSpeciesRightsTrigger<…>`). The method requires the scanned item's check to return 1 and
the null object's to return 0, read from their vtables as for [list readers](#list-readers), so a
found item is stored and a miss stores a null pointer. The resource comparisons
(`CResourceCompareTrigger<…>`) call `CStrategicResourceDatabase::GetResource` for `resource` and
scan `TGameDatabase<CEconomicCategoryDatabase>` for a nonempty `category`; an empty `category`
leaves its item unwritten.

`CCreateArmyEffect::PostInit()` walks the `CEventTarget` chain at `this+0x650` and classifies each
link's keyword token (`+0x58`) through compare trees and two `ldrh` jump tables. It names no
database, so the method finds no lookup in it; the SDK-482 prototype's unknown result for it was
not a failed reference shape. Its keyword classification is scope work (SDK-565, SDK-549).

### Trigger lookup

`CTriggerDatabase::CreateTriggerOrScriptedPlaceholder(CToken const&, CString const&) const` makes the
trigger that a token names; the weight `complex_trigger_modifier` reads its `trigger` key with it.
It is not an `NParserUtil` reader and takes no `CReader`, so `references::reader` does not name it:
that function feeds the reader joins of the field and command grammar walkers. The reference
method reads it as its own fact, `ReferenceFacts.trigger_lookup`, from two shapes. On M452:

- `trigger_token`: the overload makes `CTriggerDatabase::_pTriggerDatabase` when it is null, builds
  a `CString` from the token text (`token+0x10`) and calls the `(int, CString const&, CString
  const&)` overload with the token number (`ldr w,[token]`).
- `trigger_map`: that overload searches the `std::map` at database `+0x68` (node key `+0x20`) for
  the number and calls the factory at node `+0x28` through vtable slot `+0x10`. An empty map, a
  missing number or a null result constructs `CScriptedTrigger(CString const&)` with the name; no
  branch tests the name for emptiness. Either object then stores the number (`+0x30`) and the
  location (`+0x38`).

So the lookup runs `WhileReading`, looks up an empty key, and yields
`MissingResult::ScriptedTriggerPlaceholder` on a miss; nothing checks that a scripted trigger has
the name. The declarations method registers trigger commands by the same token number
(`RegisterTriggerEntry(int, …)`), so the target is `ReferenceTarget::Triggers`.

The key match stays `Unresolved` with an `OutsideMethod` gap (Jackson, 2026-10-09). The map holds
token numbers, and the lexer assigns them: `CTernary<int*, STernaryTrait<int*> >::Get(char const*)
const` (`0x1025b02a0`) calls `tolower` on each input character before it compares, and
`CLexer::FindTok` returns `0xc` for text with no literal token. So `HAS_TECHNOLOGY` most likely
selects `has_technology`, which the byte-for-byte contract of `KeyMatch` cannot state. A live
control of an upper-case trigger name would establish the rule; Atlas does not need it now.

### Database directories

Template databases name their directory in the `CSingleObjectGameDatabaseBase(CString const&)`
constructor argument; `registries()` already joins them. A custom loader names the directory that
its own functions pass to
`VFSGetEnumeratedFiles(char const*, CPdxArray<CString, int>&, char const*, char const*, int)`:
`CStaticModifierDatabase::Init(bool)` passes the literal `"common/static_modifiers"`, and
`CPlanetClassDatabase::Init()` loads `"common/planet_classes"` from the global
`_PLANET_CLASSES_DIR` ([Custom-loader names](registry-fields.md#custom-loader-names)).

## Result on M45-release

On M452 the executable has 188 reference readers. 181 establish every lookup fact and a content
directory: 82 of 86 deferred readers, 85 of 87 immediate readers, the 13 immediate lists and the
deferred list (SDK-635). The other 7 keep typed gaps:

| Obstacle | Readers | Databases |
| --- | ---: | --- |
| Deferred lambda of another shape (`reference-lambda-shape`) | 4 | ambient objects (calls `GetInstance()` first), bypass types (a forwarded scan with other item offsets), galaxy templates (loads `TGameDatabase<…>::_pInstance` with `ldr`), leader traits (`CTraitDatabase::GetLeaderTrait`, a getter of another database) |
| Immediate getter reader of another shape (`reference-reader-shape`) | 2 | gfx cultures (`GetCulture`), name lists (`GetNameList`) |
| Index, not a key (`reference-index-form`) | 1 | technology tiers |

Galaxy templates, leader traits, gfx cultures and name lists also have no content directory
(SDK-551 owns the directory join). No registry field reads any of the seven. Command keys and
command values that read them keep their gaps; Atlas reads only a lookup's target, so a missing
directory is what would block it.

The registry field sweep (`tests/population/m452/registry-sweep`) counts 345 root fields whose
reference relationship applies: **36 complete, 0 partial, 309 undetermined** (the reader kind is
not established). The 11 fields that SDK-635 completed are the list fields `pop_jobs#tags`,
`traits#tags`, `traits#species_class`, `traits#allowed_archetypes`,
`species_classes#ethics_to_prefer`, `megastructures#overclock_types`,
`star_classes/randomizers#stars` and `scripted_action` in megastructures and ship sizes, and the
immediate readers of `megastructures#bypass_type` and `ship_sizes#carries_colony`.
`council_agendas#finish_modifier` is `common/static_modifiers`, deferred, first equal key, empty
key looked up, null object on a miss; `pop_jobs#tags` is `common/job_tags`, while reading, first
equal key, empty key looked up, omitted on a miss.

### Owner initializers

On M452 the method analyzes every `PostInit()` in the executable. Of the 154 that name a global
instance, **85 establish their lookups (67 with every fact, 18 partial), 69 failed**:

| Result | Initializers | Shape |
| --- | ---: | --- |
| Complete | 67 | the shapes above, with a content directory and every lookup fact |
| Partial | 18 | the event and on-action getters and the 16 species-rights scans: the target and key match are known where the shape states them; the miss stores a null pointer, which `MissingResult` cannot name, and the event id match is not a key match |
| Failed | 58 | another shape (`initializer-shape`); see [Gaps](#gaps) |
| Failed | 10 | the initializer holds several lookups of other shapes (`CCreateCountry`, `CCreateSpecies`, `CFreeJobsOfType` …) |
| Failed | 1 | `CTraitDatabase::GetLeaderTrait`, a getter of another shape |

`command-population` joins the initializers to the commands. A lookup joins a child key when
a read alternative of the key stores its string at the lookup's key offset with
`CReader::Read(CString&, bool)` as the path's last write: a tail call, or a call followed only by
register and frame restores and a tail call to a function whose body is only `ret` (the event
effects' `id` returns through `CEventManager::AddFiredEvent(CString const&)`, which is empty on
M452). Any other continuation could overwrite the key before `PostInit()`. The key's
`Field.reference` then holds one lookup with stage `OwnerInitialization` for each such
alternative and each lookup at that offset, with the alternative's condition. On M45-release
every joined alternative is unconditional.

| Commands | Effects (1,080) | Triggers (1,098) |
| --- | ---: | ---: |
| Lookup joined to a child key, complete | 37 | 17 |
| Lookup joined to a child key, partial | 29 | 16 |
| Initialization lookup without an authored field | 11 | 4 |
| Initializer with no lookup | 968 | 1,029 |
| Initializer lookup not established | 33 | 32 |
| Receiver join failed | 2 | 0 |

Counts are from `command-population` at `command-grammar/v22` (`inventories[].initialization_lookups`).

Joined effects include `create_ship` (`random_existing_design`, `common/ship_sizes`, `Equal`,
empty key not looked up), `add_district` (`district_type`, `common/districts`, `FirstEqual`) and
`spawn_megastructure`. In the 41 commands without an authored field, no joined
`CReader::Read(CString&, bool)` stores the key: the child key's path stops at `reader-routing`
(`add_relic` copies the token text inline after `strlen`), or the key is the command's assigned
value, which has no child key (`add_tradition`, `remove_relic`, `set_pre_ftl_age`).
`change_pc`'s receiver joins and its initializer looks up a stored key, but no child key's
string reader stores that key.
`create_army`'s `type` is `common/armies`, `WhileReading`, `FirstEqual`: the grammar reaches
`CCreateOrModifyArmyParentEffect::ReadMember` and its `ReadKeyReference<CArmyTypeDatabase>`.

## Gaps

- Partial lookups (SDK-827). The answer gives the target and stage, and keeps the rest
  `Unresolved` with the reason in the key's gap: the event effects' `id` is selected by the id
  that `GetIDInNameSpace` parses, not by its bytes; the event, on-action and species-rights
  lookups store a null pointer on a miss; the species-rights triggers also flag token `0x3ff8`.
  `MissingResult` has no null-pointer value, and Atlas reads only the target.
- Initializers of another shape, several lookups or another getter (the 69 above). Atlas reads
  a command key's lookup; on M452 (SDK-798 recording, cwtools `config/`) 30 config `value_form`
  claims still fall on keys of these initializers, out of the 100 that SDK-827 counted. Each
  obstacle, with its claims:
  - a known lookup followed by more code that the whole-function shapes do not allow:
    `add_casus_belli` (a scan, then `CIntVariableValue::IsSet`, 1), `set_resource` and
    `ship_size_cost_resource_percent` (`GetResource`, then a validity check that logs, 2);
  - a getter call guarded by a nonempty key and split across the body: `has_resource` (1);
  - a getter called on `TGameDatabase<D>::_pInstance` loaded with `ldr`: `spawn_system`
    (`CSystemInitializerDataBase::FindInitializer`, 2);
  - a lookup in the found item, not in a registry: `set_policy`, `can_set_policy` (a policy scan,
    then `CPolicyGroup::GetOption` on the found group for `option`, 4);
  - a scan, then a tail call to the base class's initializer: `enable_patron_first_contact_project`
    (`CEnableSpecialProjectEffect::PostInit()` looks up `name`, 2);
  - two scans of one database for two keys: `pop_force_transfer_ethic`, `replace_patron` (4);
  - a getter with further code: `change_species_characteristics`, `set_habitability_trait`
    (`GetTrait`, 3);
  - several lookups of different shapes: `create_starbase` (4), `create_species` (1),
    `free_jobs_of_type` and `is_job_of_pop_category` (4), `check_economic_production_modifier_for_job` (1);
  - `GetLeaderTrait`, which searches the trait array of each entry of a hash table: `add_timed_trait` (1).

  Each needs its own whole-function shape, or a shape that matches one lookup inside a larger
  body; the second would cover most of them.
- Registry objects: no registry item class on M45-release has a `PostInit()` that names a
  database, so registry fields have no initialization lookups.
- The seven readers in the table above.
- Whether a missing key logs: the log depends on the selected object's validity check. The
  method reads that check only for list readers, where it decides whether the item is kept. The
  page records the messages; the answer does not claim them.
- Duplicate definitions in a map-backed database: SDK-552 owns load-time replacement.

## Pitfalls

- A list reader's miss is not stored. The immediate list appends only objects whose validity
  check passes, and the null object's check fails; read both checks before claiming what a list
  holds.
- An immediate or deferred list reads a block of keys (`allowed_archetypes = { BIOLOGICAL LITHOID }`), not one
  scalar: the field's shape is `Block` with `Accumulate`, and its members stay `Unresolved` until
  a members state carries the item reader (as for `ReadUniform` lists).

- A reference reader's signature does not state its lookup: `ReadKeyReference<CArmyTypeDatabase>`
  scans linearly while `ReadKeyReference<CShipSizeDatabase>` calls the map `Find`.
- Registration with the deferred resolver does not by itself say when the lookup runs, and a
  loaded null object does not by itself say that a miss selects it.
- A miss selects `TPdxNullObject<C>::_pInstance`, but the scan shapes bind `C` without joining it
  to the target database's item type. `MissingResult::NullObject` claims a typed placeholder, not
  its class.
- A search by token number is not a byte compare. The trigger map's key is the number that the
  lexer assigned to the name, so its case and normalization rules are the lexer's.
- `DeclaredScopes` states where a command may run, not which store it writes; flag stores are
  compared per scope.
- A database or null-object class in an initializer is not a lookup. The SDK-482 controls kept
  as authored tests show that a changed owner register, clobbered store, retargeted branch,
  inverted null selection, `memcmp` changed to `strlen`, or changed getter body each removes the
  lookup; mismatched string offsets and a null object of another type reject it; and a field
  joins only when its string reader stores at the lookup's key offset.
- A shared initializer can serve many commands: `CFireEventEffect::PostInit()` calls
  `CEventManager::GetEvent`, and its result appears on 20 event-firing effects.
- A string read that is not a tail call can still be the key's last write. The event effects'
  `id` read returns through the empty `AddFiredEvent(CString const&)`. A `CString const&`
  signature alone proves nothing: a member function's `this` is mutable.
- A validity check decides what an initializer stores. The species-rights scans select the null
  object on a miss, but store a null pointer because its check fails.

## Identifier grammar

What the executable states about a key, and where the method stops. Each row applies to the
lookups and flags on this page.

| Property | Result on M45-release | Evidence or boundary |
| --- | --- | --- |
| Case | Byte for byte, no case folding; the trigger lookup is not established | The scan shapes compare length, then bytes or `memcmp`; the qualified `Find<CString>` hashes with `_PMurHash32`, then compares length and bytes. The trigger lookup compares lexer token numbers, and the lexer's token lookup folds ASCII case ([trigger lookup](#trigger-lookup)) |
| Encoding | Bytes; the lookup decodes nothing | The lexer's encoding is outside the method |
| Quoting | Readers take the lexed token text at `CReader+0x288` | Quote handling is a lexer fact: on M452 the [text lexer](text-lexer.md) removes the quotes and keeps the bytes between them. The SDK-482 Intel spike saw quoted and unquoted `corvette` select one ship size |
| Length | The whole key is compared; nothing truncates it | No maximum length is established |
| Namespaces | One registry per lookup; flags intern in one table for every flag kind and are stored per scope object or in the global store | `ReferenceTarget`; `DynamicNamespace` |
| Normalization | None in a lookup or in `CreateFlagIndex` | Lexer normalization is outside the method, including the token number that the trigger lookup compares |
| Collisions | A scan returns the first equal item; equal flag names are one flag in every store | `FirstEqual`; the shared interner |
| Missing key | The typed null object; a scripted-trigger placeholder for the trigger lookup | `MissingResult::NullObject` or `ScriptedTriggerPlaceholder`, from the shape's miss edge |
| Duplicate definitions | A scan returns the first in collection order; a map keeps what loading inserted | A template registry holds one item per key: the [duplicate rule](loader-rules.md#duplicate-definitions) replaces or merges it in place |

## SDK-482 prototype

The first reference method matched four whole-function templates on the 4.5 beta and was retired.
Its sources are in the `typed-extraction` bundle under `reference-observation-prototype/` and the
`source-git` bundle ([retrieval](retrieval.md)); the Rust port is
`git show 1da4abf^:src/engine/analysis/references.rs`. Its 27 controls are authored tests
(`control_01_…` to `control_27_…`). Pitfall: the "two unfamiliar resolver shapes" it failed on were a
linear scan (District) and an event-target chain (Army), which a contiguous map-find matcher
cannot read.

## Dynamic names

`Native::dynamic_names` (`dynamic-names/v3`) groups the effects and triggers that define, remove
and read integer flags by the store that each reaches. The method is
`engine/analysis/dynamic_names.rs`, with store routes in `dynamic_names/routes.rs` and the read
shape in `dynamic_names/membership_scan.shape`. `binding/binary/dynamic_names.rs` locates the
flag functions by signature; the recipe holds each family's assign and role slots. Run it over
every command with:

```sh
cargo run --release --example dynamic-name-population -- "$STELLARIS_PATH"
```

### Flag stores on M45-release

- `CFlagEffect::Assign` and `CFlagTrigger::Assign` call `ReadAsDynamicFlag(CString const&,
  CString&, CEventTarget&, EScopeType, CString const&)`, which splits `name@target` at `@`. For a
  static name the caller stores `CPdxIntegerFlags::CreateFlagIndex(CString)` as a 16-bit index:
  the effect at `+0xa8`, the trigger at `+0x1f8`. `CSetTimedFlagEffect::ReadMember` does the same
  and stores at `+0x2b4`.
- `CreateFlagIndex` interns into one global `CPdxIntegerFlags::_AllFlags` map, keyed by the exact
  string, for every flag kind and for saved event targets. Variables use `CVariables`, a
  string-keyed map that does not intern names.
- Setter `AccessFlags` implementations forward to `CEventScope::AccessFlags()`, which tail-calls
  `CEventScope::GetFlags() const`; `CHasFlagTrigger::GetFlags` forwards to the same function.
  Global flag commands return `_g_CurrentGameState + 0x478` in both roles.
  `CHasFlagTrigger::ActualEvaluate` reads the store through its virtual getter, then scans the
  16-bit indexes inline.
- Scope sets differ between roles: `CSetStarFlagEffect::GetSupportedScopes` returns `0x80`,
  `CHasStarFlagTrigger::GetSupportedScopes` returns `0x08000080`.
- Slots from a command's vtable address point: effects `Assign` `+0x20`, `ExecuteActual`
  `+0x50`, `AccessFlags` `+0xe8`; triggers `ActualEvaluate` `+0x20`, `Assign` `+0x28`,
  `GetFlags` `+0xf8`.
- `ReadAsDynamicFlag` writes the name to its `x1` destination and the target to its `x2`
  destination, and returns true for `name@target`; the caller then returns without interning.
  At run time a kept name selects `CreateDynamicFlag(scope, target, name, …)` in setters and
  removers, and `GetDynamicFlag(scope, target, name, …, true)` in readers, instead of the stored
  index.
- Setters tail-call `CPdxIntegerFlags::SetFlag(CIntFlag<unsigned short>, CDate const&, int,
  ESetFlagMode)` on the store that the accessor returns; removers tail-call `ClearFlag`.
- `CEventScope::GetFlags() const` switches on the scope object's type field (`+0x8`, the type's
  mask bit). Each case resolves the scope's object, a cached pointer or a `TPdxRef` database
  lookup with the null object as fallback, and adds that object's flags offset (galactic object
  `+0x5f0`, leader `+0x420`). A type without a case returns `_g_CurrentGameState + 0x478`, the
  global store.
- `CHasStarFlagTrigger::GetFlags` has its own getter: for type `0x8000000` (`dlc_recommendation`)
  `CScopeObjectReference::GetDlcRecommendation() + 0x20`; otherwise
  `CScopeObjectReference::GetGalacticObject() + 0x5f0`, logging when the object is invalid.
- 206 trigger create methods `CTriggerEntry<T>::Create()` are one `b` to an out-of-line
  `NTrigger::Create<T>`, which allocates the command, runs its constructors and installs its
  vtables.

### Dynamic-name result on M45-release

The run examines all 2,170 registered commands (1,074 effects, 1,096 triggers). Counts are distinct
commands.

| Outcome | Effects | Triggers | Total |
| --- | ---: | ---: | ---: |
| Not examined: command object not joined | 10 | 2 | 12 |
| No flag name stored | 964 | 1,058 | 2,022 |
| Complete: a role and every declared scope's store | 86 | 30 | 116 |
| Partial: a role, but no declared scope set | 2 | 1 | 3 |
| Failed: a stored flag name without a role | 9 | 5 | 14 |
| Unresolved stored index | 3 | 0 | 3 |

The complete commands form 31 namespaces: one global store and 30 scope stores (57 effects
define, 29 remove, 30 triggers read). Every namespace accepts `name@target`.

| Failure shape | Commands |
| --- | --- |
| `command-vtable`, `factory-terminal`, `instruction` (command object) | 9, 2 and 1 |
| `role-store`: the flag store does not come from the accessor | `set_relation_flag`, `remove_relation_flag`, `set_saved_date` |
| `no-role`: an interned name that no setter, remover or scan uses | points of interest (4), `has_relation_flag`, `reverse_has_relation_flag`, `timed_flag_days_left`, the three above, `clear_global_event_target`, `exile_leader_as`, `save_event_target_as`, and `save_global_event_target_as` |
| `scope-set`: no declared scope set | the astral rift flags (3), `set_saved_date`, `has_relation_flag`, `reverse_has_relation_flag` |
| `branch-value`: a reader path that does not return, so the stored index and form are unproven | `timed_flag_days_left` |

Findings:

- Design flags share the global store: `set_design_flag`, `remove_design_flag` and
  `has_design_flag` declare the design scope, whose type has no case in `GetFlags`.
- A namespace is a store, not a flag kind: carrier flags reach the planet, ship and colony
  stores, and `set_timed_ambient_object_flag` declares only the fleet scope, so it joins the
  fleet namespace.
- `has_star_flag` is in two namespaces of its own. Its galactic-object route calls
  `GetGalacticObject()`; `set_star_flag` reaches the same object through the lookup that
  `GetFlags` inlines. The method does not prove the two equal, so the stores stay separate.

`clear_global_event_target`, `exile_leader_as`, `save_event_target_as` and
`save_global_event_target_as` store names without a supported flag-store route, and
`kill_exiled_leader`, `return_leader_from_exile` and `set_leader` have `index-store` gaps.

### Dynamic-name gaps

- Saved event targets and variables are `OutsideMethod`. `save_event_target_as` interns its name
  in the flag table, but the target is kept with the saved event targets of a scope or of the game
  state, not in a flag store. Variables use `CVariables`, which does not intern names.
- The 12 commands whose command object is not joined, and the relation flags, whose flag store
  does not come from the command's accessor.
- How `CreateDynamicFlag` forms the flag from the name and the target is not established; the
  answer says only that the command keeps and uses both.
- A namespace whose commands disagree on `name@target` has form `Unresolved` and a
  `ReaderSemantics` gap that names each command's form. None does on M45-release.

### Dynamic-name pitfalls

- The interner alone is not a flag: points of interest, saved dates and saved event targets use
  `CreateFlagIndex` too. A role needs the setter, the remover or the scan on the store that the
  accessor returns, with the flag loaded from the stored index.
- Follow the member reader as well as `Assign`: the timed setters split and intern their name in
  `ReadMember` (`+0x2b4`).
- The reader facts come from the paths that return. A reader path that stops, such as at an
  unknown branch target or a path limit, is a stop of the command, and its form stays
  `Unresolved`. A name that only an unreadable registration site names is not examined.
- A missing assign slot or a reader body that does not decode is a stop (`assign-slot`,
  `reader-code`), not a reader that names no flag. None occurs on M45-release.
- The evaluator cannot prove the readers' scan loop: a loop over an unknown count reaches the path
  limit. Reads need the complete-function membership-scan shape.
- A route that the method cannot follow falls back to its terminal, the function that receives
  the scope object. Different terminals for the same store, such as the star flags, stay
  separate; they are never guessed equal.
