# Reader kinds on M45

`registry_fields` reports one opaque identity for fields that share a joined reader and one
conservative broad value kind. The kinds do not promise a complete grammar, valid ranges,
occurrence rules, reference ownership, nested behavior, or runtime behavior. A known identity can
therefore still have kind `Unknown`. A missing identity means that all paths did not establish one
shared reader.

`Reader.family` separately identifies trigger, effect, modifier, unknown, or not-applicable
children. Conditional read alternatives retain their own families; conflicting or unresolved
alternatives cannot establish an unconditional family. Constructor joins refine generic persistent
reader identities. See [nested command grammar](command-grammar.md) for the current v6 counts,
all block-field families in the three samples, parser checks, and population measurements.

Run the report against an installation, application bundle, or executable:

```sh
cargo run --example reader-kinds -- '/path/to/Stellaris'
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

The agenda cost uses `CVariableValue` and intentionally remains unknown: its scoped-expression
grammar is not a plain integer or fixed-point read. Unknown and missing readers have field-specific
typed gaps. Every answer remains partial because nested grammar and runtime semantics are outside
this bounded method.
