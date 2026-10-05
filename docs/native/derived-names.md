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
- **Join search runs at loop heads, not template runs.** A search run's swap collection has an
  unknown length, and a loop over it forks until the path bound. A template run installs one
  element, so its loops end; a join at the selection loop's head merged the installed element with
  the null swap object, and the swap's name was lost.
- **Do not enter the diagnostic constructor.** `CPdxLogFileAndLine::CPdxLogFileAndLine` clears a
  large buffer one byte at a time. Composer selection entered it as a leaf, and the paths after a
  missing name spent the step bound before the diagnostic, which hid the diagnostic.
- **Give a lookup a known result.** After `PdxLocalizeAndReplaceView`, the callers copy the
  returned view. With an unknown length every lookup forked several times and `GetDesc` spent the
  path bound before its swap names. A run gives each lookup the empty text.
- **An assumed text is not the empty name.** The model writes the empty text for an unresolved
  node. A view of length zero of that text read as the literal empty name, which became a checked
  name and an outcome to enumerate.
- **An unresolved name's check after assumed text gives "missing".** Forking on it doubled the
  paths after the null swap object's unknown name, and `GetDesc` hit the path bound.
- **Model `CString::CString(char const*, int)`.** Unmodelled, it left the strings that `GetDesc`
  builds after its lookup unknown, and each destructor's flag test forked.
- **Compare a fallback per path.** In `GetDesc` another path (no valid swap) uses key + suffix at
  the same check site in the found run too. A comparison of whole runs saw no replacement; the
  method compares the paths that check the name.
- **The registry-fields page has M45-release addresses.** Match the build before reusing an
  address from [registry fields](registry-fields.md).
