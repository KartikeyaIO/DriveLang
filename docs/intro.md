# Drive — Documentation Index

Drive is a pipeline DSL for media editing. A Drive script loads images or audio, declares filters, and pipes media through them to produce output. The codebase is split into six documented layers, each with a single responsibility.

---

## How a script runs

```
source code
    │
    ▼
[lexer.rs]  — characters → tokens
    │
    ▼
[parser.rs] — tokens → AST (Program)
    │
    ▼
[engine.rs] — walks AST, compiles filter declarations, evaluates top-level items
    │         calls io.rs to load/export, builds pipelines for '->' expressions
    ▼
[filter.rs] — compiled filters are bytecode (Vec<Instruction>)
              FilterVM executes them per pixel / per sample
    │
    ▼
[pipeline.rs] — sequences Operations on a Frame or AudioPipeline on a Track
    │
    ▼
[io.rs]     — writes Frame → PNG, Track → WAV
```

---

## Where to start reading

If you want to understand the **language**, start with [lexer](lexer.md) then [parser](parser.md).

If you want to understand **how filters work**, read [engine](engine.md) — the compiler half explains how a `filter` block becomes bytecode, the VM half explains how that bytecode runs per pixel.

If you want to understand **how media is represented**, read [primitives](primitives.md) — `Frame` and `Track` are the types everything else operates on.

[pipeline](pipeline.md) and `io.md` are thin layers that are straightforward once the above is clear.

---

## Document map

| File | Covers |
|---|---|
| `lexer.md` | Tokenisation, `Token`, `TokenKind`, state machine, error types |
| `parser.md` | AST types, recursive-descent parser, expression precedence, `ParseError` |
| `engine_and_filter.md` | Bytecode compiler, `Instruction` set, `FilterVM`, `Engine` runtime, built-in functions |
| `pipeline.md` | `Operation` / `AudioOperation`, `EffectPipeline`, `AudioPipeline` |
| `media_primitives.md` | `Frame`, `PixelData`, `Track`, `AudioFrame`, `TimeStamp` |
| `io.md` | Image load/encode, audio decode, WAV export, external library boundaries |