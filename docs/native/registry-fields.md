# Registry fields

`Native::registry_fields(registry)` gives root fields, reader and storage shapes, loader
alternatives, nested object fields and local stored-value selections; `Native::registries()`
gives the registries. The module comments of `engine/analysis/fields.rs` and
`engine/analysis/discovery.rs` describe the methods. `FieldDefault` was removed (only `Unknown` was
ever established; Git `d8f9d8a`). The [keyword domains](#keyword-domains-sdk-828) of fields read
through a token converter come from this method (SDK-828), the weight keyword domains from the
[weight blocks](weight-blocks.md) method (SDK-725). [Required fields](#required-fields) have their own section.

## What complete means

Both static answers are bounded searches. `registries` searches the shared database-template
candidates and the scheduled databases outside them. It is complete when every candidate and every
scheduled database has one content directory; nested and late loaders are outside the search.
`registry_fields` searches the root reader paths of one registry's owner: the template argument of a
template loader, or the one owner that a custom loader builds ([custom-loader
owners](#custom-loader-owners)). A listed registry with no one owner is `Unsupported` with the
reason. It is complete when every path, field name and promised reader
classification is resolved. An `OutsideMethod` gap states the boundary and can accompany
`Complete`. Unnamed candidates, unresolved paths, unknown reader classifications and unreadable
required input make the answer partial.

## Current sweep

This sweep ran on M452.

`tests/population/m452/registry-field-sweep/` holds the baseline. Its `header.json` has the
counts (`summary`): registries by completeness, root fields and reader identities. Against `v31`, `events` has its 22 [kinds](#event-kinds) as root fields in place of the
30 `CEvent` fields, and the recording takes in SDK-786: `flags` and `common/planet_classes` are
listed, and `common/ship_sizes.carries_colony` looks up `common/planet_classes`. The 164 template answers did not
change when the 19 custom-loader names joined (SDK-787);
their counts are in [custom-loader owners](#custom-loader-owners). Against `v33`, only the 336
[pre_trigger keys](#pre-trigger-blocks) changed (SDK-807). Against `v34`, the
[uniform lists](#uniform-lists) and the [bit fields](#bit-fields) changed (SDK-710). Against
`v35`, 22 registries changed (SDK-676): the nested owners of traditions, ascension perks and
authorities gained their modifier, clause and weight grammar; 33 root fields that embed an object
of another class gained `accepted_categories: NotApplicable`; and 19 root fields of nine
registries joined their reader once a delegating constructor's stack string no longer stops the
constructor walk ([nested owners and unjoined fields](modifier-blocks.md#nested-owners-sdk-676)). Against `v36`, SDK-813 answered `common/planet_classes`: its 70 planet-class fields and the
reserved key `random_list` ([root-key owners](#root-key-owners-sdk-813)). Against `v39`, SDK-808 read the 69
triggered modifier clauses in their [construction scope](triggered-modifiers.md#read-scope-sdk-808)
and renamed the other read-scope gaps by [obstacle](discovery.md#read-time-block-scopes-sdk-549). Against `v40`, SDK-627 stated
the [repeat rules](#repeat-behavior) of five more readers: 131 registry answers change, four
registries become complete, and `UnresolvedStorage` gaps fall from 1,054 in 116 registries to
916 in 109. Against `v41`, SDK-822 changed two registries: civics `modifier`, `ai_weight` and
`random_weight` and star_classes `modifier` join their readers
([allocations](#allocations-in-constructor-walks)). Against `v43`, SDK-828 joined the
[token converters](#keyword-domains-sdk-828) as keyword readers: 14 registries change,
`common/scripted_modifiers` becomes complete, and four fields that a `movk`, `csel` or `tst` used
to stop join their readers (`common/deposits` `potential` and `resources`, `common/ship_sizes`
`possible_construction` and `potential_construction`). SDK-823's
[path joins](#path-joins-on-m452-sdk-823) change the entry contexts of 22 registries. Against `v46`,
SDK-838 follows [inherited member readers](#inherited-member-readers-sdk-838) and reads `stur`: 15
registries change, no field is lost, and `common/colony_automation_exceptions`, `common/decisions`
and `common/deposits` have no registry-wide path gap left. Against `v47`, SDK-848's
[inline-constructed objects](#inline-constructed-objects-sdk-848) change five registries. Against
`v48`, SDK-854 gave 70 fields of 13 registries their repeat: the planet-class Boolean bits and
the stored immediate references ([gate shapes](#repeat-gaps-of-gate-registries-sdk-854)). Against
`v50`, SDK-835 reads [serializer roots and passes the stack guard](#serializer-roots-and-the-stack-guard-sdk-835):
nine registries change and none loses a field. Against
`v51`, SDK-850's [duplicate checks and owned pointers](#duplicate-checks-and-owned-pointers-sdk-850)
give six fields of five registries their members. Against `v52`, the dispatch walker reads a compare with a
shifted immediate: `common/scripted_actions` names its 42 fields
([order check](#scripted-actions-an-order-check-before-the-token-switch)), and no other registry
changes. Against `v55`, the [order check](#scripted-actions-an-order-check-before-the-token-switch) gives 41 of 42 scripted action fields an `Always` read and 38 a known reader, and no other registry changes. The `yes` test (compound shapes, below) gives 13 members of every event kind a Boolean
reader, adds `is_triggered_only`, and removes one civics path gap. Against `v56`, the stored value token (below) adds five fields of three registries, and no field loses a reader. Against `v57`, the literal-token test (below) adds `scripted_actions.context_menu_ordering` and `steal_planet_output.mode`. Against `v58`, value text (below) gives 47 registry fields, 44 of them in events, a `String` reader, and no field loses a reader. Against M451-hotfix, civics lost `multiply_by_habitability_effect_modifier` and edicts
gained `relay_network_modifier`. Compare a new run with `registry-field-sweep --diff` ([method
authoring](method-authoring.md#run-over-the-whole-population)).

A reference member is an answer, not a default. Compared with reports from before default-member
normalization (regenerated at `f184f08` and `ab9cc7f`), 20 registries change, not 12: twelve
change readers, shapes or gaps, and eight change only non-default reference lookups
(`common/anomalies`, `common/archaeological_site_types`, `common/astral_rifts`,
`common/council_agendas`, `common/event_chains`, `common/planet_modifiers`, `common/situations` and
`common/starbase_levels`). Dropping every reference member gives 12, but hides those established
lookups, so the diff drops only members that match their serde defaults.

- **Council agendas (SDK-600).** All ten fields are found, but the answer is partial: `agenda_cost`
  uses `CVariableValue::Read`, a scoped operand. `ai_weight` has the shared weight grammar
  ([weight blocks](weight-blocks.md)) and reads in `country`; every key has a reader kind, and
  `calc` and `mode` list their accepted names. It stays partial for `days`, `months`, `years` and one `factor`
  alternative, whose `CToken::GetInt()` and `GetFloat()` readers have established storage but
  no faithful-storage range ([token value methods](numeric-conversion.md#token-value-methods)). The
  scanner, zero-mask, `trigger` lookup and `parameters` limits are `OutsideMethod`. `CPersistent` block classification of `modifier` does not
  establish its member family. `potential`, `allow`, `effect`
  and `init_effect` enter only as a country with self-linked root, from and prev
  ([block entry contexts](#block-entry-contexts)). `ai_weight` keeps an entry gap: a template
  helper evaluates it on an array element ([weight blocks](weight-blocks.md#entry-contexts)).
  The acceptance test `council_agenda_fields_are_complete_with_every_milestone_4_fact`
  (`cargo parity council_agenda`) lists each missing fact and typed gap in one run. It accepts
  that one `ai_weight` gap and the range gaps of those four weight keys, when their storage is
  known; no Atlas claim needs either. It accepts no other typed gap.
- **Unknown kinds.** Most unknown kinds come from fields with no single established reader and
  from unresolved root paths, not from unclassified reader signatures (`CVariableValue::Read`,
  `CReader::Read(CColor&)`, `CReader::Read(float&)`, `CReader::Read(CVector2FixedPoint&)`). Their
  shapes are in [fields with no shared reader](#fields-with-no-shared-reader).
- **Compound shapes.** The dispatch walker follows a copied value token constructed as an event
  target and moved to an owner destination (`Target`), a `CString` array emplace read by the shared
  string reader (`String/Accumulate`, for example `common/asteroid_belts.mesh`), and an optional
  `CString` set from token text (`String`). A different token source, a stack move destination or
  an element from an unknown call cannot establish the shape. A stack `CString` made from the
  value token's text and passed to an owner call reads `String` with no destination, by a stated
  rule ([modifier blocks](modifier-blocks.md#fixed-key-readers-sdk-675)).
- **Value text.** A reader that measures the value token's text (`strlen` of the text pointer,
  `ldr x…, [reader, #0x288]` on M452) and assigns the text and that length to an owner
  `std::basic_string` (`__assign_external(char const*, unsigned long)`) reads `String/Replace` at
  that member. The walk follows `strlen` only when the assignment comes next with no conditional
  branch or other call between (`assigns_next`): a reader that tests the length instead, as
  `text_color_code` of patrons does, keeps its stop at `strlen`. A path that measures the text and
  joins no reader keeps that stop too, so a key is never lost. On M452 this reads `title` and
  `message_desc` of every event kind, `upgrade` (astral actions), `entity` (section templates),
  `risk_event` (technologies) and about 25 command keys (`custom_tooltip`, `has_term_value`, …).
  `id` and `base` of events stay stopped: after the assignment they pass the member to
  `CEventManager::IsValidIDTag`, which the walk cannot prove leaves it unchanged.
- **The `yes` test.** A reader that compares the value token with the token `yes` alone, sets a
  register from the result (`cmp`, `cset …,eq`) and stores that register to an owner member reads
  `Boolean/Replace`. Every other token stores false and logs nothing, so the reader accepts any
  value. The binding takes `yes` from the recipe's Boolean token names
  (`PersistentRecipe::boolean_tokens`); a token table with no single `yes` binds none, and the
  idiom then stops at the `cset`. The same idiom with one literal token other than `yes`
  (`cmp` of the value token with `weight`, `cset …,eq`, store: `scripted_actions.context_menu_ordering`)
  reads `Keyword/Replace` (`value token == literal`): the field has two meanings, that token and
  every other, and every token is accepted without a diagnostic, so its domain stays `Unknown` and
  Atlas credits no value form. It needs `yes` bound: without it the literal could be `yes`, so the
  idiom stops at the `cset`. A `cset` after a comparison with several tokens, or of anything
  other than the value token, still stops. A value-token range that `csel` selects
  (`events.event_window_type`: `cmp` of the token minus `leader_recruit` with 4, `csel …,lo`)
  still stops at `csel`. On M452 the
  idiom reads 14 members of every event kind (`fire_only_once`, `hide_window`, `is_test_event`,
  …) and `status` of four triggers (`can_colonize`, …).
- **The stored value token.** A reader that loads the value token's id from the reader
  (`ldr w8, [x1, #0x278]` on M452, with the value token bound) and stores the whole word
  (`str w…`) to an owner member reads `Keyword/Replace`
  (`value token id`, `dispatch.rs`). Any token is accepted; which tokens mean something is known only
  where the member is used, so the field's domain stays `Unknown` and the keyword-domain gap stays.
  Atlas then credits the field and its cardinality but not its value form. When the same path then
  converts the value token (`TokenToEnum`, `Get…FromToken`) and stores the converted value, the
  converted keyword is the field's reader instead (`has_envoy_task.task` stores both); when the
  converted value is not stored, the id store stays the read. On M452 this adds `attitudes.type`,
  `diplomatic_actions.action_type` and `envoy_assignment`, `scripted_actions.category` and
  `uses_vfx_type`, the leader commands' `sub_type`, the galaxy-setup commands' `setting`, and a
  keyword alternative to the `side` of `join_war_on_side` and `is_war_participant`.
- **Constructed objects.** The method reads `tradition_swap` in traditions and ascension perks (13
  child fields each); in `advanced_authority_swap` the collection is established but not its
  children. The council presence branches normalize to unconditional integer reads. Objects built
  by a `PdxMakeScopedPtr` factory and moved into a scoped-pointer array join the same way; when the
  object's reader has a block family, as the 50 triggered modifier clauses do, the field carries that
  reader and its grammar instead of loader fields ([triggered modifiers](triggered-modifiers.md)).
- **Repeat behavior.** The [repeat rules](#repeat-behavior) give the eight block, operand and
  reference fields of council agendas their repeat; none keeps an `UnresolvedStorage` gap.

Pitfalls:

- A state branch before token discrimination would give a false registry-wide `UnresolvedPath`
  gap: the token-partition check needs one contiguous chain. `CScriptedActionInfo` has that shape on
  M452 ([order check](#scripted-actions-an-order-check-before-the-token-switch)).
- `b.hi` stops in `CEspionageOperationType` and `CStarClass` compare a value loaded from the object,
  not the token, so they correctly stay unknown flags.
- Unresolved concrete readers do not remove established field names or broad `Block` kinds.

### Fields with no shared reader

The sweep report's `reader_identity_stops` groups every root field, and every nested field of
`FieldMembers::Fields`, without a reader identity by the distinct shapes of its read
alternatives; these are the fields that carry the `UnresolvedReader` gap. Keys of an attached
modifier, weight or triggered-modifier grammar are outside it: their gaps come from that
grammar's own method. A stopped alternative gives its reason
and the callee or mnemonic where it stopped. `registries_by_root_gap` counts the registries with
an unfollowed root path, unnamed keys or unreadable input; `stop_shapes` gives their internal
stops, with a distinct registry count for each. Event-kind members count once for each kind.

M452, SDK-710: before at Native `34bd898` (`registry-fields/v34`), after at `registry-fields/v35`.

| Count | Before | After |
| --- | ---: | ---: |
| `UnresolvedReader` "no shared reader" gaps (fields, root and nested) | 873 in 113 registries | 779 in 100 |
| Root fields without identity | 419 | 338 |
| Root fields of kind `Unknown` | 382 | 301 |
| Root `Boolean` fields with repeat `Unknown` | 243 | 5 |
| Gaps other than `OutsideMethod` | 4,482 | 4,036 |
| Registries with an unfollowed root path / unnamed keys / unreadable input | 74 / 15 / 12 | 74 / 15 / 12 |

The shapes, grouped by family. Fields and registries are after the repair. Every census shape
belongs to one row.

| Family (census shapes) | Fields | Registries | Disposition |
| --- | ---: | ---: | --- |
| `CReader::ReadUniform<T>` lists | 98 → 0 | 41 → 0 | Resolved: [uniform lists](#uniform-lists). |
| Boolean stack temporaries (identity, repeat `Unknown`) | 240 → 2 root | | Resolved: [bit fields](#bit-fields); two named obstacles remain. |
| Owned objects: `operator new`, `make_shared`, `PdxMakeScopedPtr`, `InsertAtEmplace`, `SetSizeAndEmplace`, `shared_ptr::reset` | 352 | 40 | Gap; five fields that the root constructs inline joined since ([SDK-848](#inline-constructed-objects-sdk-848)), and six with a duplicate check, a `Class const*` array or one owned pointer ([SDK-850](#duplicate-checks-and-owned-pointers-sdk-850)). The value is read into an object that an unfollowed call allocates or inserts, then through a pointer. The event members (`trigger`, `immediate`, `option`, …, 8 × 22 kinds) are most of it. Repair: extend the collection pass (allocate, construct, virtual read, insert) to one owned pointer and to `shared_ptr` members. |
| Value text: `CString::CString(char const*)`, `strlen`, `CEventTarget::CEventTarget(CToken)`, `CReader::Read(CString&, bool)` into a non-owner destination | 171 | 29 | Gap. The value token's text goes to a name lookup, a comparison or an event target that no compound shape covers (event `id`, `title`, `location`, …). Repair: one compound shape per idiom, as for `Target` and `String` above. |
| Stack serializers: `X::CSerializer::CSerializer`, and `CReader::Read(CPersistent&)` of a stack object | 77 | 44 | Gap. The persistent read's object is a serializer on the stack (`CModifierWithTooltipData::CSerializer`, `CEconomicUnitTemplate::CSerializer`); no owner constructor installs its vtable, so no constructor join gives the reader. Repair: take the concrete reader from the vtable constant stored in the stack object. `common/country_types` `faction` is one: the root stores the `SCountryInstanceProperties::CSerializer` address point and owner `+0x48` at `sp+0x48` itself ([SDK-850](#duplicate-checks-and-owned-pointers-sdk-850)). |
| Persistent read with no constructor-agreed reader | 54 | 25 | Gap. The read joins an owner destination, but no constructor-agreed vtable at that destination gives a reader; the constructor walk's own stops are in `stop_shapes`. Repair: the constructor join of that destination. One named case below. |
| Enum readers: `TokenToEnum<E>`, `Get*FromToken` | 20 → 2 | 14 → 2 | Resolved: [keyword domains](#keyword-domains-sdk-828). `construction_type` converts a list of lexer tokens, and `requirements.shipclass_*` converts the key token. |
| Statement loops: `CReader::ReadSimpleStatement()` with `blr` | 21 | 16 | Gap. Keys of the block are read in a loop through virtual calls; the root-token walk has no key. Repair: a loop-key method. `common/country_types` `modules`, `common/map_modes` `color` and `common/ship_sizes` `section_slots` are this shape ([SDK-850](#duplicate-checks-and-owned-pointers-sdk-850)). |
| Ignored keys: a scalar path that returns with no call, and `CReader::SkipBody()` for a block (`is_colony`, `is_boss`, `hide_size`) | 16 | 10 | Gap. The rule "the caller consumed the scalar, so a return accepts and keeps nothing" is not stated; with it the kind is `Skipped`. |
| Log or singleton calls before the reader (`CPdxLogFileAndLine`, `CLogger`, `GetInstance`) | 14 | 7 | Gap. The walk ends at the first call that is not a reader. Repair: go on past a call that receives neither the owner nor the reader, as the weight value pass does. |
| `CTriggeredEventDescription<…>::ReadEntry` (`desc`, `tooltip`) | 12 | 8 | Gap. A nested description grammar with its own trigger; repair with a description block reader. |
| Other single helpers (`ReadColor`, `ReadTexture`, `ReadShipClass`, `ReadBeamState`, `CCompareOperator::Read`, joins of different readers) | 42 | 20 | Gap. One helper each; repair when Atlas needs the field. |

**Inline `CString` copies in constructors (repaired, SDK-822).** `common/governments/civics`
`modifier`, `ai_weight` and `random_weight` and `common/star_classes` `modifier` now join their
readers through the [allocation rule](#allocations-in-constructor-walks).

Registry-wide gaps keep their owners: the unfollowed root paths stop at the shapes above and at
unsupported instructions (`blr`, `ret`, `cset`) and branch conditions on values other than an
owner word ([ordered compares](#ordered-owner-compares-and-right-shifts)); the unnamed keys are
non-singleton token paths; the unreadable input is the owners with no `ReadMember`, no
[serializer root](#serializer-roots-and-the-stack-guard-sdk-835) and no base root (`CJobTag`,
`CTraitTag`, …).

### Ordered owner compares and right shifts

Three local rules of the dispatch walk (`fields/dispatch.rs`), on M452 at `registry-fields/v64`
and `command-grammar/v28`:

- **Ordered compare of an owner word.** A `b.lt`, `b.ge`, `b.gt`, `b.le`, `b.hi`, `b.ls`, … after a
  compare of a word loaded from the owner with one constant keeps each side that the word can take,
  under an unresolved condition (`ordered_sides`). A load at least as wide as the compare can hold
  every value; a narrower load may be zero- or sign-extended (the walk does not keep which), so a
  side counts only when both extensions agree, and otherwise the path stops
  (`ordered-compare-range`). Every store outside the frame and every call forgets the sides.
  `CCountryType::ReadMember` (`0x10033f6bc`): `ldr w8,[x19,#0x784]; cmp w8,#1; b.lt` around a
  diagnostic, then the read of `trade_routes_available`. A compare of any other value is still a
  stop. The path remembers its side (`State::ordered`) until a call or a store outside the frame:
  a later compare of the same word with the same pivots and the same or opposite condition takes
  only the side that the first allows, and any other test of that word, or an ordered compare of a
  word that an equality test settled, stops the path (`ordered-compare-repeat`).
- **Right shift.** `lsr` makes its destination unknown, as `ubfx` does. The Boolean bit reads load
  the bit's word, shift the top bit down into a stack temporary (`ldrb w8,[x19,#0x40]; lsr w8,w8,#7`
  at `0x10033f380`; `ldr w8,[x19,#0x40]; lsr w8,w8,#0x1f` at `0x10033f760`), call `Read(bool&)` and
  merge the byte back; the temporary's old value does not matter.
- **Floating-point move.** `fmov` makes its destination unknown (`fmov s8,#1.5` in
  `CTrait::ReadMember` and `CShipSize::ReadMember`).

Result: 37 more root fields in 20 registries (for example `country_types` `relations`,
`share_survey_data`, `trade_routes_available`; `ship_sizes` `accepts_all_modifiers`,
`auto_upgrade`; `species_classes` `playable`; `pop_jobs` `auto_generate_description`; `allow`,
`potential` and similar blocks of resolutions, espionage operation types, tradable actions), and 13
more registries complete for fields (129 to 142). No field is lost and no known reader becomes
unknown. Command grammar: the 22 `*_event` effects and `fire_on_action` gain a `scopes` key whose
reader is not established, and `create_leader`, `create_saved_leader` and `clone_leader` gain the
Boolean `skip_background_generation`. `change_dominant_species` stops one more path at
`ordered-compare-repeat`, with the same reader result.

Limits: the walk does not resolve the compare, so a field read on only one side keeps an
unresolved condition. Still stopped: `blr` on the reader's lexer (`ldr x0,[x20,#0x30]; ldr x8,[x0];
ldr x8,[x8,#0x10]; blr x8`, then a test of the token kind at `+0x18` against `4` and `0x13`) in
`CShipSize::ReadMember` and `CTrait::ReadMember`: a value-token-kind switch, which needs its own
rule; and `ret` or `lsl` paths of `traits` `leader_class`, `forced_happiness`. Remove the compare
rule when a build's owner compare selects which key is read.

### Inherited member readers (SDK-838)

A member reader can pass the tokens it does not handle to the reader of its base class. On M452,
`CStarbaseModule::ReadMember` (`0x100c561cc`) and `CStarbaseBuilding::ReadMember` handle a few
keys each, then call `CStarbaseComponent<T>::ReadMember(CReader&, int)` with `x0`, `x1` and `x2`
unchanged (`bl` at `0x100c56588`). That template reads the keys that both registries share
(`icon`, `construction_days`, `potential`, `resources`, `ship_modifier`, ...) and returns false
(`mov w0,#0` at entry, then `ret`) for any other token. After the call, `tbnz w0,#0` returns on
true; on false the derived reader tail-calls `CPersistent::ReadMember` on its base subobject at
`+0x1a18`, the base rejection.

- **Rule.** Registry dispatch follows a direct `bl` or `b` to a `ReadMember(CReader&, int)`
  other than `CPersistent`'s when the call receives the owner, the reader and the token
  unchanged (`fields/dispatch.rs`). The inherited reader's paths keep the caller's token domain
  and conditions. After a `bl`, each path of the inherited reader that returns `x0 = 0` goes on
  in the caller after the call with `x0 = 0`; a `b` hands that false to the caller's own caller.
  A read through a `bl` is not a tail call, so its repeat is not examined, and a stored read
  loses its repeat: the caller's code after the call may write the destination again. Command
  grammar dispatch is unchanged; it joins member delegates its own way.
- **Import stubs.** The call reaches the template through an import stub in `__stubs`
  (`0x102996690`), not its body. The binding (`push_inherited_readers` in
  `binding/binary/fields.rs`) accepts a stub only when its `adrp x16`, `ldr x16` and `br x16`
  load a slot that the fixups resolve locally to a text body of the stub's name. On M452 the slot
  `0x102ffee88` is a `<weak-def-coalesce>` bind of that symbol, which the fixup reader already
  resolves to the image's own definition. `FieldInput::stubs` carries each accepted stub to the
  dispatch name lookup; any other symbol of the name away from the body still makes the lookup
  fail.
- **Base subobjects.** The call may also pass the owner plus a positive constant in `x0`, with the
  reader and the token unchanged: the base class's subobject at that offset. The inherited reader
  then runs with that receiver, so its owner offsets are the subobject's plus its own. On M452
  `CCitizenshipType::ReadMember` (`0x100be5840`) ends with `add x0,x0,#0x38` and
  `b CSpeciesRightBase::ReadMember` (`0x100be42f4`), whose default case tail-calls
  `CPersistent::ReadMember` on its own receiver, the base rejection. At `registry-fields/v63` this
  gives each of the seven species-rights registries with a `ReadMember` the 8 shared keys
  (`potential`, `allow`, `ai_will_do`, `modifier`, `pop_group_modifier`,
  `triggered_pop_group_modifier`, `hidden`, `in_breach_of`); `common/patrons/deeds` gains
  `counter` and `attunement` the same way. Remove the rule when a build's adjusted receiver is not
  a base subobject (a member object at a constant offset read through its base's reader would look
  the same; its fields would then be the member's).
  - **Base root.** `CLivingStandard` and `CMilitaryServiceType` have no `ReadMember`: the group of
    their vtable at subobject offset `0x38` holds `CSpeciesRightBase::ReadMember` in the member
    slot (`0x103091830` for living standards). For an owner with neither its own reader nor a
    serializer root, the binding takes the one address point whose member slot holds another
    class's `ReadMember` (not a thunk, not `CPersistent`'s) as the root, with that point's offset
    (`base_root` in `binding/binary/fields.rs`, `FieldInput::base_root`); the walk starts with the
    owner plus the offset in `x0`. At `registry-fields/v69` both registries list the 8 shared keys.
    Since `registry-fields/v71` `triggered_pop_group_modifier` joins as well: the nested pass runs
    the base reader with the subobject as its receiver
    ([nested owners](#nested-owners-and-the-receiver-context)). No point, or several, leaves the
    owner unread as before.
- **Bit tests of a constant.** `tbz` and `tbnz` of a known constant take only the side the bit
  selects, as `cbz` and `cbnz` do. The resumed false return needs it.
- **Result on M452.** `common/starbase_modules` goes from 7 to 57 root fields and
  `common/starbase_buildings` from 4 to 54. Every path is named; the `UnnamedField` gap is gone,
  and the only registry-wide gaps left are the unnamed evaluation of `GetEquippedComponents` and
  the `UnresolvedReader` gap that sums up the fields' stopped reads. `common/static_modifiers` goes from 3 to 9 fields through
  `CGraphicalModifier::ReadMember`, whose other tokens stop at a virtual call (slot `+0x40`).
  `common/colony_automation_exceptions` gains 7 fields. The `stur` frame stores of the same
  change add fields to `common/districts` (9), `common/country_focus/focus_cards` (5),
  `common/economic_plans` (3), `common/decisions`, `common/deposits`, `common/resource_converters`
  (2 each) and `common/buildings` (`show_in_tech`); the nested `variable_string` key of
  megastructure `cycle_title` and `cycle_desc` and of the ship-set and species-name fields is
  now a member with its own gap. The whole-domain path of `common/diplomacy_economy` now stops at
  `CString(char const*)` on the key text, so it is an `UnnamedField` gap instead of an
  unfollowed path.
- **Limits.**
  - The nested-object pass enters inherited readers, through their import stubs, since
    `registry-fields/v59` ([objects constructed at an array's
    end](#objects-constructed-at-an-arrays-end)). Six `triggered_*_modifier` keys of the starbase
    template now read triggered modifier clauses in both registries; the other three keep an
    `UnresolvedReader` gap, and their stop is not yet read.
  - The other starbase fields without one shared reader belong to census families above:
    `section` and `component_set` (value text to a lookup), `collected_*_resources` (statement
    loop), `triggered_component_set` (an inserted struct), `name` and `desc`
    (`CTriggeredEventDescription`) and `resources` (stack serializer).
  - A nested owner follows the inherited readers that its own member reader reaches, since
    `registry-fields/v71` (`nested_owner` calls `push_inherited_readers`).
- **Value readers.** `CWeaponComponentTemplate::ReadMember` (`0x1001e41f4`) and
  `CStrikeCraftComponentTemplate::ReadMember` (`0x1001eef6c`) pass the tokens that they do not
  handle to `ReadAttackInfoValue(NCombatRules::SAttackInfo&, CReader&)` (`0x1001320fc`) with
  `x0` at an owner member (`+0x10f8` and `+0x1130`) and the reader in `x1`, then tail-call
  `CComponentTemplate::ReadMember` on false (`tbnz w0,#0` at `0x1001e4a88`). The value reader
  loads the token from the reader (`ldr w10,[x1,#0x38]`), switches on it through compares and a
  byte jump table, reads the attack keys (`damage`, `hull_damage`, `shield_penetration` and
  others) into the member, and returns false for any other token.
  - **Rule.** Registry dispatch follows a direct call to a function with the signature
    `F(T&, CReader&)` (`readers::is_member_value_reader`; a template argument or a third
    parameter does not match) when `x0` holds an owner member and `x1` the reader. The callee
    starts with that member in `x0` and no token in `x2`. Its false returns go on in the caller
    as an inherited reader's do. The binding collects these callees with the inherited readers.
  - **The reader's token word.** The owner walk reads a 32-bit load of the reader at the recipe's
    `reader_token_offset` (`0x38`) as the token, as the loader walk does. A member reader receives
    that word in `w2`; the strike-craft reader loads it again after the value reader returns.
  - **Known bit tests.** A `tbz` or `tbnz` of a known constant records the constant, so the
    condition drops when the alternatives collapse, as a zero test of a constant does. Before,
    the false return after the value reader left every base field under an unresolved condition.
  - **Result on M452.** Only `common/component_templates` changes: the weapon key goes from 24
    to 70 fields and the strike-craft key from 22 to 67, with the base fields and the attack
    keys; the jump-table and `UnnamedField` gaps of both keys are gone. The field sweep and every
    other registry are unchanged.
- **Pitfall.** A same-name symbol is not proof that a call reaches a body. An import of an
  unresolved or external symbol can share the name of a local definition; only the fixup slot
  shows where the stub goes.

### Serializer roots and the stack guard (SDK-835)

Two shapes stopped `common/districts`, `common/zones` and `common/zone_slots` on M452.

- **Serializer roots.** `CZoneType` and `CDistrictZoneSlot` have no `ReadMember`. Their
  `Read(CReader&)` (`CZoneType::Read` at `0x100d46df8`) builds a stack object whose first two
  words are the address point of `vtable for CZoneType::CSerializer` and the owner
  (`stp x8,x0,[sp,#8]`), zeroes the serializer's own arrays, and calls
  `CPersistent::Read(CReader&)` on it. The serializer's `ReadMember` loads the owner from its
  receiver at `+8` and reads owner keys at offsets from it; `include`, `exclude` and the other
  name lists go to the serializer's own arrays, which `InitPostRead` converts after the read; its
  default case tail-calls `CPersistent::ReadMember` with the serializer as receiver.
  `CComponentSlotTemplate::Read` builds the same object and calls `CReader::Read(CPersistent&)`
  with the reader in `x0`.
  - **Rule.** For an owner with no `ReadMember(CReader&, int)`, the binding reads
    `Owner::Read(CReader&)` (`root_reader` in `binding/binary/fields.rs`). When its code runs
    straight to its first call, stores an address point with the entry `x0` in the next word on
    the stack, keeps both words and the entry reader, and passes that object and the reader to
    one of the two persistent reads (`fields::serializer_read`), the root is the member reader in
    that vtable's member slot (`concrete_reader`), with the owner at receiver offset 8
    (`FieldInput::serializer`).
  - **Receiver.** Dispatch walks the root with `x0` as `Value::Serializer(0)`. An 8-byte load of
    the owner slot gives `Owner(0)`; a store over the slot stops the path
    (`serializer-owner-overwrite`). Serializer members are no owner destination; a list read into
    one keeps its reader and value form, as a list read into a stack array does. The base
    rejection and an inherited reader accept the serializer receiver. The modifier blocks'
    serializer delegate (`CCustomDescription::CSerializer`) uses the same receiver, so its own
    members no longer read as owner offsets.
- **Stack guard.** `CDistrictType::ReadMember` (`0x1004278e8`) saves `___stack_chk_guard` at
  entry and, before each return or tail call, compares it with the saved copy (`cmp x9,x8`) and
  branches to `bl ___stack_chk_fail` (`0x100428a14`). The compare leaves the flags unknown, so 199
  of 215 paths stopped. A `b.cond` with unknown flags whose one side is a `bl` to a function that
  never returns (`readers::never_returns`, the one list that the binding's never-return set also
  uses) takes only the other side; that side keeps the whole token domain. A side that does more
  before such a call is still a branch on unknown flags.
- **Result on M452.** `common/districts` goes from 9 to 51 root fields, `common/zones` from 0 to
  42 and `common/zone_slots` from 0 to 7, with no registry-wide path, name or input gap left.
  `common/component_slot_templates` lists 4 fields and each `common/ai_espionage` policy its
  `weight`. `common/economic_plans` and `common/country_focus/focus_cards` gain 6 fields each and
  `common/resource_converters` changes. No command grammar changes. In the registry
  sweep, eight of the nine registries become complete for `fields`, and no registry's complete
  count falls.
- **Limits.**
  - A serializer root collects inserted objects since `registry-fields/v71`: the nested-object
    pass runs the serializer's reader with a serializer that points at the owner
    ([nested owners](#nested-owners-and-the-receiver-context)). The zone `triggered_desc` and
    `triggered_*_modifier` keys join through it.
  - Keyword fields of a serializer root list no domain ("The caller reads the owner through a
    serializer, which the domain walk does not follow."). Repair both when Atlas needs them, by
    giving those walks the same receiver.
  - The remaining district and zone stops belong to census families: a log location before the
    reader (`icon`, `desc`) and owned objects (`resources`, `ai_resource_production`).
  - `CJobTag`, `CTraitTag`, `CLivingStandard`, `CMilitaryServiceType` and the scripted effect,
    trigger and value templates have no `ReadMember` and do not match this `Read` shape; the two
    species-rights owners read through a [base root](#inherited-member-readers-sdk-838).
- **Pitfall.** The vtable's name is not the reader: take the member slot, as for any concrete
  reader. A serializer member is not an owner field's storage, so treating the receiver as the
  owner gives wrong destinations.

### Allocations in constructor walks

A constructor walk (`receivers/calls.rs`) handles a call to an
allocator by a validated rule: **the allocation is outside the owner, and the allocator writes no
owner byte, even when it is given an owner address.** The walk returns an unknown value in `x0` that
is not owner-derived, keeps every owner byte, and forgets memory outside the owner (the heap, a
pool, the pool's lock). The other registers that the call does not preserve keep an ordinary
call's taint. The allocators are the imported `operator new(unsigned long)` and
`operator new[](unsigned long)` and the engine's
`CPdxCommonStringAllocator::allocate(unsigned long, void const*)`. Every entered constructor walk
applies the bound call rules, at the owner level as at a member constructor, for
`registry_fields` and `command_grammar` alike.

Without the rule, a long `CString` copy loses the owner. The copy gives the string allocator the
member's allocator object (`x0`), so an opaque call may return an owner address; the later
terminator store `strb wzr, [result, length]` at an unknown index then may reach every owner
byte.

Conditions, on M452 (`CPdxCommonStringAllocator::allocate` at `0x1025156fc`):

- The allocator reads only its object: a mode word at `+0` and a pool pointer at `+8`. With mode 1
  and a pool, it takes a global spin lock (`__bss` `0x1037995fc`) and calls the pool's virtual
  allocate (slot `+0x10`); otherwise it tail-calls `operator new`. It stores to no address given
  in its arguments.
- The pool is not in the owner. The constructors below clear the mode and the pool
  (`str wzr, [member, #0x18]`, `str xzr, [member, #0x20]`) before they allocate.

The three checks:

1. `CString::CString(char const*)` (`0x102524efc`) copies a long text: the receivers test
   `exact_build_string_and_token_constructor_effects` keeps the owner byte before the member on
   every long path. Before the rule it was lost.
2. `CGovernmentCivicType::CGovernmentCivicType(int, CString const&)` (`0x1007fb32c`) copies a long
   string inline at `+0x588`, after the `CModifier` at `+0x488`. Before the rule the walk lost the
   modifier's vtable on 24 of 32 paths; now civics `modifier`, `ai_weight` and `random_weight`
   join. The two weights have the weight grammar but no entry context yet: no evaluation that the
   method attributes evaluates them.
3. `CStarClass::CStarClass(int, CString const&)` (`0x100c0bfb0`) copies a long string on the stack,
   then calls `operator new[]` with a size that it loads from `CPlanetClassDatabase` and fills the
   result with `memset_pattern16`. The size is unknown and may be owner-derived, so the opaque
   call reached the whole owner. Now all 36 entered paths keep the `CModifier` vtable at `+0x140`.

Remove the rule, or limit it, when a build's allocator stores through its object or its size, when
a constructor gives the string allocator a pool in the owner, or when the walk can run the
allocator's body itself.

Remaining obstacles of the persistent fields that the rule does not repair:

- `common/diplomatic_actions` `ai_acceptance` (`+0x4f0`) and `should_ai_propose` (`+0x528`): on
  the "Diplomatic action is missing token" path of `CDiplomaticActionType::CDiplomaticActionType`
  (`0x10041c9b8`), the `CLogger` and stream calls that the walk does not run are given an owner
  address (16 of 48 paths).
- `common/sector_types` `system_scores` (`+0x168`) and `local_capital_scores` (`+0x1a0`): on every
  entered path, `CFixedResourceTable::CFixedResourceTable()` (`0x101d190a0`) allocates through the
  virtual allocate of `NExperimental::PdxGetNewDeleteAllocator()` (`blr`) and copies into its
  result. The summary-only run loses them at `CPdxCommonStringAllocator::deallocate` of the default
  constructor's stack string.

### Inline-constructed objects (SDK-848)

The collection pass (`fields/nested.rs`) joins an object that the root allocates, reads and
inserts. The binding finds its class through a direct constructor or `PdxMakeScopedPtr` call, or
through a direct call to the insertion of a `Class*` or a `CPdxScopedPtrImpl<Class, false>`. The
last form finds a class whose constructor the compiler put inline, with no constructor symbol. The
walk accepts such an object at its virtual read by a validated rule: **an `operator new` allocation
that no constructor built, whose word at `+0` is the class's primary address point, is that
object.** While the root builds it, two kinds of call may write the allocation and nothing else:

- a constructor that the root calls directly and whose parameters are all arithmetic types or
  `EScopeType`, so `this` in `x0` is its only address; the walk forgets the allocation from `x0` to
  its end;
- `_bzero` with a known length that ends inside the allocation; the walk forgets those bytes.

Any other call on the way still stops the path. The nested owner's constructor evidence is the
inline code: from the allocation's return to the read, with a `ret` in place of the read. The
binding takes it before each insertion of the class when nothing between the allocation and the
read branches or calls indirectly, and nothing but register moves lies between the read and the
insertion. The walk uses that evidence only when every joined path's object was allocated where
one of those bodies starts; otherwise the owner keeps no constructor evidence and its persistent
members stay unjoined.

On M452 (`registry-fields/v48`):

| Field | Site | Class | Insertion |
| --- | --- | --- | --- |
| `common/technology` `technology_swap` | `CTechnology::ReadMember` `0x100c811b8` | `CTechnologySwap` (`0x350` bytes) | `CPdxArray<CTechnologySwap*>` at `+0x930` |
| `common/governments/civics` `swap_type` | `CGovernmentCivicType::ReadMember` `0x1007fc77c` | `CGovernmentCivicTypeSwap` (`0x1f8` bytes) | scoped pointer array at `+0x548` |
| `common/buildings`, `common/traits` `triggered_desc`; `common/situations` `triggered_blocked_desc` | `CBuildingType::ReadMember` `0x1000d12d4`, … | `CTriggeredEventDescriptionOption` | |

The inline calls are `_bzero`, `CRootTrigger::CRootTrigger(bool)`,
`CScriptableValue::CScriptableValue(EScopeType, bool)` and
`CCustomDescriptionModifier::CCustomDescriptionModifier()`. `technology_swap` now reads a block
(`Accumulate`) of nine members: `name`, `weight` (weight grammar), `category`, `trigger`,
`prereqfor_desc`, `inherit_effects`, `inherit_icon`, `area` and `modifier` (modifier grammar).
`prereqfor_desc` has no shared reader and `category` no repeat rule. `swap_type` reads a block of
all six config members (`name`, `trigger`, `description`, `negative_description`,
`hide_modifiers`, `modifier`) with no gap but `OutsideMethod` and the registry-wide use-time
condition that every nested owner carries. The three description fields read five members each;
`text` and `exclusive_trigger` have no shared reader, and one root path stays unfollowed.

Remove the rule, or limit it, when a build's inline construction passes an address other than the
object to a member constructor, when a scalar-parameter constructor writes through a global into
the owner, or when the walk can run member constructors itself.

`common/pop_jobs` `swappable_data` is not this shape. `CJobType::ReadMember` reads it through a
stack `CSwapCollection<SPopJobTypeSwap, …>::CSerializer` at `0x1008b45ec`; its `ReadMember`
(`0x1008bea40`) reads `default` through a stack `SPopJobTypeSwap::CSerializer` and each `swap_type`
into a new `CSwappable<SPopJobTypeSwap>` through two more stack serializers. That is the "Stack
serializers" row above, three levels deep; `common/pop_categories` has the same chain. SDK-866
owns it.

### Duplicate checks and owned pointers (SDK-850)

The collection pass (`fields/nested.rs`) joins three more shapes on M452. Each is a validated rule
with the condition for its removal.

- **A `Class const*` array.** The binding accepts `CPdxArray<Class const*, int>::InsertAtEmplace`
  as the insertion of a class that the root constructs. It does not find a class through that
  insertion alone: such an array also holds items that the root looks up (`CBuildingType::ReadMember`
  inserts `CTechnology const*` prerequisites), and taking `CTechnology` as an object filled the
  owner's function budget.
- **Calls that change no script state.** The pass goes on past the diagnostics, their locations and
  the string release that `readers::changes_no_script_state` names, and past `_memcmp`, which only
  reads its three arguments. Remove this when a build's diagnostic writes script state.
- **Loops.** The pass joins paths at loop heads (`run_paths_joining`). A path that arrives where an
  earlier state covers its own is not joined itself. That comparison is of known values only, so the
  path must still keep the sentinel and write no owner word outside the joined site; a loop that
  clears the collection's count stops the join (`resetting_collection_storage_does_not_establish_accumulation`).
- **One owned pointer.** With no array insertion, a path joins when the only owner word that it
  writes holds the object that it constructed and read: the field reads a block whose repeat is
  `Replace` (`CollectionField::owned_pointer`). A first run with the word unknown finds the word.
  Then two runs must both join and agree: one with the word null, one with a stand-in for the object
  that an earlier read stored there. The stand-in's vtable slot `+8` holds a sentinel address; only a
  call to it with the stand-in in `x0` is the release (the Itanium ABI puts the deleting destructor
  in that slot). A call through another slot stops the path. Remove the rule when a build releases
  the old object other than through its deleting destructor.

The duplicate check is the shape of `CPolicyGroup::ReadMember` `option` (`0x100a41720`):
`operator new(0x470)`, `CPolicyOption::CPolicyOption(CPolicyGroup const*)`, the read through slot
`+0x20`, a loop over the earlier options that compares names with a byte loop or `_memcmp`, on a
duplicate the "Option '%s' already exists" log, then the insertion at `+0x280`. The owned pointer is
`CShipSize::ReadMember` `ai_ship_data` (`0x1011ec5b8`): `PdxMakeScopedPtr<CAIShipData>`, the
release of the old object at `+0x48` through slot `+8`, the store, then the read.

Result on M452 (`registry-fields/v52`):

| Field | Members | Left |
| --- | --- | --- |
| `common/policies` `option` | 11: `name`, `icon`, `prerequisites`, `potential`, `valid`, `on_enabled`, `on_disabled`, `policy_flags`, `ai_weight` (weight grammar), `modifier` (modifier grammar), `in_breach_of` | `pre_sapient_purge` is on an unfollowed path of `CPolicyOption::ReadMember`; `prerequisites` has no shared reader |
| `common/ship_sizes` `ai_ship_data` | 5: `min`, `max`, `planet_mult`, `system_mult` (scoped numeric), `fraction` (weight grammar) | |
| `common/ship_sizes` `space_fauna_values` | 9 | `age_threshold`, `culling_value` have no shared reader |
| `common/patrons` `active_accord` | 15 | `activation_cost`, `desc` have no shared reader |
| `common/patrons/callings` `tier` | 3 | `counter` has no shared reader |
| `common/pop_faction_types` `demand` | 8 | |

The other fields of the ticket keep the `UnresolvedReader` gap "The field's alternatives do not
establish one shared reader." Their stops:

| Field | Stop on M452 | Shape and follow-up |
| --- | --- | --- |
| `common/country_types` `faction` | `CCountryType::ReadMember` stores the `SCountryInstanceProperties::CSerializer` address point (`0x10304b538`) and owner `+0x48` at `sp+0x48`, then calls `CReader::Read(CPersistent&)` (`0x10033f590`). | Joined as an [inline stack serializer](#stack-serializers) (`registry-fields/v74`); its keys stay unnamed behind `PropertiesReadMember`. |
| `common/country_types` `modules` | An inline statement loop (`0x10033f874`): `ReadSimpleStatement`, then `CreateCountryModule(CCountry&, int, CReader&)` with the statement key at reader `+0x38`, then virtual calls on the module. | Statement loop; SDK-871. |
| `common/map_modes` `color` | `operator new(0xf0)` of a struct with no vtable, built inline (`0x101728acc`), then an inline statement loop on reader `+0x38`. | Statement loop; SDK-871. |
| `common/ship_sizes` `section_slots` | `CPdxArray<SSectionSlot, int>::RemoveAll()`, then an inline statement loop (`0x1011eb478`). | Statement loop; SDK-871. |
| `common/ship_sizes` `hero_ship` | Two alternatives: a block read by `Read(CPersistent&)` of owner `+0xb8`, and `Read(bool&)`. The block's reader has no family because the `CShipSize::CShipSize(int, CString const&)` walk (`0x1011e98b8`) spends its 64-path bound in a zero-fill loop at `0x1011e9f64` whose base it loads from a heap pointer, so no constructor agrees on a reader at `+0xb8`. The same bound leaves `map_icon_override` without one. | Constructor walk bound; the field's members also need a rule for a field whose alternatives differ; SDK-873. |

### Objects constructed at an array's end

A root can grow an array of objects by one and read the new last element in place. The collection
pass (`fields/nested.rs`) joins that shape by a validated rule: **a call to
`CPdxArray<Class, int>::SetSizeAndEmplace<>(int, const&)` on an owner member constructs one
`Class` at the array's end, and a `CReader::Read(CPersistent&)` of that object reads it.**

- **Binding.** `collect_objects` finds the class through a direct call to the resizing, and takes
  its layout from the resizing's body (`emplace_layout`): the count is the one receiver word that it
  increments, the element size is the immediate that the `operator new[]` size multiplies
  (`mov wS,#size; umull x0,wN,wS`), and the element pointer is the receiver member that keeps the
  new buffer. Nothing between the size's `mov` and the call may branch or be a branch target, and
  on every path from the call the buffer's register is stored before it is written or the
  function returns. Every `operator new[]` call must agree. A class with an insertion or a moving
  insertion keeps the insertion's layout. `ObjectReader::array_emplace` and `array_element_size` carry the result.
- **Walk.** A first run finds the array at the resizing call and stops there. A second run starts
  with that array holding one element, and the call counts only when the array still holds it and
  the requested size in `x1` is two. The walk then reserves an object, installs the class's address
  points and constant pointers, and makes the array hold two elements with its pointer one element
  size before the new object. The root's own address arithmetic (`data + count * size - size`) then
  reaches the object only when the root's element size is the class's. The read counts only through
  `Read(CPersistent&)` with the reader in `x0` and that object in `x1`. The path may write no owner
  word but the array's pointer and count, and those must still hold what the resizing left.
- **Inherited and value readers.** The pass enters the inherited readers and value readers that the
  binding collects (`readers::is_inheritable_member`, `readers::is_member_value_reader`), and
  `collect_objects` scans their direct calls for classes, so an object that only the base reader
  constructs joins. A call through an import stub that `FieldInput::stubs` binds to an entered
  reader runs as a branch to its body. The walk also writes the token into the reader's token
  word, as the dispatch walk does, so a value reader that loads the token from the reader sees it.
- **M452.** `CComponentTemplate::ReadMember` (`0x1001dd284`) handles `hostile_aura` (token
  `0x3542`) by `SetSizeAndEmplace` (`0x1001f6858`) on `+0x7c8`, then `madd`/`sub` with `#0x278` to
  the last element and a tail call of `Read(CPersistent&)` (`0x1025b820c`); `friendly_aura`
  (`0x3543`) does the same on `+0x7e0`. The resizing reads the count at `+0x14`, multiplies the
  capacity by `0x278` for `operator new[]`, and stores the new buffer at `+8`.
- **Result on M452 (`registry-fields/v59`).** `common/component_templates`,
  `common/starbase_modules` and `common/starbase_buildings` change. Each of
  the three keys' `hostile_aura` and `friendly_aura` reads an accumulating block of eight
  `CShipAura` members: `name`, `graphics`, `radius`, `stack_info`, `apply_on`, `damage_per_day`,
  `system_wide`, `modifier`. Through the entered base reader, `triggered_ship_modifier` and
  `triggered_ship_design_modifier` of each key now read triggered modifier clauses. Through the
  stub of `CStarbaseComponent<T>::ReadMember`, `common/starbase_modules` and
  `common/starbase_buildings` each gain six triggered modifier clauses (`planet`, `country`,
  `station`, `system`, `waystation_network_system`, `waystation_network_country`), whose category
  masks stay `Unresolved` in `tests/expected/m452/modifier-containers.json`; the field sweep's known
  reader kinds go from 1866 to 1878. Every other registry is unchanged.
- **Remove or limit the rule** when a build's resizing constructs its new element other than by
  the class's default construction, when the root reads an element other than the last one, or
  when a resizing's element size comes from a computed value.

### Stack serializers

A reader can wrap an owner member in a serializer on its stack and read the block through it. On
M452 `CCountryType::ReadMember` reads `resources` (`0x10033f578`): `add x1,x19,#0x330;
add x0,sp,#0x48; mov w2,#4; bl CEconomicUnitTemplate::CSerializer::CSerializer(CEconomicUnitTemplate&,
EScopeType)` (`0x101d14b14`), then `add x1,sp,#0x48; mov x0,x20; bl CReader::Read(CPersistent&)`.
The constructor only installs its vtable at `+0`, stores `x1` at `+8` and the scope at `+0x10`.

- **Binding.** `stack_serializers` (`binding/binary/fields.rs`) takes each
  `*::CSerializer::CSerializer(…)` that an input function calls directly, when its body is
  `adrp/add` of the vtable point, `stp xV,x1,[x0]`, only stores of other arguments at `[x0,#16]`
  or beyond, and `ret` (`serializer_constructor_point`). The vtable's member slot gives the
  serializer's `ReadMember`, which joins the functions; the serializers that it builds are
  collected in turn, four readers deep (`CTriggeredResourceTable::CSerializer` inside the economic
  unit's reader). `FieldInput::stack_serializers` carries them.
- **Walk.** The call of such a constructor with a stack address in `x0` and an owner address in
  `x1` records the serializer, as the [modifier-block delegate](modifier-blocks.md) does. A
  following `CReader::Read(CPersistent&)` with the reader in `x0` and that stack address in `x1`
  stores a `Block` at the wrapped member (`+0x330`), with the serializer's `ReadMember` as its
  reader. The kind comes from that proven read, not from the reader's name: a serializer reader
  that another join names keeps no kind. Any other call while the serializer is recorded stops the path, as before.
- **Members.** When every path of a field joins one serializer reader, the field's members are
  that reader's keys, walked with the serializer as receiver and the wrapped member at slot `+8`
  (`persistent::embedded`).
- **Result on M452 (`registry-fields/v68`).** 46 fields go from an `Unknown` to a `Block` reader
  (`resources` in 33 registries, other resource tables such as `astral_actions`
  `activation_cost`, `megastructures` `dismantle_cost`, `pop_categories` `automation_resources`);
  47 nested paths appear, mostly `category`. The economic unit's other keys (`cost`, `upkeep`,
  `produces`) are compared by text with `_memcmp`, not by token, so they are not named yet. No
  command answer changes.
- **Inline serializers** (`registry-fields/v74`). A reader can build the serializer without a
  constructor call: it forms the vtable address point and stores it with the wrapped member's
  address as one pair on its stack, then reads the pair. On M452 `CCountryType::ReadMember` reads
  `faction` so (`0x10033ec24`: `stp x9,x8,[sp,#0x48]`, `x9` the
  `SCountryInstanceProperties::CSerializer` point, `x8` the owner plus `0x48`), and so do
  `CJobType::ReadMember` `swappable_data` (`0x1008b4600`, a
  `CSwapCollection<SPopJobTypeSwap,…>::CSerializer` that stores two more words at `+0x10` and
  `+0x18`), `CAgreementPreset::ReadMember` `term_data` (`0x100047b54`) and
  `CAgreementTermValue::ReadMember` `triggered_loyalty_change` (`0x10004e1e8`, a
  `CTriggeredLoyaltyChangeSerializer` that wraps the whole owner and stores a scope word at
  `+0x10`).
  - **Binding.** `inline_serializer_points` takes each address that an input function forms with
    `adrp` and `add` when it is an address point of a vtable of a class named `*Serializer`
    whose member slot holds that class's own `ReadMember`. `FieldInput::stack_serializers` keys
    such a serializer by its address point; a constructor's serializer stays keyed by the
    constructor's entry. The same four-reader recursion collects the serializers that their
    readers build.
  - **Walk.** An `stp` to the frame whose first register holds such an address point and whose
    second holds an owner address records the serializer, with the wrapped member at slot `+8`,
    as the constructor call does. A later store to a fixed frame slot outside the serializer's
    two words keeps it; any other store forgets it, for both forms. A store writes its registers'
    width, so a pair of `q` registers writes 32 bytes, and a store whose width or address (a
    writeback form) the walk does not read forgets it too. The persistent read and the
    members then follow the constructor rule.
  - **Result on M452 (`registry-fields/v74`).** Additions only: 19 fields go from an `Unknown`
    reader to a `Block` reader with a serializer identity. `term_data` gets 9 keys (one
    reference, six Booleans; `discrete_terms` and `resource_terms` stop at
    `NSerializer::ReadUniformWithCustomSerializer`), `agreement_term_values` `ai_acceptance` 2
    and `triggered_loyalty_change` 2 (unresolved), `pop_jobs` and `pop_categories`
    `swappable_data` 2 (`default`, `swap_type`, unresolved: a `_strlen` and an `operator new`
    while the serializer is recorded), `traits` `forced_integration` 2. The modifier fields that
    `CModifierWithTooltipData::CSerializer` reads (`buildings`, `pop_jobs` and `traits`
    `*modifier`), `spynetworks` `operations` (`CPdxArraySerializer`) and `resource_groups`
    `localization` get only the reader: their walks name no key.
  - **Stays a gap.** `faction` names no key: its serializer's `ReadMember` first passes the token in
    `w1` to `PropertiesReadMember(CReader&, int, CSimpleBitMask<…>&)`, a jump table over the
    property bits, which the walk does not follow. `global_ship_designs` `growth_stages` reads a
    list through `NSerializer::ReadUniformWithCustomSerializer<…>`, not a stack serializer.
- **Limits.** Remove the rules when a build's serializer constructor writes the wrapped member or
  reads more than its arguments, or when an inline serializer's reader takes the wrapped member
  from a word other than `+8`.

### Objects in a `make_shared` control block

A root can build an object with `std::make_shared`, read it and move the `shared_ptr` into an
array. The collection pass (`fields/nested.rs`) joins that shape by a validated rule: **an
allocation whose first word is the primary address point of
`std::__1::__shared_ptr_emplace<Class, std::__1::allocator<Class> >` holds a `Class` at `+0x18`,
and `CPdxArray<std::__1::shared_ptr<Class>, int>::InsertAtEmplace` moves the 16-byte
`shared_ptr` into the array.**

- **Binding.** `collect_objects` takes the control block's primary address point
  (`ObjectReader::shared_emplace_point`) and the `shared_ptr` insertion as a moving insertion with
  the bytes that it clears at its source (`ObjectReader::moving_insert`, a map from entry to
  width). `moved_source_width` proves the width: on every return the source that `x2` names on
  entry holds zero through `str xzr` (8) or `stp xzr, xzr` (16) with no offset or writeback,
  through a register that holds the entry `x2` on every path. A store through a register that may
  hold an address computed from it, or a call that may receive one, makes the source unknown. The
  binding also collects the class's `shared_ptr` destructors that the root calls
  (`shared_releases`); the walk runs their code.
- **Walk.** A constructor call counts on an object at `block + 0x18` only when `block` is an
  allocation's start and holds the control block's point at `+0`. The calls that build or set a log
  ID (`SetLogID`, `GetLogID`, the two `CString` concatenations) change no script state. The moving
  insertion zeroes the width that the binding proved at its source.
- **M452 checks.** `CEvent::ReadMember` (`0x1004bbc9c`) reads `option`: `make_shared` stores the
  control block's vtable `0x103049d18` point `+0x10` (`0x103049d28`) at the block, calls
  `CEventOption::CEventOption(EScopeType)` (`0x1004dc844`) at `+0x18`, sets the log ID, reads
  through slot `+0x20`, then calls `InsertAtEmplace<shared_ptr<CEventOption>>` (`0x1004c2d2c`) at
  `0x1004bc83c`. The insertion clears its source with `stp xzr, xzr`. Other callers of the same
  insertion shape: `CSituationType::ReadMember` `+0x998` and `CStarbaseLevelType::ReadMember`
  (two). `picture_event_data` joins through the `make_shared` at `0x1004c2f58`.
- **Result on M452 (`registry-fields/v62`).** Each of the 22 event kinds gains the members of
  `option` (16 `CEventOption` fields: `name`, `icon`, `sound`, `trigger`, `ai_chance`, `allow`,
  `response_text`, `custom_gui`, `exclusive_trigger`, `ai_trade_facility` and six `yes` tests)
  and of `picture_event_data` (9), 550 member paths, both accumulating blocks. The field sweep's
  unresolved paths go from 1271 to 1733: the new members' own stops. Every other registry is
  unchanged; the `shared_ptr` insertions of `CSituationType` and `CStarbaseLevelType` do not
  join yet.
- **Remove or limit the rule** when a build's `make_shared` puts the object at another offset than
  past the control block's vtable pointer and its two counts (libc++ `__shared_ptr_emplace`), or
  when a `shared_ptr` insertion keeps its source.

### Scripted actions: an order check before the token switch

`CScriptedActionInfo::ReadMember` (`0x100b10ae4`, M452) tests owner state before it tests the token.
It loads the `user_scope` word at `+0x48` and the `scope` word at `+0x40` and compares each with
`0x200000`, the value that the checks take as not yet read. The compare is `cmp x8, #0x200, lsl #12`; the
dispatch walker reads the shifted immediate since `registry-fields/v53`, and before that every path
stopped there, so the registry had no field. The order rule, from the engine's own messages:

- A key other than `user_scope` while `+0x48` is unread logs "First parameter for scripted actions
  must always be user_scope" and returns without a read.
- A key other than `user_scope` or `scope` while `+0x40` is unread logs "Second parameter for scripted
  actions must always be scope" and returns without a read.
- `user_scope` reads `GetScopeTypeEnumFromToken` of the value token into `+0x48`; a scope other than
  fleet or megastructure logs "Supported user_scopes for scripted actions are megastructure and fleet"
  and is stored all the same.

The dispatch walker follows the order rule with three general rules (`registry-fields/v56`):

- **Order check.** A `b.eq` or `b.ne` on a compare of a word loaded from the owner is an order
  check when the path has not yet read anything and one side only logs and returns. "Not yet read"
  (`reads_nothing_yet`): every instruction so far is in the reader, none is a call or a store outside
  the frame, and no reader idiom has started (stored, converted or copied value, value text, target,
  emplace). "Only logs and returns" (`logs_and_returns`): every way from the side runs only register
  moves and arithmetic, loads, compares, stores to `[sp]` or `[sp, #imm]` and branches inside the reader
  (`diagnostic_side_instruction`, an allow list), calls only functions that change no script state
  (`readers::changes_no_script_state`), calls at least one diagnostic writer (`readers::DIAGNOSTICS`;
  a location helper or a string release alone is not one), and reaches its own `ret`, within 128
  instructions over all ways, so a loop fails. The walk takes
  the other side and does not fork, so the key keeps no condition for the check. A side that writes
  the owner, calls anything else, or branches out of the function is no order check, and the walk
  forks as before (`a_rejecting_side_that_writes_the_owner_is_no_order_check`).
- **Settled comparison.** Each path keeps the outcome of the owner-word comparisons that it has
  taken, as `EqualsAny(word, pivots)`. Frame words and other values are never settled. A later `b.eq` or `b.ne` on the same comparison takes the
  recorded side. A call or a store outside the frame clears the record, because either can change the
  word; a compound reader call that the walk follows (such as a string emplace) clears it too.
- **Key order note.** The registry answer states each order check as an `OutsideMethod` gap that
  starts with "Key order:": the named keys that some path reads without crossing the check, and the
  field whose storage is the tested word, when exactly one field stores it. Atlas does not block on it; it is the only
  place that the order is stated, since the field read conditions leave it out.

Two related rules, in the same version: a call that changes no script state after the path called a
converter with the value token (other than a `CReader::Report*` call) does not stop the path, so the
converted value's store is followed; and the field read normalization collapses complementary
alternatives whose outcomes are the same `Stored` join with a destination, as it already did for
`Joined` and `Rejected` (`collapse_equivalent_branches` in `src/session/fields.rs`).

Result on M452: 41 of 42 fields read `[Always]` and 38 have a known reader. The answer has two key
order notes: "no key other than `user_scope` until `user_scope` is read" (`0x100b10b14`) and "no key
other than `scope`, `user_scope` until an earlier key sets the word that it tests" (`0x100b10b6c`).
The second check is reached only after `+0x48` is set: the first `user_scope` skips it, so the note
allows `user_scope`, but a repeated `user_scope` crosses it and is rejected until `scope` is read.
`scope` stores into `+0x40` through checks that the walk does not join, so the note names no field.
The command grammars change too: 189 commands lose a contradictory `All[Unresolved, Unresolved]`
alternative of the shared `Target` reader (the settled second test), and `kill_leader.ability` gains
one `Unresolved` alternative that now stops at `keyword-store` instead of at the diagnostic.

Limits, each with the condition for a change:

- Only `b.eq`/`b.ne` on a value compare are settled. A `cbz`, `tbz` or ordered compare of the same word
  forks again. Extend `settled_comparison` when a reader repeats such a test.
- The note names the key order, not which values are accepted: the tested pivot (`0x200000`, unread)
  is not part of the answer. Add it when Atlas needs to name the unread state.
- A key is held by the check only when every path of it crosses the check; a key with one path
  that skips it is allowed, although its other paths are rejected in the state that reaches the check
  (the repeated `user_scope` above). The note does not state that state. Add it when Atlas needs the
  order of a repeated key.
- Only the root reader of an owner walk follows order checks: the registry root, its collection
  objects and its embedded objects (`DispatchInput::follows_order_checks`). An order check inside a
  collection or embedded object is a note on the parent field. An inherited, delegated or wrapped
  child reader starts with a fresh state that does not show its caller's reads, so it forks at the
  check; so do member readers of a field and command grammars, whose answers have no place for the
  note. None of them has an order check on M452. Carry the caller's read state into the child when
  one does.
- Settled comparisons apply in command walks too, so the 189 command answers above change with
  `command-grammar/v24`.

Other stops in the same reader: the `0x31a` and `0x451f` paths store the value token's id at
an owner offset and return (`ldr w8, [x20, #0x278]`, `str`, `ret`); token `0x4662` stores a Boolean
from `cmp` of the value token with `0x251` and `cset`; `shortcut` calls
`CKeyEvent::GetKeyEventFromName` on a stack string; `cost` builds a
`CTriggeredResourceTable::CSerializer`; `specific_behavior` calls `ValidateSpecificCode`.

## Repeat behavior

`FieldShape.repeat` comes from one rule for each shared reader, in `session/fields.rs::repeat`.
Each read alternative gets its own value, and the field keeps it when all alternatives agree. A
rule applies only to a tail call: a call with an unexamined continuation proves no final storage.
A string read whose continuation only restores registers and the frame, then tail-calls a
function whose body is only `ret`, counts as a tail call (`fields/dispatch.rs`).

| Reader | Repeat | Engine fact (M452) |
| --- | --- | --- |
| Primitive readers (Boolean, integer, fixed-point, float, string), except `CPreTriggers::ReadValue` | `Replace` | The reader assigns one destination. A fixture repeats `unlocks_agenda`: two occurrences keep the second. |
| `NParserUtil::ReadTrigger<T>`, `ReadEffect<T>` | `Replace` | `ReadTrigger<CRootTrigger>` (`0x100047bd8`) logs `Duplicate trigger at '%s'` when the count at `+0x7c` is nonzero, calls `CTriggerCollectionBase::DeleteAll()` and reads into the same object. `ReadEffect<CEffect>` (`0x10020cc30`) logs `Duplicate effect`, deletes each child, clears the count at `+0x1c` and reads again. All five instantiations share the shape. |
| `CVariableValue::Read(CReader&, EScopeType)` | `Merges` | A literal replaces the literal slot; variable, trigger and script-value slots stay ([scoped numeric](scoped-numeric.md), live `repeat_*` cases). |
| `CReader::Read(CPersistent&)` of a weight block | `Merges` | The persistent read (`0x1025b820c`) tail-calls the virtual `Read` at slot `+0x20`. `CMeanTimeToHappen::Read(CReader&)` (`0x10092e3f4`) reads a bare value into `base` or calls `CPersistent::Read`, so `base` is replaced and earlier entries stay. |
| `CPreTriggers::ReadValue(CReader&, EPreTriggers)` | `Merges` | `0x1004d0008` logs `Pre-trigger "%s" is specified more than once` and still ORs the key's bit into the `yes` or `no` mask; no bit is cleared ([pre-trigger blocks](#pre-trigger-blocks)). |
| `CReader::Read(CPersistent&)` of a modifier block | `Merges` | All four variants reach `CPdxModifier<…>::Read` (`0x100071fcc`). It deletes the child array and clears the entry counts at `+0x1c`, `+0x44` and `+0x9c`, then calls `CPersistent::Read`. Fixed keys are written only when present, so an omitted icon, tooltip or flag stays; `description_parameters` appends. |
| `ReadKeyReference<D>` whose caller stores the item to an owner member, then returns | `Replace` | The immediate reader returns the item or the null object ([references](references.md#reference-readers)). Every M452 call is a `bl` followed by `str x0,[x19,#off]` (or `x20`) and the frame restore, directly or through a local `b` (`CShipSize` `ship_category`, `0x1011ec8c0`). The walk follows that continuation (`fields/dispatch.rs`, `stores_item_and_returns`), so the store is the path's last write. A tail call gives the item to the loader's caller and stores nothing; no M452 loader has one. |
| `ReadKeyReferenceDeferred<D>` with an established lookup | `Replace` | Each occurrence registers its key and a lambda that writes the destination on a hit and on a miss. The resolver runs registrations in order ([references](references.md#reference-readers)), so the last occurrence's item is stored. |
| `CTrigger::Read(CReader&, EScopeType)`, `CEffect::Read(CReader&, EScopeType)` | `Merges` | `0x100d07f98` and `0x100456ae4` clear nothing: they read each statement and add its child to the block they receive, so a second block's children join the first's. Contrast `ReadTrigger<T>`, which clears first. |
| `CReader::ReadUniform<T>(CPdxArray<T, int>&)` ([uniform lists](#uniform-lists)) | `Accumulate` | Each value is appended at the array's count: `InsertAtEmplace(count)` for an object element, and a grow-and-copy that keeps the old elements for an enumeration or `int` element. No instantiation clears the array (`CString` `0x10004e608`, `EScopeType` `0x100cb7588` and six more read). |
| `ReadKeyReferenceUniform<D, CPdxArray<…>>` | `Accumulate` | Each key is looked up and its item appended with `InsertAtEmplace(count)` (`CEthicDatabase` instantiation). The deferred list reader keeps `Unknown`. |
| `CReader::Read(CPersistent&)` into an object whose read is `CPersistent::Read` or `CPersistentName::Read` and whose two post-read hooks are the base's | `Merges` | `CPersistent::Read` (`0x1025b43bc`) calls `ReadWithoutInitPostRead`, a statement loop into the virtual `ReadMember` (`+0x28`) that resets nothing, then `InitPostRead()` (`+0x30`) and `InitPostRead(CString const&, int, int)` (`+0x38`); the base hooks (`0x1025b4440`, `0x1025b4444`) are one `ret`. `CPersistentName::Read` (`0x100a1c0d8`) clears its key and variables but not `literal` (`+0x30`). |

A Boolean read into a stack temporary gets `Replace` from the walk, not from a rule: the walk
follows the temporary into its member bit ([bit fields](#bit-fields)).

**Validated assumption of the persistent rule.** A member reader changes only the storage of the
key that it reads. Checked on M452 by reading the member readers of `CPreTriggers`,
`CPlacementRules`, `SCountryAISettings`, `CEspionageAssetAppearance`, `SProjectRequirements`,
`CGraphicalCultureSelector` (it clears only its own weight member), `CEmpireFlag`,
`CAIStarbaseDesign`, `SBeamFade`, `SFleetManagerSettings`, `SPersonalityBehaviour` and
`CDustCloudType::SAmbientObject`: none resets the object. Change the rule for a member reader that
does. An object whose hook is not the base's (`CLayeredIcon`, `CColonyShelterDatabase`,
`CDustCloudType::STextures`, `CDustCloudType::SParameters`) keeps `Unknown`: the hook is not read.

Every other reader keeps `Unknown`, and the storage gap names why:

| Detail | Fields (M452) | Cause |
| --- | ---: | --- |
| "waits for one shared reader" | 855 | No shared reader ([fields with no shared reader](#fields-with-no-shared-reader)). |
| "the read is not a tail call" | 51 | A primitive or scoped reader whose caller continues after it returns ([gate shapes](#repeat-gaps-of-gate-registries-sdk-854)). |
| "…or its lookup does not store every resolution" | 5 | An immediate reference reader whose caller does more than store the item; a deferred reader without an established lookup or a deferred list. |
| "no stated rule for this block reader, or the read is not a tail call" | 53 | A list or block read with a continuation, a weight block that is not a tail call, an overridden hook, an unexamined persistent read. |
| "waits for the reader's value form" | 7 | `CReader::Read(CColor&)`, `CReader::Read(CVector2FixedPoint&)`. |
| "belongs to the loader" | 22 | The [event kinds](#event-kinds): each statement builds a new `CEvent`, and the loader's duplicate handling decides which one is kept ([loader rules](loader-rules.md)). |

The counts are gaps at `registry-fields/v49`, all registries and nesting depths.

### Repeat gaps of gate registries (SDK-854)

Atlas credits a config `cardinality` maximum only from an unconditional read with a known repeat.
Over the 218 D1 gate types (config `8574760`), 287 such claims kept a repeat gap at pin move B
(Native `2c94b30`); 244 keep one at `registry-fields/v49`. SDK-854 credits 39: 28 planet-class
Boolean bits ([bit fields](#bit-fields)) and 11 immediate reference fields
([rule table](#repeat-behavior)). SDK-848's [inline-constructed
objects](#inline-constructed-objects-sdk-848) credit 4 more. The open claims, by the shape of
their field:

| Shape | Gate claims | Examples |
| --- | ---: | --- |
| No shared reader: a stack serializer (`CEconomicUnitTemplate::CSerializer` and other `X::CSerializer` constructors), or `CReader::Read(CPersistent&)` of a stack object with a vtable constant (`CStatInterval<int>::CSerializer`) | 63 | `country_type.resources`, `planet_class.planet_size` |
| No shared reader: an owned object (`operator new`, `InsertAtEmplace`, `SetSizeAndEmplace`, `PdxMakeScopedPtr`, `make_shared`) | 47 | `patron_type.passive_accord`, `archaeological_site_type.stage` |
| No shared reader: value text (`CString::CString(char const*)`, `strlen`, `CReader::Read(CString&, bool)` into a non-owner destination) | 21 | `job.purge`, `building.ruined_icon` |
| No shared reader: a persistent read with no constructor-agreed reader | 21 | `global_ship_design.name`, `ship_size.map_icon_override` |
| No shared reader: triggered descriptions (`CTriggeredEventDescription<…>::ReadEntry`) | 19 | `situation_type.title`, `building.desc` |
| No shared reader: alternatives that stop at a `blr` or join different readers | 18 | `country_type.modules`, `ship_size.graphical_culture` |
| No shared reader: one idiom each (log helpers, `SkipBody`, `COnActionList`, `CObjectiveCounterSet`, singletons) | 29 | `special_project.on_success`, `on_action.events` |
| Known reader, continuation: every `CAgreementPreset::ReadMember` path calls `CAgreementTermData::SetPresetData` before it returns | 8 | `agreement_preset.hidden`, `agreement_preset.potential` |
| Known reader, a block read with no stated rule (a list loop after `ReadUniform<CString>`, a presence byte, a persistent object whose reader has no rule) | 7 | `technology.feature_flags`, `situation_type.total_progress` |
| Known reader with no value form (`CColor`, `CVector2FixedPoint`) | 4 | `patron_type.color` |
| Known reader, other continuations: a derived path, a presence byte, `NShipMovement::UnscaleShipSpeed`, a second read, a deferred list | 6 | `building.icon`, `special_project.energy`, `ship_size.max_speed`, `planet_class.planet_custom_label_offset`, `ship_size.scripted_action` |
| The config field is not a root field of the registry | 1 | `resource.prerequisites` |

A field with no shared reader gets its repeat from the reader rule once its reader is known; the
sweep's `reader_identity_stops` names its stop ([fields with no shared reader](#fields-with-no-shared-reader)).
The resource tables are SDK-849's. A continuation that only writes other members (the preset
pointer, a presence byte) could earn `Replace` by a continuation shape like the one for immediate
references; each such shape is small.
Reproduce the claim set with Atlas `ledger` and `compare` from the pin move B snapshot, then join
each `cardinality_maximum` claim with the gap "Native did not establish unconditional repeat
behavior" to the Native field by the config type's `path`.

**A recorded manual exception.** The rules are stated per reader, not derived: the method reads no
reader or resolver body for storage. Conditions: the exact M452 build and a tail call. Obstacle:
fixtures decode no block or pointer storage, and no shape reads the clear-and-reread or the resolver
order. Removal route: block and pointer storage decoders, or body shapes for those two facts. The
checks are the disassembly above, the live scoped matrix, an M45-observe probe (repeated trigger
and effect blocks change their child counts, a repeated graphical modifier resets its entries and
keeps omitted metadata, two deferred keys resolve to the second), `cargo live
fixture_block_parsing`: a repeated `potential` logs `Duplicate trigger` on the second
occurrence's line, and a repeated `ai_weight` reads twice with no diagnostic, and `cargo live
fixture_repeat_readers` (SDK-627): an edict's repeated `potential` and `effect` and a
megastructure's repeated `upgrade_from`, `placement_rules`, `scripted_action` and `cycle_title`
each read twice. The second `placement_rules` logs `Duplicate trigger` for its
`planet_possible`, so the object kept the first block's trigger; no other line logs.

**The storage check** runs on the assembled answer, after every grammar is attached. A field at
any `FieldMembers::Fields` depth gets an `UnresolvedStorage` gap when its repeat is `Unknown`, with
the cause above, or when its members are `Unresolved` and its reader family is `Unknown`. A
loader's root key gets the loader's gap instead. A trigger or effect family is
the members answer; a weight, modifier or triggered modifier family whose grammar did not attach
already has the attach step's `UnresolvedReader` gap.

Pitfalls:

- `CMeanTimeToHappen::Read(CReader&, EScopeType)` (`0x10092e328`) resets `base` and the entries,
  but the persistent path never calls it. Read the slot that `CReader::Read(CPersistent&)` calls.
- A deferred reader's key string is a temporary; storing it proves nothing. The destination that
  the lambda captures, the resolver order and the lambda's write decide the result.
- A logged duplicate is not a rejection; the repeat is read.

**Storage observations (M452, 2026-10-20).** Fixture storage read back after two writes (`a b`,
`b a` and each alone) gives `Replace` for every one of the 20 root fields whose open gap was "the
read is not a tail call" and that has a storage binding: ship_sizes `max_speed`; buildings `icon`;
districts `gridbox`, `district_background`; starbase_buildings and starbase_modules `category`,
`custom_tooltip`, `custom_tooltip_with_modifiers`, `icon_frame`, `construction_days`,
`scripted_effect_cooldown`, `scripted_effect_cooldown_flag` (`icon_frame` has no open claim);
agreement_presets `parent`, `icon`; and the nested special-project `requirements.fleet_power`. Ten
controls with a static answer agreed. The continuation does not undo the last write, so a
continuation shape for these callers would be sound. Storage is a check, not a source: the same
run answered none of 10 minimum claims and none of 13 lookup value forms. Atlas owns the claims
and results: `atlas/docs/prototypes/storage-probe-spike/` (copy in
`.local/evidence/storage-probe-2026-10-20/`). Facts found on the way:

- `max_speed` is not stored as written: `7` stores raw 31500 and `1` raw 4500 at scale 100000
  (×0.045, after `NShipMovement::UnscaleShipSpeed`). The occurrence value already has the factor.
- A block given to a direct numeric reader (`rotation_speed = { native_probe = 1 }`) logs
  `Malformed token` and `Unexpected token`, stores 0, and its closing brace ends the definition:
  the definitions after it in the file are not constructed.
- The template fixed-point reader stores 0 for `yes` with no diagnostic; the direct readers log
  `Malformed token` for `yes`, a bare word and a quoted word and store 0. The direct `int` reader
  stores `1.5` as 1 with no diagnostic.
- `Ship size '<key>' contains 'icon' entry that point to unknown sprite '<text>'.File '<file>',
  lines [<a>-<b>].` arrives during file load as `engine-parser-log` and does not join: the source
  join reads `File: '…', Lines: […]`, not this spelling.
- A ship-size fixture through validation replaces every vanilla ship size: 19,350 `error.log`
  lines, the pause at 171 s, and no fixture terminal (storage `Partial`). Use the file-load window
  for registries that other content references.
- Only the ordinary log names two lookups: `base ship size <key> not found in database.`
  (`ship_size.cpp:726`, file load) and `Invalid ship size for names <key> in ship size <key>`
  (`ship_size.cpp:1257`); neither has a file or line. Sprite, localisation, sound, entity and
  behavior fields give no message for a missing key.

## Required fields

`Field.requirement` says whether a definition must contain a root field. A field is required only
where validation reports its absence (decision 5 of the
[simplification decision](../design/simplification.md)): a missing field keeps its constructor
default, and the loader keeps the definition. The answer comes from one recorded census run per
build, in `binding/requirements.rs`; a build without a census gives `Unknown` everywhere.

| Requirement | Rule | Root fields (M452) |
| --- | --- | ---: |
| `Required` | An unconditional check logs the field's absence | 30 |
| `Cases` | A traced check depends on another field's value (one case for each value of its condition that the trace states), or the subtype census probed the registry (one case for each probed selector state, and one for the state that the empty definition stands for) | 267 |
| `Unknown` | The check depends on another field's value with no traced or probed cases; the registry is outside the census; or the field is below a definition's fields | 243 |
| `Optional` | Every other root field of a census registry | 1,846 |

Counts at `registry-fields/v73`. A definition that meets no case of a `Cases` answer has no
established requirement.

**A recorded manual exception.** Claim: loading a definition without a field logs a report of
its absence, or logs none. Conditions: the exact M452 build, and a registry whose empty
definition the census run read. Obstacle: the checks are one-off member tests in post-read hooks
with no shared shape (below), and the method cannot join a tested member to its key. Removal
route: a static reading of those hooks. Change the census for a new build by running it again.

**Census run (M452, 2026-10-14).** A probe launched the installed game once, with the window
guard, a private `-userdir` and Native's host lock, and a mod that adds one empty definition
`native_required_N = { }` to each of the 185 registries of the field sweep
(`common/special_projects` takes `special_project = { key = … }`; `events` takes a namespace and
one `country_event`; `flags` and `common/scripted_variables` hold no blocks and are skipped). The
same mod with no files logs one line in `error.log`; the probe logs 239. A second run adds an
unknown key `native_required_witness = yes` to each definition: the absence reports are the same,
and 163 registries report the unknown key, which shows that they read the definition. The census
is those 163, less `common/species_names`, whose report joins no key (the definition key is read
as a species class). Two reports that first joined no key are joined by their emitting code:
`common/diplomatic_actions` `Diplomatic action is missing token` tests the definition key, not a
field (see the shapes below), so every root field is `Optional`; `common/astral_rifts` `astral
rift possible stage #%i: Invalid event ''` comes from `CAstralRiftType::PostReadInit()`
(`0x100f438c4`), which looks up the string at `+0x88` with `CEventManager::GetEvent` and logs when
no event is found. `CAstralRiftType::ReadMember` reads token `0x1b8` (`event`) into `+0x88`, so
`event` is `Required`; all 32 vanilla rifts set it. Probe, logs and joins:
`.local/evidence/sdk-829-required-fields-2026-10-14/`.

- **Crashes.** An empty `common/technology` definition stops the game in
  `CTechnologyDatabase::PostReadInitInstance()` (inferred: an array insert at the index of the
  missing `tier`), and an empty `common/tradition_categories` definition in
  `TSingleObjectGameDatabase<CTraditionCategoryDatabase, …>::PostReadInit()`, before any item's
  hook logs. Both registries are outside the census. A crash is not a report: tradition
  categories' `adoption_bonus` and `finish_bonus` are `Required` from the static check in
  `CTraditionCategory::PostReadInit()` (`0x100cda348`), which logs `is missing required field
  '…'`, and no vanilla category omits them.
- **Joins.** Each report is joined by hand to the key that its text names, and where the text
  names no returned field, to the field whose storage the logging code tests. A report about a
  localisation key, a sprite, a texture or an entity is not a field check. A report whose key is
  not a returned field leaves the other fields `Optional`: special_projects `event_scope`,
  mutations `type`, war_goals icon, colony_automation_exceptions category, federation_laws
  category, patrons category, patrons/deeds attunement and counter,
  archaeological_site_types visible trigger, economic_plans income goals, zones `zone_sets` and
  buildings, districts zone slots, and technology `tier`.
- **Conditional checks.** Vanilla content logs nothing, so a vanilla definition that omits a
  candidate field shows that its check depends on another field. `buildings.building_sets` (65 of
  498 omit it; `CBuildingType::InitPostRead()` tests `owner_type`), card_categories `background`,
  `background_expanded` and `icon` (`core_focus`, `selectable = no`), ship_categories `namelist`
  (`uses_name_list = no`) and missions
  `icon` and `counter` (51 of 102) are `Unknown`. Planet_classes `icon` and `icon_large`
  (`CPlanetClass::PostProcess()`, a conditional check that the empty definition does not reach)
  take the subtype census's cases below. Resolutions `resources` (the report is about a category
  inside it) and the six keys of which traditions and ascension perks need one (`Missing effects
  tradition`) are `Unknown` too.

| Registry | Required fields (M452) | Report |
| --- | --- | --- |
| ai_budget | `category`, `resource` | `No category type defined for budget entry`, `No resource type defined…` |
| archaeological_site_types, espionage_operation_types, first_contact | `on_roll_failed` | `has no on_roll_failed, … will never progress` |
| buildings | `category` | `No category assigned to building type` |
| colony_automation | `category` | `has a missing or invalid category` |
| country_focus/card_categories | `name`, `desc` | `has empty 'name' field`, `… 'desc' field` |
| country_focus/focus_rewards | `category` | `The focus reward type has an invalid category value` |
| ethics | `category`, `cost` | `Missing category for ethic`, `Non-positive cost for ethic` |
| galactic_focuses | `effect`, `trigger` | `has no effect set!`, `has no fulfilled trigger set!` |
| game_concept_categories | `name`, `icon` | `has empty 'name' field`, `… 'icon' field` |
| mutations | `icon`, `ship_components` | `has no icon`, `No components are attached to mutation` |
| patrons | `text_color_code` | `doesn't have a text_color_code` |
| patrons/callings | `tier` | `A calling needs at least one tier` |
| portrait_sets | `species_class` | `doesn't reference a valid species class` |
| resolution_categories | `resolution_types`, `group` | `does not contain any resolutions!`, `Could not find resolution group ""` |
| situations | `stages` | `No stages detected in situation_type` |
| species_classes | `graphical_culture` | `is missing graphical culture` |
| specimens | `type` | `Invalid specimen type:` |
| star_classes/randomizers | `stars` | `random_list does not contain any star classes!` |
| tradition_categories | `adoption_bonus`, `finish_bonus` | `is missing required field '…'` (static) |
| war_goals | `casus_belli` | `lacks a Casus Belli` |
| events (each kind's members) | `id` | `Event at %s has no ID.` (static, [event members](#event-kinds)) |
| map/galaxy | `stars_min_dist` | `has stars_min_dist = 0 (expected positive)` |

The ethics `cost` and galaxy `stars_min_dist` checks test the default value, which an absent
field keeps; they report absence too. No vanilla definition omits a field in this table.

**Static census.** Before the run, absence checks were found by their log text: every string
with "missing", "not set", "not specified", "must have", "must specify", "lacks" or "no … set",
then the functions that form its address (`inspect --strings`). That found 19 unconditional and
14 conditional checks; the run found the ones whose text does not use these words.

**Shape.** No two checks share code, and they do not share one form, so no general method reads
them. Each logs its own message through `CPdxLogFileAndLine` or `CLogger::Log` with `CLogStream`.
The forms on M452:

- A member tested against its default (a null pointer, a zero count, `-1`, an empty string). A
  count lives inside the member's type: `CGalacticFocus::PostReadInit()` (`0x10057b620`) tests
  the effect's count at `+0xd4` (`+0x1c` inside the `CRootEffect` at `+0xb8`) and the trigger's
  count at `+0x294` (`+0x7c` inside the `CRootTrigger` at `+0x218`), each with `cbz` to its log.
- An equality between two members (`CEspionageOperationType::InitPostRead()`, below).
- A value test inside a switch on another field, which the field's default fails (patrons
  `position`, below).
- A helper shared by several fields that receives the field's storage as an argument
  (`CPatronType::ParseCountryEventID`, below).
- A virtual call on a member pointer: `CResolutionType::PostReadInit()` (`0x100ae737c`) calls
  slot `+0x10` of the object at `+0x40` and logs `resolution.cpp:101` when bit 0 of the result is
  clear. The target is not resolved.
- A test of the definition key, not of a field: the `CDiplomaticActionType(int, CString const&)`
  constructor (`0x10041c9b8`) looks the key up with `CStaticLexer::FindTok` and logs `Diplomatic
  action is missing token` through `CLogger::Log` when the stored token at `+0x48` is `0xc`
  (`0x10041cbf4`). Its `PostReadInit()` has no field check, and the witness run read the item, so
  every root field of `common/diplomatic_actions` is `Optional`.

The same two loggers make 1,230 calls from 877 functions named `InitPostRead`, `PostReadInit`,
`PostProcess`, `PostInit`, `PostValidate` or `Validate`; most report a bad value, not an absent
one.

**Traced conditional checks (M452).** A check that depends on another field's value is not
observed by the census's empty definition. A static trace of its hook on the exact build states
the condition, and the census publishes it as `FieldRequirement::Cases`, one case for each side
(`traced` in `binding/requirements.rs`):

- `CEspionageOperationType::InitPostRead()` (`0x100497188`) compares the stored `stages` (`+0x6e8`,
  `CReader::Read(int&)`, default 0) with the size of the `stage` array (`+0x54`; each `stage` is
  inserted at the end with `InsertAtEmplace`) and logs `does not have the expected number of
  stages.` (`espionage_operation_type.cpp:407`) when they differ. `stage` is required when
  `stages` is not zero: cases `FieldZero { stages, zero: true }` (not required) and
  `zero: false` (required).
- `CPatronType::PostReadInit()` (`0x100a13198`) switches on the category byte (`+0x40`; minor 0,
  major 1, great 2; the constructor stores 3, unset) through a byte table at `0x102ca81f9` and
  `br x9` at `0x100a1349c`. Each set category tests `position` (`+0x110`, `+0x118`, fixed point
  ×100000): minor and great log when x² + y² is above 1, and major logs unless the position is a
  unit step on one axis. The constructor stores (-1, -1) (`0x100a12724`), which fails every test,
  so `position` is required for every set category: three cases, one `FieldKeyword` for each
  category keyword. An unset category logs its own report and tests no position, so it meets no
  case and has no established requirement.
- `CPatronType::ParseCountryEventID(CString const&, CString const&)` (`0x100a1442c`) logs
  `expected a %s event to be specified.` (`patron_type.cpp:451`) for an empty event string. Its
  callers in the hook (`0x100a13c00`, `0x100a13c68`) make `first_contact_event` (`+0x278`)
  required unless the category is great, and `covenant_event` (`+0x2a8`) required only for major;
  a great patron parses a covenant event only when one is given. The cases are
  `FieldKeyword { category, great_patron }` (`first_contact_event` not required; any other value
  required) and `FieldKeyword { category, major_patron }` (`covenant_event` required; any other
  value not required). `CPatronType::ReadMember` stores the category byte from the value token
  (`minor_patron` 0, `major_patron` 1, `great_patron` 2, at `0x100a13030`). `category` is not a
  returned field of the registry (its path stops at a branch condition), so the condition names
  the key that the definition writes.

**Subtype census (M452, 2026-10-20).** A second run of the same probe
(`run-subtypes/summary.md` in the evidence directory) added, for each subtype of the Atlas
claims, one definition that sets the subtype's selector fields and nothing else, and read all of
them. Each registry whose selectors are all stored Booleans, numbers or keywords gives every
root field except the selectors `Cases` (`subtypes` in `binding/requirements.rs`). A registry
with any other selector keeps its empty-definition answer: the probe set that selector only at
its default, so a case that leaves it out would also cover its unprobed values.

- One case for each probed definition: the stored state of its selectors, required where the run
  reported the field absent.
- One case for the state that the empty definition stands for: each probed keyword not held, or
  the Boolean or number zero. Its answer is the empty definition's (`Required` or `Optional`);
  a field whose empty answer is `Unknown` gets no such case. Where both values of a Boolean were
  probed (civics `is_origin`), there is no other state.
- Where a registry has two or more selector fields, each case states every selector, so the
  cases do not overlap. This needs every selector's default to be zero, read from the
  constructor: `CPlanetClass` stores `0x62432207b` at `+0x4a0`, which clears `colonizable`
  (bit 7), `star` (bit 10) and `ringworld` (bit 22). `CDepositType` also clears its two Boolean
  selectors (`is_for_colonizable` at `+0x990`, `is_null` at `+0x998`), but deposits keep their
  answer for the `category` selector below. A combination that no definition probed (a
  colonizable star) meets no case.
- Keyword selectors were read in the member reader: `CEspionageOperationType::ReadMember`
  stores `target` as an enumeration at `+0x6c8`; `CSituationType::ReadMember` stores
  `progress_direction` at `+0xb34` from `bidirectional` (`0x41e5`) or its other keyword
  (`0x41e4`) and logs any other value.

The probed definitions changed one answer from the empty definition's: planet_classes `icon` and
`icon_large` are required for a colonizable class (`Colonizable planet class … requires a set
icon`, `CPlanetClass::PostProcess()`). Every other probed field keeps its empty answer under
every case.

Registries left as today (known gaps), with the selector that no `FieldCondition` states:

| Registry | Subtype | Selector | Why |
| --- | --- | --- | --- |
| agreement_presets | `has_parent` | `parent` | a key reference |
| agreement_term_values | `resource` | `term` | a key reference |
| deposits | all three | `is_for_colonizable`, `is_null`, `category` | `category` is a key reference |
| megastructures | `has_prereqs` | `prerequisites` | a block's presence |
| missions/missions | `contract` | `category` | a key reference (the `icon` and `counter` checks skip contracts) |
| opinion_modifiers | both | `trigger`, `block_triggered` | `trigger` is a block's presence |
| ship_sizes | all five | `class`, `accepts_all_modifiers`, `is_bio_ship`, `carries_colony`, `hero_ship` | a key reference and a block's presence, and the keyword default of `class` (`ReadShipClass`, `+0x780`) is not shown to be zero |
| strategic_resources | `max`, `tradable` | `max`, `tradable` | the constructor stores -1 (`-100000` fixed point) at `max` (`+0x108`), so its default is not zero |
| traits | both | `leader_class`, `auto_mod` | `leader_class` is a list's contents |

**Check.** `cargo live fixture_required_field` loads two card categories through validation: the
one without `name` logs `has empty 'name' field` on its own line, the other logs nothing about
it, and `registry_fields` gives `name` as `Required`. The message names its definition as `File:
'<file>', Lines: [<first>-<last>]`; the fixture joins that form to the first line.

**Atlas mapping.** `Required` answers a config minimum of 1; a larger minimum is a count, which
this answer does not give. `Optional` answers a minimum of 0. `Unknown` answers neither. A
`Cases` answer gives a minimum only where Atlas can show a case's condition holds (Atlas
decision).

Accepted limits: a check that runs only when another field has a value is not observed for the
fields that an empty definition leaves at their defaults, so such a field can be `Optional` here
and required in context. A check at use time, not at load, is not validation.

Pitfalls:

- A missing-asset message (a sprite, a sound or a localisation key) tests the resolved asset, not
  the field. The field can be present with a wrong name.
- Run-time messages, such as `Ship size %s was missing ai_ship_data` from
  `CShipSize::GetAIDataRawFactor`, are not validation.
- A registry that reads no unknown key is not a witness: a silent run proves nothing about a
  definition that the loader never read.
- Join a report by the code that logs it, not by its text. `has no fulfilled trigger set!` tests
  the `trigger` storage; no key is named `fulfilled_trigger`. The census first read it as a
  non-field and left `galactic_focuses.trigger` `Optional`; every vanilla focus sets it.
- A check on a value also catches absence when the default fails the check: patrons `position`,
  ethics `cost`, galaxy `stars_min_dist`.
- An empty probe item with no selector field (a patron with no `category`) skips the checks that
  depend on it. `Optional` from such an item does not cover definitions that set the selector.
- A count lives inside the member's type: `+0x1c` in `CRootEffect`, `+0x7c` in `CRootTrigger`.
  Do not read the owner offset of the test as the member's start.

## Keyword domains (SDK-828)

M452 executable `c621723d9c8e0c1cd153319208d30a9dfbb9e63675be86f9d0ae7debeaa7fe1b`. A converter is
a pure switch from a token to an engine enum: `E TokenToEnum<E>(int const&)` reads the token
through a pointer, `Get…FromToken(int)` takes it in `w0`. Each returns one fallback for every
token that it does not compare, for example 4 from `TokenToEnum<ERarity>` (`0x1006c64fc`) and 2
from `GetTechAreaFromToken` (`0x1006c3f18`).

**Reader.** When a path calls a converter with the value token, as the stack word that a
`TokenToEnum` argument points to or as `w0`, and stores the result in the owner, the field's
reader is `Keyword` with that destination and `Replace`: the store assigns one destination. The
converter is the reader identity. Branches after the call test the result or later state, so
they are not read conditions. A path that calls the converter and stops later keeps its field
with an unresolved reader; a path that returns without the store stops at `keyword-store`. A
converter of another token keeps the `reader-routing` stop. Command children go through the same
walk, so 28 command answers gain `Keyword` keys (`create_ship` `rarity`, `give_dna`,
`count_tech_options`); the command method states value domains as `OutsideMethod` and computes
none.

**Method.** `fields/keywords.rs` runs the owner's member reader concretely for the key and each
value token from 0 to the largest literal token, and the first value after it, which stands for
every name that the engine does not know. The converter runs its own code, and the
[weight method's](weight-blocks.md#keyword-domains) diagnostic calls mark a report. A value is
accepted when some path returns with no diagnostic, and reported when every path writes one,
whatever the caller stored: a reported fallback is a recovery, as `mode`'s is. When the
run-time token is reported, the accepted values are the domain; when it is accepted, the caller
keeps the fallback of an unknown name with no diagnostic, and the domain stays `Unknown` with the
gap "An unknown name keeps the converter's fallback with no diagnostic." Every path must return
through the converter, or the gap is "A path of the caller could not be followed for some
value."; every accepted value needs one literal name, or the gap is "An accepted value has no
single literal name."

Validated rules, from the weight method: the run-time token behaves as any unknown name, and a
call that receives neither the owner nor the reader changes no script state; a call that receives
either stops the path. A diagnostic, its location, the log line's constructor and a string
release change no script state, so the walker goes on past them after the store too
(`readers::changes_no_script_state`). "Some path accepts" is also a validated rule: on
`CEconomicCategory` (`0x101c383e8`) a stored category logs `modifier_category … not specified
before triggered … modifiers` when an earlier key set an owner flag; that diagnostic is about the
key order, not the name.

Accepted limit: the owner's `InitPostRead()` is not run, so a fallback that a hook would report
and replace keeps `Unknown`. On M452 no hook of these owners reads the stored offset, except
`CBuildingType::InitPostRead()` (`0x1000d9964`), which reads `owner_type` to log a missing
`building_sets` and overwrites nothing. Change this for a build whose hook reads a keyword's
offset.

| Fields (M452) | Converter | Domain |
| --- | --- | --- |
| buildings `category` | `GetBuildingCategoryEnumFromToken` | `Listed`, 9 names. A fallback 0 logs `Failed to get any building category for this building…`. |
| scripted_modifiers `category`, economic_categories `modifier_category` | `GetModifierCategoryFromToken` | `Listed`, 24 names. A fallback 0 for a name other than `none` logs `Invalid modifier category '%s' at %s`. |
| situations `category`, `fail_category`, `complete_category` | `TokenToEnum<EPolarityType>` | `Unknown`: an unknown name is kept as 2, the value of `neutral`. |
| authorities `election_type`, `advanced_authority_swap.election_type` | `GetElectionTypeFromToken` | `Unknown`: kept as 0, the value of `none`. |
| mutations `component_slot_size_to_disable`, ship_sizes `mutation_components_size` | `TokenToEnum<EComponentSlotSize>` | `Unknown`: kept silently. |
| global_ship_designs `ship_owner_type`, pop_jobs `possible_precalc`, traits `leader_trait_type` and `category`, buildings `owner_type`, specimens `rarity`, target_types `target_type`, technology `area` | their own converters | `Unknown`: kept silently. |
| ship_sizes `construction_type`, special_projects `requirements.shipclass_starbase` | `TokenToEnum<EShipConstructionType>`, `GetShipClassEnumFromToken` | No reader: the argument is a lexer token appended to an array, or the key token, whose result indexes the storage. |

The parity test `enum_fields_list_their_names_or_say_why_not` checks each row and compares two
domains with the config (cwtools `8574760`). `enum[building_category]` also names
`planet_buildings`, `resources` and `amenities`, which the converter does not compare, so the
engine logs that the building has no category. `enum[scripted_modifier_category]` names `pop_job`,
which the engine reports, and lacks `galactic_community`, `storm_influence_field` and
`waystation`, which it accepts. `cargo live fixture_enum_domain` loads an economic category with
`modifier_category = planet` and one with `modifier_category = native_unknown`: only the second
logs, `Invalid modifier category` on its line. (`common/scripted_modifiers` has no fixture loader
and owner boundary, so the live case uses the other field of that converter.)

M452, SDK-828: before at Native `af4fb57` (`registry-fields/v41`), after at `registry-fields/v42`; the counts are from that pair, before SDK-820, SDK-822 and SDK-824 merged.

| Count | Before | After |
| --- | ---: | ---: |
| `Keyword` fields / with a `Listed` domain | 0 / 0 | 18 / 3 |
| `UnresolvedReader` "no shared reader" gaps (fields, root and nested) | 763 in 97 registries | 746 in 95 |
| Root fields found | 1,926 | 1,930 |
| Gaps other than `OutsideMethod` | 3,662 | 3,633 |
| `UnresolvedStorage` gaps | 916 | 899 |
| Complete registries | 18 | 19 |

Pitfalls:

- A converter's fallback can be the value of a name it compares, as `neutral` and `none` are
  above. Listing the names whose result is not the fallback would drop them; decide by what the
  caller does with an unknown name.
- `GetShipClassEnumFromToken` in `SProjectRequirements::ReadMember` receives the key token
  (`x2`), not the value token: the key selects a slot.

## Compiler jump tables

The compiler chooses between a jump table and direct comparisons on each build, so the method
reads both. On M45-observe (the 4.5 beta), `CMegaStructureType::ReadMember` used a table for
tokens 18066–18115 (`overclock_loc_key`, `overclock_cooldown`, `dismantle_possible`,
`dismantle_potential`, `should_ai_dismantle`); the release compiler used direct comparisons for
these five.

On M45-release, `CMegaStructureType::ReadMember` at `0x101122c70` dispatches two token ranges
through halfword jump tables:

```text
mov  w8,#-base            ; 14112 at 0x101122ca4, 17766 at 0x101122dd4
add  w8,w2,w8             ; the index: token - base, a zero-extended word
cmp  w8,#last             ; 188 and 19
b.hi <out of range>
adrp x9,<table> ; add x9,x9,#off   ; 0x102cd49d8 and 0x102cd4b52, in __TEXT,__const
adr  x10,<entry base>     ; 0x101122ccc and 0x101122dfc
ldrh w11,[x9,x8,lsl#1]
add  x10,x10,x11,lsl#2
br   x10
```

165 of the 189 entries in the first table, and 12 of the 20 in the second, reach `0x101123830`,
which tail-calls `CPersistent::ReadMember`, the verified base rejection. The `b.hi` side is split
by unsigned intervals of `token - base`, and it holds more direct comparisons
(`dismantle_cost`, `ai_weight`).

**An unknown index (SDK-812).** A path walk forks to every case of a table whose index is unknown
only when the code before the `br` proves the bound in this straight-line shape: the guard
`cmp wJ,#K; b.hi`; then only moves, address arithmetic and loads that write no base register,
which set no flags and do not branch; a frame that began at or before that run, and no direct
branch into it; and the index copied once from `wJ` by a 32-bit `mov` after the guard or, with no
copy, set by a 32-bit write before the `cmp`, so its upper half is zero. The adr's base must not be
the index or the table register, and the load must not overwrite the base. Any other shape keeps
the `branch-value` stop, and one path of `Machine::run` stops there too. Assumption: a branch
through a register does not enter a guard run. Checked on `CSpatialObjectRefCaster::FillEventScope`
(byte table, 11 cases, `mov w9,w0`), `CFleetOrbitalSlotHandle::ExecFunctor<SSetupScopeObjectFunctor&,
CEventScope&>` (byte table, 4 cases, `sub w8,w8,#1` before `cmp w8,#3`) and
`CreateFleetOrderButtons` (halfword table, 39 cases, `mov w8,w0`). On M452 it changes block entry
contexts, one `derived_names` gap, one on_action and five game-rule gaps
([SDK-812](#block-entry-context-stops-on-m452-sdk-812)).

**The default case.** The rule is in the module comment of `fields.rs`. Pitfall: "the most
frequent target is the default" is wrong. `CMissionType::ReadMember` has a table for tokens
11653–11656 with no default slot, and `on_fail` and `on_cancel` share one case that reads the same
effect member (`+0x430`); that case ties for the most frequent target. When a wide interval that
leaves through the table's guard does not end at the rejection, the default may be past its end,
so a case is kept only when it joins a known reader. No table on M45-release gives a `JumpTable`
gap: each default reaches the rejection, or the table has no default slot.

### Bit fields

Boolean bit fields such as `tooltip_show_star_resources`,
`place_entity_on_planet_plane`, `use_planet_resource`, `can_prevent_crisis_terraformation`,
`is_ruined_orbital_ring` and `hide_name` copy the bit to a stack temporary with `ldr` and `ubfx`,
or with `ldrb w8,[x19,x8]` where `x8` is a constant, then call `CReader::Read(bool&)`. The walker
reads a register-offset load with a constant index and forgets the result of `ubfx` and `and`, so
these paths reach the call and name their fields. `CReader::Read(bool&)` joins a stack
destination, as the 4-byte integer reader does.

After such a call the walk goes on (`fields/dispatch.rs`, `follow_temporary`, SDK-710). On M452,
240 of the 242 paths merge the temporary into its member with one idiom:

```text
bl    CReader::Read(bool&)       ; x1 = sp + t
ldrb  w8,[sp,#t]                 ; the value read
ldr   w9,[x19,#off]              ; the owner word: ldrb, ldrh, ldr w or ldr x
and   w9,w9,#~(1 << k)           ; clears exactly bit k
orr   w8,w9,w8,lsl #k            ; no shift for bit 0
b     <shared store>             ; optional
str   w8,[x19,#off]              ; same width and offset as the load
ret                              ; after ldp and add sp restores
```

or copy the byte itself (`ldrb w8,[sp,#t]; strb w8,[x19,#off]`). The path then joins
`Stored { CReader::Read(bool&), Boolean, destination, Replace }`: the bit gets the value that the
reader wrote, so a second occurrence replaces the first. `destination` is the member's offset for
a copy and `None` for a bit, because a storage offset names a whole word, and the presence test
of [read conditions](#read-conditions-and-use-time-inheritance) would read a byte test of the word
as a test of the bit. Any other instruction on the way, a call, a second store to the word, a
register loaded from the temporary before the call, an `ldp` that moves the stack pointer, or a
path bound keeps the plain join at the call. Root Boolean fields with repeat `Unknown` fall from
243 to 5.

`CPlanetClass::ReadMember` keeps its temporary at `sp + 0` (`strb w8,[sp]; mov x1,sp`) and merges
64-bit words (`ldr x9`, `and x9,x9,#~(1 << k)`, `str x8`). The call forgets the registers that
hold a value loaded from the temporary, but keeps one that holds its address, such as `sp`; the 32
planet-class Boolean fields join their bits (SDK-854).

Two temporaries keep the join at the call, with a `Repeat behavior remains unresolved.` gap:

- `common/governments/civics` `pickable_at_start`: the temporary is never stored; the path restores
  the frame and returns.
- `common/global_ship_designs` `alliance_ship_design`: the copy (`str w8,[x20,#0x114]`) is followed
  by a second read of the value token's type and, for a block, `CReader::ReadUniform<CString>`.

The other three (`common/agreement_presets` `hidden`, `should_ai_use_for_proposals`,
`can_preset_be_changed`) read straight into the owner and then call
`CAgreementTermData::SetPresetData`, so their call is not a tail call.

## Members and shared readers

A field's reader identity, kind and family come from every token path whose interval holds the
field's token, not only from its singleton paths. A wider path that is unresolved, or reads with
another reader, leaves the shared claim unknown; each read alternative keeps its own reader.
Rejected paths do not change the claim. Storage offsets, storage selections and derived-name
storage come from the singleton paths only, so they hold only when `partition_accounted` is true:
an overlapping wider path could join another reader, and then no offset is proven. No M45 registry
has an overlap. A condition on a nested field, on its reference lookup or
on a modifier-block fixed key names the full field path from the root field or command key.

### Reader identities and kinds

`registry_fields` reports one opaque identity for fields that share a joined reader, and one
conservative broad value kind. The kinds promise no complete grammar, valid ranges, occurrence
rules, reference ownership, nested behavior or runtime behavior, so a known identity can still
have kind `Unknown`. A missing identity means that the paths did not establish one shared reader.
The gap follows the field's own `Field.reader`, which the method builds once: no identity gives an
`UnresolvedReader` gap, and an identity with kind `Unknown` gives a `ReaderSemantics` gap. A nested
field of a collection gets its gap under its dotted name, and a collection field itself, which
always has an identity, gets none. A reader path gap on the same field adds no second
`UnresolvedReader` gap. A persistent read whose destination holds no known reader has no identity,
so its field has the gap too. On M452, 402 of the 1,770 root and nested fields have no identity
and 5 have an identity with kind `Unknown`; each has its gap. A field that a gap already names
gets no container-category gap of its own ([modifier masks](modifier-masks.md)).
`Reader.family` identifies the child family separately. Conditional read alternatives keep their
own families; conflicting or unresolved alternatives cannot establish an unconditional family.
Constructor joins refine generic persistent reader identities. Modifier fields carry
[modifier blocks](modifier-blocks.md); conversion facts are on
[numeric conversion](numeric-conversion.md).

A reader ID is the first 16 hexadecimal digits of the SHA-256 of the demangled callee name
(`ReaderId::from_callee`). So the same callee gives the same ID on every build: the
`registry-fields/v2` sweep on M45-observe and the `registry-fields/v3` run on M45-release found the
same 19 IDs. The hashed name of each `NParserUtil` template keeps its `void ` return type.
`cargo run --release --example reader-kinds -- "$STELLARIS_PATH" [REGISTRY…]` counts fields by
kind; the default registries are traditions, tradition categories and council agendas.

Pitfalls:

- **Identity does not establish value kind.** A command reader proves the constructor-installed
  virtual `Read` and `ReadMember` targets. It does not prove what the `Read` override accepts, so
  marking every such receiver `Block` adds an unsupported fact.
- **An outer reader's family is not the set of its child families.** `random_list` is an effect
  reader whose outer numeric keys lead to a separate effect-child grammar.
- **The family checks answer different questions.** Shared reader-entry classification gives the
  broad value of established helper signatures; constructor joins refine a generic persistent
  destination; conditional normalization also accounts for unresolved and rejected paths. Do not
  merge them: removing the last check promotes conditional facts.

### Uniform lists

`CReader::ReadUniform<T>(CPdxArray<T, int>&)` appends each value of a `key = { a b c }` list to the
array in `x1` (`0x10004e608` for `CString`: `InsertAtEmplace`, then `CReader::Read(CString&,
bool)`, until the closing brace or the end of input; it clears nothing first). Any element type
and both `enable_if` forms of the name are one shared reader shape (`readers.rs`,
`is_uniform_list`): `x0` the reader and `x1` an owner array, or a stack array that the loader
converts after the call (17 fields on M452, such as `common/traits` `requires_traits`). The kind is
`Block` and the family `NotApplicable`: the values are not commands, so no modifier container is
read and the read scope is `Known([])`. The identity keeps the element type. A tail call into
an owner array is `Accumulate` ([repeat behavior](#repeat-behavior)); a stack array has no
destination, and its repeat stays unknown. The members are `FieldMembers::Items` with the reader of one value
when every read alternative joins one list reader and the element `T` has a scalar read
`CReader::Read(T&)` or `CReader::Read(T&, bool)`, whose kind the item takes (`readers.rs`
`uniform_list_item_kind`; numeric facts stay `Unresolved`). Validated assumption: the list reader
reads each value with that read, as `0x10004e608` does for `CString`. A list of keys (`ReadKeyReferenceUniform`,
`ReadKeyReferenceDeferredUniform`) gives a `Reference` item, looked up as the field's `reference`
states. Any other element type (a struct such as `CResolutionTypeBreachData` for `in_breach_of`),
another reader on some path or a stopped path keeps `Unresolved`. At `registry-fields/v67`: 122
fields, 112 `String`, 9 `Reference`, 1 `Integer`; 16 list blocks stay `Unresolved`. `NSerializer::ReadUniformWithCustomSerializer` is not this reader. Command
grammar shares the reader rule: nested lists such as `create_species` `adjective.variables`
join. 17 command answers change (`tests/population/m452/command-population/`): list and
bit-field joins remove `reader-routing` gaps or give `Replace`, and `has_active_event` now stops
later, at its value acceptance.

### Read conditions and use-time inheritance

These facts are for M45-release.

- `CCouncilAgenda::ReadMember` (`0x10020bfa8`) tests presence bytes at `+0x6d0` and `+0x6d8`. Both
  sides reach `CReader::Read(int&)` after clearing the value at `+0x6d4` or `+0x6dc`: the branches
  initialize presence and do not restrict which occurrences are read. Equal complete outcomes on
  opposite presence branches collapse to `Always`; unresolved outcomes and unequal destinations
  cannot.
- A presence test names its field (`FieldZero`) only when the loader tests, with `cbz` or `cbnz`, a
  word of the field's own width at the offset where an integer or Boolean reader stores it. A
  float or fixed-point reader at that offset leaves the condition `Unresolved`: a zero word is not
  shown to be the scalar's zero for them (`-0.0` is a nonzero float word). Remove the limit when
  a loader tests such a field and the test is read.
- `CTraditionType::ReadMember` (`0x100cdcf38`) allocates and reads a swap, then inserts its pointer
  into the collection at `+0x5c8`; the method proves construction, virtual read and insertion of the
  same object on all paths. The pointer insertion specialization (`0x100ce42ac`) stores into the
  buffer at `+8` and increments the count at `+0x14`; the binding derives the buffer member from it.
  Reconstruction before or after insertion, owner writes that could reset the collection, and
  unknown-pointer writes invalidate the proof.
- `CTraditionSwap::ReadMember` (`0x100cdc0b4`) routes tokens `0x397e`, `0x397f` and `0x3980`
  (`inherit_effects`, `inherit_name`, `inherit_icon`) to Booleans at `+0x4f0`, `+0x4f1` and
  `+0x4f2`. These flags do not gate the other fields in that reader.
- Use-time selections, not parser conditions: `CTraditionType::GetName` (`0x100cdd944`) picks a
  swap by possibility and weight, checks its validity and `+0x4f1`, and on zero uses the swap's name
  at `+0xf8` (`0x100cdda28`), otherwise `GetBaseName`. `GetIconKey` (`0x100ce0028`) checks `+0x4f2`
  (`0x100ce0104`) and returns the swap name or the base key at `+0x10`. `OnEnabled` (`0x100ce25b8`)
  and `OnDisabled` test `+0x4f0` (`ldrb` `0x100ce26ac`) and `csel` (`0x100ce26b8`) the swap effect
  `+0x378` or the base effect `+0x418`; the same flag selects modifier and tooltip members.
  `CTraditionType::CalcAIWeight` (`0x100ce2cdc`) reads its own `+0x568`, which does not show that
  a swap inherits the weight; the SDK-545 amendment removed that question.

Use selections carry `All(Unresolved, FieldZero(...))`: the unresolved part covers the choice of
swap, validity, other branches and method bounds. An empty selection does not prove no runtime
condition. Use analysis admits direct `const` owner methods, whose signatures establish the
receiver; static, nested-class and other receiver shapes, including
`CTraditionType::PostReadInit()`, are not proven. The local flag proof follows copies of the
receiver; a shared loop load address does not establish object identity, and a conditional select
must get its flags from the adjacent comparison on every incoming path. SDK-546 depends on these relationships for
conditional name and icon templates. Locate the inheritance tokens with `inspect -- --strings inherit_`.

Prototype findings, in the `atlas-discovery`, `atlas-command-grammar` and `atlas-numeric-grammar`
bundles (see [retrieval](retrieval.md)): token dispatch is discovered without field or config seeds
and transferred to AI attitudes; factories bind to inherited readers (six `create_starbase` and
three `add_district` fields); one shared numeric operand grammar serves agenda cost and the
timed-flag units, while `agenda_cooldown` is a plain integer.

### Unresolved loader conditions on M452 (SDK-811)

An accepted limit. On M452 the sweep keeps 207 "loader condition remains unresolved" gaps: 132 on
the six `CEvent` members of each of the 22 event kinds, and 75 root fields in 33 other registries.
By the test that stops:

| Tested value | Outcomes | Gaps | Registries | Examples |
| --- | --- | ---: | ---: | --- |
| Owner word with no established named-field destination (pointer members, unnamed bytes) | Every one unresolved | 107 | 8 | `events` `desc`, `mean_time_to_happen`; `common/governments/civics` `potential` |
| Reader input (token kind or reader state) | Every one unresolved | 88 | 30 | `events` `picture`; `common/colony_automation` `buildings` |
| Owner word with no established named-field destination | Some unresolved | 5 | 1 | `common/special_projects` `on_start` |
| Reader input | Some unresolved | 4 | 4 | `common/traits` `icon`; `common/ship_sizes` `graphical_culture` |
| Reader input: block token or scalar | Established, unequal destinations | 2 | 2 | `common/ship_sizes` `hero_ship`; `common/pop_categories` `can_change_category` |
| Global load and reader input | Every one unresolved | 1 | 1 | `common/traits` `leader_class` |

No tested owner offset matches an established float or fixed-point destination, so the presence
word limit of [read conditions](#read-conditions-and-use-time-inheritance) has no member here. A reader-input test chooses a value form; `FieldCondition` states
stored values only. Atlas credits a field's value form and repeat only when `read` is exactly
`[Always]`, and reads no `FieldCondition` term, so naming a condition credits no claim. A branch
collapses to `Always` only when complementary tests reach equal complete outcomes; no gapped field
has them. The other 205 gaps have at least one unresolved outcome: reader paths that SDK-710 owns.
The 45 use-time gaps also stay: Native's `Field::uses` publishes the selections, and Atlas credits
no claim from them.

`ACCEPTED_LIMITS` in `examples/registry-sweep.rs` lists the 207 fields by registry, and accepts
the use-time gaps by their details, so the sweep counts them apart from open work (SDK-819); a
loader-condition gap of another field stays open. Remove a field from the list when its reopen
condition holds. Revisit the 205 when SDK-710 gives complementary paths equal complete outcomes (rerun the sweep:
the existing collapse removes those gaps), or when Atlas consumes condition terms, for example to
select a config subtype (SDK-716). The two block-or-scalar fields already have complete, unequal
outcomes, so neither trigger changes them: revisit them when Atlas needs a value form that depends
on the input token, and decide then how an answer states a reader-input condition.

## Registry scheduling and owner joins

From the SDK-489 prototype on M45-observe (`atlas-ownership/prototype/registry-ownership/`):

- Symbol enumeration finds 164 exact template `LoadFile` candidates, including owners without a
  separately named member reader. Neither the candidate count nor the scheduling table proves that
  all registries were found.
- An owner is established by the loader's receiver and directory, file activity, the root
  constructor key, the concrete owner, the persistent base, the vtable offset-to-top and the
  shared member-dispatch slot. A filename on the stack is not enough: top-frame names can be stale,
  so use the direct caller and the receiver directory.
- The static-modifier custom loader has a key-only phase and then a full-read phase. The owner rule
  transfers to six economic-plan roots; the other 243 reader occurrences are not named definitions.
- With two mods in normal and reversed order, the game selects one shared virtual filename, and
  separate duplicate files are processed in a/b order. A category duplicate reconstructs the same
  object and keeps its numeric ID; a modifier duplicate reuses the owner. The category loader reads
  a `.bin` fixture. Final merged values were not measured: these runs establish no extension
  policy, physical-file resolution or "last value wins". On M452 the
  [loader rules](loader-rules.md) state these from the code.
- Failed approaches: one observer form had too much overhead and others failed to start; the first
  transfer to the AI budget failed until the owner rule was revised.

### Registry names

`Native::registries` names each template registry by the content directory that its constructor
passes to the shared base constructor; the module comment of `engine/analysis/directories.rs`
gives the two compiled shapes. On M45 the method named all 164 template registries in about two
seconds, and all 162 directories that the retained live run observed are in the result, with no
conflict. The two candidates that the M45-observe startup run did not observe are
`CAIEspionageTargetPoliciesDatabase` (`common/ai_espionage/targets`) and `CGameScenarioDatabase`
(`common/game_scenarios`) (`atlas-ownership` bundle,
`prototype/registry-ownership/review.md`). Both are template registries on M452: a loader that
did not run before the startup boundary is not a missing registry.

- On M452, six template registries load from outside `common/`, such as `map/galaxy`,
  `sound/advisor_voice_types`, `gfx/portraits/sprite_configurations` and
  `interface/resource_groups`. So a registry name is the full content directory.
- On M452, code forms the address of 43 directory literals under `common/` that no template
  registry names (51 literals; 8 of them repeat a directory with a trailing `/`). Find them with
  `inspect -- --strings common/` and remove the 164 names of `registries()`. Examples are
  `common/component_templates`, `common/static_modifiers`, `common/traits` and `common/on_actions`.
  20 of the 43 are named by their database's own loader ([Custom-loader names](#custom-loader-names)).
  A literal is not a loader: `RunGame` forms the address of 157 of the 158 `common/` template
  directories and of `events`, and for 13 of the 43 (such as `common/agendas`, `common/planet_classes` and `common/subjects`) it is
  the only function that does. The "59" of the M45-observe count had no list.
- `common/lawsuits` is the template registry of `CLawsuitTypeDatabase`, and the game installs
  that directory. The config's `lawsuit` type gives `game/common/lawsuit_type`, a directory that
  the engine does not load and the game does not have: a config path error, not a missing
  registry.
- **Nested definitions have no identity of their own (M452).** A swap object is built by its
  owner's reader and kept in the owner's item. No swap class (`CTraditionSwap`,
  `CTechnologySwap`, `CAdvancedAuthoritySwap`, `CGovernmentCivicTypeSwap`, and the
  `CSwappable` data of jobs) has a database, a `LoadFile`, a `TGameDatabase` instance or a key
  reference reader; each has only a null object. `CTraditionType::ReadMember` inserts each swap
  pointer into the item's collection (buffer `+0x5d0`, count `+0x5dc`). The one swap-name lookup
  read, the deferred lambda of `CHasActiveTraditionTrigger::Read` (`0x1021bedb0`), walks
  `TGameDatabase<CTraditionTypeDatabase>`: it compares the key with each tradition's key (`+0x10`),
  then with the name (`+0xf8`) of each swap whose `inherit_effects` byte (`+0x4f0`) is zero, and
  stores the tradition at `+0x68` and the swap at `+0x70`. So a swap name resolves only as the
  pair (owner item, swap), through its owner, and a swap that inherits effects is not found by
  name. The config's `swapped_tradition`, `swapped_ascension_perk`, `swapped_technology`,
  `swapped_authority`, `swapped_civic` and `swapped_job` types are these objects. A registry is
  named by its content directory, which a swap shares with its owner, so a nested definition
  stays a field under its owner's path (`FieldMembers::Fields`). The other `skip_root_key` types
  of the config skip a wrapper key (`collection`, `named_lists`, `alerts`, the graphics and
  interface families); they are not nested definitions.
- Pitfall: the first version of the method took any directory-shaped literal in the constructor.
  It missed the global `CString` shape of `common/ship_categories`, and it had no tie to the
  meaning of the literal. The base-constructor call anchor replaced it.
- Pitfall: a scan that stops at every conditional branch loses loaders. `CSpeciesNamesDatabase::Init`
  runs a loop before it passes `common/species_names` to the file enumeration. The scan continues
  past a conditional branch, and the target of one forgets every register and built object.

### Database schedule

`Native::registries` reads the table of `NNullObjAndDatabaseInitUtil::SetupDatabases` to find
the scheduled databases that no template candidate covers (`discovery/schedule.rs`; its module
comment gives the scan). Each unjoined row is a custom-loader candidate, named as the next
section says; the answer counts the rows it cannot name in its `UnnamedRegistries` gap, with no
class name. The scan finds the function by its name
and derives the end of the fill from the code; no recipe holds an address.

- On M452 the function starts at `0x1005eb000`. After `mov x19, sp`, the table is at `x19 + 96`:
  **198 rows of 48 bytes**, six 8-byte slots each: the name as a C-string address, then
  `CreateInstance()`, `DestroyInstance()`, a fourth slot, `InitInstance()` and
  `PostReadInitInstance()`. The fourth slot is zero (`stp x8, xzr`) or a `CanInitialize()`
  function; 126 rows have the function. Row 0 is `CNamedColorDatabase`. Every slot of every row
  resolves.
- The fill ends at `0x1005ee4b8` (`ldrsw x8, [x0, #0x14]`), the first instruction that is not a
  literal step. The code after it copies `0xc6` (198) rows of `0x30` bytes from `x19 + 0x60`.
  The compiler reorders the stores: a slot can be stored long after its row, such as
  `str x9, [x19, #0x1f90]` near the end.
- A template row's functions are `TGameDatabase<T>::…` in `__DATA_CONST,__got` slots, so they
  resolve only through the chained fixups. A custom-loader row's functions are the database's
  own (`CStaticModifierDatabase::CreateInstance()`), often through `adrp` and `add`.
- 163 rows join exactly one template candidate by the database type in their function symbols;
  `CGameScenarioDatabase` is the one candidate with no row. 35 rows are outside the template
  method: `CStaticModifierDatabase`, `CWeaponTagDatabase`, `CStrategicResourceDatabase`,
  `CShipBehaviorDatabase`, `CAmbientObjectDatabase`, `CEmpireFlagDatabase`, `CGfxCultureDatabase`,
  `CProjectileGfxDatabase`, `CPortraitDatabase`, `COpinionModifierDatabase`, `CPlanetClassDatabase`,
  `CAdvisorDatabase`, `CPingMapDatabase`, `CFallenEmpiresDatabase`, `CShipDesignTemplatesDatabase`,
  `CSpeciesNamesDatabase`, `CNameListDatabase`, `CDesignerDatabase`, `CEventManager`,
  `CSpecialProjectDatabase`, `COnActionDatabase`, `CTraitDatabase`, `CPrescriptedSpeciesDatabase`,
  `CDiploPhraseDatabase`, `CMessageSystem`, `CAlertSystem`, `CTerraformDatabase`,
  `CStartScreenMessageDatabase`, `CSystemInitializerDataBase`, `CGalaxyTemplateDatabase`,
  `CWorldGfxDatabase`, `CColorsDatabase`, `CGameSettingsDatabase`, `CEmpireDesignDatabase` and
  `CScriptableLocalizationDatabase`. Some are not script registries (`CMessageSystem`,
  `CAlertSystem`); the row count is not a registry count. M45-release had the same counts.
- Pitfalls, checked again on M452: the stack probe `blr x16` (`___chkstk_darwin`) at
  `0x1005eb028` is a call before the table owner, so the scan accepts calls before `mov x19, sp`
  and stops at the first branch; without that move the answer is a gap, never an empty table. A
  new value in `x19` during the fill invalidates the owner and is a gap. An `ldr` from a `__got`
  slot gives nothing without the chained fixups.
- The M45-release reader decoded a hand-found window (`0x1005eb938` to `0x1005eedf0`) and was
  removed in `e9261a1`; the current reader derives the end instead.

### Custom-loader names

For each unjoined row, `loader_directory` (`directories.rs`) reads every function qualified by the
database class (`CTraitDatabase::…`) and takes the literal first argument of each
`VFSGetEnumeratedFiles` call, as the call's own basic block sets it (the block scan that
[loader rules](loader-rules.md#method) use). The literal can also come through a global
`char const*`: an `ldr` from a known address loads the pointer slot, and the slot's rebase target
is the literal. Each distinct literal names one registry, so a loader that enumerates several
directories names each of them (SDK-843); no literal, or an argument that the scan does not
establish, leaves the row in the `UnnamedRegistries` gap. The directory can also be the text of a
`CString` member of the instance, read through the short-string select
([below](#name-lists-and-random-names-sdk-844)). A class outside the schedule is a custom loader too
when its own functions call the file enumeration and `CReader::ReadSimpleStatement()`, and the
enumeration names a directory (`references::unscheduled_loaders`); such a class never counts in
the `UnnamedRegistries` gap.
`registry_fields`, `derived_names` and `modifier_families` read these names through their
[owner](#custom-loader-owners) ([derived names](derived-names.md#population),
[modifier families](modifier-families.md#roots)). `UnknownRegistry` is for names that `registries()` does not list. The
references and modifier-block methods use the same join (`references::database_directories`).

On M452, 30 of the 35 rows are named, with 31 names, and three classes outside the schedule name
four more (SDK-844): `common/ambient_objects`, `common/colors`,
`common/component_sets`, `common/component_templates` and `common/section_templates`
(`CShipDesignTemplatesDatabase`, [below](#one-loader-three-directories-sdk-843)),
`common/component_tags` (`CWeaponTagDatabase`), `common/diplo_phrases`, `common/fallen_empires`,
`common/game_rules` (`CGameRules`, unscheduled), `common/global_ship_designs`
(`CDesignerDatabase`), `common/graphical_culture` (`CGfxCultureDatabase`, a member),
`common/message_types` (`CMessageSystem`, a stack `CString`), `common/name_lists`
(`CNameListDatabase`, a member), `common/on_actions`,
`common/opinion_modifiers`, `common/planet_classes` (through `_PLANET_CLASSES_DIR`),
`common/random_names` and `common/random_names/base` (`CRandomNameDatabase`, unscheduled),
`common/scripted_loc` (`CScriptableLocalizationDatabase`, a stack `CString`),
`common/ship_behaviors`, `common/solar_system_initializers` (`CSystemInitializerDataBase`, a
stack `CString`), `common/special_projects`,
`common/species_names`, `common/start_screen_messages`, `common/static_modifiers`,
`common/strategic_resources`, `common/terraform`, `common/traits`, `events` (`CEventManager`),
`flags` (`CEmpireFlagDatabase`, through `_pEMPIRE_FLAG_FOLDER`), `gfx/worldgfx`, `music`
(`CSongDatabase`, unscheduled), `gfx/pingmap` (`CPingMapDatabase`, a stack `CString` from
`_PING_MAP_DIR`), `gfx/projectiles` (`CProjectileGfxDatabase`, a stack `CString`) and
`prescripted_countries`. None repeats a template name.

- **A `CString` on the stack.** `CSystemInitializerDataBase::Init()` (`0x100aad0a4`) builds
  `CString(sp + 0x70, "common/solar_system_initializers")`, then selects its text with
  `ldrsb w8, [sp, #0x87]; ldr x9, [sp, #0x70]; cmp w8, #0; csel x0, x9, x20, lt` (`x20` holds
  `sp + 0x70`) and passes `x0` to the enumeration in the same block (`0x100aad114`).
  `CMessageSystem::InitTypes()` (`0x10093fd4c`) and `CScriptableLocalizationDatabase::Init()`
  (`0x100b0dbfc`) have the same shape, the message system with frame addresses (`ldursb`,
  `ldur` from `x29`) after a `cbz` that skips one store; `CPingMapDatabase::Init()`
  (`0x100a29124`) builds its `CString` from the slot `_PING_MAP_DIR`. The block scan
  (`built_text_select`) gives `x0` the text when the select's flag byte is the `CString`'s
  `+0x17`, its pointer load and the `csel`'s other register are the `CString`'s address, the
  two loads write registers that no later word of the select reads, no branch enters the select
  after its first word, and no call (direct or indirect) and no store that may reach the
  `CString`'s 24 bytes comes between the construction and the select. A store counts as reaching
  them unless it is an immediate-offset `str`, `stur` or `stp` from a known stack address that
  lies outside them. A forward branch into a block keeps a value that every path into the block
  holds; a backward branch makes its block start with nothing known, and a register jump
  anywhere in the function makes every word start with nothing known.

- **One directory, two databases.** `CEmpireDesignDatabase` and `CPrescriptedSpeciesDatabase`
  both enumerate `prescripted_countries`, so it is one name. The owner join does not select one of
  the two: `registry_fields` gives `Unsupported` ("2 databases load this directory").
- **A global `char const*` is read at its initial value.** `CPlanetClassDatabase::Init()` loads
  `_PLANET_CLASSES_DIR` (`0x1032af358`, rebased to `"common/planet_classes"`) with `adrp x8,
  #0x1032af000; ldr x0, [x8, #0x358]` and passes it to the enumeration. Both globals are in
  writable `__DATA,__data`, so the method assumes that nothing changes them before the loader
  runs. M452 checks: every instruction that uses either address after an `adrp` of its page
  loads from it (8 loads; one `add` that a load follows), and no fixup targets either slot
  (`dyld_info -fixups`). If a store or an escaped address is found, or a run reads another
  directory, a slot is no longer proof. A load into `xzr` is not a slot value.
- **The `RunGame` caller is not the loader.** `RunGame` passes `"common/planet_classes"` and
  `_pEMPIRE_FLAG_FOLDER` to `VFSPreEnumerateFiles(char const*, char const*, char const*, int)`, a
  warm-up of the file list; the database's own `Init()` enumerates the files. The ticket's caller
  shape was therefore not built. Whether a caller supplies the directory of one of the 10 rows
  below was not checked; their obstacles are in their own functions.
- **The 5 rows that stay gaps**, by obstacle:
  - The directory is a built path: `CGalaxyTemplateDatabase` (`ReadFiles`; its constructors
    build the member `+0x30` from `map/setup_scenarios`, which `ReadFiles` extends before the
    call).
  - A directory that is not a literal: `CPortraitDatabase` enumerates a global path, a member
    path and `gfx/portraits/portraits/substitutions`.
  - No enumeration call in the class's own functions: `CAdvisorDatabase`, `CAlertSystem` and
    `CGameSettingsDatabase`.
- Pitfall: a row with one literal is not proof that the database owns the items there; it is the
  directory the loader reads. `prescripted_countries` shows that a directory can feed two
  databases.
- Pitfall: the linear scan of the base-constructor method stops at the first `b`. Before SDK-843
  `loader_directory` used it, so `CShipDesignTemplatesDatabase::Init(bool)` stopped at
  `0x1011d04f0`, after the first of its three calls, and was a gap for an unread call, not for
  several directories. The block scan reads every call; the literal must be in the call's block.

### One loader, three directories (SDK-843)

`CShipDesignTemplatesDatabase::Init(bool)` (`0x1011d0438`) loads three directories, one after the
other, in one function. Each phase calls `VFSGetEnumeratedFiles(dir, files, ".txt", null, 0)`,
skips a file named `sets.txt`, lexes each other file and loops `CReader::ReadSimpleStatement()`
(reader at `sp+0xa0`); after the read it loads the token at `[sp,#0xd8]` and compares it:

| Call | Directory | Root switch | Built and read through slot `+0x20` |
| --- | --- | --- | --- |
| `0x1011d0494` | `common/section_templates` | `cmp w8,w26` (`0x2c60`, `ship_section_template`), `b.ne` back to the statement loop | `CSectionTemplate()` (`new 0x600`) |
| `0x1011d0908` | `common/component_sets` | `cmp w8,w21` (`0x2dfd`, `component_set`), `b.ne` back | `CComponentSet()` (`new 0xb8`) |
| `0x1011d0f30` | `common/component_templates` | `cmp w8,w23; b.eq` (`0x2c61`, `weapon_component_template`); `cmp w8,w27; b.eq` (`0x2d1a`, `utility_component_template`); `cmp w8,w21; b.ne` (`0x3230`, `strike_craft_component_template`) to `CReader::ReportUnexpected()` and back | `CWeaponComponentTemplate`, `CUtilityComponentTemplate`, `CStrikeCraftComponentTemplate` |

Each constant is moved after its phase's enumeration call. After the third loop the function
reads `common/component_templates/*_components.csv` by literal name (`ReadWeaponComponentCSV`
and the others). Vanilla M452 has 476 `ship_section_template`, 439 `component_set`, 591
`weapon_component_template`, 854 `utility_component_template` and 61
`strike_craft_component_template` root statements.

- **Owner of each directory.** `directory_owner` (`loader_owner.rs`) reads the route of one
  directory from its enumeration call up to the function's next enumeration call (or its end),
  with the database's own functions that this code calls. Several calls of one directory must give
  the same owner, or the directory is `Several`. Validated assumption, not a proof: the
  compiler places each phase's loop after its call and before the next call. The exact-build test
  `m452_schedule_rows_outside_the_template_method_are_custom_loader_candidates` fixes the three
  owners; a build that moves a loop changes them.
- **Names.** Each item is named by its `key` field (`0xdc`), whose hash the directory's lookup
  compares: `CSectionTemplate::ReadMember` hashes it into `+0x160` (`GetSectionTemplate`),
  `CComponentSet::ReadMember` into `+0x30` (`GetComponentSet`), and `CComponentTemplate::ReadMember`
  into `+0x1c8` (`GetComponentTemplate`, over the three template arrays; the subclass readers do
  not read `key`). `loader_rules` states `RootKey(k)` / `Field("key")` for each root key.
- **What stays unjoined.** `references` and the modifier-block join map a lookup to a registry by
  its database. `CShipDesignTemplatesDatabase` names three, so its lookups (`GetComponentTemplate`
  and the others) stay without a registry target.

### Name lists and random names (SDK-844)

Two loaders that the literal scan did not name, read on M452 with `examples/inspect`:

- **A `CString` member.** `CNameListDatabase::CreateInstance()` (`0x100996838`) allocates the
  instance (`operator new(0x68)`), builds `CString::CString(x0 = object, "common/name_lists")` in
  it and stores it to `_pInstance`. `Init()` (`0x100996924`, `mov x20, x0`) passes that member to
  the enumeration through the short-string select `ldrsb w8, [x0, #0x17]; tbz w8, #31, L;
  ldr x0, [x20]; L:`, with `".txt"` and `-1`. `CGfxCultureDatabase` has the same shape, with
  `common/graphical_culture` built in both constructors and in `CreateInstance()`.
  `enumeration_calls` takes `x0` at a function's entry as the instance and gives the select's
  join `x0 = MemberText(k)` only when the select's two loads address the same member, `x0`
  already holds that member's address, and no other branch enters the select or its join.
  `member_texts` reads the class's constructors (the instance in `x0`) and a `CreateInstance()`
  that allocates once; several texts at one offset give nothing. Validated assumptions, not
  proofs: the enumerating function is a member function, and the allocated object is the instance.
- **A loader outside the schedule.** The galaxy generator calls
  `CRandomNameDatabase::FillDatabase()` (`CGalaxyGenerator::Generate`), which enumerates
  `common/random_names/base` (`0x100ac0434`, null suffix, depth 0) and reads each fixed root key
  (`star_names`, `custom_star_names`, …) into string lists, the lists of `custom_star_names` and
  `custom_planet_names` keyed by their nested keys (`SetEmplace` at `0x100ac07b4` and
  `0x100ac08d4`). It then calls `ReadRandomNames()`, which enumerates `common/random_names`
  (`0x100ac0e40`, depth 0, so `base` is not read again) and builds a `CCompositNamePartList`
  (`*_name_parts_list`, named by `key`) or a `CCompositNameFormat` (`*_name_format`, no name) by a
  root-key switch that is partly a jump table (`br x8` at `0x100ac1038`). The unscheduled-loader
  rule also names `common/game_rules` (`CGameRules::InitInstance()`, which reads each fixed rule
  key as a trigger block) and `music` (`CSongDatabase`, `song` blocks named by `name`).
  `CInlineScriptDatabase` enumerates `common/inline_scripts` but calls no statement read, and
  `CGameGraphics` enumerates `gfx/loadingscreens` for `.dds` files; neither is named.
- **A route of each directory.** `FillDatabase()` calls `ReadRandomNames()` after its own loop, so
  a route does not follow a call into an own function that enumerates: that function is the route
  of its own directory.

| Registry | `registry_fields` | Definition names |
| --- | --- | --- |
| `common/name_lists` | `CNameList`, 15 fields, partial | `Root` / `Key` |
| `common/random_names` | `Unsupported`: several classes | `RootKey(*_name_parts_list)` / `Field("key")` |
| `common/random_names/base` | `Unsupported`: stated reason, no item | nested-key obstacle |
| `common/graphical_culture` | `CGfxCulture`, 8 fields, partial | `Root` / `Key` |
| `common/game_rules` | `Unsupported`: stated reason, no item | `Root` / `Key` |
| `music` | `Unsupported`: several classes (`CSong`, `CMeanTimeToHappen`) | `RootKey("song")` / `Field("name")` |

- **The owner of `common/name_lists` is the object that the loader reads.** `Init()` allocates
  one `CNameList` (`new 0x318`), stores its address point with `str x8, [x19]` (`0x100996cbc`)
  and reads it with `mov x0, x19; add x1, sp, #0x1b0; blr x8` through slot `+0x20`
  (`0x1009971f0`); `sp+0x1b0` is the reader of its one `ReadSimpleStatement()`. The same route
  forms the vtables of `CNameListShipClass`, `CNameListShip`, `CNameListPlanet`,
  `CNameListCharacter`, `CNameListFleet`, `CNameListArmy`, `CNameListByRarity` and
  `CNameListDesign`, stored into the list at `+0x10` and after or on the stack as the default
  contents of `SNameListContent`. All have a member reader, so the built classes are several;
  [the read object](#custom-loader-owners) selects `CNameList`.
- **Name-list contents come from the loader's inline build.** `CNameList::ReadMember` reads
  `ship_names`, `character_names` and the other lists with `CReader::Read(CPersistent&)` on
  embedded objects. `CNameList::CNameList()` (`0x10061aa54`, used for the null object) installs no
  vtable there; `Init()` builds the contents inline. The [loader's part
  points](#loader-part-points) give the eight readers. `fleet_names` gets its keys
  (`random_names`, `sequential_names`, `sequential_name`). `character_names` reads any key, each
  a `CNameListCulture` ([readers of any key](#readers-of-any-key)). The other lists keep
  `Unresolved` members; their failed shapes are in that section.
- **`language` is not read.** The config states `language` for `name_list`, `species_name` and
  `species_named_list`. `GetTokenArray()` names the token, but neither `CNameList::ReadMember`
  (15 keys) nor `CSpeciesNames::ReadMember` (5 keys) compares it, the cold clone of
  `CNameList::ReadMember` (`0x1028d3210`) only calls `_OUTLINED_FUNCTION_0`, and vanilla M452 does not
  use the key. The claims are likely config errors.
- **Why `common/random_names` has no owner.** Five keys build each class, and the switch is a
  jump table, which [root-key owners](#root-key-owners-sdk-813) do not read. The route also builds
  `CWeightedStringTable` and `CMeanTimeToHappen`, members of the two classes.

### Custom-loader owners

`registry_fields` needs only an owner class: its root is `<owner>::ReadMember(CReader&, int)`. A
template loader names its owner as a template argument. For a custom-loader registry,
`loader_owner` (`discovery/loader_owner.rs`; its module comment gives the rule) reads the
database's own functions. The loader route is each of them that calls the file enumeration and
each of them that the route calls directly. The owner is the one class with a member reader that
the route builds: it calls the class's constructor or forms its primary vtable address point, as
an inlined constructor does. Several built classes are owners [by root key](#root-key-owners-sdk-813)
when the root keys select between them. Otherwise the owner is the one built class whose object
the route reads: a route function with one statement read stores the class's address point to
the object's first word, then passes the object in `x0` and that statement's reader in `x1` to a
call (a virtual `Read(CReader&)`). A loader of several directories has a route for each
([one loader, three directories](#one-loader-three-directories-sdk-843)). None, or several in any
other shape, is `Unsupported` with the reason, never a choice.

- **Read object: validated assumption, not a proof.** The registers are followed in code order,
  not by path. A `mov` between 64-bit registers copies the object, so `mov x0, x19` before the
  call counts. A constructor call is not counted as the store, and neither is a store with
  writeback, such as the post-index `str x8, [x0], #8`. On M452 the rule selects only `CNameList`
  (`common/name_lists`). It does not select one class for `common/random_names`: the part lists
  are read by `CReader::Read(CPersistent&)` with the reader in `x0`, and each format's address
  point is stored with a post-index store into `x0` after `mov x19, x0`. `music` stays `Several`
  (`CSong`, `CMeanTimeToHappen`). Change the rule when a loader reads its object through a
  constructor-built object, or stores its address point with writeback; the exact-build test of
  the owners fixes the three answers.

- **Representation.** The join lives in `ScheduledDatabases::loader_owners`, beside
  `loader_directories`, and is read once with the schedule. It is not a `NamedCandidate`: the
  candidate slice feeds the reference, modifier-block and expansion readers and the
  `RegistryRoot` hosts of `script_expansions`, and a custom loader has no `LoadFile`, initial
  loader or template class for them. The field input carries only the owner (`FieldInput::owner`),
  since every field reader used only `owner_candidate` of the candidate record. The block-entry
  method takes the joined owners with the template owners, so the blocks of a custom owner get
  entry contexts (`common/strategic_resources` `potential` enters as a country).
- **Validated assumption, not a proof.** When the route builds one class, that class is taken
  as the item that the loader reads; the method follows the object to the read call only to
  select one of several built classes. On M452 each joined route
  builds the object, then reads it: `CStaticModifierDatabase::Init(bool)` allocates, forms `vtable
  for CStaticModifier`+0x10 and calls slot `+0x20` of the object (`CPdxModifier<…>::Read(CReader&)`,
  whose member dispatch at slot `+0x28` is `CStaticModifier::ReadMember`);
  `CAmbientObjectDatabase::InitInstance` forms the `CAmbientObjectData` point and calls
  `CPersistent::Read(CReader&)`; `CTerraformDatabase::InitInstance` forms the `CTerraformLink`
  point and calls slot `+0x20`; `ReadEventsFile` constructs `CEvent` ([the events
  loader](#the-events-loader-on-m452)). The parity test
  `m452_schedule_rows_outside_the_template_method_are_custom_loader_candidates` fixes every owner.
  Apply the read-object rule to one built class too when a build has a route that builds a
  persistent object that it does not read as the item.
- **Static modifiers.** The key-only phase of `CStaticModifierDatabase::Init(bool)` calls
  `CStaticModifier::ReadOnlyKey(CReader&)` and reads no definition. The full-read phase reads
  through `CStaticModifier::ReadMember(CReader&, int)`, the root. That reader handles `key`,
  `apply_modifier_to_other_planets` and `divide_over_pop_groups`, and tail-calls
  `CGraphicalModifier::ReadMember` for every other token. The method follows that
  [inherited reader](#inherited-member-readers-sdk-838) (SDK-838): 6 more fields, such as
  `icon_frame`. Its other tokens go to a virtual call, so the answer stays partial with an
  `UnresolvedPath` gap on the registry.
- **Function budget.** `CTrait` has 174 functions in its field input, above the old limit of 128
  functions (`fields.rs`); the limit is 256 now. The 4 MiB code limit is unchanged (the input is
  0.4 MiB). Raise the count again only with a registry that exceeds it.

On M452, 20 of the 30 names join an owner (sweep at `registry-fields/v31`, `v34` for
`common/planet_classes`, SDK-843 for the two ship-design directories, SDK-844 for
`common/graphical_culture` and the read-object rule for `common/name_lists`; fields, then the
answer). `common/component_templates` has a class for each root key and no owner
([root-key owners](#root-key-owners-sdk-813)):

| Registry | Owner | Fields | Answer |
| --- | --- | --- | --- |
| `common/ambient_objects` | `CAmbientObjectData` | 7 | partial |
| `common/component_sets` | `CComponentSet` | 9 | partial |
| `common/diplo_phrases` | `CDiploPhrase` | 5 | partial |
| `common/fallen_empires` | `CFallenEmpireType` | 6 | partial |
| `common/global_ship_designs` | `CShipDesign` | 32 | partial |
| `common/graphical_culture` | `CGfxCulture` | 8 | partial |
| `common/name_lists` | `CNameList`, the read object | 15 | partial |
| `common/on_actions` | `COnActionList` | 2 | partial |
| `common/opinion_modifiers` | `COpinionModifier` | 10 | partial |
| `common/section_templates` | `CSectionTemplate` | 19 | partial |
| `common/planet_classes` | `CPlanetClass`, by root key | 70 | partial |
| `common/special_projects` | `CSpecialProjectType` | 26 | partial |
| `common/species_names` | `CSpeciesNames` | 5 | partial |
| `common/start_screen_messages` | `CStartScreenMessage` | 3 | partial |
| `common/static_modifiers` | `CStaticModifier` | 3 | partial |
| `common/strategic_resources` | `CStrategicResource` | 22 | partial |
| `common/terraform` | `CTerraformLink` | 10 | partial |
| `common/traits` | `CTrait` | 105 | partial |
| `events` | `CEvent` | 30 | partial |
| `flags` | `SFlagColor` | 4 | partial |

None is complete and none failed. Six were `Unsupported`; SDK-813 answers `common/planet_classes`
([root-key owners](#root-key-owners-sdk-813)) and states a narrower reason for the other five.

- Pitfall: a constructor anywhere in the database's functions is not an owner.
  `CDiploPhraseDatabase::GetRandomValidPhrase` and `CTerraformDatabase::HasAvailableLinks` build a
  `CEventScope` (which has a `ReadMember`) on the stack to test triggers; they are off the route.
- Pitfall: "any class whose method the loader calls" gives `CGameState`, `CEventScope` and
  `CScopeObjectReference` for `CEventManager`.

### Root-key owners (SDK-813)

`CPlanetClassDatabase::Init()` (`0x10115a26c`) builds two classes with a member reader. After
each `CReader::ReadSimpleStatement()` (reader at `sp+0x70`) it loads the token at `sp+0xa8`
(reader + `0x38`), compares it with `w23`, which holds `0x2714` (`random_list`), and branches
`b.ne`. The equal side allocates `0x90` bytes, forms `vtable for CPlanetClassRandomList`+0x10 and
calls `CPersistent::Read(CReader&)`. The other side builds `CPlanetClass(CString const&, int)` from
the key, or finds the class of that key and reads into it again, and reads through slot `+0x20`.
Both go back to the statement read.

- **Rule.** `loader_owner` takes several built classes as owners by root key when a route
  function has one statement read; the next instruction loads the word at the reader argument plus
  the token offset; then a chain of comparisons follows, each `cmp` of that word with a constant
  that names a token (an immediate, or a register whose last write before the comparison, in
  code order, is a `mov` of one; a call replaces a caller-saved register) and a `b.eq` to the
  key's code, the last one possibly a `b.ne` to the code of every other token; and the code from
  each key's side up to the next statement read builds exactly one class, a different one for
  each key, while the other side builds one more class or none. The classes must be all that the
  route builds. Any other shape stays `Several`. Validated on M452 (the exact-build test above),
  not a proof: the method does not follow a path-dependent write of the constant. The planet
  loader is the one-key chain with another class; `common/component_templates` is a three-key
  chain whose other side reports the key and builds nothing.
- **Answer.** The registry's owner is the class of every other key (`CPlanetClass`), and its
  fields are the registry's root fields. The reserved key is one more root field, `random_list`:
  a root statement, as an [event kind](#event-kinds) is, not a member of a planet class. Its
  members are the fields of `CPlanetClassRandomList`, `name` and `planets`, from a field run of
  their own (the kept results are by registry and owner); their conditions, member gaps and registry-wide gaps move
  under the key, and the run's `OutsideMethod` gaps, which repeat the owner's, are dropped. The
  key's reader is a block with no identity, and its read scope is unresolved with a gap: the
  method does not follow the inlined construction to `CPersistent::Read(CReader&)`.
  `derived_names` and `modifier_families` answer `CPlanetClass` with one `OutsideMethod` gap
  that names the reserved key. Atlas maps `random_list` to `type[planet_class_random_list]` (12
  claims) and the other root fields to `type[planet_class]` (375) in SDK-818.
- **No owner (SDK-843).** When every class belongs to a key and other keys build nothing, no
  class owns the registry's items. `registry_fields("common/component_templates")` answers one
  root statement per key, built as `random_list` is, with the class's fields as members; the
  method's registry-wide `OutsideMethod` gaps, which an owner answer would carry, stay once on
  the registry, and a member's at the member below its key. `derived_names` and
  `modifier_families` are `Unsupported` ("each root key builds its own class …"). Atlas reads
  key `K` as `type[component_template]/subtype[K]` (SDK-800). The three subclass readers
  go on in `CComponentTemplate::ReadMember` (`0x1001dd284`) for every other token, so each key
  lists the base fields (`key`, `size`, `power` and the others); the weapon and strike-craft keys
  reach it through a [value reader](#inherited-member-readers-sdk-838).
- **The random list's reader.** `CPlanetClassRandomList::ReadMember` (`0x10115ce44`) first tests
  the sign bit of a string's last byte (`ldrsb w8,[x0,#0x1f]`, `tbnz w8,#31`) and, when its
  location string is empty, sets it from `CReader::GetFileLocationDescription()`; then `planets`
  (`0x323b`) reads with `ReadUniform<CString>` into `+0x60` and `name` (`0x1b`) with
  `Read(CString&)` into `+0x30`. The walker loads `ldrsb` as a byte and follows a `tbz`/`tbnz`
  of a value that is not the token both ways, under an unresolved condition (a bit test of the
  token is a gap). The paths through the location call stop there, so each key keeps an
  unresolved alternative. Across the sweep these two rules changed only this answer.
- **Pitfall.** A consumer that reads every root field as a member of an item reads `random_list`
  as a planet-class field. The reserved key is a root statement; a planet class has no
  `random_list` member.

The other registries keep `Unsupported`, with the reason that the build's recipe states for each
loader (`LoaderReason`, `binding/groups.rs`); a loader with no stated reason gets the general one,
so another build never reuses an M452 reason. What would answer each:

| Registry | Loader on M452 | Reason | What would answer it |
| --- | --- | --- | --- |
| `common/colors` | `CColorsDatabase::Init()` (`0x1012fb45c`) reads 7 fixed root keys (`0x2ec8`, `0x2b52`, `0x2c88`, `0x2e95`, `0x2ec9`, `0x3066`, `0x42f6`), each a block of `rgb`/`hsv` entries read with `CReader::Read(CColor&)` into the database | Fixed keys of unnamed colours; no item | An answer form for a block of unnamed values. Atlas's `type[country_color]` has 3 open claims |
| `common/component_tags` | `CWeaponTagDatabase::InitInstance()` passes each token to `SComponentTag::RegisterTag` | Each token is a tag; no item | Nothing: Atlas reads it as `enum[component_tag]` |
| `common/ship_behaviors` | `CShipBehaviorDatabase::ReadFile` (`0x1011b78b4`) takes `ship_behavior` (`0x311c`) and calls `CShipBehaviorReader::CreateAndRead(CReader&)` (`0x1011b79b4`), which builds `CShipBehavior` (no `ReadMember`), keeps it at `[x19]` and reads its own statements: token at `[x20,#0x38]`, compares and two byte jump tables | Own statement loop, no member reader | A field walk that starts after that statement read, with `[x19]` as the owner and a return to the loop as unresolved, not rejected. Only `Read(CString&)`, `Read(bool&)` and `Read(CFixedPoint&)` are shared readers there; the other keys use `CToken::GetFloat`, `CreateMovementPatternFromToken` on the value token or `ParseAdvancedStatement`, so most fields would stay unresolved. Atlas's `type[ship_behavior]` has 94 claims |
| `gfx/worldgfx` | `CWorldGfxDatabase::ReadMember` builds `CWorldGraphicalMapData` on the stack and reads it with `CReader::Read(CPersistent&)` | The database is the read object | Milestone 6 |
| `prescripted_countries` | Two databases enumerate it | "2 databases load this directory" | Milestone 6 |

### Owner vtables on M45-release

For a new owner join, use `vtable_group` in `binding/binary/families.rs`: it reads one class and
checks its typeinfo. The removed image-wide scan is at `git show 8d9a073:src/binding/binary/discovery.rs`.

- The owner rule needs the persistent base's offset-to-top and the shared member-dispatch slot:
  slot 5, the address point + 40. For 159 of the 164 candidate owners that slot holds
  `ReadMember(CReader&, int)`, through a `virtual override thunk` for 149; the base's offset-to-top
  is -56 for 155 owners. `CComponentSlotTemplate`, `CJobTag`, `CTraitTag`, `CStarbaseBuilding` and
  `CStarbaseModule` have no such vtable. Example: `CTraditionCategory` address point `0x103095310`,
  offset-to-top -56, member slot `0x100cd92b0`.
- Pitfalls: a scan that does not check that the second word is the class's own typeinfo accepts
  false address points (`CMegaStructureType` `0x1030b08f8` is `~CMegaStructureTypeDatabase()`;
  `CCouncilAgenda` `0x10301b9d8` holds its typeinfo in the member slot). Slot 5 holds the reader
  only in the persistent base.
- The chained fixups bind 29,695 slots to other images, 10,994 of them named; `internals::inspect`
  names imports from the fixups.

### The events loader on M452

`events` is not a template registry: `CEventManager` has no `LoadFile`. `registries()` lists it
through its own loader, and `registry_fields` reads it from `CEvent` ([custom-loader
owners](#custom-loader-owners)). Its route:

- `CEventManager::InitInstance` calls `ReadEvents(CGameGraphics*, bool)` (`0x1004d51fc`), which
  enumerates `events` for `.txt` files (`VFSGetEnumeratedFiles("events", …, ".txt")`) and calls
  `ReadEventsFile(CString const&, bool)` for each. `FileWasChanged` calls the same function. Only
  these two `CEventManager` functions, `RunGame` and the token table form the address of `"events"`.
- `ReadEventsFile(CReader&, int&, bool)` (`0x1004d5944`) reads each root statement and switches
  on its token, with direct comparisons and two byte jump tables (`0x102c784ca` for tokens
  `0x2c80`–`0x2c84`, `0x102c784cf` for `0x3eea`–`0x3eee`). An event kind sets one `EScopeType`
  constant; then the loader allocates `0x630` bytes, calls `CEvent::CEvent(EScopeType)` with that
  constant and reads the event through slot `+0x20` of its vtable. Other root keys:
  `namespace` registers a `CNameSpace` in the manager's hash table; `inline_script` reads the
  expanded script with a recursive `ReadEventsFile`; any other key logs `Corrupt Event Table Entry`.
- **One owner reader.** Every kind builds the same class, so `CEvent::ReadMember(CReader&, int)`
  (`0x1004bbc9c`) is the one member reader for all kinds. The kind is only the scope constant.
- **The scope.** `CEvent::CEvent` passes the constant to `SEventData(EScopeType)`, which stores
  it at `CEvent + 0x68`. `ReadMember` loads that word as the scope argument of
  `CEventOption::CEventOption(EScopeType)` (three sites), of
  `CTriggeredEventDescription<CDescOption>::ReadEntry(CReader&, int, EScopeType)`, and of two
  virtual block reads: the trigger at `+0x508` (slot `+0x30`) and the effect at `+0x518` (slot
  `+0x10`). So the child blocks read in the kind's scope.

The 22 kinds and their scope constants, named by `NEventScope::GetScopeName`:

| Root key | Token | `EScopeType` | Scope |
| --- | --- | --- | --- |
| `event` | `0x1b8` | `0` | none ("scope not set") |
| `country_event` | `0x2c80` | bit 2 | country |
| `observer_event` | `0x3714` | bit 19 | country (the second country type; scope keyword `observer`) |
| `planet_event` | `0x2c81` | bit 1 | planet |
| `colony_event` | `0x2c82` | bit 40 | colony |
| `carrier_event` | `0x2c83` | bits 1 and 3 | planet or ship |
| `ship_event` | `0x2d4f` | bit 3 | ship |
| `pop_event` | `0x2d6b` | bit 4 | pop |
| `pop_group_event` | `0x2d6c` | bit 5 | pop_group |
| `fleet_event` | `0x2ef6` | bit 6 | fleet |
| `system_event` | `0x3eeb` | bit 7 | galactic_object |
| `leader_event` | `0x3eec` | bit 8 | leader |
| `bypass_event` | `0x422a` | bit 12 | bypass |
| `pop_faction_event` | `0x33f2` | bit 14 | pop_faction |
| `starbase_event` | `0x3eea` | bit 17 | starbase |
| `first_contact_event` | `0x3e58` | bit 23 | first_contact |
| `espionage_operation_event` | `0x294e` | bit 25 | espionage_operation |
| `agreement_event` | `0x3eee` | bit 28 | agreement |
| `situation_event` | `0x3eed` | bit 29 | situation |
| `astral_rift_event` | `0x4223` | bit 31 | astral_rift |
| `cosmic_storm_event` | `0x2991` | bit 33 | cosmic_storm |
| `cosmic_storm_influence_field_event` | `0x2992` | bit 34 | cosmic_storm_influence_field |

The 20 `*_event` effects on M452 are the typed kinds without `pop_event`. The config at
`8574760` has 21 kinds: no `pop_event`, and `carrier` as the push scope of `carrier_event`, which
the engine reads as planet or ship. No vanilla file in `events/` has a `pop_event` key.

**`namespace` gets no field answer (SDK-814).** A decided limit of the events answer. A
`namespace = x` root statement registers a `CNameSpace` in the manager's hash table and is not a
`CEvent` field, so `registry_fields("events")` does not list it. `loader_rules("events")` already
answers the statement: `RootKey(namespace)` with `DefinitionName::Value`, so the value is the
namespace's name. The sweep counts no Native gap for it, so `ACCEPTED_LIMITS` has no entry. Atlas's
`events/` exception "`event_namespace`" keeps its 3 claims (`type_existence`, `field_existence`
and `value_form` of `namespace`), because Atlas does not join that `loader_rules` answer to them.
Add the join in Atlas, and add a Native answer only if the join needs more than the definition
name, when Atlas needs `namespace` beyond these 3 claims.

### Event kinds

`registry_fields("events")` answers one root field per kind (SDK-702, decision D4). Each kind's
`read_scope` is the scope that the loader passes to `CEvent::CEvent(EScopeType)`: a known empty
list for `event` (scope `0`), one alternative with planet and ship for `carrier_event`. Its
members are the `CEvent` fields, the same for every kind, and a gap of a member is repeated
below each kind (`country_event.trigger`). `namespace` and `inline_script` are not fields. The
parity test `each_event_kind_is_a_root_field_read_in_the_scope_that_the_kind_selects` compares
`tests/expected/m452/event-kinds.json` with the table above. Atlas joins kind `K`'s read scope to
`type[event]/subtype[K]/$annotation:push_scope` through the kind's `type_key_filter`, and its
known empty list to `scopeless`.

- **The loader root.** The kind switch is in the file loader, not in an owner `ReadMember`, so the
  field input has a second root (`FieldInput::loader_root`). The binding takes the one function
  of the owner's database that calls `CReader::ReadSimpleStatement()` and an `(EScopeType)`
  constructor of the owner; the walk starts after the statement read, with the reader in the
  callee-saved register that the call's argument is copied from (`x23`). The token is the word at
  the recipe's `reader_token_offset`, so the existing compares and byte jump tables partition it.
- **The walk.** A call to the owner's constructor ends a path and joins with its arguments; `x1`
  is the kind's scope. Every other call clobbers `x0`–`x18` and the walk goes on: the ABI keeps
  `x19`–`x28`, and the case holds the scope in `w21`. A branch back to the statement read ends the
  path as rejected. A loader with none of these functions has no loader root, and its answer is
  unchanged.
- **Validated assumption.** As for every custom-loader owner, the built object is taken as the
  item that the loader reads; the walk ends at the constructor and does not follow the virtual
  read. The kind's reader identity is the owner's primary vtable point (`CPersistent::Read` with
  `CEvent::ReadMember`). Repeat behavior is not established: the duplicate rule belongs to
  `loader_rules`.
- **Known gaps.** The default case (`Corrupt Event Table Entry`) stops at a `movk`, and the
  `namespace` case at an `ldursb`, so the answer has one registry gap for a root path that the
  walk could not finish. The walk names every accepted kind regardless.

`registry_fields` over `events/` at `registry-fields/v53`: 22 kinds, each read in an established
scope, with 44 `CEvent` members. 24 have a reader: 13 [`yes` tests](#current-sweep)
(`fire_only_once`, `is_test_event`, `hide_window`, `diplomatic`, `auto_opens`,
`is_advisor_event`, `force_open`, `trackable`, `archaeology`, `first_contact`,
`espionage_operation`, `auto_select`, `astral_rift`), 7 strings (`custom_gui`,
`custom_gui_option`, `notification_event_icon`, `diplomatic_title`, `event_picture_background`,
`specimen`, `event_message_type`), `event_chain`, `difficulty`, `notification_event_icon_frame`
and `pre_triggers` ([pre-trigger blocks](#pre-trigger-blocks)). 20 have no established reader.
Their failure shapes, from `examples/inspect --registry-fields events`:

- the path reaches a call that is not a reader: `operator new` for an object held by a
  `shared_ptr` (`trigger`, `immediate`, `after`, `abort_trigger`, `abort_effect`, `desc`);
  `option` and `picture_event_data` now join ([`make_shared` objects](#objects-in-a-make_shared-control-block)); `make_shared<…>(EScopeType const&)` (`picture`, `show_sound`,
  `mean_time_to_happen`, `weight_multiplier`); `CEventTarget::CEventTarget(CToken)` (`situation`,
  `location`); `strlen` or `CString::CString` of the token text (`id`, `title`, `message_desc`,
  `base`); `CTriggeredEventDescription<…>::ReadEntry(CReader&, int, EScopeType)` (`desc`);
  `shared_ptr<…>::reset` (`desc_clear`);
- `is_triggered_only` is a `yes` test (`strb` at `0x1004bc3b0`). When `mean_time_to_happen`
  (`+0x558`) is already set, it logs `'is_triggered_only' should always come before
  'mean_time_to_happen' or 'weight_multiplier'!` through a `CLogStream`. At `registry-fields/v65`
  it reads a Boolean (`Replace`) in each kind: the stream calls (`readers::DIAGNOSTIC_STREAM`:
  `CLogger::AccessInstance`, the stream's `<<` operators, `__put_character_sequence`, `endl`,
  `~CLogStream`) change no script state, `ldursb` loads as `ldrsb`, and a call that receives an
  owner word narrower than a pointer (`w1 = [x19,#8]` for `CEventManager::GetLogID(int) const`)
  cannot write through it. Remove the narrow-word rule only for a build whose calls take a
  pointer in a 32-bit register. In command grammar, `add_random_research_option` stops one path
  later (`instruction` in place of `compound-reader-overwrite`), with the same keys;
- no field: token `11510` stores `token - 0x2fa9` for the four tokens from `0x2faa` and `0` for
  any other (`csel` after `cmp`), an inline keyword domain that the walk does not read
  (`event_window_type` in the config); the four `*_clear` keys (`option_clear`, `picture_clear`,
  `trigger_clear`, `show_sound_clear`) stop at a `b.lt` over an array count or a `movi`;
- `major` and `major_trigger` are not keys of `CEvent::ReadMember` on M452.

**Members are definition fields.** Each kind is a root statement and a definition, so the
requirement of its members comes from the [census](#required-fields), as a root field's does: the
census run's empty `country_event` is the definition, and every kind builds the same `CEvent`
through one loader. The kind itself stays `Unknown`: the census establishes what a definition
must contain, not which statements a file must hold. `id` is `Required`: `ReadEventsFile` (`0x1004d5e40`) logs `Event at %s has no
ID.` for an event whose id word is negative and whose id text is empty, and keeps no such event.
`title`, `desc`, `picture` and `option` are `Unknown`: their absence reports (`eventmanager.cpp`
446–464) run only when `hide_window` (`CEvent + 0x5ff`) is false. `mean_time_to_happen` and
`is_triggered_only` are `Unknown`: `CEvent::InitPostRead()` logs when both are absent. Every other
member is `Optional`.

The block children take their scope from `CEvent + 0x68`, where the constructor stores the kind's
scope, but none of their readers joins, so none is `Enclosing` yet. When a child's reader joins
with scope `Load(Owner(0x68))`, it reads in the kind's scope once the constructor is shown to
store its argument there.

Pitfalls:

- A constructor of a nested class (`CEvent::SEventData::SEventData`) is a constructor of
  `CEvent::SEventData`. A class name taken before the first `::` misses it; the walk then does
  not enter it, and `pre_triggers` has no reader.
- The kind switch is in the file loader, not in an owner `ReadMember`. Each case reads no value;
  it sets a constant that a later constructor call receives. Read it from the loader root, not
  from the owner's member walk.
- The `*_event` effect declarations do not establish the loader's kinds: `pop_event` has a kind
  and no effect, and `event` has neither an effect nor a scope.
- A token number is per build. Name tokens from `GetTokenArray()` (the constant in `w1` after
  each name string), and scope bits from `NEventScope::GetScopeName`.

## Embedded object keys

A root field that reads one embedded persistent object on every token path that holds its token,
including a wider path, where every owner
constructor installs the same reader there and that reader has no block family, gets the keys of
the object's own `ReadMember` as `FieldMembers::Fields` (`fields/persistent.rs`, `embedded`). The
object is an owner in its parent's allocation, and the same analysis walks it
([nested owners](#nested-owners-and-the-receiver-context)). The binding adds the member reader of each such reader that the owner's constructors
can install to the field input. A walk that names no key leaves the members `Unresolved`. The keys take the read-scope,
scoped-operand and duration passes that a collection's fields take. A gap of
the object's walk as a whole names the parent field; a gap of one key names `parent.key`. The same
rule renames the whole-reader gaps of a collection's object, which earlier carried the registry
name after the dot.

On M452 (`registry-fields/v31`), 17 fields in 11 registries get keys this way, 131 keys in all.
Three walks name no key: `common/anomalies` `on_success` and
`common/country_customization` `graphical_culture_for_random_empire` keep `Unresolved` members.

Pitfalls:

- The constructor join must succeed first. `CJobType`'s constructor used `mvni` (vector move of
  an inverted immediate), which the evaluator did not run, so no `common/pop_jobs` field joined a
  persistent reader. `mvni` is now evaluated like `movi`; `common/pop_jobs` and
  `common/pop_categories` gained their modifier and weight readers with it.
- A key whose value goes to a class helper that the dispatch walk does not follow keeps an
  `UnresolvedReader` gap. The key is still established: its path is a singleton.

### Loader part points

A custom-loader owner whose own constructors do not build its items' parts takes the address points
that the loader's inline build installs (`loader_part_points` in `binding/binary/fields.rs`,
`discovery/inline_build.rs`). The build is the code of the loader's one
[read object](#custom-loader-owners) on its route from the file enumeration
(`loader_owner::read_object`), walked from the store of the owner's address point to the read
call. The walk follows each branch and keeps the registers that hold the object plus an offset
(through `mov`, `add` and store writeback) and the addresses that `adrp` and `add` form. Where
paths meet, a register keeps every object offset that it holds on some path and a formed address
only when every path holds it, and the walk goes on until no path brings a new state, so the stores
are those of any path. A branch out of the code or through a register, a store through the object
that the walk cannot read (such as one with a register index), or a walk that does not settle marks
the build open; code that ends in a call (a landing pad's `_Unwind_Resume`) ends there. A loader
that names several directories infers no part points, since its read object is not scoped to one
directory. A direct call that the
build makes with the object plus an offset in `x0` names a part: when every call target at that
offset has a symbol and is a method of one class `Part` (the name before the last top-level `::`),
and `Part` has one default constructor (`Outer::Part<T>::Part()`), the same walk over that
constructor, from `x0`, gives the part's stores. Their vtable address points, shifted by the
offset, become the owner's points at offsets that the owner's constructors do not establish.

- **Check, not only an assumption.** Neither build may be open. The part's constructor must store
  one formed address and no other value at each of its offsets. A shifted point counts only where
  the loader stores no other value, and at least one of them must be a store of the loader's own
  (`inline_build::part_points`). An offset that two parts give different points has none.
- **M452.** `CNameListDatabase::Init()` calls `SNameListContent::operator=(SNameListContent&&)`
  with the list plus `0x10`. `SNameListContent::SNameListContent()` (`0x1009950a0`;
  `0x1009957ac` only branches to it) stores eight vtables. The build's branch that constructs the
  contents in place (from `0x10099734c`) stores the same eight at `+0x10`, `+0x30`, `+0x68`,
  `+0x88`, `+0xb0`, `+0xf8`, `+0x180` and `+0x1a0`, so all eight agree.
- **Result on M452 (`registry-fields/v60`).** Only `common/name_lists` changes. `ship_names`,
  `planet_names`, `character_names`, `fleet_names`, `ship_class_names`, `by_rarity_names` and
  `design_names` gain a reader identity; `fleet_names` reads its three keys (`random_names`,
  `sequential_names`, `sequential_name`). The other lists keep `Unresolved` members (see
  [name lists](#name-lists-and-random-names-sdk-844)).
- **Remove or limit the rule** when a build's loader constructs a part other than by its default
  constructor's stores, or when the owner's own constructors come to install the parts.
- Pitfall: running the build through the constructor evaluator does not work. `_bzero` is not
  modelled, so the flag test at `+8` splits the paths. The path that moves the contents in
  through `operator=` keeps no vtable. The other path's copy-assign calls are opaque and forget the
  earlier vtable words. Running `SNameListContent::SNameListContent()` stops at the 64-path
  limit. The walk reads stores; it does not run calls.
- Pitfall: a part that the build constructs on the stack and copies in is not seen. Only stores
  into the object count.
- Pitfall: `Part::Part()` also stores member vtables that are not the part's own, such as
  `0x1030648e8` at `+0x8`; the loader's build stores other values there (`+0x18` in the list), so
  those offsets keep no point.

### Nested owners and the receiver context

The method analyzes an owner wherever it meets one: the registry's root, an embedded object, a
stack serializer and an object that a field inserts. Each analysis needs the member reader, the
receiver, and the constructor evidence of the allocation that holds the receiver.

- **Receiver.** An embedded object and a stack serializer are in their parent's allocation: the
  receiver is the parent's offset plus the member's offset, and the parent's constructors install
  their vtables. On M452 `CSpecialProjectType::CSpecialProjectType()` (`0x100b973cc`) stores the
  `SProjectRequirements` vtable at `+0x5a0` (`0x100b97548`) and the `SizeRequirement` vtable at
  `+0x5c0` (`0x100b9756c`); `CSpecialProjectType::ReadMember` reads `requirements` at `+0x5a0`
  and `SProjectRequirements::ReadMember` reads `size_requirement` at its own `+0x20`. An inserted
  object is a new allocation, with its own constructors. A base subobject and a member object at
  a constant offset have the same receiver; dispatch models both (`explore_owner`,
  `Value::Owner(offset)`, and a serializer with its owner at `(slot, owner_offset)`).
- **The rule** (`registry-fields/v71`; `Owner` and `Receiver` in `engine/analysis/fields.rs`):
  - An embedded object or a stack serializer keeps its parent's allocation and constructor
    evidence. Its receiver is the field's destination: `Receiver::Owner(offset)`, or
    `Receiver::Serializer { slot, owner_offset }` for a serializer whose word at `slot` points at
    the wrapped member. `owner_result` walks it as it walks a root: dispatch, the constructor
    join at the shifted offsets, its own embedded keys and keyword domains. It collects no
    objects yet.
  - An inserted object starts a new allocation, at offset 0, with its own constructor evidence.
    The binding finds its member reader as it finds a root's (`owner_reader`): its own, a stack
    serializer's, or a base class's on its subobject.
  - The collection pass runs the owner's reader with `x0` built from the receiver: the allocation
    plus the offset, or a serializer whose slot points at the owner. Serializer roots and base
    roots collect objects.
  - The binding gives each nested owner its constructor evidence, inherited readers and objects
    for the root and three owners below it (`fields::OWNER_DEPTH` is 4, the root included), and
    none to a class that an enclosing owner has. The analysis stops at the same bound (a fourth
    owner below the root keeps its keys), and at an owner whose reader and receiver an enclosing
    owner has: the deeper passes leave a reader gap.
  - An embedded object collects inserted objects only when its member reader can read any key
    (`embedded_objects`, keyed by the member reader; see [readers of any
    key](#readers-of-any-key)). Any other collection that an embedded reader inserts stays
    unresolved: collecting the objects of every embedded reader passed the 256-function input
    budget of `common/traits`, `common/megastructures` and `common/section_templates`.
  - The objects of a member reader are those that it, and the inherited readers that it reaches
    by calls and jumps, construct (`reached_inherited_readers`).
- **Result on M452 (`registry-fields/v71`).** Additions only. `special_projects`
  `requirements.size_requirement` gets its reader (`SizeRequirement` from the point at `+0x5c0`)
  and the keys `count` (integer) and `ship_size`. Every event kind's
  `option.ai_trade_facility.info_gathering` and `option_selection` get reader identities from the
  depth-2 constructor. `scripted_actions` `ai.weight` and `starbase_types` `ai_design.min` and
  `ratio` get weight blocks. `living_standards` and `military_service_types` join
  `triggered_pop_group_modifier` through the base reader, with no class-specific code, as
  `citizenship_types` does. `zones` `triggered_desc` and the six `triggered_*_modifier` fields
  join through the serializer receiver. `zones` `ai_resource_production` stays unjoined. The
  largest field input is `CTrait` with 217 functions (unchanged); `events` has 117 (126 before),
  `zones` 117 (85 before).
- **Not this cause.** Most nested open members stop at a first-level shape, not at a lost context:
  statement loops, collection runs that meet an unmodelled call (string concatenations other than
  the log-ID ones, `SetDebugID`, a lookup through a virtual call), inline or nested stack
  serializers, readers that read any key (name lists), fields with a block and a scalar
  alternative, and owner constructor walks that stop (a static-local guard in
  `CPatronCallingTier()`, owners with no constructor symbol). Recursion adds a level only after
  such a shape joins.

Pitfalls:

- The member class of an embedded object often has no constructor symbol of its own (it is built
  inline), so only the parent's constructors give its evidence.
- An object found by `collect_objects` without evidence (beyond the bound) still lists its keys, so
  the gap is easy to miss: its persistent reads have no reader and no gap names the missing
  evidence.
- Counting the calls of every inheritable reader in the input, not only of those that the owner's
  reader reaches, gives a nested owner the objects of unrelated readers. With deeper nesting,
  `CPolicyOption` took `CCountryAI`'s objects and `common/policies` passed the 256-function input
  budget.
- An embedded walk in the member's own coordinates, at offset 0, finds no constructor point: the
  parent's constructors store the member's vtables at parent offsets.
- `CPersistent::ReadMember` as a vtable's member reader means the class reads in its own `Read`
  (for example a statement loop in `CObjectiveCounterSet::Read`); it is not a key table.

### Readers of any key

A member reader that compares no token and reads the key's text reads any key the same way. The
field's members are `FieldMembers::AnyKey`, and its `entry` is the object that the reader builds
for each key (`fields.rs`, `any_key_entry`; `registry-fields/v75`).

- **Rule.** The reader's only token path covers every token and loads the key's text, the
  reader's token word plus the token's text offset (`+0x48` on M452). Such a path stops with the
  reason `key-text` when it joins no reader. The collection pass then runs that path under the
  token `ANY_KEY`, with a planted key text (`native any key`) that the string model
  (`strings.rs`) follows through `CString::CString(char const*)`, `_strlen`, `_memmove`, the string
  allocator and `__assign_external`. The entry is the one object that every returned path inserts
  and reads; its paths' unnamed-path gaps are dropped. A text compare (`_memcmp`, `_strcmp`,
  `_strncmp`) stops the run (`key-text-compare`): a compared key may read another way.
- **A moving insertion at the array's end.** `Class& CPdxArray<Class, int>::InsertAtEmplace<Class>(int, Class&&)`
  joins as `SetSizeAndEmplace` does ([objects at an array's end](#objects-constructed-at-an-arrays-end),
  `ArrayEnd::Insert`): at index one, the count of the one-element array that the run gives, from
  a source whose word at `+0` is the class's primary address point. The class's destructor on that
  moved-from source returns with no effect. The layout comes from the insertion's body, as for the
  resizing; its count load may be `ldrsw`.
- **Binding.** The objects of an embedded member reader are collected only when its words load
  the key's text from the reader in `x1` (or a `mov` copy of it) and name no `w2` before the
  first call or branch (`loads_key_text`, `ignores_token`). That only selects candidates; the
  walk establishes the read.
- **M452.** `CNameListCharacter::ReadMember` (`0x1009928ac`) loads the key's text
  (`ldr x1,[x1,#0x48]`), makes a `CString` of it, and for a non-empty key stores the
  `CNameListCulture` address point (`0x103082d38`) in a stack object, moves it to the end of the
  array at `+8` with `InsertAtEmplace<CNameListCulture>` (`0x10099d334`; element size `0x1f0`, count
  at `+0x14`), destroys the stack object, assigns the key to the new culture's name and reads it
  with `CReader::Read(CPersistent&)` (`0x100992ad4`). An empty key goes to `CPersistent::ReadMember`;
  the planted key does not take that branch.
- **Result on M452 (`registry-fields/v75`).** `common/name_lists` `character_names` gets
  `AnyKey` with a `CNameListCulture` entry (repeat `Accumulate`) of 22 keys, and keeps its
  own read scope unresolved (`no-scope-parameter`): only a scalar entry, a parameter list, reads
  in no scope. The entry's keys are: the eighteen name
  lists (`first_names`, `full_names_female`, `regnal_second_names_male`, …) as string lists, the
  three `use_full_regnal_name*` Booleans, and `weight`, which keeps no shared reader. The
  modifier blocks' parameter lists (`description_parameters`) keep their scalar entry. Atlas does
  not read `AnyKey` yet: it must match a config pattern key (`<ship_size>`, `scalar`) or a literal
  key that the reader does not name (`default`) to `entry`, and read `value_forms` only for a
  scalar entry.
- **Stays a gap.** The other name lists read their keys in shapes the rule does not cover:
  - `CNameListShipClass::ReadMember` (`0x10099484c`) compares the key with `generic` (a short-text
    byte loop or `_memcmp`) and reads a string list; another key is looked up in
    `CShipSizeDatabase`'s map (`Find` at `+0x58`, the null object, the validity check) and reads a
    string list at the size's index. `CNameListShip::ReadMember` does the same and then parses the
    value by hand (an `@` test on the lexer's token, `strcmp` with `sequential_name`).
    `CNameListPlanet::ReadMember` looks the key up with `GetPlanetClass` and runs its own statement
    loop. A rule for these needs text-compared fixed keys and a key reference.
  - `CNameListArmy::ReadMember` compares token `generic` first (an inline
    `SNameListArmyTypeReader` serializer), so its default path is not the only path; it also
    merges a repeated key through `GetIndex`. `army_names` itself has no reader join.
  - `CNameListByRarity::ReadMember` converts the token with `TokenToEnum<ERarity>`: keyword keys,
    not any key.
- **Limits.** The planted key matches no fixed key, so a reader that compares the key by an inline
  byte loop against a constant and reads the same object either way is not told apart; a
  `_memcmp` or `_strcmp` stops the run. Entry members take the read-scope, numeric, scoped-operand,
  storage and gap passes below an empty path segment; the entry's own read scope is not
  established. Remove or limit the rule when a build's any-key reader reads a key before its
  object is in the array, or inserts other than at the count.

### Pre-trigger blocks

`common/pop_jobs` `possible_pre_triggers` and `common/pop_faction_types` `can_join_pre_triggers`
read one embedded `CPreTriggers` object. Both fields have reader identity `87346602eebc8fd3`
(`CPersistent::Read`, `CPreTriggers::ReadMember`), so the blocks share one key table.
Every event kind reads the same object: `CEvent::ReadMember` sends token `pre_triggers` to
`CReader::Read(CPersistent&)` on `CEvent + 0x4e0`, so all 22 kinds accept the block, not only the
config's three (`colony`, `carrier` and `country` subtypes in `events.cwt`). Each
`<kind>.pre_triggers` has the same reader identity and the same 14 keys.
`tests/expected/m452/pre-triggers.json` holds the keys of the two registry fields and of the
config's three kinds; `cargo parity pre_trigger` checks them, and
`every_event_kind_reads_the_pre_trigger_table` checks all 22 kinds.

Facts on M452:

- `CPreTriggers::ReadMember` (`0x1004d0168`) maps 14 tokens to `EPreTriggers` values 0 to 13 and
  tail-calls `CPreTriggers::ReadValue` (`0x1004d0008`); any other token goes to
  `CPersistent::ReadMember`. `TokenToEnum<EPreTriggers>` (`0x1004cfe3c`) has the same 14 cases.
  The keys: `has_ground_combat`, `has_owner`, `has_planet`, `is_ai`, `is_being_assimilated`,
  `is_being_purged`, `is_capital`, `is_enslaved`, `is_homeworld`, `is_idle`, `is_occupied_flag`,
  `is_robotic`, `is_sapient`, `original_owner`.
- `ReadValue` compares the id of the reader's value token (`+0x278`) with `0x3ff8` (`yes`) and
  `0x2cac` (`no`) directly, not through `CReader::Read(bool&)`. `GetTokenArray()` names them at
  `0x100dd26f8` and `0x100db0b1c`; they are the recipe's `boolean_tokens`. `yes` sets the key's
  bit in the required-true mask at `+0x8`, `no` in the required-false mask at `+0xa`. Any other
  value is ignored silently: no store and no diagnostic, where `CReader::Read(bool&)` reports
  `Malformed token`. A key whose bit is already set in either mask logs
  `Pre-trigger "%s" is specified more than once` and still sets the bit, so `yes` then `no`
  sets both masks.
- The config splits the keys into colony, pop and country sets (13 names; no `is_idle`). The
  reader has one table and no scope split. The `PreCalc*Value` functions that fill the values per
  scope object are use-time code; which keys each one sets is not established.
- The blocks' read scope is the [read-scope](#block-entry-contexts) answer (SDK-549); it stays
  unresolved with a `read-scope: no-scope-parameter` gap (SDK-808): `CPersistent::Read` takes no
  scope, and the method does not establish a scope that the object stores. Each key is `Boolean`
  and has its own read scope.
- `CEvent::CEvent(EScopeType)` builds its data in the nested constructor
  `CEvent::SEventData::SEventData(EScopeType)` at `this + 0x68`, which stores the `CPreTriggers`
  vtable point at its `+0x478`. The owner constructor walk enters a nested class's constructor
  like any other called constructor, and takes reader candidates from the vtable pages of the
  entered bodies as well as the owner's.

Each of the two fields and each of the 22 event `pre_triggers` fields is listed with all 14 keys,
and every key is `Boolean` with repeat `Merges` (`registry-fields/v34`, SDK-807). The rule is
stated, not derived: `readers::PRE_TRIGGER_VALUE` classifies a tail call to
`CPreTriggers::ReadValue(CReader&, EPreTriggers)` with the owner in `x0` and the reader in `x1`,
like the stated `CReader::Read(bool&)` rule. The key has no one destination, because the value
goes to one of two masks. A repeat keeps the earlier bit, so `Merges`, not `Replace`. `Boolean`
fits because the values are the Boolean pair; the `Keyword` rule of
[weight blocks](weight-blocks.md) is for an engine keyword set. This removed 672 key gaps
(`UnresolvedReader` and `UnresolvedStorage`); each block keeps its own
read-scope and repeat gaps. Derive the rule from the body (run the helper under `yes`, `no`
and another token, as command grammar forms do) when a second helper of this shape appears, or
when `ReadValue` changes on a new build.

## Block families

Fields and their conditional read alternatives carry conservative block families. Generic
persistent destinations are joined to constructor-installed virtual readers; a shared
`CPersistent::Read` call alone does not give a concrete identity. Command block families are on
[nested command grammar](command-grammar.md).

## Block entry contexts

`Field.entry_contexts` gives, for each root trigger, effect and weight block, the scopes that the
engine's evaluation calls supply for `this`, `root`, the `from` chain and the `prev` chain
(`registry-fields/v13`; `callbacks/blocks.rs`, `callbacks/climb.rs`, `callbacks/contexts.rs`,
`src/binding/binary/callbacks.rs`, `src/binding/binary/type_pointers.rs`,
`src/session/field_entries.rs`). The answer keeps `EntryScope::SelfLink`; read it by the [self-link rule](engine-commands.md#on_actions-game-rules-and-their-entry-scopes).
`this` is the scope at the call; the scope that the engine reads the block in is SDK-549's.

**How the engine evaluates a stored block.** A registry item evaluates its own blocks in its
methods: it passes `this` plus the block's storage offset to a trigger evaluator, an effect
executor or a weight evaluator (`CTrigger::Evaluate`, `EvaluateExtended`, `CEffect::Execute`,
`ExecuteExtended`, `CRootEffect::Execute`, and, called directly, `CAndTrigger::ActualEvaluate`,
`SafeExecuteEffect` and `SafeExecuteEffectExtended`; the weight evaluators are on
[weight blocks](weight-blocks.md#entry-contexts)), with a scope in `x1`. The method either builds the scope itself
(`CTraditionType::IsPotential(CCountry const*)`: a fresh scope, `SetCountry`, then `this + 0x108`)
or takes it from its caller (`CCouncilAgenda::IsPotential(CEventScope&, CString*)` evaluates
`this + 0x1c8` with its scope parameter; `CGovernment::UpdateCouncilAgenda` builds that scope).
The field method's storage offset is the offset from the method's `this`. Another class's method
can evaluate the block through the block object's own vtable, on a pointer to the item that it
received or keeps: `CCountry::AddEdict(CEdict const*)` runs `[[x1 + 0x2e0]] + 0x48` (`Execute`)
on `x1 + 0x2e0`, and `CMission::Start` loads its mission type from `this + 0x18` and runs slot
`+0x48` of the effect at `+0x190`. Some direct evaluator calls are in another class too
(`CTechnologyStatus::GetTechWeight(CTechnology const*)` evaluates `x1` plus an offset).

**The method.** A *type pointer* is a register that points at an owner's item at entry: the
receiver of a member of the owner, by the function's exact qualifier (a member of
`CAnomalyType::COutcomeEffect` is not a member of `CAnomalyType`); a parameter of type `O const*`,
`O*`, `O const&` or `O&`; or the word at offset `k` of an object of class `C`, when the
constructors of `C` and the members of `C` that load `TPdxNullObject<O>::_pInstance` store there
an item of `O` (a parameter of one of those types, or that null object) and no item of another
owner or other named value, and each of them decodes (`CMission` `+0x18`, `CResolution` `+0x18`,
`CTechnology` `+0x600`, `CGalacticCommunity` `+0x2c0`; SDK-830). A stored null, and a value that
the name pass does not name, agree with any owner. A parameter's register is known up to the
first parameter that one general register may not pass (pointers, references, integers, `bool` and `TPdxRef<…>` take one);
an enumeration, which a demangled name does not tell from a class, ends them, and a function whose
qualifier is not a known class or `(anonymous namespace)` has none. A class is a qualifier with a vtable, a type info, a
constructor, a destructor or a `const` member function: `CSubjectSpecialization` has only `const`
members. Two more values lead to an owner's item without a type pointer
([SDK-824](#arrays-and-owner-getters-on-m452-sdk-824)): an element of a buffer of owner pointers,
and the receiver of an *owner getter*, an offset getter that is a member function of `O` by every
name at its address. A `CPdxArray<O const*, int>` holds its vtable at `+0`, its buffer at `+8`, its
capacity at `+0x10` and its size at `+0x14`. Its buffer is the word at `+8` of a parameter of that
type by pointer or reference; the word at `k + 8` of an object of class `C`, when every
constructor of `C` decodes and every store that they make at `k` is the address point of
`vtable for CPdxArray<O const*, int>` (or `O*`), the symbol plus `0x10` (`CTradeDealSide` `+0xa0`,
`CCrisisLevelType` `+0x48`; [SDK-830](#member-and-database-arrays-on-m452-sdk-830)); and the word at
`k + 8` of a global database instance, `TGameDatabase<D>::_pInstance` or `D::_pInstance` loaded
through its pointer slot, when `D` has such a member array at `k`
(`CArchaeologicalSiteTypeDatabase` `+0x40`). A load from a buffer, at an immediate multiple of 8 or
at a register index scaled by 8 (`[x8, x23, lsl #3]`), is an element, and a move of a buffer
address by a multiple of 8 stays in the buffer. The candidate calls are the direct calls to an
evaluator or a tooltip builder and every `blr`, in the functions that have a type pointer, call an
owner getter or load such a database instance. The name pass names a load from an argument by
its chain of loads, three loads deep (`Member`, `Deref`, `DerefAt`), with or without pre-index
writeback (`ldr x8,[x0,#0x40]!` loads the vtable of the object at `x0 + 0x40` and leaves that
address in `x0`), and attributes a call to
`(owner, offset, family)` only when `x0` holds one type pointer plus one offset other than zero on
every path. A `blr` must also call a slot of the vtable at that same address whose displacement is
an evaluation slot. The binding derives the slots from the vtables: each slot of an evaluator
class's primary vtable that holds the evaluator, by its displacement (M452: trigger `+0x10` and
`+0x18` `Evaluate`, `+0x20` `ActualEvaluate`; effect `+0x48` `Execute`). The family is the
evaluator's or the slot's; a field joins only a block of its reader family, so a trigger field
does not join a call through effect slot `+0x48`, and an effect's `Read` at `+0x10` joins no
effect field. Every owner has a vtable, so the word at offset zero of its item is the item's
vtable pointer and holds no block. Callers and receivers are decoded only for the functions with
an attributed call. A function whose scope is its own parameter is a *wrapper*; the method
goes up its direct callers, at most 2, to the call that builds the scope. From that call it runs
the caller to the call and through it, and runs every wrapper and every function that receives a
scope, up to 6 calls deep, inline on the path with the caller's arguments and memory. Before the
selected call, evaluations act only through their effects. A call through a vtable slot that the
name pass attributes is an evaluation, and the run treats it as it treats an evaluator: the name
pass established the block's family there (SDK-824). Another call through a register is applied as
a call that the pass does not follow, so a scope that it receives is unknown to the later
evaluations on the path. A call
reads the argument registers that its demangled signature uses; a call to a C library function, direct to its stub or through an
import pointer, reads those that the C or POSIX signature uses (`LIBRARY_ARGUMENTS`: `_strlen`
one, `_memmove` three, the stack probe `___chkstk_darwin` none) and not `x8`; a call to a
function that never returns ends the path. A copy of a
scope pointer in the stack reaches a call only in the frame of a stack address that the call
receives, at or above that address, and not in a register save: a store of `x19` to `x30` to the
stack in a function's prologue, before its first other instruction, that no later store has
overwritten. A virtual call on the object that a proven instance pointer holds calls the slot of
that object's vtable, so it reads the registers that the slot's signature uses. A call receives
`x8` only when its target may read it ([engine commands](engine-commands.md#pitfalls)), and a
call that fires an on_action or evaluates a game rule is a scope reader
([engine commands](engine-commands.md#on_actions-game-rules-and-their-entry-scopes)), whose
method shares this caller climb (`callbacks/climb.rs`). A copy
constructor of a scope that the pass can read, `CopyInternalScopes` and a function that builds a
scope in the object that `x8` addresses are followed as the engine runs them (same page); a copy
into an object that the pass does not know lets its source escape.

**Offset getters, helpers and tooltip calls** (SDK-732 part 2). An *offset getter* is a function
whose whole body is `add x0, x0, #k; ret`, such as
`CSpecialistSubjectType::GetOnProgressCompleteEffect` (`k = 0xa0`). The binding lists the getters
that the decoded functions call; the name pass gives a call to one the result `x0 + k`, so the
evaluation that follows names one block. A call to an owner getter returns the owner's offset `k`
whatever `x0` holds: by C++ typing its receiver is an item of its class. A *helper* is a
function whose evaluation's block is its own parameter: `x0` is `Argument(n, 0)` for a parameter
`n` that leads to no owner, such as `CMission::Stop(CRootEffect const&, EMissionStatus)`, which
runs slot `+0x48` of `x1` on a scope that it builds. At each direct caller the name pass names the
block in register `n` as at a site, and that call becomes an entry that runs through the helper.
Only the contexts that the run reads at the helper's own evaluation go to that caller's block; the
run reads no other evaluation, and other runs do not enter the helper. The method goes one caller
up: a caller that passes its own parameter on (`block-caller-depth`), a caller whose register names
no block (`unattributed`) and a helper with no caller (`no-caller`) give no block a context and are
counted for the helper in `BlockEntries::helpers`, for Native's developers; a helper whose scope
is a parameter too charges the caller's block `block-and-scope-from-caller`. A helper must have a
type pointer, as every function with a site must, because the candidates are the functions with
one; give helpers a candidate rule of their own if an Atlas-needed block is passed to a helper
outside that set. A *tooltip call* is a direct call to a `BuildToolTip(CEventScope&, bool, int,
CSimpleBitMask<…>) const` of any class, or a call through the tooltip slot, which the binding
derives from the `CTrigger` and `CAndTrigger` vtables as it derives the evaluation slots (M452:
`+0x58`). It names a block of the trigger family but evaluates nothing. A block that a tooltip call
names and no attributed evaluation does gets the gap "only tooltip calls name this block; the
method established no evaluation of it".

On M452 the binding finds 27 offset getters that the decoded functions call. 49 functions are
helpers; 17 calls to them name a block: 12 calls to `CMission::Stop` and 5 to
`CScriptedActionInfo::IsClickable` and `TestScopeDependentTrigger` (whose blocks join no root
field). The others count 446 `unattributed` caller evaluations, 64 `block-caller-depth` and 8
`no-caller`: most are virtual calls at an evaluation slot's displacement on a parameter that is
not a block (`CPdxArray<…>::InsertAt`, `CCountryEthos::ShiftTowardsEthic`), whose callers pass no
owner item plus an offset.

**Instance pointers** (`callbacks/instances.rs`, `src/binding/binary/instances.rs`). A global word
that holds one object's address, such as `TPdxNullObject<CTraditionSwap>::_pInstance`, is set at
run time (`Allocate` stores the address that `ProtectedMemoryAccessBuffer` returns), so the image
does not hold the object. One forward pass over a function's branches (`register_flow`) keeps
what each register may hold: a slot's page, the pointer's address, its object, the object's
vtable and a vtable slot. The binding takes the pointer slots through which a virtual call in the
decoded functions may load its receiver, and every function that loads such a slot and has an
`adrp` of a page that holds a vtable. The same pass finds the ones that may write through the
slot: a store whose base or stored register may hold the pointer's address or its object, or a
call that may receive the pointer's address. Each of those runs with the
pointer holding a scratch object, calls not followed. The pointer is proven when every path of
every such writer returns, leaves the object in place or clears the pointer (the destructor), and
either leaves its first word unwritten or stores one vtable address point there, and no call
receives the object or the pointer after that store; a path that stops at a search bound, or a
writer that does not decode, rejects it. The method then places the object in the read-only data
with only that word known. On M452 the block input proves one pointer,
`TPdxNullObject<CTraditionSwap>::_pInstance`. The null ship's pointer is not proven: 66 functions
may store it and their searches stop at the path limit. The proof runs about 2,900 writers and
adds about 1.4 s to the block input; narrow the writer test if that grows. It serves one call
site on M452, `ExecuteTradition` → `CTraditionType::GetUnlocksAgenda` (council agenda `potential`
and `allow`), and stays because no other rule reads that virtual call's registers. None of the
SDK-712 shapes goes through an instance pointer, so it was not extended; prove more pointers only
with a plan review, because a deeper search or a weaker writer rule changes a stated soundness
rule.

**Assumptions.** Beside the self-link rule:

- Every function that writes a member word that leads to an owner keeps the owner's layout: it
  stores an item of that owner, its null object, or null. The method reads the constructors and
  the members that load an owner's null object; a writer that does neither, and a stored value that
  the name pass does not name, are taken on this assumption. Checked by hand on M452 for `CMission`
  (its constructors, and `ReadMember`, which stores a lookup result or
  `TPdxNullObject<CMissionType>::_pInstance` at `+0x18`), `CResolution` (the constructor from a
  `CResolutionType const&` stores it at `+0x18`; the default constructor stores
  `TPdxNullObject<CResolutionType>::_pInstance`; `ReadMember` stores the result of
  `ReadKeyReference<CResolutionTypeDatabase>`) and `CCosmicStorm` (`+0x328`: the constructor
  from a `CCosmicStormType const*`, the default constructor, which passes it the null object, and
  `ReadMember`, which stores a lookup result or `TPdxNullObject<CCosmicStormType>::_pInstance`),
  and for the SDK-830 words: `CTechnology` `+0x600` (`ReadMember` stores
  `TPdxNullObject<CTechnologyTier>::_pInstance` or an element of the tier database's item array,
  `PostReadInitialize` a value that the pass does not name), `CGalacticCommunity` `+0x2c0`
  (`DailyUpdate` stores `TPdxNullObject<CGalacticFocus>::_pInstance` or the chosen focus) and
  `+0x348` (`TPdxNullObject<COwnershipLimits>::_pInstance`).
  Remove the assumption with a writer check that covers every function that stores the word.
- A member array keeps the vtable that its constructors store, and a database instance is an
  object of its database class. Checked by hand on M452: `CTradeDealSide()` stores the
  `CPdxArray<CTradableAction const*, int>` vtable at `+0xa0` and `CalcValueFor` walks the buffer
  at `+0xa8` with the size at `+0xb4`; `CCrisisLevelType(int, CString const&)` stores the
  `CPdxArray<CCrisisPerk const*, int>` vtable at `+0x48` and `UnlockLevel` reads the buffer at
  `+0x50` and the size at `+0x5c`; `CArchaeologicalSiteTypeDatabase()` stores the
  `CPdxArray<CArchaeologicalSiteType const*, int>` vtable at `+0x40` and `CreateRandomSite` reads the
  buffer at `+0x48` and the size at `+0x54`.
- A demangled name does not say whether a member function is static; a function of a class is
  read as taking its receiver in `x0`, as the owner test did before SDK-732.
- [Evaluation leaves a scope as it found it](engine-commands.md#on_actions-game-rules-and-their-entry-scopes):
  trigger, effect and weight code that receives a scope is a scope reader.
- A bounded search with no contradiction: when the paths of a call stop only at the path, loop or
  step limit, and every evaluation that the followed paths reach receives a scope that the method
  can read, the found contexts stand with no gap. A path that reaches an evaluation with an
  unreadable scope is a contradiction; then the bounds are gaps too. So is a bound inside a
  function that receives the scope (SDK-830): the path that it cut may have built the scope
  differently. `CSpatialObjectRefCaster::FillEventScope` switches on the spatial object's type
  through a jump table; the fork of the table exceeds the path limit, and the one followed path,
  the default for a type above 10, sets no scope.
- A prologue's register save is not memory that a callee can reach through a lower stack address
  that it receives. Checked by hand at three of the eleven call sites that the rule changes:
  `CDiplomaticActionType::IsPotential(CEventScope const&, CString*)` saves the caller's `x23`, the
  from scope, at `sp+0x88`; `CSystemType::IsPotential` saves `x22`, the from country, at `sp+0x60`;
  `CColonyType::IsPotential` saves `x20`, the from country, at `sp+0x90`. Each then passes a local
  string below the saves to `CString::CString` on the script profiler's path.
- The object that an instance pointer holds gets its vtable only in code that loads the pointer
  and forms an address on a vtable's page, keeps that vtable once set (a call on it, such as
  `IsValid`, does not change it), and is never written through an unknown address or through a
  copy of the pointer in memory. Checked by hand for `TPdxNullObject<CTraditionSwap>`,
  `<CCouncilAgenda>` and `<CShip>`: `Initialize` clears the object, runs its base constructor and
  then stores `vtable for TPdxNullObject<T>` + 0x10 at word 0; the calls after it only change
  memory protection, and the destructor stores null in the pointer.

**Result on M452.** The field sweep's `entry_contexts` section gives these counts
([method authoring](method-authoring.md#run-over-the-whole-population)). 312 root trigger,
effect and weight blocks in 88 of the 164 registries. Of the 239 trigger and effect blocks, 154
have contexts and no entry gap, 54 have contexts and a gap, and 31 have none (SDK-735;
154, 50 and 35 before); 26 keep several
contexts with a known `this`, 66 name a typed `from` and 4 a typed `prev`. The 73 weight blocks
are on [weight blocks](weight-blocks.md#entry-contexts). 6 registries have 7 direct
evaluation calls in the owner's methods whose block the method cannot name. SDK-726's two rules
changed 10 blocks in 6 registries, each only removing an unresolved context and its gaps: the
register saves those of astral action `potential` and `is_exhausted`,
colony type, observation mission and system type `potential`, and diplomatic action `potential`,
`possible` and `proposable`; the instance pointer those of council agenda `potential` and
`allow`. SDK-712 changed 15 blocks in 9 registries, and each change only adds a context, removes
an unresolved one or narrows a gap: the new direct evaluators add armies `potential` and
`potential_country`, megastructure `outliner_trigger`, `dismantle_potential` and
`should_ai_dismantle`, and button effect `effect` (an unreadable context, from
`CButtonEffect::ExecuteEffect`); `x8` and the firing readers resolve megastructure
`on_build_start` and `on_build_complete`; `ands` and `bics` open pop faction `can_join_faction`,
pop job `possible` and the buildings `on_queued` paths, and situation `on_abort` gains a context.
SDK-729 changed 3 blocks: ship size limit `show` gains a country context, which a stale value hid
(no `replace_scopes`); the path to ai budget `potential` and to megastructure `on_build_complete`'s
context with a starbase `prev` now passes the path limit. The budget keeps its bound gaps; the
megastructure keeps its other, readable context, so by the bounded-search assumption below its
bound is not a gap.
SDK-730, which follows copies and scope factories
([engine commands](engine-commands.md#on_actions-game-rules-and-their-entry-scopes)), changed the
8 psionic aura blocks: each gains a galactic object context with a country from, from the scope
that `CPsionicAura::InitAuraScope` returns. The config agrees on the stated keys
(`this = galactic_object`) and omits the from. SDK-734, which gives C library stubs their argument
registers, changed 4 blocks. Psionic aura `on_gain_level` and `on_lose_level`
(`CIntensityLevel::OnEnter`, which the lambda in `CPsionicAura::UpdateIntensityLevel` calls first)
and casus belli `on_proxy_war_start` (`CCasusBelliType::OnProxyWarStart`, `0x100102d18`) each call
`_strlen` on the script profiler's path while `x1` still holds the scope; each loses an unreadable
context and its gaps. Ai budget `potential` gains a country context with no from:
`CCountryAI::UpdateUpkeepBudget` calls `_bzero` with a stack address in `x8` (`mov x8,sp`,
`0x100e00c38`), which let the frame that holds the scope escape. It keeps the bound gaps of
`UpdateExpenditureBudget`, which reaches no evaluation.
Starbase buildings and modules each gain one unnamed evaluation: `GetEquippedComponents` evaluates
a trigger in an element of a component list, not `this` plus an offset.
SDK-731, which shares the caller climb with on_actions and game rules, makes each run read only
the evaluations of the blocks that its entry carries, and changed 3 blocks. Casus belli `is_valid`
loses an unreadable context and its gap: a run that carries another block read its evaluation
with a scope that the run's entry did not build. Artifact action and astral action `potential` gain a
`path-limit` gap: `IsAllowed` passes its scope on to `IsPotential`, and the runs from its callers
that carry `potential` reach no `potential` evaluation before the path limit; the `allow`
evaluation that those runs reach had hidden the bound. SDK-733, which makes the weight
evaluators block evaluators, changed one trigger block: planet modifier `potential` gains a planet
context and loses its path-limit gap. Its run starts at
`CPlanetModifierDatabase::GetRandomModifier`, which builds the planet scope and calls
`CPlanetModifier::GetSpawnChance(CEventScope&)`, which calls `IsPotential`. Before, the run entered
`GetRawFactor` as a scope receiver, and its modifier loop used up the path limit before a path
reached the evaluation in `IsPotential`. Anomalies and civics gain unnamed weight evaluations
([weight blocks](weight-blocks.md#entry-contexts)).
SDK-732 part 1, which attributes evaluations through the block's own vtable and direct evaluator
calls in any function with a type pointer, changed 28 blocks in 13 registries; no block lost a
context. 25 trigger and effect blocks and 2 weight blocks (technology `ai_weight` and
`weight_modifier`) gain contexts where they had none: archaeological site `on_create`,
`on_roll_failed` and `on_visible`, armies `allow`, artifact action and astral action `effect`, dust
cloud `condition`, `on_system_added` and `on_system_removed`, edict `effect` and `on_disabled`,
espionage operation `on_create` and `on_roll_failed`, mission `on_daily`, `on_monthly`, `on_start`
and `on_stop`, relic `possible` and `active_effect`, resolution `effect` and `fail_effects`, and
storm `on_start`, `on_monthly`, `on_moved` and `on_finished`. Seven of them have only an unreadable
`from` or context (missions, through `CSpatialObjectRefCaster::FillEventScope`'s jump table;
espionage operations; archaeological site `on_visible`). Buildings `potential` gains a second,
unreadable context: `CColony::CanAddBuildingType(CEventScope const&, CBuildingType const&, CString*)`
evaluates `x2 + 0x108` through slot `+0x18` with its scope parameter, and both of its callers stop
at the path limit. Anomalies lose their 2 unnamed evaluations (the exact receiver rule). These
call sites were checked by hand in the disassembly:

- armies `potential`: `CArmyType::IsPotentialTrigger` builds a colony scope at `sp+0x170`, sets a
  second scope's type to species (`Set(0x800, …)`), links it as from (`str x21,[sp,#0x1a8]`) and
  passes `this + 0x4f8` to `CAndTrigger::ActualEvaluate`.
- council agenda `potential` and `allow`: `CGovernment::UpdateCouncilAgenda` builds a country scope
  and calls `IsPotential` and then `IsAllowed` with it (`this + 0x1c8`, `this + 0x280`).
  `CAIInteriorMinister::HandleCouncilAgenda` passes its country scope through
  `SelectByWeightedRandom<CCouncilAgenda>`, which keeps it in `x20`. `ExecuteTradition` passes its
  country scope to `CTraditionType::GetUnlocksAgenda`, which, for a tradition with no swaps, calls
  slot `+0x40` of the null tradition swap (`TPdxNullObject<CTraditionSwap>::IsValid() const`) while
  `x1` still holds the scope.
- council agenda `init_effect`: `CGovernment::SetCouncilAgenda` builds a country scope;
  `ExecuteInitialEffect` tail-calls `CEffect::Execute` on `this + 0x580`.
- tradition `potential`: `CTraditionType::IsPotential` builds its own country scope.
- diplomatic action `on_accept`: `CDiplomaticActionType::OnAccept` builds two country scopes, links
  the second as the first's from and runs `this + 0x2f8` with the first.
- tradition and ascension perk `on_enabled` and `on_disabled` (SDK-735): both registries use
  `CTraditionType`. `OnEnabled` (`0x100ce3648`) builds a country scope at `sp + 0x60`, selects
  the swap's block `+0x378` or its own `+0x418` at `0x100ce3748`, then calls slot `+0x48` of
  that same selected object at `0x100ce3848`. `OnDisabled` (`0x100ce3a80`) builds the same scope,
  selects the swap's `+0x420` or its own `+0x4c0` at `0x100ce3b7c`, and calls the slot at
  `0x100ce3c7c`. Each owner's block gains a country with self-links. The indexed swap load is
  still unknown; those arrivals retain `selected-block-identity`, which also keeps the path-limit
  gap. The two `on_enabled` contexts agree with the config's country `this` and `root`.
- edict `effect`: `CCountry::AddEdict(CEdict const*)` (`0x10028d49c`) keeps the edict in `x21`,
  builds a scope at `x19 + 0x60` (`CEventScope(int)`, `SetCountry` of `this`) and calls
  `[[x21 + 0x2e0]] + 0x48` with `x0 = x21 + 0x2e0`: a country with self-links. `RemoveEdict` runs
  `on_disabled` at `+0x388` the same way.
- resolution `effect`: `CGalacticCommunity::PassResolution(CResolution&, bool, bool)`
  (`0x10055ce08`) keeps the resolution type `[x1 + 0x18]` in `x23` across its calls and calls
  `[[x23 + 0x3a8]] + 0x48` with `x1 = sp + 0x358`. When bit 0 of the type's byte `+0x168` is set
  (a targeted resolution), that scope is the target country (`GetTarget`) with the resolution's
  country at `sp + 0x78` as its from (`str x8,[sp,#0x390]`); otherwise it is the resolution's
  country with no from. `FailResolution` runs `fail_effects` at `+0x450` the same way.
- mission `on_start`: `CMission::Start` (`0x1009531d4`) builds four scopes, fills them with
  `BuildEffectScopeForOperator`, and calls `[[[this + 0x18] + 0x190]] + 0x48` with
  `x1 = sp + 0x450`. The country `this` is read; the from goes through
  `CSpatialObjectRefCaster::FillEventScope`, a jump table on a run-time type, so it stays
  unresolved, and a path on which the operator sets no country gives `this` no type.

SDK-732 part 2 (offset getters, helpers, tooltip calls, pre-index loads and the `const` member class
rule) changed 6 blocks in 3 registries; no block lost a context and no other answer changed. Read
by hand:

- specialist subject type `on_progress_complete` gains an agreement with self-links:
  `CSubjectSpecialization::FinishConversion(CSpecialistSubjectType const&, CAgreement const&)`
  (`0x100c6f8a0`) builds a scope at `sp + 0x30` (`CEventScope(int)`, `SetAgreement(x20)`), calls the
  getter `GetOnProgressCompleteEffect` (`add x0,x0,#0xa0; ret`) on `x1`, and calls slot `+0x48` of
  the result with `x1 = sp + 0x30`. Its class has no vtable or constructor symbol; the `const`
  member rule makes `x1` a type pointer.
- mission `on_success`, `on_fail` and `on_cancel` gain one unreadable context each (`this` of no
  type, an unresolved from) with path-limit and branch-value gaps: `CMission::Succeed`, `Fail`,
  `Abort`, `TimeOut`, `ForceStop`, `AbortIfNeeded` and the two daily updates pass
  `[this + 0x18] + 0x388` (`on_success`) or `+ 0x430` (`on_fail`, `on_cancel`) in `x1` to the
  helper `CMission::Stop`, which builds its scope with `BuildEffectScopeForOperator` and calls slot
  `+0x48` of `x1` at `Stop+0x150`. From the callers, the paths that set a country stop at the path
  limit, and the from goes through `FillEventScope`'s jump table.
- mission `on_issue` gains a country with an unresolved from:
  `CContractManager::IssueContract(CCountry&, CMission&)` builds a country scope from `x1`, loads
  the mission type `[x2 + 0x18]` and calls slot `+0x48` of it plus `0x40` through
  `ldr x8,[x0,#0x40]!`; the from goes through `FillEventScope`.
- decision `custom_tooltip` has the tooltip gap: `CDecision::GetToolTip(CToolTip&,
  CColonyCarrier const&, bool) const` calls `CCustomTooltipTrigger::BuildToolTip` on
  `this + 0x198`, and no evaluation names the block.

**Comparison with the config's `replace_scopes`** (M451-hotfix; the SDK-726 changes above remove
only unresolved contexts, and of the SDK-712 changes only armies `potential` disagrees). Read through the self-link rule, comparing the
keys that the config states (`system` is the engine's `galactic_object`; `any` matches any scope):
107 blocks have a readable context and a config expectation. 82 agree, 5 agree on the stated keys
where the engine also sets a first link that the config omits, and 20 disagree. 103 blocks have no
readable context and 27 have no `replace_scopes`. Each disagreement was read by hand:

- **Source errors (10).** The engine agrees with Native. Armies `potential` (SDK-712, M452) is an
  eleventh: the config gives a country, but `CArmyType::IsPotentialTrigger` evaluates it on a
  colony with a species from. Armies `on_built`, `on_queued` and
  `on_unqueued` run on a colony with no from (`CArmyType::OnBuilt`: `SetColony`), not a planet with a
  species from. Starbase building and module `abort_trigger` and `destroy_trigger` run on a starbase
  with no from (`ShouldAbort`, `ShouldDestroy`: `SetStarbase`). Overclock `potential` and `possible`
  run on the megastructure (`COverclockType::IsPotential`: `SetMegaStructure`), not a country.
  Megastructure `on_cycle_complete` runs on the megastructure with no links (`OnCycleComplete`).
- **The config states one of several contexts (9).** Bypass `potential` and `country_can_use` link
  the second country as from only when it is set (`CBypassType::CanPotentiallyBeUsedByCountry`).
  Megastructure `on_build_queued` and `on_build_unqueued` link the fleet as fromfrom only when it is
  set (`OnBuildQueued`). Colony type `potential` also runs on a planet from the surface view
  (`CSurfaceView::RefreshDesignations`). Observation mission `potential` and `valid` also run from
  the order button with a from of no type. Button effect `potential` and `allow` also run from
  `CEffectButton::PerFrameUpdate` on a scope with no type.
- **A Native limit (1).** Deposit `can_be_cleared_potential` is filled by
  `CDepositHolderRefCaster::FillEventScope`, whose type is a run-time value: the followed fallback path
  has no type and the typed paths stop at a jump table, which keeps a gap.

The five omissions name a from or prev that the engine sets and the config leaves out: bombardment
stance `trigger` (from planet on one path), casus belli `potential` (from country), diplomatic action
`possible` (prev country, from the three-country `IsPossible`), pop faction `on_destroy` (from pop
faction) and system type `potential` (from country). Per-name conclusions go to the Atlas ledger
(SDK-704).

The 27 blocks that SDK-732 part 1 gives contexts (M452, same rules): 9 agree (archaeological site
`on_create` and `on_roll_failed`, artifact action `effect`, edict `effect` and `on_disabled`, relic
`possible` and `active_effect`, technology `ai_weight` and `weight_modifier`), 3 disagree, 8 have no
`replace_scopes` (astral action `effect`, the three dust cloud blocks, the four storm blocks), and 7
have no readable context. The disagreements, read by hand:

- Resolution `effect` and `fail_effects`: the config states the targeted resolution's context
  (`from = country`); an untargeted resolution has no from. The config states one of several
  contexts.
- Armies `allow`: `CCountry::CalcIsArmyTypeAllowedOnAnyColony(CArmyType const&)` sets each colony
  (`Set(1 << 40, …)`) on a fresh scope and calls slot `+0x20` of `x1 + 0x440`: a colony with no
  from. The config gives a planet with a species from, as for armies `potential`. A source error,
  unless another evaluation that the method does not find supplies the config's context.

Of the SDK-732 part 2 changes (M452, same rules), 1 block has a readable context and it disagrees;
the 4 mission blocks have no readable context. Specialist subject type `on_progress_complete`: the
config gives `this = country`, but `FinishConversion` runs it on the agreement (above). The vanilla
blocks are empty, so no script shows the intended scope; read it as a source error.

**Conditional selections (SDK-735).** Against `30574fe`, the full sweep changes only these four
blocks in two registries; no existing context or other answer changes. Across all 312 blocks,
187 have contexts without an entry gap, 57 have contexts with a gap, and 68 have no context.
The registry totals remain 8 complete, 156 partial and 0 failed. Unknown receiver memory is
reserved only for the selection fallback, so existing singleton runs retain their former memory
behavior. A test confirms that storing a scope pointer in this receiver still lets it escape at
an opaque call.

The fallback covers positive receiver offsets below 1 MiB, with at most 16 candidate offsets per
register, and a direct evaluator or a locally proved same-receiver virtual slot. Receiver spills,
array elements and caller-supplied scopes need further origin tracking. Keep these limits until
an Atlas-required block needs those shapes; increasing bounds alone cannot establish an origin.
The local dispatch proof must discard a vtable identity after a 32-bit write: `mov w8,w8`
truncates the pointer even though the register number stays the same.

**Gaps.**

- Four effect blocks retain `selected-block-identity` and `path-limit`: the swap paths above do
  not establish which block they evaluate. A readable owner context does not erase this uncertainty.

- 21 trigger and effect blocks have no attributed evaluation and 1 is named only by tooltip calls
  (25 before SDK-735). The 35 weight blocks with this gap are on
  [weight blocks](weight-blocks.md#entry-contexts). By group, read by hand on M452:
  - *Offset getters.* Specialist subject type `on_progress_complete` resolves (above). Agreement
    term value `activate_effect` and `deactivate_effect` keep the gap:
    `CAgreementManager::CreateAgreement` calls `GetActivateEffect` on each element of the term
    data's term arrays (`ldr x0,[x22]`), and `CAgreement::SetTermData` and `DestroyAgreement` call
    the getters on `[x23, #8]`, array elements that no type pointer leads to. Specialist subject
    perk `activate_effect` and `deactivate_effect` have no getter:
    `CSpecialistSubjectLevel::OnLevelUp` and `DeactivatePerks` call slot `+0x48` of each perk of the
    level's perk array through `ldr x8,[x0,#0x40]!` and `#0xe8]!` on an agreement scope.
  - *Helper.* Mission `on_success`, `on_fail` and `on_cancel` gain an unreadable context (above).
  - *Members that no type pointer leads to.* First contact `on_roll_failed`:
    `CFirstContact::HandleRollFailed(int)` calls slot `+0x48` of `[this + 0x38] + 0x1d0`, and no
    `CFirstContact` constructor fills `+0x38` with a stage parameter. Pop faction `on_create`:
    `CPopFaction::Initialize(CCountry const&, CPopFactionType const&)` calls slot `+0x48` of the
    type that it stored at `this + 0x18`, plus `0x6c0`; `valid`: `CPopFaction::IsAlive() const`
    calls slot `+0x10` of `[this + 0x18] + 0x608`. `Initialize` is not a constructor, so the member
    rule does not establish `+0x18`. Crisis level `on_unlock`:
    `CCrisisProgression::UnlockLevel(CCrisisLevelType const*, CCountry*)` spills its parameter
    (`stur x1,[x29,#-0x60]`), and the name pass forgets the spill at a call that receives a lower
    stack address.
  - *Arrays and lookups.* Menace perk `on_unlock`: `UnlockLevel` calls slot `+0x48` of
    `x28 + 0x190`, where `x28 = [x26]` is an element of the level's perk list. Tradable action
    `on_traded_effect`, `on_deal_ended_sender_effect` and `on_deal_ended_recipient_effect`:
    `TradeActions` and `EndActions` (anonymous namespace) call slot `+0x48` of an element of a
    `CPdxArray<CTradableAction const*>` parameter plus `0x268`, `0x310` or `0x3b8`. Mission
    `on_accept`: `CContractManager::PickupContract(TPdxRef<CCountry>, TPdxRef<CMission>)` calls
    slot `+0x48` through `ldr x8,[x0,#0xe8]!` on a mission that it looks up from a `TPdxRef`.
    Portrait sprite configuration `trigger`:
    `CPortraitSpriteType::EvaluateConfigurationIndex(CEventScope&) const` calls slot `+0x10` of an
    element of the type's configuration array at `this + 0x240`, plus `0x40`, with its scope
    parameter.
  - *Tooltip calls.* Decision `custom_tooltip` has the narrower gap (above). System tooltip
    `custom_tooltip` keeps the wider one: `CGalacticObject::GetToolTip` calls the tooltip slot
    `+0x58` of `x21 + 0x40`, where `x21` is a database element, which no type pointer leads to.
  - *No evaluation found.* A scan of every function for an `add`, or a pre-index `ldr`, of the
    block's offset followed by a call finds only constructors, destructors, `ReadMember`, database
    loading and interface text for espionage asset `possible` and `potential`, lawsuit `effect`,
    galactic focus `effect` (only `CGalacticCommunityView::BuildGalacticFocusTooltip`, a
    description call), and first contact `on_create` and `on_abort`.
- 37 trigger and effect blocks keep an unreadable context: a call that the method cannot see into
  receives the scope. The main shapes: a setter or filler whose scope type is a run-time value
  (`SetColonyCarrierRef` for decisions and deposit `on_cleared`; `FillEventScope`,
  `SetupScopeObject` and `CSelectable::DetermineScope` jump tables, which also hide the mission
  and espionage operation from, and give mission `on_success`, `on_fail` and `on_cancel` a `this`
  of no type through the helper `CMission::Stop`); a `from` that is the caller's scope parameter
  (megastructure `potential`, `possible`, `context_menu_potential`, which
  [SDK-812](#block-entry-context-stops-on-m452-sdk-812) resolves); a scope that is a member of a
  parameter or
  heap object (event chain `abort_trigger` and button effect `potential` and `allow` from
  `CExecuteButtonEffectCommand::IsValid`, which copies the command's member at `this + 0x20`); and
  a function that the pass enters and that stops at the path limit (`AddSpentResourcesToScope`
  under buildings `on_queued` and `on_unqueued`; the callers of `CColony::CanAddBuildingType` for
  buildings `potential`).
- Path, loop and step limits are gaps only where a contradiction appears (41 trigger and effect
  blocks at the path limit, 15 at the loop limit); 16 stop at a branch on an unknown value and 4 at
  an instruction. The weight blocks' bounds are on
  [weight blocks](weight-blocks.md#entry-contexts).
- 7 trigger and effect blocks and 1 weight block keep `no-caller`, and 4 trigger blocks keep
  `caller-depth`. These gaps are already narrow; the wrapper that each one ends at:
  - `no-caller`: casus belli `destroy_if` (`CCasusBelli::ShouldDestroyScripted`), colony
    automation `available` (`CColonyAutomationDatabase::GetNextDistrictToBuildPrio`), decision
    `potential` (`CSelectPlanetDecisionEffect::ExecuteActual`), dynamic text `available`
    (`CDynamicTextDatabaseEntry::IsAvailable`), greeting overlay sound `possible`
    (`CGreetingOverlaySound::IsPossible`), pop faction `is_potential`
    (`CEnableFactionTypeEffect::ExecuteActual`), `on_set_leader` (`CPopFactionType::OnSetLeader`)
    and the weight `leader` (`CPopFactionType::GetLeaderWeight`). An effect's `ExecuteActual` is
    reached only through its vtable, and the others through a call that the method does not find.
  - `caller-depth`: astral action `potential` and `is_exhausted` (`CAstralAction::GetTooltip`),
    casus belli `is_valid` (`CCasusBelli::ShouldDestroy`) and pop job `possible`
    (`CPopJob::IsPossible`).

  Script values and modifier blocks are outside the method.

Pitfalls:

- A call with the item itself in `x0` through slot `+0x10` is the item's own virtual call: before
  the offset-zero rule, such calls on owners made 840 of 1,372 entry runs and doubled the block
  method's time. Destructors and `ReadMember` functions still make runs on other members (an
  effect's `Read` is slot `+0x10`, a trigger slot); they join no field of another family.
- The member rule reads constructors only. A class that fills the word in an initializer, such as
  `CPopFaction::Initialize`, leads to no owner.
- A type pointer spilled to the stack is forgotten at a call that receives a lower stack address,
  such as the scope object, by the name pass's stack rule (crisis level `on_unlock`).
- Most helpers are not block helpers: a virtual call on any parameter at an evaluation slot's
  displacement (`+0x10`, `+0x48`) makes a function a helper. A call to one names a block only when
  the caller passes an owner's item plus an offset, and a field joins only its own storage offset,
  so these helpers add no answer; they add runs only where a caller passes an item plus an offset.
- A pre-index load (`ldr x8,[x0,#k]!`) both loads the block's vtable and moves `x0` to the block:
  without it the name pass loses the vtable word (mission `on_issue`, the specialist subject
  perks).
- A scope pointer left in an argument register is not an argument when the callee's signature does
  not take it; an unknown virtual call cannot be checked this way, but a call on a proven instance
  pointer's object can. A C library stub keeps its raw name (`_strlen`), so only
  `LIBRARY_ARGUMENTS` gives its count; it lists the stubs that the decoded code calls on M452, and a
  stub that it lacks reads every argument register. Add a stub when a gap names a call to it.
- Only a prologue's save is a register save. A save after the function's first other instruction,
  and a spill of the same pointer in the body, are stack memory like any other; telling them from
  an object field needs object extents, which the method does not have.
- An instance pointer's writers include its destructor, which clears the pointer; rejecting a
  cleared pointer rejects every null object. Tooltip builders such as `CTraditionType::GetDesc`
  load the null swap and form vtable addresses but only read it, and their searches stop at the
  path limit, so a writer must be one that may store through the pointer. A pass in address order
  that keeps a register's value through later writes marks them as writers, because they reuse
  the register; one that clears a register on any write misses a store on a branch around that
  write.
- The stand-in objects lie 1 MiB apart in the read-only data, so a load from one at a field
  offset reads an unknown word, not the next object's vtable.
- `run_paths_to` skips the site checks inside an entered call, and `follow` runs a setter outside the
  entered calls; without them a prefix that enters a wrapper is lost or runs into the caller.

### Block entry-context stops on M452 (SDK-812)

**Atlas need.** Atlas credits a block's `scope_context` against a config `replace_scopes` claim
only when the read scope is known, the block has an entry context, every slot resolves, and the
field has no `UnresolvedPath`, `UnreadableInput` or `UnnamedDeclaration` gap (Atlas
`assemble_scope_context`). In the SDK-798 comparison (Atlas `95e260b`, config `8574760`), 143
registry `replace_scopes` claims were blocked by such a gap. 56 of them, on 53 blocks in 25
registries, had a known read scope and no blocking gap other than an entry-context stop, so only
these could gain from this method. The other 87 also wait on the read scope (SDK-808) or the reader
(SDK-710). The 16 type-level `replace_scopes` claims have no Atlas subject.

**Repairs.** Three rules, each with authored tests:

- `csetm` runs as `cset` with all ones for true. Councilor `possible` resolves
  (`CCouncilView::RefreshCouncilPickingView` sets a mask with it before the evaluation).
- A switch table with an unknown index forks to each case when its guard proves the bound
  ([compiler jump tables](#compiler-jump-tables)). Observation mission `potential` resolves: the
  order button's `CFleetOrbitalSlotHandle::ExecFunctor` now gives each orbital type. Button effect
  `potential` and `allow` gain the types of `SetupScopeObject`'s cases but keep a `branch-value`
  stop on a walk that begins after a guard, and deposit `can_be_cleared_potential` gains planet,
  ship and astral rift (one case stays unreadable, so its `branch-value` gap becomes an unresolved
  scope). The mission blocks lose `branch-value`, but
  `FillEventScope`'s eleven cases then take their runs past the path limit.
- A scope parameter linked into the entry's scope is read at the callers (module comment of
  `callbacks/blocks.rs`). `CMegaStructureType::IsPotential(CCountry const&, CEventScope&)` builds a
  country scope and stores `x2` as its from; `IsPotentiallyBuildable` passes a fleet scope
  (`Set(0x40, …)`) and `IsPotentiallyUpgradableFrom` a megastructure. Megastructure `potential`
  (from fleet or megastructure), `possible` (country then fleet, or country then megastructure) and
  `context_menu_potential` (country then fleet) resolve. A retry that the climb charges, or whose
  run stops for a reason or reads an unread slot, is dropped and the entry's own run stands: this
  keeps the gaps of buildings `potential` and the decision blocks. A retry replaces the run only
  when each context that the run read comes back at the same evaluation with its read slots
  unchanged and its unread ones read, so a bound cannot hide an arrival that the retry missed.

Before the accepted limit, `registry-sweep --diff` against the baseline names 7 registries: five
of the six above (button effects keeps its gap counts), and districts and specimens, whose `derived_names` each lose one path gap (an
`instruction` stop at `csetm`, a `branch-value` stop at a switch table) with the same names. With
the limit, it names 49: 46 move gaps to the limit, and 10 change their scope context counts (3 from
the repairs). No command grammar changes (`command-population --diff`), and the field sweep changes
only these entry contexts.

Parity changes the three megastructure blocks and two callback answers. On_action
`on_entering_war` gains a country with a country from: `CWar::SetupParticipant` builds a scope at
`sp + 0x200`, links the one at `sp + 0x90` as its from, sets a country in each and fires the
on_action, and its two switch tables (`0x100d29a68`, `0x100d29b2c`) no longer stop the run; its
path-limit gap stays. The five `can_fill_*_job` game rules lose their `branch-value` gap; their path
and loop bounds stay.

**Credit.** 5 of the 56 claims are now creditable: councilor `possible`, megastructure `potential`,
`possible` and `context_menu_potential`, and observation mission `potential`. Each field has a
known read scope, established contexts and no blocking gap.

**Before and after.** Over the M452 recordings, before at `main` `77847bb` (the tracked baseline):

| Stop | Before | After |
| --- | ---: | ---: |
| no attributed evaluation | 79 | 79 |
| path-limit | 54 | 56 |
| some entry scopes unresolved | 46 | 44 |
| loop-limit | 21 | 21 |
| branch-value | 16 | 2 |
| N unnamed direct evaluations (registry) | 10 | 10 |
| no-caller | 8 | 8 |
| caller-depth | 5 | 5 |
| instruction | 5 | 4 |
| selected-block-identity | 4 | 4 |
| only tooltip calls | 1 | 1 |
| **Total (registries)** | **249 (69)** | **234 (69)** |

The SDK-553 run had 242 in 66 registries; SDK-710, SDK-676 and SDK-813 brought more blocks into
the method before this work. Of the 234, an accepted limit covers 138 (below) and 96 stay open: 95
on 61 fields with a field-level `replace_scopes` claim, and the planet classes' unnamed-evaluation
gap, a registry that the limit does not list. Scope context goes from 1,309 to 1,323 complete
fields: five from the repairs, the rest because the accepted gaps leave the counts.

**Accepted limit.** The entry-context gaps of the 94 fields with no field-level `replace_scopes`
claim (129 gaps at SDK-812; SDK-830 added ethic `playable`), and the 9 registry gaps of direct evaluations whose block the method cannot name,
credit no Atlas claim: `ENTRY_CONTEXTS_WITHOUT_CLAIM` in `examples/registry-sweep.rs` lists the
fields. Reopen a field's gaps when the config gives it a claim, or when Atlas maps the type-level
`replace_scopes` claims onto blocks.

**Open, with a claim.** For the 51 claims that remain, by stop:

| Stop | Disposition | Checked members |
| --- | --- | --- |
| Path and loop limits | A larger bound reaches no new evaluation: 1,024 paths take over 20 minutes for one registry and remove one gap. The runs fork on script-profiler and string-length branches. [Path joins](#path-joins-on-m452-sdk-823) (SDK-823) merge those paths. | astral action `potential`, casus belli `potential`, megastructure `tooltip_system_score` (`BuildBestSystemsText` reaches no evaluation) |
| caller-depth | One more caller turns casus belli `is_valid` into path and loop bounds and an unreadable context (measured at depth 3). | casus belli `is_valid`; astral action `potential` and `is_exhausted` (`GetTooltip`) |
| No attributed evaluation | [SDK-824](#arrays-and-owner-getters-on-m452-sdk-824) resolves 8 of these claims and [SDK-830](#member-and-database-arrays-on-m452-sdk-830) 3 more; that section names the obstacle of each claim that is left. | tradable action `on_traded_effect`, agreement term value `activate_effect`, terraform `ai_weight` |
| instruction | The astral `IsAllowed` run from `CAlertManager::Update` stops in floating-point alert arithmetic (`fminnm`, `fmaxnm`, `fcmp`, `fadd`, `fcsel` in turn), and the same run reaches its loop bound, so more instructions change no answer. | astral action `allow`, `potential`, `is_exhausted` |
| selected-block-identity | The indexed swap load in `CTraditionType::OnEnabled` stays unknown, and the run also reaches the path bound. | tradition and ascension perk `on_enabled` (one function) |
| Some entry scopes unresolved | Missions: the operator's eleven spatial types reach the path bound. The others pass their scope through a call that the pass does not follow and also reach a bound. | mission `on_start`, terraform `on_queued`, anomaly `on_spawn` |

### Arrays and owner getters on M452 (SDK-824)

**Atlas need.** After SDK-812, 30 fields with a field-level `replace_scopes` claim kept the gap "no
evaluation that the method attributes evaluates this block" (one claim each; Atlas ledger at config
`8574760`). SDK-824 took the 22 whose item comes from an array or an owner getter, or that had no
evaluator call that the method found. Each has a known read scope and no other blocking gap.

**Repairs.** Four rules, each with authored tests:

- `CScriptableValue::GetValue` and `GetValueNoScopeCopy` are weight evaluators
  ([weight blocks](weight-blocks.md#entry-contexts)). The "no direct evaluator call" fields were
  `CScriptableValue` blocks: `CEthic::CalcCountryAttraction(CCountry const*, int) const` evaluates
  `this + 0x4d8` on a country scope, `CEthic::CalcPopAttraction` evaluates `this + 0x4a0` on a pop
  group with a colony `from` (`Set(1 << 40, …)`), and `CGovernmentType::CalcWeight(CEventScope&)
  const` evaluates `this + 0x1a0` with the country scope of `CGovernmentTypeDatabase::GenerateType`.
- An element of a typed array parameter is an owner's item. A `CPdxArray<O const*, int>` holds its
  vtable at `+0`, its buffer pointer at `+8`, its capacity at `+0x10` and its size at `+0x14`
  (`TradeActions`, `CAgreementManager::CreateAgreement`, `CScriptableValue::GetValue`). The name
  pass gives the buffer word of such a parameter the fact `Elements`; a move by a multiple of 8
  keeps it, and a 64-bit load through it is an element. `(anonymous namespace)` is a namespace, so
  `TradeActions` and `EndActions(CPdxArray<CTradableAction const*, int> const&, CCountry const&,
  CCountry const&)` take the array in `x0`. Each builds a country scope at `sp + 0x170`, links the
  other country's scope as its `from` (`str x21,[sp,#0x1a8]` in `TradeActions`), and calls slot
  `+0x48` of each element plus `0x268` (`on_traded_effect`), `0x310` or `0x3b8`.
- An owner getter returns the owner's offset `k` whatever `x0` holds, and a function that calls one
  is a candidate. `CreateAgreement` builds an agreement scope (`Set(0x10000000, …)`), calls
  `CAgreementTermValue::GetActivateEffect() const` (`add x0,x0,#0x978; ret`) on each element of
  `GetDiscreteTermArray()` and of `GetResourceTermValueArray()`, and calls slot `+0x48` of the
  result. `DestroyAgreement` does the same with `GetDeactivateEffect`.
- An attributed call through a vtable slot is an evaluation, so the run treats it as a scope
  reader. Before, the first loop's virtual call in `CreateAgreement` (and in `DestroyAgreement`)
  made the scope unknown for the second loop's call, which gave each term value block an unreadable
  context beside the agreement. Archaeological site `on_visible` loses its unreadable context and
  its gaps the same way; its country with an archaeological site `from` agrees with the config. A
  helper's attributed evaluations are scope readers too: mission `on_stop`, which `CMission::Stop`
  runs, gains a `this` of no type with a country `prev` in place of an unreadable context, and
  keeps its gaps.

**Before and after.** Over the M452 recordings, before at `main` `64bd448` (the tracked baseline):

| Stop | Before | After |
| --- | ---: | ---: |
| no attributed evaluation | 81 | 47 |
| path-limit | 56 | 61 |
| some entry scopes unresolved | 44 | 48 |
| loop-limit | 21 | 24 |
| N unnamed direct evaluations (registry) | 10 | 12 |
| no-caller | 8 | 10 |
| caller-depth | 5 | 6 |
| instruction | 4 | 5 |
| selected-block-identity | 4 | 4 |
| branch-value | 2 | 2 |
| only tooltip calls | 1 | 1 |
| **Total (registries)** | **236 (69)** | **220 (62)** |

Open gaps go from 98 to 89. In the field sweep, 31 blocks gain contexts and no block loses one.
Scope context goes from 1,422 to 1,448 complete fields. Parity and the command population do not change:
the callback method passes no getters, buffers or virtual evaluations. `opinion_modifiers` and
`federation_laws` gain one unnamed direct evaluation each, a registry gap that credits no claim, so
the SDK-812 accepted limit lists them: `CFederationLaw::IsAllowed` evaluates `x22 + 0x1f0` and
`COpinionModifier::CalcGrowthRate` evaluates `x19`, registers that the name pass does not lead to
the receiver. A registry gap can hide a field, so federation laws go from complete to partial
fields (104 complete registries, from 105).

**Credit.** 8 of the 22 claims now have established contexts and no entry gap: tradable action
`on_traded_effect`, `on_deal_ended_sender_effect` and `on_deal_ended_recipient_effect` (a country
with a country `from`, as the config states), agreement term value `activate_effect` and
`deactivate_effect` (an agreement), ethic `country_attraction` (a country), ethic `pop_attraction`
(a pop group with a colony `from`; the config states a planet `from`, read as a source error as for
the other colony scopes) and government `weight` (a country). Outside the 22, leader class
`ai_location_weight`, psionic aura `daily_decrease` and archaeological site `on_visible` resolve.

**Open, with a claim.** The other 14 claims, by obstacle:

| Obstacle | Claims | Reopen path |
| --- | --- | --- |
| A bound or instruction on the run | opinion modifier `opinion` (a country with a country `from`, beside the `CCountry::CalcOurOpinionOfOther` run, which stops at the path limit), councilor `ai_hiring_weight` (the `CCachedPositionWeights` lambda stops at `fdiv` and at the loop limit) | SDK-823 |
| A member array whose element type no signature names | tradable action `ai_weight` (`CTradeDealSide + 0xa0`), specialist subject perk `activate_effect` and `deactivate_effect` (`CSpecialistSubjectLevel + 0x8`, a class with no constructor symbol), menace perk `on_unlock` (`CCrisisLevelType + 0x48`, through a spill of the level) | [SDK-830](#member-and-database-arrays-on-m452-sdk-830) |
| A local, returned, map or database array | terraform `ai_weight` (an sret array of `CTerraformDatabase::GetAllAvailableLinks`), technology category `weight_modifier` (`CTechnology::GetCategoriesFor`), astral rift `event_weight` (a map value at entry `+0x10`), archaeological site `weight` (`TGameDatabase<CArchaeologicalSiteTypeDatabase>::_pInstance` `+0x48`), agreement preset `overlord_weight` and `subject_weight` (a filtered copy of the preset database, and `CAgreementTermData::GetPresetData()`) | [SDK-830](#member-and-database-arrays-on-m452-sdk-830) |
| A member word that no type pointer leads to | technology tier `weight_modifier` (`CTechnology + 0x600`), galactic focus `effect` (`CGalacticCommunity + 0x2c0` in `DailyUpdate`) | [SDK-830](#member-and-database-arrays-on-m452-sdk-830) |

Checked members: `CAIInteriorMinister::Terraform` loads each `CTerraformLink const*` from a stack
array that `GetAllAvailableLinks` fills and calls `GetRawFactor` on `x23 + 0x350`;
`CAstralRiftManager::PickRandomAstralRiftTypeForCountry` loads the element from the array value of
a `CPdxUnorderedMap` entry and evaluates `+0x50`; `CArchaeologicalSiteManager::CreateRandomSite`
loads each type from the database's item array and evaluates `+0x58`. No signature names these
arrays' element types. Psionic aura `monthly_decrease` and `monthly_increase` gain an unreadable
context at the path limit and trait `cost` a country beside a `no-caller` gap; they are outside this
group.

### Path joins on M452 (SDK-823)

**Atlas need.** After SDK-812 and SDK-824, 30 fields with a field-level `replace_scopes` claim and
a known read scope kept an entry-context stop at a path, loop or caller bound, an instruction, a
selected-block identity or an unresolved entry scope (Atlas ledger at config `8574760`): astral
action `potential`, `allow`, `allow_unlock` and `is_exhausted`; casus belli `potential`, `is_valid`
and `destroy_if`; building and decision `show_tech_unlock_if`; event chain `abort_trigger`;
federation law `on_enact`; megastructure `tooltip_system_score` and `tooltip_system_filter`; the
eight mission blocks; observation mission `valid`; anomaly `on_spawn`; terraform `on_queued`;
tradition and ascension perk `on_enabled`; opinion modifier `opinion`; councilor
`ai_hiring_weight`; and psionic aura `monthly_decrease` and `monthly_increase`.

**Repair.** A run of several paths (`run_paths`, `run_paths_to`) drops a path that arrives at a
join point, an instruction with more than one direct predecessor, in the state of an earlier path
of the run (`evaluate/joins.rs`). The state leaves out the general registers and flags that no
path from there reads before writing them, by a backward liveness over the decoded code that
follows the calling convention (a call reads `x0` to `x8` and the callee-saved registers; a return
gives `x0` to `x7` and the callee-saved registers), and the stack below the stack pointer. A path
that has come round each loop at least as often as the earlier path joins it too, so a loop pass
that changes nothing reaches no loop bound. Profiler and string-length tests (`_g_ScriptProfiler`,
the long-string bit of a `CString`) split the paths, and the paths meet again after each test with
the same live state. Rules, each with authored tests:

- Steps are not compared: when the earlier path stops at the step limit, the run reports that
  bound.
- A run that joins at loop heads (`run_paths_joining`) does not join paths this way: its paths go
  on from facts that later arrivals make fewer, so two paths in one state at different times need
  not run the same way. Joining them lost the silent `{key}_delayed` of traditions.
- A direct call to code that the run may enter reads what that code reads: a `.cold` clone reads
  its caller's scratch registers.
- A run that tracks an owner, watches receiver reads, records decisions or intercepts tail calls
  keeps every path.
- When every path of a run joins and none ends, the run gives one path stopped at `loop-limit`.
- Lane-wise vector `add` and `sub` run (`add v0.2d,v4.2d,v0.2d`): a joined run of
  `CMegaStructureType::BuildBestSystemsText` reaches one.

**Before and after.** Over the M452 recordings, before at `main` `f06c8a3`, the field sweep's
entry-context gaps:

| Stop | Before | After |
| --- | ---: | ---: |
| path-limit | 67 | 36 |
| loop-limit | 27 | 14 |
| some entry scopes unresolved | 51 | 55 |
| instruction | 6 | 7 |

The other stops do not change. 12 blocks lose every entry gap and 5 gain contexts; `scope_context`
goes from 1,473 to 1,479 complete fields. Most runs that stopped at a bound now reach their
evaluation, and where the scope there is not readable the gap becomes the unresolved scope that
was behind the bound.

Run time, measured before SDK-830 at `a82d1df` in two paired runs on one machine with other work
beside them: `registry-sweep --record`
146.2 s and 154.7 s before, 157.4 s and 152.7 s after; `registry-field-sweep` 67.9 s and 66.2 s
before, 71.0 s and 70.7 s after. The block facts take 19 s before and 24 s after
(`inspect --entry-contexts common/astral_actions`): the runs that stopped at the path bound now go
on.

Parity changes, each from paths that the bound cut: megastructure `tooltip_system_score` and
`tooltip_system_filter` gain a context and `on_build_complete` three more; 15 on_actions gain
contexts and the on_action gaps go from 241 to 133 ([engine commands](engine-commands.md#on_actions-game-rules-and-their-entry-scopes));
game rule gaps go from 32 to 25; the 80 table-search defines reach their reader
([defines](engine-commands.md#defines)); traditions and ascension perks regain a second
`{tradition_swap/custom_tooltip_with_modifiers}` entry ([derived names](derived-names.md)). The
command population does not change.

**Credit.** 4 of the 30 claims lose every entry gap: federation law `on_enact` (a federation),
astral action `allow_unlock` (a country), megastructure `tooltip_system_filter` (a system with a
country `from`, as the config states) and `tooltip_system_score` (a system; the engine also links
the country as `from`, which the config does not state).

**Open, with a claim.** The other 26, by what the runs now stop at:

| Obstacle | Claims |
| --- | --- |
| The scope comes from a caller that the run cannot read: the evaluation is reached, with an unresolved context beside any known one | anomaly `on_spawn` (`CPlanet::OnSurvey`), casus belli `potential` and `destroy_if` (`UpdateCasusBelliOnlyChangeSafePrivateWithTypeAgainst`), event chain `abort_trigger`, observation mission `valid` (`CObservationMissionFleetOrderButton::IsValid`), opinion modifier `opinion` (`CalcOurOpinionOfOther`), psionic aura `monthly_decrease` and `monthly_increase`, astral action `potential` (`GetTooltip`) |
| Paths that differ in what later code reads: string temporaries, scope copies, loop iterators | the mission blocks (`CMission::Stop`, after `FillEventScope` gives each of eleven spatial types its own scope), terraform `on_queued` (`AddSpentResourcesToScope`), building and decision `show_tech_unlock_if` (`TechnologyViewAddUnlocks`, whose callee-saved iterators differ) |
| An instruction and a loop bound | astral action `allow` and `is_exhausted` (`CAlertManager::Update` stops at `fminnm` and the loop bound), councilor `ai_hiring_weight` (`fdiv` in the `CCachedPositionWeights` lambda) |
| A caller bound | casus belli `is_valid` |
| The selected block | tradition and ascension perk `on_enabled` (`CTraditionType::OnEnabled`) |

Reopen a row when its obstacle is repaired, not with a larger bound.

### Member and database arrays on M452 (SDK-830)

**Atlas need.** After SDK-824, 12 fields with a field-level `replace_scopes` claim kept the gap "no
evaluation that the method attributes evaluates this block" (one claim each; config `8574760`).
Recounted at Native `main` `a82d1df`: all 12 still had the gap and no other blocking gap.

**Repairs.** Three typing rules and one contradiction, each with authored tests
([the method](#block-entry-contexts)):

- A constructor's array vtable types a member array: `CTradeDealSide()` stores the
  `CPdxArray<CTradableAction const*, int>` vtable at `+0xa0`, so `CTradeDealSide::CalcValueFor`'s
  loop over `+0xa8` loads tradable actions and evaluates `+0x460` (`ai_weight`) and `+0x498`.
- A null object types a member word: `CTechnology::ReadMember` stores
  `TPdxNullObject<CTechnologyTier>::_pInstance` at `+0x600`, which
  `CTechnologyStatus::GetTechWeight` loads to evaluate the tier's `+0x40`; `CGalacticCommunity::
  DailyUpdate` stores `TPdxNullObject<CGalacticFocus>::_pInstance` at `+0x2c0` and executes the
  focus's `+0xb8` (`effect`). The reader keeps every fact of a joined value, so `ReadMember`'s
  other store, an element of the tier database's item array, agrees.
- A database instance is an object of its database class: `CArchaeologicalSiteManager::
  CreateRandomSite` loads `TGameDatabase<CArchaeologicalSiteTypeDatabase>::_pInstance`, whose
  constructor stores the `CPdxArray<CArchaeologicalSiteType const*, int>` vtable at `+0x40`, and
  evaluates `+0x58` (`weight`) of the element at a register index (`ldr x26,[x8,x23,lsl#3]`).
- A bound inside a function, other than a wrapper, that receives the scope contradicts a run that
  reaches a scope with no `this`. `CreateRandomSite` passes its scope to
  `CSpatialObjectRefCaster::FillEventScope`, which switches on the spatial object's type through a
  jump table. The table's fork exceeds the path limit, so the only followed path is the default
  for a type above 10, which sets no scope; without the rule, archaeological site `weight` read
  `this` not set beside the config's planet.

`Field(g, k)` in the name pass is now only a load, the word at `[g] + k`; a move of it by an
amount is unknown, so a database instance is the word that its slot's address holds.

**Before and after.** Over the M452 recordings, before at `main` `a82d1df` (the tracked baseline),
counted as field gaps:

| Stop | Before | After |
| --- | ---: | ---: |
| path-limit | 62 | 67 |
| some entry scopes unresolved | 49 | 51 |
| no attributed evaluation | 47 | 35 |
| loop-limit | 24 | 27 |
| unnamed direct evaluations (registry) | 12 | 12 |
| no-caller | 10 | 11 |
| caller-depth | 8 | 9 |
| instruction | 5 | 6 |
| selected-block-identity | 4 | 4 |
| branch-value | 2 | 2 |
| only tooltip calls | 1 | 2 |

Scope context goes from 1,465 to 1,473 complete fields. The command population does not change;
parity changes in megastructure `outliner_trigger` alone (below). Nineteen fields change; each is
listed here:

- Gain a context and lose their gap: tradable action `ai_weight` (a country with a country `from`,
  as the config states), technology tier `weight_modifier` and galactic focus `effect` (a country,
  as the config states), first contact `on_roll_failed` (a first contact), trait
  `trade_acceptance_weight` (a leader, as the config states), and pop faction type `on_create` and
  `valid` (a pop faction; a `CPopFaction` constructor stores the type's null object).
- Gain a context, no gap change: greeting overlay sound `possible` and `weight` (a country), and
  megastructure `outliner_trigger`, which `CMegaStructureTypeDatabase::PostReadInit` evaluates on a
  fresh scope with no `this` to check the outliner icons; the config states a megastructure, which
  the other runs read.
- A narrower gap: archaeological site `weight` and tradable action `ai_acceptance` (path and loop
  limits), deposit `drop_weight` (the bounds and an unreadable context), leader class `ai_weight`
  (a country beside an `instruction` stop), system tooltip `custom_tooltip` (only tooltip calls).
- A newly attributed evaluation with a gap: army `allow` (`CBuildableArmy::CanBuild` reaches it
  with a colony and an unreadable scope at the path limit), building `potential`
  (`CZoneType::CollectPotentialBuildingTypes` walks the zone's building array with a scope
  parameter and has no direct caller) and dynamic text `available` (`NDynamicText::
  BuildDynamicText` is past the caller depth).
- The contradiction rule: ethic `playable` (`NGameUtil::SetupFakeCountryScope` is cut at the path
  limit beside a scope with no `this`), which has no claim, so the SDK-812 accepted limit lists it.

**Credit.** 3 of the 12 claims now have established contexts that agree with the config:
tradable action `ai_weight`, technology tier `weight_modifier` and galactic focus `effect`.
Outside the 12, trait `trade_acceptance_weight` resolves. Army `allow` has a claim and loses its
complete status; its earlier context was a colony, beside the config's country.

**Open, with a claim.** The other 9 claims, by obstacle:

| Obstacle | Claims | Reopen path |
| --- | --- | --- |
| A bound on the run | archaeological site `weight` (the `FillEventScope` jump table above) | SDK-823 |
| A level class with no constructor | specialist subject perk `activate_effect` and `deactivate_effect`: `CSpecialistSubjectLevel::OnLevelUp` and `DeactivatePerks` walk `this + 0x10`, but only the `CPdxArray<CSpecialistSubjectLevel, int>` methods (`InsertAt`, `GrowCapacity`) store the perk array's vtable, at element `+8` | type an element class by the array vtable that its array's methods store at element `+k` |
| A spilled level | menace perk `on_unlock`: `CCrisisProgression::UnlockLevel` spills the level with `stur x1,[x29,#-0x60]`, passes that address to `InsertAtEmplace`, and reloads it with `ldur`. The name pass follows neither `stur` nor `ldur`, and a call that receives a slot's address forgets the slot. `CCrisisLevelType + 0x48` is typed. | follow `stur` and `ldur`, and keep a slot that a call receives as `T const&` |
| An array returned through `x8` | terraform `ai_weight`: `CAIInteriorMinister::Terraform` walks the array that `CTerraformDatabase::GetAllAvailableLinks` builds at `x29 - 0x78`; the callee stores the vtable | type an `x8` result by the array vtable that its callee stores at `[x8]` |
| A returned reference | technology category `weight_modifier`: `CTechnology::GetCategoriesFor` returns `this + 0x4f8` or `GetTechnologySwap(…) + 0x2f0`, not an offset getter | a getter that returns one of two typed arrays |
| A map value | astral rift `event_weight`: `PickRandomAstralRiftTypeForCountry` loads the array value of a `CPdxUnorderedMap` entry | type a map's value array by the map's class |
| A sort comparator | agreement preset `overlord_weight` and `subject_weight`: the weights are evaluated in the comparator inlined into `std::__stable_sort<…CalcBestAIAgreementProposal() const::$_2&, CAgreementPreset const**>`, with a captured scope | elements of a `O const**` parameter, and a scope captured by a lambda |

