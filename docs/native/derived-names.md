# Derived names

`Native::derived_names(registry)` (`derived-names/v1`) gives the names that a registry's own code
composes from an item key or a string field, and then checks or looks up: the parts of each name,
what it is looked up in (localisation, sprites, files), the stage, the field condition, and what a
missing name gives. Atlas matches the names to the config's `localisation` and `images` lines and
decides what `## optional` means.

The method is `src/engine/analysis/names.rs`; its module comment describes the two kinds of run,
the enumerated inputs and the miss and condition rules. `src/binding/binary/names.rs` reads the
sink table, the diagnostic functions and each registry's roots. `src/session/names.rs` gives the
public answer and its gaps. The string model is shared with
[modifier families](modifier-families.md) (`engine/analysis/families/strings.rs`). The
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
  adds `GFX_`. It is outside the method.
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
  [what the answer does not publish](#what-the-answer-does-not-publish).
  `common/ascension_perks` uses the same class and gives the same names.
- **Tradition categories** (partial). `{key}` is `Always` and shows the key. `{key}_desc` shows the
  key; its condition is `Unresolved` (the count of the `description` block).
- **Second entries with `Unresolved`.** A const member that calls one getter after another also
  records the second getter's names, after assumed text. `CTraditionType::GetTooltip` calls
  `GetName` (`+0x38`) and then `GetDesc` (`+0xd0`). So `{key}_desc` and `{key}_delayed` of
  traditions and `{key}` of categories appear a second time with an `Unresolved` miss, each with
  its gaps. The entry with the same name, lookup and stage and an
  established miss is the one to match.

### Population

The run covers every registry from `Native::registries()`:

```sh
cargo run --release --example derived-name-population -- "$STELLARIS_PATH"
```

| Inventory | Total | Complete | Partial | Failed |
| --- | ---: | ---: | ---: | ---: |
| Registries | 164 | 62 | 102 | 0 |

Eighty-five registries return names, and all of them are partial. The 62 complete answers return
no name: no own method of theirs reaches a lookup or check. Eight of the 17 partial answers with
no name have no established key place.

Gap shapes, by registry (the answer-item shapes also give their gap count):

| Shape | Registries | Gaps |
| --- | ---: | ---: |
| Paths go on after assumed text (`assumed-text`) | 92 | |
| A lookup or check receives a name with an unfollowed part (`unresolved-name`) | 49 | |
| Paths stop at the path bound (`path-limit`) | 47 | |
| Paths stop at an unmodelled string object (`string-object`) | 44 | |
| Lookup or check calls that no run reaches (`unreached`) | 41 | |
| Paths stop at the loop bound (`loop-limit`) | 14 | |
| Paths stop at an unsupported instruction (`instruction`) | 7 | |
| Paths stop at an unknown branch value (`branch-value`) | 5 | |
| A fixed-size buffer bounds the name (`name-limit`) | 5 | |
| No established key place (`constructor` 4, `key-storage` 4) | 8 | |
| Every-item initialization not established | 36 | |
| Field values of a name not established | 47 | 112 |
| What a missing name gives not established | 38 | 111 |
| Field-derived name with unexplored field states | 25 | 64 |

The 36 every-item gaps are: a database path keeps an item without its post-read code (25), a
function forms an item's vtable address without its constructor (6), another function constructs
items (5).

The 293 entries: 270 localisation, 18 file and 5 sprite; 214 when used and 79 at owner
initialization. On a miss, 153 show the key, 111 are unresolved, 15 are silent and 14 are
diagnostics; no entry is a fallback. The 64 names with a field part are all unresolved on a miss.
Conditions: 117 `Always`, 64 with field terms, 112 `Unresolved`.

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
- **Run time.** The run takes about 400 s: 2.2 to 6.1 s per registry, of which about 2.2 s is the
  read and hash of the executable that every static query does
  ([performance](performance.md#the-static-query-invariant)). `common/agreement_presets` is the
  slowest.

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
| `tradition` `icon = "GFX_$"` | None | Outside the method: `GetIconKey` returns the key, and interface code adds `GFX_` |
| `tradition` `custom_tooltip = localisation` | `{custom_tooltip}` at owner initialization and when used, miss unresolved | Miss not in the answer. By hand, a miss in `PostReadInit` is a diagnostic, which agrees |
| `tradition` `custom_tooltip_with_modifiers = localisation` | `{custom_tooltip_with_modifiers}`, the same | As `custom_tooltip` |
| `swapped_tradition` `subtype[not_inheriting_name]` `name = "$"` | `{tradition_swap/name}`, miss unresolved, `inherit_name` zero | Condition agrees: `inherit_name = yes` with cardinality `0..0` is `inherit_name` zero. Miss not in the answer |
| `swapped_tradition` `subtype[not_inheriting_name]` `## optional flavor = "$_delayed"` | `{tradition_swap/name}_delayed`, miss unresolved | Miss not in the answer. By hand it falls back to `{key}_delayed`, which agrees with optional. Condition differs: `GetDesc` (`0x100cdd3c8`–`0x100cdd7a4`) reads no inheritance flag, so it checks the name of an inheriting swap too. Only the swap's validity, a vtable call outside the method, could depend on one |
| `swapped_tradition` `subtype[not_inheriting_effects]` `## optional effects = "$_desc"` | `{tradition_swap/name}_desc`, miss unresolved | Miss not in the answer. By hand it falls back to `{key}_desc`, which agrees. Condition differs: `GetDesc` does not test `inherit_effects` |
| `swapped_tradition` `subtype[not_inheriting_icon]` `icon = "GFX_$"` | None | Outside the method, as the tradition icon |
| `tradition_swap` `name = localisation` or `name = scalar` | `{tradition_swap/name}` only under `inherit_name` zero | Agrees: an inheriting swap's name is not looked up, so it can be any scalar |
| `tradition_swap` `custom_tooltip`, `custom_tooltip_with_modifiers` | `{tradition_swap/…}` at owner initialization and when used, miss unresolved | Miss not in the answer. By hand, a miss in `PostReadInit` is a diagnostic, which agrees. The use also carries `inherit_effects` zero, which the config does not state |
| `tradition_category` `name = "$"` | `{key}`, shows key, `Always` | Agrees |
| `tradition_category` `desc = "$_desc"` | `{key}_desc`, shows key, condition `Unresolved` | Miss agrees. The engine looks it up only when the `desc` block is empty; the config requires it always |

## Gaps

- **Outside the method:** names that interface code composes or looks up, such as the tradition
  icon (`GFX_` + `GetIconKey`); the run-time choice behind a selection (which swap, its validity);
  localisation-file existence; sprite file paths; the meaning of engine-set fields such as the
  tradition type at `+0x40` and the agenda word at `+0x2fc`.
- **Not followed:** the localisation wrappers `PdxLocalizeView<…>` and `InternalPdxLocalizeView<…>`
  that take key-value pairs. A name passed to one is an unfollowed call, not a derived name.
- **Post-read initialization:** a name of `PostReadInit` is never `Always` unless
  `families::loading::every_item` establishes that the engine runs it for every item. The database
  loop, not the item's read, calls `PostReadInit()`, so it is not established for traditions or
  agendas.

## Pitfalls

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
