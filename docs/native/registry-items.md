# Registry items on M45

`Game::registry_items(name)` reads a selected registry's collection when its initial loader
returns. `Complete` means that every slot, key, owner, thread, sequence and terminal witness
agrees at that boundary. It says nothing about later validation or gameplay. A loader that has
not returned before the pause gives `Unsupported`, even when it was the only selected registry.
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
| `common/game_scenarios` | Unsupported: initial loader did not run before the pause |

These are historical M45-observe results. On M45-release, SDK-567 derives the key offset from
each selected registry's item constructor before the worker reads keys. The method established
148 of 164 named registries; 16 with unresolved key storage refuse item reads. Both
`common/bypass` and `common/map_modes` have keys at `+0x18`. On M45-release, one selected
`common/map_modes` session returned eight complete live keys equal to its top-level source keys;
another paused before its loader ran and returned `Unsupported`. The worker also checks key
uniqueness, nonempty keys and control characters. SDK-551 covers custom, nested-definition and
late loaders outside this template method.
