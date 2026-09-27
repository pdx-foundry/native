# References and dynamic names

A reference is a field or command argument whose value the engine looks up by key in a loaded
collection. A dynamic name is a name that script both defines and reads, such as a country flag.
SDK-543 owns both methods. This page holds their engine facts on M45-release, the results, the gaps
and the pitfalls. The [discovery index](discovery.md) lists the operations.

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
- `NParserUtil::ReadKeyReference<D>(CReader&, D const&, bool)` has 87 instantiations. It looks the
  token up at read time, returns the item or the typed null object, and logs
  `Failed to read key reference '%s' from database '%s' at '%s'` for an invalid result unless the
  `bool` argument is true.
- `ReadKeyReferenceUniform<D, CPdxArray<…>>` has 13 instantiations,
  `ReadKeyReferenceDeferredUniform<CScriptedActionDatabase>` and
  `ReadIndexReferenceDeferred<CTechnologyTierDatabase>` one each.
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
  `CStaticModifierDatabase::_pInstance`, stores the result and tail-calls vtable slot `0x98`. The
  getter scans its own items (`this+0x3d0`, count `this+0x3dc`, key at `item+0x158`) with the same
  comparison and returns the null object on a miss.
  `CPlanetClassDatabase::GetPlanetClass(CString const&)` wraps a `CHashTable::Find` and substitutes
  the null object for a null result.

### Effect initializers

| Effect | Key storage | Lookup in `PostInit()` | Miss |
| --- | --- | --- | --- |
| `create_ship` | `CReader::Read(CString&, bool)` into `+0x600` | inline map find in `TGameDatabase<CShipSizeDatabase>`; skipped for an empty key | null object; `PostValidate` logs |
| `add_district` | string at `+0xa8` | linear scan of `TGameDatabase<CDistrictTypeDatabase>` | null object at `+0xd0`; `PostValidate` logs |
| `add_relic` | string at `+0xa8` | linear scan of `TGameDatabase<CRelicsDatabase>` | null object at `+0xd0`; `PostValidate` logs |
| `change_pc` | token text at `+0x238` | `CPlanetClassDatabase::GetPlanetClass` | null object at `+0x260`; `PostValidate` also accepts random planet lists and scope keywords |
| `create_army` | `ReadKeyReference<CArmyTypeDatabase>` in `CCreateOrModifyArmyParentEffect::ReadMember`, at read time | none | the reader's null object and log |

`CCreateArmyEffect::PostInit()` walks the `CEventTarget` chain at `this+0x650` and classifies each
link's keyword token (`+0x58`) through compare trees and two `ldrh` jump tables. It makes no
database lookup; the SDK-482 prototype's unknown result for it was not a failed reference shape.

### Database directories

Template databases name their directory in the `CSingleObjectGameDatabaseBase(CString const&)`
constructor argument; `registries()` already joins them. Custom loaders do not:
`CStaticModifierDatabase::Init(bool)` passes `"common/static_modifiers"` to
`VFSGetEnumeratedFiles(char const*, CPdxArray<CString, int>&, char const*, char const*, int)`, and
`RunGame` pre-enumerates `"common/planet_classes"`.

### Flags

- `CFlagEffect::Assign` and `CFlagTrigger::Assign` call `ReadAsDynamicFlag(CString const&,
  CString&, CEventTarget&, EScopeType, CString const&)`. It splits `name@target` at `@`. For a
  static name the caller stores `CPdxIntegerFlags::CreateFlagIndex(CString)` as a 16-bit index:
  the effect at `+0xa8`, the trigger at `+0x1f8`. `CSetTimedFlagEffect::ReadMember` does the same
  and stores at `+0x2b4`.
- `CreateFlagIndex` interns into one global `CPdxIntegerFlags::_AllFlags` map, keyed by the exact
  string, for every flag kind and for saved event targets.
- Setter `AccessFlags` implementations forward to `CEventScope::AccessFlags()`, which tail-calls
  `CEventScope::GetFlags() const`; `CHasFlagTrigger::GetFlags` forwards to the same function.
  Global flag commands instead return `_g_CurrentGameState + 0x478` in both roles.
- `CHasFlagTrigger::ActualEvaluate` reads the store through its virtual getter, then scans the
  16-bit indexes inline.
- Scope sets differ between roles: `CSetStarFlagEffect::GetSupportedScopes` returns `0x80`,
  `CHasStarFlagTrigger::GetSupportedScopes` returns `0x08000080`.
- Variables use `CVariables`, a string-keyed map that does not intern names.

## Pitfalls

- A reference reader's signature does not state its lookup: `ReadKeyReference<CArmyTypeDatabase>`
  scans linearly while `ReadKeyReference<CShipSizeDatabase>` calls the map `Find`.
- Registration with the deferred resolver does not by itself say when the lookup runs, and a
  loaded null object does not by itself say that a miss selects it.
- `DeclaredScopes` states where a command may run, not which store it writes; flag stores are
  compared per scope.
