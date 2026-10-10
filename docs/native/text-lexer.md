# Text lexer token boundary

The text lexer cuts script into the tokens that every reader receives. `Reader.numeric`
(`registry-fields/v27`, `command-grammar/v17`) uses this boundary: `engine/analysis/numeric/lexer.rs`
matches `CTextLexer::GetTok` whole, with its three character tables byte for byte. When the match
holds, no numeric reader keeps `numeric-lexical-boundary`. A changed body gives
`numeric-lexer-shape` and a changed table gives `numeric-lexer-table`. The facts below are for
M452 (`c621723d…`).

## Engine facts (M452)

### Input

- `CTextLexer::GetTok` (`0x1025b0fe0`) reads the object at lexer `+8`. Both
  `CTextLexer(CFile*, bool)` bodies (`0x1025b0e68`, `0x1025b0f24`) and `CLexer(CFile*, bool)`
  store their `CFile*` argument there.
- It calls `CFile` virtual slot `0x10` (`Get`) for each byte and slot `0x58` (`IsValid`) after
  each call. It calls `CFile::UnGet` (`0x102505c50`) to push back one byte: `UnGet` sets the
  flag at `+0x11`, and the next `Get` returns the byte at `+0x10` again.
- Whitespace: a byte up to 0x7f is space when its `__DefaultRuneLocale` runetype has `0x4000`
  (`_CTYPE_S`). A higher byte is sign-extended and passed to `___maskrune(…, 0x4000)`.
  `tests/numeric_scanner.rs` observes the space bytes 9–13 and 32 in locale C on the recorded
  `libsystem_c` image. The executable imports no `setlocale`. This is a platform-library fact
  with the same condition as the scanner's faithful ranges
  ([numeric conversion](numeric-conversion.md#exact-platform-scanner)): the probe pins the
  library identity and fails on another image. The static check does not read the library.

**Stated input rule.** The rule is: the `CFile` that a lexer reads returns the file's bytes in
order, returns the pushed-back byte after `UnGet`, and at the end returns 0xff with `IsValid`
false. Callers choose the input object at run time, so the method does not join it. Hand-read
checks on M452:

1. `CMemoryFile::Get` (`0x10250e718`) returns the byte at its position and advances; past the end
   it returns 0xff. `CMemoryFile::IsValid` (`0x100859cc4`) is false past the end.
   `CSyncedMemoryFile` and `CRemoteFile` inherit both bodies.
2. `CArchiveFile::Get` (`0x102543788`) reads one byte with `VFSRead`; when the read fails it sets
   the end flag at `+0x78` and returns 0xff. `CArchiveFile::IsValid` (`0x1009bdbd8`) is false
   when the flag is set. `CSyncedArchiveFile` inherits both bodies.
3. `CFile::UnGet` is the pushback above. Both `Get` bodies return the pushed-back byte first.

`CChecksumFile::Get` and `CRemoteStreamFile::Get` return 0; they are not lexer inputs. The rule
holds on M452 for `CMemoryFile`, `CArchiveFile` and the subclasses that inherit their `Get`. To
remove it, join each content loader's `CFile` construction to `CTextLexer`.
`m452_numeric_boundary_engine_parity` checks the two constructor stores and the `Get` and
`IsValid` slots of `CMemoryFile` and `CArchiveFile`.

### Tokens

The three tables (`0x102da1bc0`, `0x102da1c1d` and `0x102da1c7a`) classify bytes `!` (0x21)
through `}` (0x7d). The shape sends every other byte to a word.

| First byte | Token |
| --- | --- |
| `#` or `;` | Comment to `\n` or the end of input; the lexer then reads the next token |
| `"` | Quoted string, kind `0xf`, quoted flag at token `+4` |
| `=` `{` `}` `(` `)` `,` | One-byte token |
| `<` `>` `!` | One-byte token, or `<=` `>=` `!=` when `=` follows |
| Any other byte | Word: digits, letters, `-`, `.`, `+`, `@`, control bytes and bytes from 0x80 |

- **A word** ends at whitespace, which the lexer consumes, or at a byte of `!"#(),;<=>{}`, which
  it pushes back as the first byte of the next token. No other byte ends a word. A word's kind
  is `0xc` when its first byte is a digit or `-`, or when every byte is a digit, `-` or `.`.
  Otherwise its kind is the keyword kind from `CStaticLexer::FindTok`, or `0xf`.
- **A quoted string** ends at a `"` that no backslash precedes, or at the end of input. `\"`
  gives `"` and `\\` gives `\`. Any other backslash stays in the text, and the lexer logs
  "Backslash followed by neither quote nor backslash". Spaces, newlines, `#` and `;` stay in the
  text.
- `CToken::Init` and `CToken::SetCharInString` grow the token buffer, so no length limit applies.

### What the numeric readers receive

- `CReader::ReadSimpleStatement` (`0x1025b7994`) copies `CLexer::Tok()` (lexer `+0x18`) to the
  value token at reader `+0x278`. There are two exceptions. A value whose text starts with `@`
  goes to `ParseAdvancedStatementWithVariables` ([script expansion](script-expansion.md)). A `}`
  value is rebuilt as token 4.
- The numeric token conversions read the text pointer at token `+0x10` and do not check the kind
  or the quoted flag ([numeric conversion](numeric-conversion.md#engine-boundary-facts-m451-hotfix)).
- **End of input.** `GetTok` returns 0 when the byte that ended a word was 0xff
  (`0x1025b1544`–`0x1025b1550`). The reader's lookahead (`0x1025b7ab4`–`0x1025b7ae4`) then
  writes kind `0x13` to the lexer token and keeps the result. The next `GetTok` call returns the
  kept result, and `0x1025b7c14`–`0x1025b7c28` copies the token. So a final value with no
  delimiter after it has kind `0x13` and its text. The numeric readers ignore the kind.

### Three traced tokens

| Input | Lexer result | Live result (`tests/expected/numeric-m452/live.json`) |
| --- | --- | --- |
| `12tail` | One word, kind `0xc`; the newline after it ends it | Stored 12 with no diagnostic, so `tail` is not a separate token |
| `"12 34"` | One quoted token with text `12 34` | Stored 12 by `int`, 46 by the fixed-point template |
| `7}` | Word `7`; `}` is pushed back and is the next token | Read by hand only |

## Pitfalls

- **A NUL byte is a word byte.** The lexer keeps it in the token, but a C function that reads the
  text stops there.
- **A quoted `"@x"` takes the variable path.** The `@` test reads the text after the quotes are
  removed.
- **Byte order marks are not established.** The constructors do not test for one, and the
  loader's file handling is outside this method.
