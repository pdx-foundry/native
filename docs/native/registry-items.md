# Registry items on M45

`Game::registry_items(name)` reads a selected registry's collection when its initial loader
returns. `Complete` means that every slot, key, owner, thread, sequence and terminal witness
agrees at that boundary. It says nothing about later validation or gameplay. A loader that has
not returned before the pause gives `Unsupported`, even when it was the only selected registry.
The session pauses when every selected loader with an active hook has returned, or at the
worker's deadline (the startup budget less a margin) when a loader is not reached in time; the
`Unsupported` reason says which (SDK-573).
A registry whose key layout cannot be read
also gives `Unsupported`; it never gives an empty complete answer for that failure.

Select content directories from `Native::registries()` with `GameOptions::registries` before
`start_game`. The default M45 selection is the two tradition registries. The selection bounds
the worker's four MiB trace and the private content copy. A recorded-answer game reads the same
names from `registry_items/<name>.json` without starting a process.

Run the complete report on the exact M45 installation:

```sh
cargo run --release --example registry-items-report -- '/path/to/Stellaris' --batch 16
```

`--limit M` limits attempted names; names after the options select a subset. Every discovered
registry still gets a row, with `not attempted` for names outside the selection. Set
`RECORD_ANSWERS_TO` to record each live answer. The reporter closes each session and prints a
batch startup error once if that batch cannot run.

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
`common/map_modes` have keys at `+0x18`. The live case `nonstandard_key` requires the eight
`common/map_modes` keys, which equal its top-level source keys, about 22 seconds after launch. The
live case `generator_registries` requires the six generator registries (`common/buildings`,
`common/bypass`, `common/districts`, `common/megastructures`, `common/situations`,
`common/zones`) to return 498, 10, 147, 164, 90 and 146 complete items in one session. Their
loaders run on the launch thread, and the session pauses after registry initialization about 24
seconds after launch. The worker also checks key uniqueness, nonempty keys and control characters.
SDK-551 covers custom, nested-definition and late loaders outside this template method.

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
