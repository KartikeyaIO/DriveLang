# Lexer — `lexer.rs`

The lexer is the first stage of the Drive pipeline. It takes raw source code as a `&str` and produces a flat `Vec<Token>`, or a `LexError` on failure. It operates as a single-pass byte scanner with an explicit state machine — no regex, no external dependencies.

---

## Core Types

### `Token`
```rust
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub line: usize,
}
```
Every token carries its classification (`kind`), raw text (`value`), and the source line it appeared on (`line`) for error reporting.

---

### `TokenKind`
The full set of tokens the lexer can emit:

| Category | Variants |
|---|---|
| Literals | `Int`, `Float`, `String` |
| Identifiers | `Identifier` |
| Keywords | `Let`, `Import`, `As`, `For`, `In`, `If`, `Else`, `Elif`, `And`, `Or`, `Not`, `Print` |
| Media keywords | `LoadFrame` (`frame`), `LoadTrack` (`track`), `Filter`, `Export`, `AudioFilter` (`af`), `Blank`, `Silence`, `Kernel` |
| Operators | `Plus`, `Minus`, `Star`, `Slash`, `Equal`, `EqualEqual`, `NotEqual`, `LessThan`, `GreaterThan`, `LessEqual`, `GreaterEqual` |
| Punctuation | `LeftParen`, `RightParen`, `LeftBrace`, `RightBrace`, `LeftBracket`, `RightBracket`, `SemiColon`, `Comma`, `Dot`, `DotDot`, `DoubleColon`, `Arrow` |
| Sentinel | `EOF` |

---

### `State`
The internal scanner state. Only one variant is active at any point:

| State | Meaning |
|---|---|
| `Default` | Between tokens; routing on the current character |
| `Identifier` | Accumulating an alphanumeric/underscore run |
| `String` | Inside a `"…"` string literal |
| `Number` | Accumulating digits (and at most one `.`) |
| `Comment` | Inside a `//` line comment; discards until `\n` |

---

### `LexError`
Three error variants, all carrying a `line` number and a human-readable `message`:

| Variant | Trigger |
|---|---|
| `InvalidCharacter { ch, line, message }` | Unrecognised character, bare `:`, or bare `!` |
| `UnterminatedString { line, message }` | Source ends while still inside a string |
| `InvalidNumber { value, line, message }` | Defined but not yet emitted by the current implementation |

---

## Key Functions

### `lexer(source: &str) -> Result<Vec<Token>, LexError>`
The main entry point. Iterates over `source` as raw bytes with an index `i` and a shared `buffer: String`.

**State transitions:**

```
Default ──"──────────────────────────────► String
        ──alpha/underscore────────────────► Identifier
        ──digit──────────────────────────► Number
        ──'//'───────────────────────────► Comment
        ──single/multi-char operators────► emit Token immediately

String  ──"──────────────────────────────► Default (emit String token)
        ──\n─────────────────────────────► increment line, stay in String

Identifier ──alphanumeric/underscore──────► stay
           ──anything else───────────────► Default (emit via identify_token)

Number  ──digit──────────────────────────► stay
        ──'.' (first, not followed by '.') ► stay (becomes Float)
        ──'..'───────────────────────────► emit Int, back to Default
        ──anything else───────────────────► Default (emit Int or Float)

Comment ──\n─────────────────────────────► Default
        ──anything else───────────────────► discard, stay
```

Multi-character operators are handled inline with a one-character lookahead (`bytes[i+1]`): `..`, `::`, `==`, `!=`, `>=`, `<=`, `->`. A bare `:` or `!` is immediately a `LexError`.

After the main loop, any in-progress `Identifier` or `Number` is flushed. An in-progress `String` emits `UnterminatedString`. A trailing `EOF` token is always appended.

---

### `identify_token(s: &str) -> TokenKind`
Maps a completed identifier buffer to a keyword `TokenKind`, or falls back to `Identifier`. All Drive keywords are matched here: `frame`, `track`, `filter`, `export`, `import`, `as`, `kernel`, `for`, `in`, `af`, `if`, `else`, `elif`, `blank`, `silence`, `and`, `or`, `not`, `let`, `print`.

---

### `char_to_token(c: char) -> Option<TokenKind>`
Maps single unambiguous characters to their token kind: `(`, `)`, `{`, `}`, `;`, `[`, `]`, `,`, `+`, `*`. Returns `None` for anything else, causing the `Default` branch to emit an `InvalidCharacter` error. Note: `-` and `/` are handled separately in `Default` because they are prefixes of multi-char tokens (`->`, `//`).

---

## Tests

Four unit tests covering edge cases in multi-character token disambiguation:

| Test | Input | Checks |
|---|---|---|
| `lex_minus_is_binary_op` | `r - 1` | `-` alone is `Minus`, not confused with `->` |
| `lex_arrow_and_double_colon` | `a -> b::c` | `->` and `::` are emitted correctly |
| `lex_range_dotdot` | `0..10` | Integer followed by `..` does not consume the dots into the number |
| `lex_as_keyword` | `import "f.drive" as filt;` | Full import statement tokenises correctly |