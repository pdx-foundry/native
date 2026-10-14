# Derived names

`Native::derived_names(registry)` (`derived-names/v5`) gives the names that a registry's own code
composes from an item key or a string field, and then checks or looks up, and the names that the
direct callers of an item getter compose from the key or field that it returns
([interface callers](#interface-callers-of-name-getters-m452)): the parts of each name,
what it is looked up in (localisation, sprites, files), the stage, the field condition, and what a
missing name gives. Atlas matches the names to the config's `localisation` and `images` lines and
decides what `## optional` means.

The method is `src/engine/analysis/names.rs`; its module comment describes the two kinds of run,
the enumerated inputs and the miss and condition rules. `src/binding/binary/names.rs` reads the
sink table, the diagnostic functions and each registry's roots. `src/session/names.rs` gives the
public answer and its gaps. The string model is shared with
[modifier families](modifier-families.md) (`engine/analysis/strings.rs`). The
[engine knowledge index](../engine-knowledge.md) lists the other pages.

## Engine facts (M451-hotfix)

Executable `29fa877366040a528098da39ec7e70b7baac76782a2a6bd161616d691f86fa38`, read with
`examples/inspect`.

### Lookup and check functions

| Function | Address | Name argument | Result |
| --- | --- | --- | --- |
| `PdxLocalizeAndReplaceView(CPdxStringView, CPdxLocalizeKeyValuePair const*, int)` | `0x10257151c` | text in `x0`, length in `w1` | a `CPdxTemporaryLocalizationStringView` through `x8`: text pointer at 0, length at 8, a byte at 12 |
| `HasLocalizeKey(CPdxStringView)` | `0x102584718` | text in `x0`, length in `w1` | found in `w0` |
| `PdxHasLocalizeKey(CPdxStringView)` | `0x10257bd50` | the same | `b HasLocalizeKey` |
| `CGuiGraphics::SpriteExists(CString) const` | `0x10264e660` | the string object in `x1` (by value, passed by address) | found in `w0` |
| `CGuiGraphics::GetSpriteType(CString const&) const` | `0x10264e658` | the string object in `x1` | the sprite |
| `VFSExists(CString const&)`, `VFSExists(char const*)` | `0x10253f238`, `0x10254378c` | the object, or the text, in `x0` | found in `w0` |

- Call sites form the view from a string object: `CString::GetSize() const` (`0x102524808`)
  returns the flag byte for a short string and the word at `+8` for a long one, and a `csel` on
  the flag byte picks the buffer or the object as the text. `CTraditionType::GetBaseName`
  (`0x100cdd050`–`0x100cdd068`) and `GetDesc` (`0x100cdd744`–`0x100cdd758`) show the shape.
- The call sites pass no key-value pairs (`x2` = 0, `w3` = 0). Bits 32–39 of the length register
  carry a flag that the lookup tests only for a diagnostic text.
- When the byte at `0x103796cf0` is nonzero, `PdxLocalizeAndReplaceView` skips `LocalizeString`
  and writes ` (LOC: `, the key and `)`: a localisation debug display, not a miss path.
- `LocalizeString(CPdxStringView)` (`0x1025838bc`) and `HasLocalizeKey` hash the view with
  `PMurHash32` and binary-search the same table under `pData` (`0x103796cf8`).
- The diagnostic functions are `CPdxLogFileAndLine::operator()(char const*, ...)` (`0x102508718`;
  format in `x1`, one eight-byte stack argument per directive), after its constructor
  (`0x1025086d4`), and `CLogger::Log` (`0x102507c3c`) with `CLogStream::operator<<`.

### The stated rule for an unchecked localisation miss

A localisation lookup with no existence check of that name before it returns the key text and
writes no diagnostic. The binding states this beside `PdxLocalizeAndReplaceView`
(`Miss::ShowsKey`). Three hand-read checks:

1. **The miss returns null and logs nothing.** `LocalizeString` returns the found text, or at
   `0x102583960` `x0` = 0 and `x1` = `0x100000000`. Its only call is `PMurHash32`.
2. **The lookup then uses the key's own text.** `PdxLocalizeAndReplaceView` calls `LocalizeString`
   at `0x10257177c`; `cbnz x0` at `0x102571780` takes a found text. On null, when the key pointer
   is nonnull, `bfxil x1, x22, #0, #0x28; mov x0, x26` (`0x102571788`) passes the key's own view
   to `LocalizeAndReplaceInStringInternal` (`0x102571e1c`), the same call that a found text takes.
   No diagnostic call is on that path. The function's one diagnostic, "Localization buffer
   exhausted for key: %s" (`0x102571718`), depends on the per-thread buffer, not on a miss.
3. **A check and a lookup agree on a miss, and the replacement does not log one.**
   `HasLocalizeKey` runs the same hash and binary search over the same table, so a name that a
   check misses is the name that a lookup misses. `LocalizeAndReplaceInStringInternal` writes
   only "Unmatched \"$\"", a recursion-depth message and a truncation message: they depend on `$`
   tokens and the text's length, and a derived key text has no `$`.

### Traditions (`CTraditionType`, key at `+0x10`)

- `GetBaseName() const` (`0x100cdd020`) looks up the bare key.
- The swaps are a `CPdxArray<CTraditionSwap*>` at `+0x5c8`: buffer at `+0x5d0`, count at `+0x5dc`.
  The insertion specialization `CPdxArray<CTraditionSwap*, int>::InsertAtEmplace` (`0x100ce3a74`)
  loads capacity and count with `ldp w8, w25, [x0, #0x10]` and stores the count plus one at
  `+0x14`; the binding derives the count offset from it as it derives the buffer offset.
- `GetName(CCountry const*) const` (`0x100cdd10c`) starts from `TPdxNullObject<CTraditionSwap>`
  and picks a swap with `CTraditionSwap::IsPossible` and `CalcWeight`, which the method does not
  follow. It calls the swap's validity through vtable slot `0x40`, then tests `inherit_name`
  (`+0x4f1`): zero looks up the swap's `name` (`+0xf8`, `0x100cdd254`), otherwise it calls
  `GetBaseName`.
- `GetDesc(CCountry const*) const` (`0x100cdd3c8`) picks a swap the same way and a suffix,
  `_desc` (`0x102dcdce7`) or `_delayed` (`0x102e0d4f9`), by the engine-set type at `+0x40`. It
  checks swap name + suffix at `0x100cdd5a8`. On a hit it builds that name again; on a miss, or
  without a valid swap, it builds key + suffix (`0x100cdd624`). Both are moved with `__move_assign`
  (`0x100cdd6bc`, `0x100cdd714`) into one string, which is checked at `0x100cdd758` and looked up
  at `0x100cdd7a4`. A miss there gives `CString::_NullString` and no diagnostic.
- `GetIconKey` (`0x100cdf7f0`) returns the key or the swap name with no lookup; interface code
  adds `GFX_`. The method follows those callers: see
  [interface callers](#interface-callers-of-name-getters-m452).
- `PostReadInit()` (`0x100cdc994`) tests the length of `custom_tooltip` and
  `custom_tooltip_with_modifiers` (`+0x1c0`, `+0x1e8`; on swaps `+0x120`, `+0x148`), checks each
  nonempty one with `HasLocalizeKey`, and on a miss writes "Missing localization %s for tradition
  %s" (`0x102e1c06b`) or "... for tradition swap %s" with the name as the first argument.
- `GetCustomTooltip` and `GetCustomTooltipWithModifiers` look the same fields up when the item is
  used, the swap's under `inherit_effects` (`+0x4f0`) zero.

### Tradition categories (`CTraditionCategory`, key at `+0x10`)

- `GetName(CCountry const*) const` (`0x100cd8c5c`) runs into `GetBaseName() const`
  (`0x100cd8c60`), which looks up the key.
- `GetDesc(CCountry const*) const` (`0x100cd8d4c`) writes its result through `x8` and uses the
  `description` block; only when its count (`+0x16c`, tested at `0x100cd8d98`) is zero does it look
  up key + `_desc`, with no check. A profiler branch (`_g_ScriptProfiler`) composes
  `tradition_category.{key}.desc` for `CScopedStartProfile`; that name is no lookup.

### Council agendas (`CCouncilAgenda`, key at `+0x10`)

- `CalcName() const` (`0x10020cf74`) looks up `council_agenda_{key}_name`;
  `GetDescription() const` and `GetDescriptionForCountry` compose `council_agenda_{key}_desc`.
- `GetSpriteKey() const` (`0x10020bb0c`) returns `GFX_council_agenda_icon_{key}` with no lookup.
- `PostReadInit()` (`0x10020b7b8`) composes the same sprite name, checks it with `SpriteExists`
  (`0x10020b8dc`) and stores the result at `+0x578`. A miss writes "Missing sprite for council
  agenda, expected sprite ..." through `CLogger::Log` only when the word at `+0x2fc` is nonzero, so
  the miss is not a diagnostic on every path.

### What the answer does not publish

The swap name + suffix of `GetDesc` falls back to key + suffix, and a missing `custom_tooltip` or
`custom_tooltip_with_modifiers` of `PostReadInit` writes a diagnostic (both read by hand above).
The answer gives these names an `Unresolved` miss behavior. Their names have a field part, and a
template run plants one state for every string field, so a miss path that depends on another
field's state, such as one that logs only when another field is empty, is never explored. The
method gives a miss behavior only from search runs, whose names hold the key alone.

## Interface callers of name getters (M452)

Executable `c621723d9c8e0c1cd153319208d30a9dfbb9e63675be86f9d0ae7debeaa7fe1b`, read with
`examples/inspect`.

A **name getter** is a direct `const` member of the item class that reaches no sink, has a direct
`bl` (not a tail `b`) from a function outside the class, and returns a string with the key or a
field in it. The binding keeps a candidate only when a sink call or a candidate sprite call
follows one of its calls (`Code::reaching`). The method then probes it with one run: a returned
string object in `x0` is a reference return; `x0` equal to the entry result object of `x8` is a
by-value return. For each call it runs the caller from the return address to each use site
(`Machine::run_paths_to`), with the getter's name as a placeholder key text, and reads the name at
the site. The getter's own runs (search and template) then use each returned name through each
caller's composition, so the getter's flag and field terms stay (`inherit_icon` zero for the swap
name). A caller name is never `Always`, since the caller's own branches decide whether it reaches
its use, and its miss is `Unresolved`.

- `CTraditionType::GetIconKey(CCountry const*) const` (`0x100ce10b8`) selects a swap like `GetName`
  and returns in `x0` the swap's `name` (`+0xf8`) when the swap is valid and `inherit_icon`
  (`+0x4f2`) is zero, otherwise the key (`+0x10`).
- `CColonyType::GetDesc() const` is `add x0, x0, #0x48; ret`: an offset getter.
- By value: `CCouncilAgenda::GetSpriteKey`, `CWarGoalType::GetIcon`, `CSpecimen::GetDesc*Key` and
  the `CPatronType` background and icon keys compose the name into the `x8` object. A caller
  that sets `x8` to `sp` plus a constant just before the call is followed with the name in that
  slot. `CCardinalPatronPanelEntry::Update` uses `sub x8, x29, #0x98`, a frame-pointer slot,
  which gives a `getter-result` gap.

### The sprite rule

A survey of all 15,293 primary vtables shows that displacement `0x2e8` holds unrelated methods in
other classes (`CPlanet`, `CContainerWindow::MoveToGuiFront`, `CScrollbar::HideSlider`, …), so a
displacement alone is no lookup. The rule includes the receiver's class:

- **Element classes**: `T` of `T* CContainerWindow::GetGuiObject<T>(CString const&) const` whose
  primary vtable holds a `ChangeSprite(CString const&)` that calls
  `CGuiGraphics::GetSpriteType` directly: `CButtonStandard` and `CIcon`, both at `0x2e8` (the
  sprite slot). `CButton::ChangeSprite` and `TButton::ChangeSprite` are a bare `ret`, so a class
  that holds one of them is not an element class.
- **Element calls** in a caller: a direct call to a `GetGuiObject<T>` or to a function whose first
  instruction branches to one (`CContainerWindow::GetButton`, `GetIcon`), or a `blr` whose
  register's last straight-line write is `ldr xN, [xM, #D]` with `D` the displacement at which
  `CContainerWindow`'s primary vtable holds such a function (`GetButton` `0xe0`, `GetIcon`
  `0x100`).
- In a caller run, an element call returns an element object whose first word points at a
  synthetic vtable, whose word at the sprite slot is a marker. At a `blr` use site, the target
  register holds the marker only when the function pointer came from the sprite slot of an
  element's own vtable; with an element in `x0`, the call looks the string object in `x1` up in
  sprites.

**Assumption:** the receiver of a call through an element getter's displacement is a
`CContainerWindow`. The method does not type the container. Three checks:

1. `CEffectButton`, `CCallbackButtonStandard`, `CMultiSpriteButton` and `CNullButtonStandard` hold
   `CButtonStandard::ChangeSprite` at `0x2e8`, and `CNullIcon` holds `CIcon::ChangeSprite` there;
   both pass the argument to `GetSpriteType`. `GetGuiObject<CButtonStandard>` calls
   `GetGuiObjectByString<CButtonStandard>`, which searches the container's array of
   `CButtonStandard*` by name.
2. `CContainerWindow`'s vtable (address point `0x103276408`) holds `GetButton` at `0xe0` and
   `GetIcon` at `0x100`; each is a branch to its `GetGuiObject<T>`.
3. The `GetIconKey` callers: `CTraditionCategoryGridEntry::SetupTraditions` gets the container
   from `CreateContainerWindow`, calls `GetButton` through `0xe0` at `0x101b49024`, and calls the
   sprite slot at `0x101b49084` with `GFX_` and the returned name in `x1`. The
   `CAscensionPerkEntry` constructor reads its container at `[x19, #8]` and calls `GetIcon` through
   `0x100` at `0x10126613c`. `CTraditionsView::UpdateAscensionPerks` passes the composed string
   to `UpdateAscensionPerk`, a callee the method does not follow, so it gives no name.

The assumption fails only for an unrelated object whose method at `0xe0` or `0x100` returns an
object whose own vtable then holds, at `0x2e8`, a call that receives a string made from the
getter's name. Type the container, from its own source, when such a case appears.

### What the callers give

`common/traditions` and `common/ascension_perks`: `GFX_{key}` (sprite, condition `Unresolved`) and
`GFX_{tradition_swap/name}` (sprite, `inherit_icon` zero and a nonempty swap name);
`common/council_agendas`: `GFX_council_agenda_icon_{key}` when used, from `GetSpriteKey` by
value; `common/governments/civics`: `{picture}` from `GetPicture`.

Gaps: a use site with a caller path that stops before it (`caller-path`), a name at the site that
has an unfollowed part while it holds the getter's name or after the name went into an unfollowed
call on that path (`caller-name`), a by-value call whose result object is not a stack slot
(`getter-result`), and a getter whose probe has no path to its return (`getter-path`, such as
`CTechnology::GetCost`). An unresolved name with no trace of the getter's name is not a use of it.
A getter's runs enter the members and composers that the getters call; a root's runs do not, since
those members reach no sink.

## Result on M451-hotfix

`tests/expected/m452/derived-names-traditions.json` and `derived-names-tradition_categories.json`
hold the two acceptance answers. The ignored test
`derived_names_follow_the_tradition_getters_and_keep_swap_conditions` checks them against the
engine facts above.

- **Traditions** (partial). `{key}` shows the key when missing and is `Always`. `{key}_desc` and
  `{key}_delayed` are silent when missing, and their condition is `Unresolved` (the engine-set type
  at `+0x40`). `{tradition_swap/name}` has a nonempty name and `inherit_name` zero as its
  condition. `{tradition_swap/name}_desc` and `_delayed` have a nonempty swap name. The tooltip
  fields `custom_tooltip`, `custom_tooltip_with_modifiers` and the swap's two fields are looked up
  in `PostReadInit` and when used; the swap's use carries `inherit_effects` zero. Every name with a
  field part has an `Unresolved` miss; the hand-read fallback and diagnostics are in
  [what the answer does not publish](#what-the-answer-does-not-publish). The interface callers of
  `GetIconKey` give the sprite names `GFX_{key}` and `GFX_{tradition_swap/name}`, the latter under
  `inherit_icon` zero; three of their use sites keep a `caller-path` gap.
  `common/ascension_perks` uses the same class and gives the same names.
- **Tradition categories** (partial). `{key}` is `Always` and shows the key. `{key}_desc` shows the
  key; its condition is `Unresolved` (the count of the `description` block).
- **Second entries with `Unresolved`.** A const member that calls one getter after another also
  records the second getter's names, after assumed text. `CTraditionType::GetTooltip` calls
  `GetName` (`+0x38`) and then `GetDesc` (`+0xd0`). So `{key}_delayed` of traditions and `{key}`
  of categories appear a second time with an `Unresolved` miss, each with its gaps. The entry with
  the same name, lookup and stage and an established miss is the one to match. Since SDK-820 the
  `GetTooltip` paths that stopped at its tooltip string go on and spend the path bound first, so
  the second `{key}_desc` and one `{tradition_swap/custom_tooltip_with_modifiers}` entry without the
  `inherit_effects` term are gone; the same names and lookups keep their other entries.

### Population

The run covers every registry from `Native::registries()` (M452):

```sh
cargo run --release --example derived-name-population -- "$STELLARIS_PATH"
```

| Inventory | Total | Complete | Partial | Unsupported | Failed |
| --- | ---: | ---: | ---: | ---: | ---: |
| Registries | 185 | 62 | 118 | 5 | 0 |

Eighty-eight registries return names, and all of them are partial. The 62 complete answers
return no name: no own method of theirs reaches a lookup or check, and no getter of theirs has a
caller with one. Twenty-one of the 30 partial answers with no name have no established key place.
`common/starbase_levels` became complete with SDK-820: its `getter-path` gap
(`CStarbaseLevelType::GetTooltip(CString&)`) was a stop at its out-argument string.

The 21 registries that a database's own loader names
([custom-loader names](registry-fields.md#custom-loader-names)) are read through the owner that
their loader builds, as `registry_fields` reads them. The five without one owner are
`Unsupported` with the join's reason: `common/colors`, `common/component_tags`,
`common/ship_behaviors` and `gfx/worldgfx` (no class with a member reader) and
`prescripted_countries` (two databases). The key probe
runs the template key constructor `Owner::Owner(int, CString const&)` (key in `x2`; the by-value
form has the same call shape) and the key-first `Owner::Owner(CString const&, int)` (key in `x1`).
On M452 the key-first shape gives `common/strategic_resources` (`+0x20`; three names, `{key}`,
`{key}_desc` and `{key}_custom_tooltip`) and `common/opinion_modifiers` (one name, `{key}`) a key
place. The other 13 answer partial with no name, each with the `constructor` gap; their owners
have other shapes, such as `CTrait::CTrait(CString, int)` and
`CStaticModifier::CStaticModifier(CString const&)`. A further shape is a change when Atlas needs
the names of one of these registries.

The `UnresolvedPath` shapes, with their obstacles, are in
[paths that stop](#paths-that-stop-sdk-820). The `UnresolvedCondition` shapes, all
[accepted](#accepted-limit-conditions-and-missing-names-sdk-810):

| Shape | Registries | Gaps |
| --- | ---: | ---: |
| Every-item initialization not established | 4 | 4 |
| Field values of a name not established | 58 | 136 |
| What a missing name gives not established | 54 | 141 |
| Field-derived name with unexplored field states | 26 | 68 |

The four every-item gaps (36 before SDK-713) are listed in [gaps](#gaps).

The 330 entries: 300 localisation, 18 file and 12 sprite; 251 when used and 79 at owner
initialization. On a miss, 159 show the key, 141 are unresolved, 16 are silent and 14 are
diagnostics; no entry is a fallback. The 68 names with a field part are all unresolved on a miss.
Conditions: 126 `Always`, 68 with field terms, 136 `Unresolved`. The interface callers give six of
the sprite entries ([what the callers give](#what-the-callers-give)).

Findings of the run:

- **Post-read diagnostics of the key.** `PostReadInit` checks `{key}` and writes a diagnostic on
  a miss in `common/economic_categories`, `common/governments/councilors`,
  `common/megastructures`, `common/ship_categories`, `common/specimens` and `common/technology`;
  `{key}_desc` is a diagnostic in councilors, technology and the three patron registries.
  `common/game_concepts` writes one for `{key}` when used.
- **Post-read file checks.** Owner initialization checks `.dds` paths with `VFSExists`, such as
  `gfx/interface/icons/decisions/{key}.dds` (silent) and `gfx/interface/icons/districts/{key}.dds`
  (unresolved), and the `icon` and `arkship_picture` fields.
- **Sprite checks** are rare: `GFX_{key}_bg` and `GFX_{key}_box_icon_rectangle` of districts,
  `GFX_{icon}` and `GFX_{map_counter_icon}` of ship sizes, and the council agenda icon, whose miss
  is unresolved as the engine facts above say.
- **Run time.** The run takes about 100 s on one process, about 65 s without the interface
  callers. `common/agreement_presets` is the slowest, at about 17 s.

### Config agreement

Comparison with `config/common/traditions.cwt` of `cwtools-stellaris-config`, a test expectation
only (Native reads no config). `$` is the item key; in `swapped_tradition` it is the swap's `name`
(`name_field = name`). Atlas owns the meaning of `## optional`; a silent or fallback miss is the
engine form of an optional line, a shown key or a diagnostic the form of a required one. A name
with a field part has an `Unresolved` miss in the answer; the hand-read miss is in
[what the answer does not publish](#what-the-answer-does-not-publish).

| Config line | Engine entry | Agreement |
| --- | --- | --- |
| `tradition` `name = "$"` | `{key}`, shows key, `Always` | Agrees |
| `tradition` `## optional flavor = "$_delayed"` | `{key}_delayed`, silent | Agrees |
| `tradition` `## optional effects = "$_desc"` | `{key}_desc`, silent | Agrees. The engine looks up one of the two suffixes, chosen by the type at `+0x40`; the config lists both |
| `tradition` `icon = "GFX_$"` | `GFX_{key}`, sprite, when used, condition `Unresolved` | Agrees on name and lookup. The interface caller decides when; miss not in the answer |
| `tradition` `custom_tooltip = localisation` | `{custom_tooltip}` at owner initialization and when used, miss unresolved | Miss not in the answer. By hand, a miss in `PostReadInit` is a diagnostic, which agrees |
| `tradition` `custom_tooltip_with_modifiers = localisation` | `{custom_tooltip_with_modifiers}`, the same | As `custom_tooltip` |
| `swapped_tradition` `subtype[not_inheriting_name]` `name = "$"` | `{tradition_swap/name}`, miss unresolved, `inherit_name` zero | Condition agrees: `inherit_name = yes` with cardinality `0..0` is `inherit_name` zero. Miss not in the answer |
| `swapped_tradition` `subtype[not_inheriting_name]` `## optional flavor = "$_delayed"` | `{tradition_swap/name}_delayed`, miss unresolved | Miss not in the answer. By hand it falls back to `{key}_delayed`, which agrees with optional. Condition differs: `GetDesc` (`0x100cdd3c8`–`0x100cdd7a4`) reads no inheritance flag, so it checks the name of an inheriting swap too. Only the swap's validity, a vtable call outside the method, could depend on one |
| `swapped_tradition` `subtype[not_inheriting_effects]` `## optional effects = "$_desc"` | `{tradition_swap/name}_desc`, miss unresolved | Miss not in the answer. By hand it falls back to `{key}_desc`, which agrees. Condition differs: `GetDesc` does not test `inherit_effects` |
| `swapped_tradition` `subtype[not_inheriting_icon]` `icon = "GFX_$"` | `GFX_{tradition_swap/name}`, sprite, `inherit_icon` zero | Condition agrees: `inherit_icon = yes` with cardinality `0..0` is `inherit_icon` zero. Atlas joins no naming line of a nested type |
| `tradition_swap` `name = localisation` or `name = scalar` | `{tradition_swap/name}` only under `inherit_name` zero | Agrees: an inheriting swap's name is not looked up, so it can be any scalar |
| `tradition_swap` `custom_tooltip`, `custom_tooltip_with_modifiers` | `{tradition_swap/…}` at owner initialization and when used, miss unresolved | Miss not in the answer. By hand, a miss in `PostReadInit` is a diagnostic, which agrees. The use also carries `inherit_effects` zero, which the config does not state |
| `tradition_category` `name = "$"` | `{key}`, shows key, `Always` | Agrees |
| `tradition_category` `desc = "$_desc"` | `{key}_desc`, shows key, condition `Unresolved` | Miss agrees. The engine looks it up only when the `desc` block is empty; the config requires it always |

## String fields with a lookup claim

The SDK-626 triage named 62 string fields (M451-hotfix) whose config claim is a localisation key,
file or sprite and that no derived name reaches. On M452, against the Atlas coverage at Native
`b2b3969` and before the interface callers, there are 118. Each now has one of these causes; a
field is "outside" only with a caller found. The inputs were a run that listed each registry's
planted string fields and probed every getter that other code calls, not only the getters with a
use site.

| Cause | Fields | Examples |
| --- | ---: | --- |
| A getter returns it to callers whose use the method does not reach | 7 | `agreement_presets.icon` (`GetIcon`, `GetLargeIcon`: caller paths stop), `governments/councilors.icon`, `specialist_subject_types.icon` and `icon_large`, `game_concepts.texture`, `economic_categories.scaled_value_key` and `megastructures.build_megastructure_no_cost_localization_key` (no lookup after the call: the caller passes the text on) |
| No key place, so no root runs | 9 | `ambient_objects`, `game_concept_categories`, `precursor_civilizations`, `special_projects`, `start_screen_messages` |
| The registry's own paths stop (`path-limit`, `string-object`, `unreached`, `unresolved-name`, …) | 69 | `country_types.colonizing_title`, `bypass.galactic_map_tooltip_header`, `buildings.custom_tooltip`, the eleven `situations` fields. SDK-820 re-ran them: [most are read by no method that the runs follow](#the-fields-in-registries-whose-paths-stop) |
| The registry's own paths only go on after assumed text | 10 | `colony_types.custom_tooltip`, `megastructure_overclock_types.custom_tooltip` |
| No stop in the owner's code, and no getter of it has a caller with a lookup | 23 | `country_focus/card_categories` (5), `resolution_categories.icon`, `map/galaxy` (3), the two `pop_tooltip` fields |

Every field of the last two rows is planted in template runs, so its text is followed wherever an
owner method reads it: those fields reach no lookup in the owner's code. Interface code that reads
the field directly, without a getter, is the likely reader; the method does not show it. The
interface callers reach no further field of the 118; they add the tradition and agenda icons and
civics `picture`, whose claims are naming lines or not in the list.

## Paths that stop (SDK-820)

Population runs at `main` `af4fb57` and after SDK-820, over M452. A gap is one registry's gap of
the shape; the count is the sum of the numbers in their details (methods or call sites).

| Shape | Before: gaps | Count | After: gaps | Count | Obstacle |
| --- | ---: | ---: | ---: | ---: | --- |
| `assumed-text` | 95 | 357 | 95 | 358 | A lookup's returned view is given the empty text; the later path is followed but establishes nothing ([pitfalls](#pitfalls)) |
| `unresolved-name` | 51 | 199 | 52 | 182 | A part of the name is text that the runs do not follow: another object's string, a lookup's result, an unfollowed call's result |
| `path-limit` | 48 | 146 | 59 | 174 | Search runs fork on unknown content in 56 description builders (`GetDetailsDesc`, `GetToolTip`, `Append*ModifiersDetailsDesc`, `PostReadInit`). No shared fork: a bound of 256 paths in place of 64 closes 4 of the 59 and adds 7 `loop-limit` |
| `string-object` | 45 | 88 | 0 | 0 | Repaired: see below |
| `unreached` | 42 | 406 | 32 | 336 | 323 of the 336 sites are in methods whose paths stop. The other 13 are behind a test of run-time state that is null in the image, such as `_g_CurrentGameState` (`CEdict::GetDesc`, `CPolicyGroup::GetName`) |
| `constructor`, `key-storage` | 21 | 21 | 21 | 21 | No established key place (see above) |
| `loop-limit` | 14 | 30 | 17 | 34 | Template runs do not join, and a loop over a collection that the run does not install (modifier lists, upgrades) never ends |
| `caller-path` | 13 | 67 | 13 | 67 | Caller paths stop before a use site: mostly at the path bound, also at the loop bound and at moves from unknown strings |
| `name-limit` | 5 | 18 | 5 | 19 | A `PdxStrFmt<N>` buffer bounds the name |
| `instruction` | 4 | 4 | 3 | 3 | SIMD forms and `sbfx` that the evaluator does not run (`CIntelCategory::GetInfoPerLevel`, `CScriptedMapMode::GetToolTip`, `CResourceConverter::CanAfford`) |
| `branch-value` | 4 | 9 | 4 | 9 | A `br` through a register: the virtual `CTrigger::Evaluate` and colony state calls on objects that the runs do not know |
| `caller-name` | 3 | 25 | 3 | 25 | A caller passes the getter's name to a call that the method does not follow |
| `getter-path` | 3 | 4 | 2 | 3 | Every probe path stops: `CTechnology::GetCost` (an `instruction` stop in `CFixedPoint::GetRounded`) and a `CShipSize` modifier getter (path bound) |
| `getter-result` | 1 | 1 | 1 | 1 | `CCardinalPatronPanelEntry::Update` gives a frame-pointer slot |
| **All** | **349** | **1,375** | **307** | **1,232** | |

**The repair.** Every `string-object` stop was `CString::operator+=` or the standard string's move
assignment on an `x0` that the run does not know: a string member of a `CToolTip&` that the root
receives (44 `GetToolTip(CToolTip&, …)` roots) or a `CString&` or `CString*` out-argument
(`CTraditionType::BuildEffectDescInternal`, `CanActivate`). A root run now treats a string function
that writes an unknown object as a call that the model does not follow: it records the derived texts
that the call receives and forgets the argument strings, and the path goes on. The caller runs
already did this. A move from an unknown source into a known object still stops in both: it changes
that object, and the model cannot say to what. The repair gives 22 new names, all with an
`Unresolved` miss and condition (`{key}_desc` of tooltip builders, `edict_{key}_desc`,
`policy_{key}_desc`, `mission_{key}_desc`, `{key}_aftermath_modifier_tooltip`, civics
`{custom_tooltip_with_modifiers}`), and closes `common/starbase_levels`. Paths that went on also
meet the bounds: `path-limit` grows by 11 and `loop-limit` by 3.

### The fields in registries whose paths stop

SDK-707 counted 69 fields but kept no list. Rebuilt from the same recording (Atlas
`.scratch/native-b2b3969`: `value_form` claims of `localisation`, `filepath`, `icon[…]` or
`<sprite>` on fields that Native reads as a string, joined to a registry by the config type's
`path`), 123 fields are unreached and 76 of them are in registries with a stop gap. The 76 include
every example above, and seven getter fields that SDK-707 counted in its first row. For each, the
template runs watched the item's reads (`Machine::watch_reads`), the runs recorded the calls that
receive its planted text, and the roots, getters and entered functions were searched for an
immediate inside its 24 bytes. After SDK-820:

| Result | Fields | Gap that keeps it |
| --- | ---: | --- |
| Reaches a lookup | 1 | `governments/civics.custom_tooltip_with_modifiers` |
| Returned by a getter whose callers' uses are not reached | 7 | `caller-path`, `caller-name`: `agreement_presets.icon`, `governments/councilors.icon`, `specialist_subject_types.icon` and `icon_large`, `game_concepts.icon` and `texture`, `megastructures.build_megastructure_no_cost_localization_key` |
| Passed to a call that the method does not follow | 5 | `OutsideMethod`: `game_concepts.tooltip_override` (`NTooltipUtil::TryGetDatabaseConceptTooltip`), `megastructures.custom_tooltip_requirements` (`PdxLocalize<…>` with key-value pairs), `ship_sizes.pre_communications_name` (`CheckLocalization(bool, CString const&)`), `storm_types.cosmic_storm_texture_path` and `cosmic_storm_texture_color_path` (`CTextureHandler::AddTexture_sRGB`) |
| Read by a method whose paths stop before a call receives it | 5 | `path-limit`: `buildings.custom_tooltip` (`GetEffectsDesc` looks it up at `0x1000d3620`, after three `Append*ModifiersDetailsDesc` calls that spend the bound), `astral_actions.custom_usage_tooltip` (`IsExhausted`), `governments/councilors.custom_tooltip_with_modifiers` (`GetModifierLoc`), `megastructures.tooltip_best_systems_header` (`BuildBestSystemsText`), `star_classes.icon` (`InitPostRead`) |
| Read, and no call receives it on any path | 2 | The consumer is not established: `bypass.galactic_map_tooltip_desc` and `mutations.icon` (`InitPostRead`, which completes) |
| Composed by a method of another class | 1 | `OutsideMethod`: `governments/authorities.advanced_authority_swap/description` (`CAdvancedAuthoritySwap::GetCustomDescription`; the runs enter only the item class's members) |
| No read observed | 55 | The consumer is not established. No template path reads the field and no instruction of the followed code uses its offset, so the registry's stops are not its obstacle; interface code that reads the field directly is the likely reader (`OutsideMethod`) |

The 55: `anomalies.picture`; `armies.icon`; `astral_actions.picture`; `buildings.icon`;
`bypass.galactic_map_tooltip_header` and `system_view_tooltip_header`;
`casus_belli.aggregated_message_key`; `country_types.colonizing_desc`, `colonizing_title`,
`pre_communications_fleet_name`, `sub_title` and `sub_title_desc`; `deposits.custom_tooltip_short`;
`diplomatic_actions.icon`; `edicts.icon`; `espionage_operation_types.picture`;
`event_chains.picture`; `federation_types.icon`; `governments.heir_title`, `heir_title_female`,
`ruler_title` and `ruler_title_female`; `governments/civics.room`;
`governments/councilors.custom_unlock_title`; `leader_classes.name`; `map_modes.icon`;
`megastructures.cycle_icon`, `order_icon` and `portrait`; `missions/missions.picture`;
`observation_station_missions.icon`; `patrons/psionic_auras.aura_galaxy_texture`,
`aura_hyperlane_texture` and `background`; `pop_faction_types.election_header` and `visual_ethic`;
`relics.portrait`; `ship_sizes.automation_icon` and `role_background`;
`situation_log/categories.icon`; the eleven `situations` fields; `specimens.icon`;
`storm_types.cosmic_storm_event_sprite`, `description` and `icon`.

"No read observed" is not a proof: a read watch misses loads after a store to an unknown address,
and paths cut by a bound are not observed. It is evidence that the registry's stop gap is not the
field's obstacle.

**Atlas need.** None of the 76 credits a claim now. Atlas gives a field's `value_form`
`localisation` claim "no comparable form" (Atlas `src/coverage/comparison.rs`,
`comparison_outcome`), and joins a derived name only to a `localisation` or `images` naming line
(`src/coverage/language_subject.rs`, `Joins::derived_name`). Reopen a row when a naming line names
one of these fields.

## Gaps

- **Outside the method:** names that interface code composes or looks up without calling a name
  getter of the item, or after passing the getter's name to another function (such as
  `UpdateAscensionPerk`); callers of callers; a sprite call on an element that the caller keeps in
  a member instead of fetching it (the patron views); the run-time choice behind a selection
  (which swap, its validity);
  localisation-file existence; sprite file paths; the meaning of engine-set fields such as the
  tradition type at `+0x40` and the agenda word at `+0x2fc`.
- **Not followed:** the localisation wrappers `PdxLocalizeView<…>` and `InternalPdxLocalizeView<…>`
  that take key-value pairs. A name passed to one is an unfollowed call, not a derived name.
- **Post-read initialization:** a name of `PostReadInit` is never `Always` unless
  `families::loading::every_item` establishes that the engine runs it for every item. The
  database's post-read loop, not the item's read, calls `PostReadInit()`; `every_item` establishes
  the call from that loop ([modifier families](modifier-families.md#the-per-item-post-read-call)).
  On M452 4 of the 36 registries that had the gap keep it. `system_types` and
  `colony_automation_categories` have an `InitPostRead` root, and a destructor-shaped function
  names their item's vtables (`TPdxNullObject<X>::~TPdxNullObject`, a destructor thunk);
  `technology` items are also constructed by `CTechnology::ReadMember`; and
  `CEconomicCategoryDatabase` overrides the post-read loop with its own `PostReadInit()`, which
  the method does not run and which calls `CEconomicCategory::PostReadInit()` by no direct call.

## Accepted limit: conditions and missing names (SDK-810)

The four `UnresolvedCondition` shapes of the answer are an accepted limit. Native still reports
them; the [sweep report](method-authoring.md#accepted-limits) counts them under SDK-810. On M452
at `main` `29b025c`, before the limit:

| Shape | Gaps | Registries |
| --- | ---: | ---: |
| What a missing name gives is not established | 119 | 39 |
| The field values under which the engine uses a name are not established | 113 | 48 |
| A field-derived name depends on field states that the method does not explore | 69 | 26 |
| That the engine initializes every item after reading it is not established | 4 | 4 |

The every-item gaps are the narrowed SDK-713 remainder, each with its obstacle in
[gaps](#gaps).

**Atlas need: none now.** Atlas `ledger` and `compare` at Atlas `main` `63c2028` (Native pin
`ee01bf3`), over the SDK-798 recording and the `config` directory of `cwtools-stellaris-config`,
give the SDK-798 headline (42.15%):

- `naming_rule`, 495 claims, 130 covered. Atlas joins a `localisation` or `images` line to a
  derived name by registry, rendered name and lookup only (Atlas
  `src/coverage/language_subject.rs`, `Joins::derived_name`). It copies `stage`, `condition` and
  `on_missing` into the rule without testing them, and no per-name gap of this answer becomes an
  Atlas gap (`src/snapshot/registry.rs`, `assemble_derived_names`). The 365 uncovered claims
  match no Native name: a resolved condition or miss credits none of them.
- `naming_requirement` (`## optional`, `## required`), 121 claims. Atlas joins none of them.
- Conditions: Atlas leaves out claims with config conditions, and the naming lines of a subtype go
  through nested selectors that it does not credit (`src/coverage/projection.rs`,
  `language_questions`).

**Reopen** a shape when an Atlas claim depends on it: a naming claim that reads a name's condition
or field states, a `## optional` or `## required` join to `on_missing`, or a claim that needs a name
of post-read initialization to be `Always`. For a `## optional` join, 16 optional lines have a
naming line that joins a Native name; 10 have an established miss, and 6 are one-field names with
an unresolved one: `common/leader_classes` `{name_plural}` and `{description}`,
`common/governments/civics` `{negative_description}` and `{description}`, and the `{icon}` files of
`common/notification_modifiers` and `common/missions/missions`. The repair is a search run that
varies the field's text state, so that the miss paths that depend on it are explored
([what the answer does not publish](#what-the-answer-does-not-publish)).

**After.** Over a recording at `main` `29b025c`, the open gaps go from 4,031 to 3,726 and the
accepted gaps from 853 to 1,158. No relationship status or rate changes: a naming rule reads the
completeness of the answer, which the limit does not change. `registry-sweep --diff` from the
tracked baseline names the 57 registries whose open and accepted gaps changed.

## Pitfalls

- **A registry's stop gap is not a field's obstacle.** SDK-707 sorted unreached fields by their
  registry's gaps; most of them are read by no method that the runs follow. Check that a followed
  method reads the field (a read watch on the item, the calls that receive its planted text, its
  offset in the code) before you blame a stop.

- **A vtable displacement is not a method.** Displacement `0x2e8` is `ChangeSprite` only in the GUI
  element classes; other classes hold unrelated methods there. Recognize the sprite call by the
  receiver's own vtable (the synthetic vtable of an element object), not by the displacement.
- **Probe a getter before following its callers.** Most `const` members with outside callers
  return no name (`GetCost`, `IsVisibleForCountry`); running their callers first spent most of
  the run on path bounds. Probe with one run, and count only the returned string: other events,
  such as an unfollowed call that receives the key, are not a returned name.
- **A string function that writes an unknown object is not a stop.** Callers build strings in
  members of their own objects, and roots in the tooltip or out-string that they receive; the
  `string-object` stop made thousands of caller paths and 45 registries' root paths fail with no
  relation to a name. A move from an unknown source into a known object is different: it changes
  that object, so it still stops.
- **Give the caller's placeholder its own key node.** The item key's node survives every
  unfollowed call, since a root's item key does not change. A caller may change the string that
  a getter returned, so its placeholder needs a node that an unfollowed call makes unresolved.
- **An unresolved name at a use site is not a use of the getter's name.** Count it only when it
  still holds the name, or when the name went into an unfollowed call on that path.

- **Do not trace outward from the lookup.** `PdxLocalizeAndReplaceView` has about 7,144 direct
  callers. Start from the registry's own methods.
- **A longer name holds a shorter one's text.** The text of `{key}_desc` holds the key's text, so
  a substring test made a diagnostic that names `{key}_desc` a diagnostic for `{key}`. A message
  names a name only when no key character (letter, digit or `_`) is directly before or after it.
- **Follow the member call graph to its end.** A depth bound on same-class member calls dropped a
  root whose sink was deeper, with no gap. The graph is finite; a run's own bounds give the gaps.
- **Join search runs at loop heads, not template runs.** A search run's swap collection has an
  unknown length, and a loop over it forks until the path bound. A template run installs one
  element, so its loops end; a join at the selection loop's head merged the installed element with
  the null swap object, and the swap's name was lost.
- **Do not enter the diagnostic constructor.** `CPdxLogFileAndLine::CPdxLogFileAndLine` clears a
  large buffer one byte at a time. Composer selection entered it as a leaf, and the paths after a
  missing name spent the step bound before the diagnostic, which hid the diagnostic.
- **Give a lookup a known result, as assumed text.** After `PdxLocalizeAndReplaceView`, the callers
  copy the returned view. With an unknown length every lookup forked several times and `GetDesc`
  spent the path bound before its swap names. A run gives each lookup the empty text, but the
  real text is not empty: a later length test takes one arm. The lookup's own facts stand; the
  path's later events establish no condition or miss behavior, and the search keeps its gap.
- **An assumed text is not the empty name.** The model writes the empty text for an unresolved
  node. A view of length zero of that text read as the literal empty name, which became a checked
  name and an outcome to enumerate.
- **An unresolved name's check after assumed text gives "missing".** Forking on it doubled the
  paths after the null swap object's unknown name, and `GetDesc` hit the path bound.
- **Model `CString::CString(char const*, int)`.** Unmodelled, it left the strings that `GetDesc`
  builds after its lookup unknown, and each destructor's flag test forked.
- **Plant a string only where a plain `CString` is read.** A template run plants text at the
  offset of each string field. A compound reader's string (`ReaderJoin::Stored`) is not a plain
  `CString`: the `CString` array emplace read by the shared string reader (`String/Accumulate`,
  for example `common/asteroid_belts.mesh`) reports the array header, whose data and count are
  at other offsets, so a planted `CString` there gave the run a made-up pointer and count. Only
  a field that `CReader::Read(CString&, bool)` reads into an owner offset is planted. The
  optional `CString` set from token text (`Stored`, repeat unknown) is not planted either: the
  method does not establish its layout.
- **Prove a fallback on every path that checks the name.** In `GetDesc` another path (no valid
  swap) uses key + suffix at the same check site in the found run too, so a comparison of whole
  runs saw no replacement. A union over paths was wrong the other way: a check whose result is
  ignored, followed by a content choice between the name and another at one site, read as a
  fallback. The method counts only paths that check the name: every found path uses only the name
  at the sites where found paths use it, every missing path uses only the one replacement there,
  and every missing path that does not end in a trap reaches such a site. A use of an unresolved
  name at such a site could be another replacement, so it refutes the fallback.
- **The registry-fields page has M45-release addresses.** Match the build before reusing an
  address from [registry fields](registry-fields.md).
