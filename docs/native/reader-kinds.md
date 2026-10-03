# Reader kinds on M45

`registry_fields` reports one opaque identity for fields that share a joined reader and one
conservative broad value kind. The kinds do not promise a complete grammar, valid ranges,
occurrence rules, reference ownership, nested behavior, or runtime behavior. A known identity can
therefore still have kind `Unknown`. A missing identity means that all paths did not establish one
shared reader.

`Reader.family` separately identifies trigger, effect, modifier, unknown, or not-applicable
children. Conditional read alternatives retain their own families; conflicting or unresolved
alternatives cannot establish an unconditional family. Constructor joins refine generic persistent
reader identities. See [nested command grammar](command-grammar.md) for the counts,
all block-field families in the three samples, parser checks, and population measurements.

Pitfalls:

- **Identity does not establish value kind.** A command reader proves the constructor-installed
  virtual `Read` and `ReadMember` targets. It does not prove what the `Read` override accepts, so
  marking every such receiver `Block` adds an unsupported fact. On M45-release, 137 triggers and
  463 effects had an established block kind at `command-grammar/v9`; the others kept `Unknown`.
- **An outer reader's family is not the set of its child families.** `random_list` is an effect
  reader whose outer numeric keys lead to a separate effect-child grammar.
- **The family checks answer different questions.** Shared reader-entry classification gives the
  broad value of established helper signatures; constructor joins refine a generic persistent
  destination; conditional normalization also accounts for unresolved and rejected paths. Do not
  merge them: removing the last check promotes conditional facts.

## Reader identities

A reader ID is the first 16 hexadecimal digits of the SHA-256 of the demangled callee name
(`ReaderId::from_callee`). So the same callee gives the same ID on every build: the
`registry-fields/v2` sweep on M45-observe and the `registry-fields/v3` run on M45-release found the
same 19 IDs. The hashed name of each `NParserUtil` template keeps its `void ` return type.

Run the report against an installation, application bundle, or executable:

```sh
cargo run --example reader-kinds -- "$STELLARIS_PATH"
```

The default report covers `common/traditions`, `common/tradition_categories`, and
`common/council_agendas`. Registry names after the installation argument replace this default.
Counts are fields, not paths or unique readers, so each row sums to its field total.

The original reader-kind baseline on the exact M45 executable in [targets](targets.md) was:

| Registry | Boolean | Integer | Fixed-point | String | Reference | Block | Unknown | Total | Missing ID |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `common/traditions` | 0 | 0 | 0 | 3 | 0 | 6 | 2 | 11 | 2 |
| `common/tradition_categories` | 0 | 0 | 0 | 1 | 0 | 2 | 4 | 7 | 4 |
| `common/council_agendas` | 0 | 2 | 0 | 0 | 1 | 6 | 1 | 10 | 0 |

This table is the baseline before SDK-645. `agenda_cost` now reports `ScopedNumeric` with an
integer literal representation and partial operand forms. The reader remains shared across
integer and fixed-point destinations; the constructor vtable point selects concrete storage.
See [scoped numeric](scoped-numeric.md). Other unknown and missing readers retain field-specific
typed gaps.

## Modifier blocks

Constructor-bound root fields with `family: Modifier` carry `FieldMembers::ModifierBlock`: named
keys and entry forms, each with explicit partial or unresolved results. The block itself has
`numeric: Known(None)`. Its fixed-key readers and numeric entry use the same conversion pass as
ordinary fields. String readers make no localisation claim. See [modifier blocks](modifier-blocks.md)
for the four variants, their shared identities and remaining gaps.

## Numeric properties

SDK-644 adds a separate `Reader.numeric` property and the broad `Float` kind. The identity and
broad kind still do not promise a complete grammar. Conversion properties establish storage,
scale, partial literal forms and explicit reader clamps independently. Missing accepted ranges,
overflow behavior and unsupported shapes remain gaps. See [numeric conversion](numeric-conversion.md)
for the exact-build controls, live comparisons and population limits.
