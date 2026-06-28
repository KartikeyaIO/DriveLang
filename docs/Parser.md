# Parser — `parser.rs`

The parser is the second stage of the Drive pipeline. It consumes the `Vec<Token>` produced by the lexer and builds a typed AST rooted at `Program`. It is a hand-written recursive-descent parser with no backtracking — every parse decision is made by looking at exactly one token ahead (`peek`).

The public entry point is the `parse(source: &str)` free function, which runs the lexer and parser in sequence and wraps both error types into `ParseOrLexError`.

---

## AST Types

### `Program`
```rust
pub struct Program {
    pub items: Vec<Item>,
}
```
The root of every Drive script. A program is a flat sequence of top-level `Item`s.

---

### `Item`
Top-level constructs that can appear at program scope:

| Variant | Syntax |
|---|---|
| `Import(Import)` | `import "path";` or `import std::module;` |
| `Print { args }` | `print(expr, ...);` |
| `Assign { name, value }` | `name = expr;` |
| `FilterDecl(FilterDecl)` | `filter name(params) { statements }` |
| `AudioFilterDecl(AudioFilterDecl)` | `af name(params) { statements }` |
| `KernelDecl { name, matrix }` | `kernel name = [[...]];` |
| `Export { value, path }` | `export(expr, "path");` |
| `ForLoop { variable, range, items }` | `for x in range { items }` |
| `IfElse { cond, true_branch, false_branch }` | `if expr { items } else { items }` |



---

### `Import`
```rust
pub enum Import {
    Std(Vec<String>),                    // import std::filters;
    File { path: String, alias: String } // import "my.drive" as x;
}
```

---

### `FilterDecl` / `AudioFilterDecl`
All three share the same shape: a `name`, a `params: Vec<String>` list, and a `body: Vec<Statement>`.

---

### `Statement`
Statements are only valid inside filter/af/effect bodies — they are not top-level items.

| Variant | Meaning |
|---|---|
| `Channel(ChannelAssign)` | Assign a value to a pixel/audio channel: `r = expr;` |
| `Let { name, value }` | Local variable binding: `let x = expr;` |
| `IfElse { cond, true_branch, false_branch }` | Conditional inside a filter body |

---

### `ChannelAssign` and `Channel`
```rust
pub struct ChannelAssign {
    pub channel: Channel,
    pub value: Expr,
}

pub enum Channel { R, G, B, A, T, L }
```
Which channels are legal depends on the context passed as `caller: &str`:

| Context | Legal channels |
|---|---|
| `"filter"` | `r`, `g`, `b`, `a` — `t` is rejected with `TimeError` |
| `"af"` | `l`, `r` (left/right audio) — any other is `InvalidChannel` |
| `"effect"` | `r`, `g`, `b`, `a`, `t` |

---

### `Expr`
All expression forms:

| Variant | Description |
|---|---|
| `Ident(String)` | A bare name |
| `Int(i64)` | Integer literal |
| `Float(f64)` | Float literal |
| `Str(String)` | String literal |
| `Neg(Box<Expr>)` | Unary negation `-expr` |
| `Not(Box<Expr>)` | Logical negation `not expr` |
| `BinOp { op, lhs, rhs }` | Binary expression |
| `Call { path, args }` | Function/filter call, path may be `a::b` |
| `Range { start, end, step }` | Range `start..end` or `start..end..step` |
| `Pipe { base, stages }` | Pipeline `expr -> f(args) -> g(args)` |
| `Array(Vec<Expr>)` | Array literal `[e1, e2, ...]` used for kernel matrices |

---

### `BinOp`
```
Add  Sub  Mul  Div         — arithmetic
Eq   Ne   Gt   Ge  Lt  Le  — comparison
And  Or                    — logical
```

---

### `PipeStage`
```rust
pub struct PipeStage {
    pub path: Vec<String>,   // filter name, possibly namespaced
    pub args: Vec<Expr>,     // arguments passed to the filter
    pub mask: Option<(Expr, Expr)>, // optional spatial mask [x_range, y_range]
}
```
Each `->` stage in a pipeline. The optional mask restricts the filter to a pixel region.

---

### `ParseError`
Five variants, all carrying line numbers:

| Variant | Trigger |
|---|---|
| `UnexpectedToken { expected, found, line }` | Wrong token in a required position |
| `UnexpectedEof { expected }` | Stream ends while more input is expected |
| `InvalidNumber { value, line }` | Token parses as a number lexically but fails `i64`/`f64` conversion |
| `InvalidChannel { name, line }` | Channel name not valid for the current context |
| `TimeError { message, line }` | `t` channel used inside a `filter` body |

`ParseError` implements `Display` and `std::error::Error`.

---

### `ParseOrLexError`
Wraps both error sources for the public `parse()` function:
```rust
pub enum ParseOrLexError {
    Lex(LexError),
    Parse(ParseError),
}
```

---

## The `Parser<'a>` Struct

```rust
pub struct Parser<'a> {
    tokens: &'a [Token],
    pos: usize,
}
```
Holds a borrow of the full token slice and a cursor `pos`. Never backtracks — `pos` only moves forward.

### Primitive operations

| Method | Behaviour |
|---|---|
| `peek()` | Returns the current token (clamped to last token to avoid out-of-bounds) |
| `peek_kind()` | Returns `&TokenKind` of the current token |
| `advance()` | Clones and returns the current token, increments `pos` |
| `check(kind)` | Returns `true` if `peek_kind() == kind` |
| `expect(kind, label)` | Advances and returns the token if it matches, else `UnexpectedToken` or `UnexpectedEof` |
| `expect_identifier(label)` | Like `expect` but specifically for `TokenKind::Identifier`, returns the `value` string |

---

## Parse Functions

### Item-level

| Function | Parses |
|---|---|
| `parse_program()` | Top-level loop; calls `parse_item()` until `EOF` |
| `parse_item()` | Dispatches on the current token to the correct item parser |
| `parse_import()` | `import "path";` or `import a::b;` |
| `parse_filter_decl()` | `filter name(p1, p2) { body }` |
| `parse_audiofilter_decl()` | `af name(p1, p2) { body }` |
| `parse_kernel_decl()` | `kernel name = expr;` |
| `parse_export()` | `export(value, path);` |
| `parse_assignment()` | `name = expr;` |
| `parse_print()` | `print(args...);` |

`for` and `if/else` at item level are parsed inline inside `parse_item()`.

---

### Statement-level

| Function | Parses |
|---|---|
| `parse_statement(caller)` | Dispatches: `let` → `Let`, `if` → `IfElse`, anything else → `Channel` |
| `parse_statement_if_else(caller)` | `if expr { stmts } else { stmts }` or `elif` via recursion |
| `parse_channel_assign(caller)` | `channel = expr;` with strict context-aware channel validation |

`elif` is desugared recursively: the `elif` branch becomes the single element of the `false_branch` `Vec<Statement>`.

---

### Expression-level (precedence chain)

Expressions are parsed through a standard precedence ladder, each level calling the next:

```
parse_expr
  └─ parse_pipe          (->)
       └─ parse_logical  (and, or)
            └─ parse_comparison  (==, !=, <, <=, >, >=)
                 └─ parse_range  (.. and ..step)
                      └─ parse_additive  (+, -)
                           └─ parse_multiplicative  (*, /)
                                └─ parse_unary  (-, not)
                                     └─ parse_primary
```

| Function | Handles |
|---|---|
| `parse_pipe()` | Base expr followed by zero or more `-> stage` |
| `parse_pipe_stage()` | `name(args)` with optional `[x_range, y_range]` spatial mask |
| `parse_logical()` | Left-associative `and` / `or` |
| `parse_comparison()` | Left-associative `==`, `!=`, `<`, `<=`, `>`, `>=` |
| `parse_range()` | `start..end` or `start..end..step` |
| `parse_additive()` | Left-associative `+` and `-` |
| `parse_multiplicative()` | Left-associative `*` and `/` |
| `parse_unary()` | Prefix `not` and `-`, right-recursive |
| `parse_primary()` | Literals, identifiers, calls, `frame(...)`, `track(...)`, `blank(...)`, `silence(...)`, parenthesised expressions, array literals `[...]` |

---

### Helpers

| Function | Does |
|---|---|
| `parse_path()` | Parses `a::b::c` into `Vec<String>` |
| `parse_arg_list()` | Parses comma-separated `Expr` list until `)` |

---

## Public API

```rust
// Run lexer + parser together
pub fn parse(source: &str) -> Result<Program, ParseOrLexError>

// Run parser alone on an existing token slice
impl Parser<'_> {
    pub fn new(tokens: &[Token]) -> Self
    pub fn parse_program(&mut self) -> PResult<Program>
    pub fn parse_expr(&mut self) -> PResult<Expr>
    pub fn parse_statement(&mut self, caller: &str) -> PResult<Statement>
}
```