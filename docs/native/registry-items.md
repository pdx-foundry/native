# Registry items on M45

The public item query and registry selection were retired by the 2026-10-02
[simplification review](../design/simplification.md), which supersedes the Milestone 2
instruction to retain them. Native selects session registries and includes the fixture registry.
`LoadedModifiers.registry_items` retains the keys needed for the modifier explanation join.
The live `loaded_modifier_key_layouts` control covers the six generator registries, including
the `+0x18` bypass layout, with independent source-key comparison. The former `map_modes`
control and the full initial-loader report remain in Git at `d8f9d8a`.

`internals::check_registry_load` retains one bounded, live-only registry observation for
SDK-552 loader-rule controls. It owns launch and close, accepts an optional fixture in that same
registry, and returns no persistent game. It reads the registry's collection when its initial
loader returns. `Complete` means that every slot, key, owner, thread, sequence and terminal
witness agrees at that boundary. It says nothing about later validation or gameplay. A registry
whose key layout cannot be read gives `Unsupported`; it never gives an empty complete answer for
that failure. The public item query, `GameOptions::registries` and the `registry-items-report`
example that produced the measured result below are in Git at `d8f9d8a`.

## Measured result

On 2026-09-21, the full 164-registry report took 457 seconds on the exact M45-observe
installation. 161 registries returned `Complete`. A follow-up run established precise
`Unsupported` results for the other three. Selected controls:

| Registry | Result |
| --- | --- |
| `common/traditions` | Complete, 234 items |
| `common/tradition_categories` | Complete, 33 items |
| `common/ascension_perks` | Complete, 49 items |
| `common/ethics` | Complete, 17 items |
| `common/edicts` | Complete, 171 items |
| `common/governments/civics` | Complete, 358 items |
| `map/galaxy` | Complete, 10 items |
| `common/bypass` | Unsupported: item key layout is not established |
| `common/map_modes` | Unsupported: item key layout is not established |
| `common/game_scenarios` | Unsupported: initial loader did not run before the startup deadline stopped the game |

These are M45-observe results. On M45-release, the worker reads keys at the offset that the item
constructor establishes before the session starts (see [modifier
families](modifier-families.md#item-keys)): 156 of 164 named registries. The other eight refuse an
item read with a reason; no key is read from an unestablished offset. `common/bypass` and
`common/map_modes` have keys at `+0x18`. The former `map_modes` control found its eight keys equal
to its top-level source keys about 22 seconds after launch. The live case
`loaded_modifier_key_layouts` requires the six generator registries (`common/buildings`,
`common/bypass`, `common/districts`, `common/megastructures`, `common/situations`,
`common/zones`) to give 498, 10, 147, 164, 90 and 146 keys in one loaded-modifier session, and
compares the `common/bypass` keys with its source keys. Their loaders run on the launch thread;
a registry session paused after their registry initialization about 24 seconds after launch. The
worker also checks key uniqueness, nonempty keys and control characters. SDK-551 covers custom,
nested-definition and late loaders outside this template method.

**Pause cause.** A registry session pauses when every registry with an active hook has returned
from its initial loader, or at the worker's deadline (170 seconds of the default 180-second
startup budget). Only the deadline pauses with no loader returned. A registry whose loader the
game did not reach is then `NotLoaded`. The worker's pause record carries its cause
(`loaders-returned`, `content-loaded` or `deadline`), the reducer refuses a loaders-returned pause
that omits an active loader, and a `NotLoaded` answer names the cause. One M45-release session
paused at the deadline before any of the six generator registries loaded; why it did not reach
them is not known. `close` keeps the work directory after a read error or a failed start, but
that session's answers were `Unsupported` and its disposal was clean, so its work directory was
removed.
