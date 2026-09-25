# Static analysis methods

Each static method records its engine facts, current result, gaps and pitfalls on the page for
its subject. Add a page when a method starts a new subject. The code and its module comments
describe the methods; these pages hold what the code cannot.

| Page | Subjects |
| --- | --- |
| [Registry fields](registry-fields.md) | The current field sweep and its stops; compiler jump tables and bit fields in root readers; members and shared readers; registry scheduling and owner joins |
| [Engine commands](engine-commands.md) | Effect and trigger declarations and their scopes; target getters; modifier, category, scope and link declarations; localization contexts; on_actions and game rules |
| [Modifier families](modifier-families.md) | Modifier families from database generators, post-read code and shared helpers; the per-item post-read call; item keys; the loaded modifier table |

This page also holds the define read helpers. The retired SDK-482 reference seam is on
[reference method retirement](reference-method-retirement.md).

## Define read helpers

On M45-release, `Native::defines()` (`defines/v1`) finds 2,385 compiled `NDefines` and
`NUncheckedDefines` `ReadDefine` helpers, and follows 2,305 of them to a literal namespace, a
literal name and a typed engine reader, in about four seconds. Resolved types are 1,091
fixed-point, 672 integer, 326 string, 172 float, 23 list, 13 vector and 8 boolean. The other 80
named helpers use a table-search loop that exceeds the path search; they are `UnresolvedReader`
gaps, including `NGraphics.ORBIT_HSV`. There are no failed or unnamed helpers.

The method classifies the target of each direct `GetValue`, `GetArrayValue` or
`ReadDefinesValue` call. `GetValue` takes namespace and name arguments; `GetArrayValue` reads a
named value from a namespace table. Shipped define entries, defaults, comments, bounds and uses
are not established here. SDK-610 owns the broader extraction question, and Atlas owns the
comparison with shipped content and config.
