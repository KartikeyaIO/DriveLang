# Engine & Filter VM — `engine.rs` + `filter.rs`

These two files together form the execution core of Drive. `filter.rs` defines the bytecode IR and the stack-based VM that runs it. `engine.rs` is the compiler and interpreter that walks the AST, compiles filter declarations into bytecode, and evaluates top-level items to produce output.

The flow is:
```
AST (Program)
  └─ Engine::run()
       ├─ exec_item() — top-level evaluation (assigns, exports, loops, if/else)
       ├─ eval() — expression evaluation; pipelines dispatch here
       ├─ compile_filter_decl() ──► Filter { r/g/b/a_program: Vec<Instruction> }
       ├─ compile_audiofilter_decl() ──► AudioFilter { l/r_program }
       ├─ compile_kernel_decl() ──► Kernel { matrix, size, divisor }
       └─ compile_stage() / compile_audio() ──► Operation / AudioOperation
            └─ FilterVM::run_program() — executes bytecode per pixel/sample
```

---

## `filter.rs` — Bytecode IR and VM

### Filter Types

#### `Filter`
A compiled image point-filter. Holds four independent `Vec<Instruction>` programs — one per output channel (`r`, `g`, `b`, `a`). Each program is run independently per pixel.

#### `AudioFilter`
A compiled audio point-filter. Holds two programs: `l_program` and `r_program` for left and right stereo channels.

#### `Effect`
Defined but not yet active (compilation commented out in engine). Extends `Filter` with a fifth `t_program` for time-based operations.

---

### `Instruction`
The bytecode enum. All values are `f32` on the stack. Booleans are represented as `1.0` (true) / `0.0` (false).

**Loads**

| Instruction | Pushes |
|---|---|
| `LoadR/G/B/A` | Current pixel channel value (0–255) or audio R channel |
| `LoadL` | Audio left channel |
| `LoadT` | Time (currently a no-op placeholder) |
| `LoadX/Y` | Pixel coordinates |
| `LoadWidth/Height` | Frame dimensions |
| `LoadParam(i)` | The `i`-th call-site argument |
| `LoadLocal(i)` | A `let`-bound local variable |
| `LoadTime` | Audio sample time in seconds |
| `LoadSampleRate` | Audio sample rate |
| `PushInt(v)` / `PushFloat(v)` | Literal constants |

**Arithmetic:** `Add`, `Sub`, `Mul`, `Div` (div-by-zero → 0), `Mod`, `Pow`, `Neg`

**Comparison** (push `1.0` or `0.0`): `Eq`, `Ne`, `Gt`, `Ge`, `Lt`, `Le`

**Logic:** `And`, `Or`, `Not`

**Math (single-arg):** `Abs`, `Sin`, `Cos`, `Tan`, `Asin`, `Acos`, `Atan`, `Sqrt`, `Exp`, `Log`, `Log10`, `Floor`, `Ceil`, `Round`

**Math (multi-arg):**
- `Min` / `Max` — pop 2, push result
- `Clamp` — pop `max`, `min`, `value`; push `value.clamp(min, max)`
- `Lerp` — pop `t`, `b`, `a`; push `a + t * (b - a)`
- `SmoothLerp` — same as `Lerp` but applies smoothstep `t² * (3 - 2t)` to `t` first

**Control flow:**
- `Jump(target)` — set `ip = target` unconditionally
- `JumpIfFalse(target)` — pop; if `0.0`, jump to `target`

**Locals:**
- `StoreLocal(i)` — pop into `locals[i]`, resizing if needed
- `LoadLocal(i)` — push `locals[i]`

---

### Execution Contexts

#### `PixelContext`
```rust
pub struct PixelContext { pub color: Color, pub x: u32, pub y: u32, pub width: u32, pub height: u32 }
```
Passed to the VM when running an image filter. Provides the current pixel's color and position.

#### `AudioContext`
```rust
pub struct AudioContext { pub l: f32, pub r: f32, pub time: f32, pub sample_rate: f32 }
```
Passed to the VM when running an audio filter.

#### `VMContext<'a>` (private)
Internal enum wrapping either context so `run_program` is unified — a single loop handles both image and audio instructions.

---

### `FilterVM`

```rust
pub struct FilterVM {
    stack: Vec<f32>,   // operand stack, pre-allocated to 64
    locals: Vec<f32>,  // let-binding storage, pre-allocated to 16
}
```

The stack machine. `run_program` loops over instructions with an `ip` counter. Jump instructions set `ip` directly and `continue`, skipping the standard `ip += 1`. All other instructions fall through to increment.

**Public methods:**

| Method | Does |
|---|---|
| `execute(program, ctx, params) -> f32` | Clears stack, runs program in pixel context, pops result |
| `execute_audio(program, ctx, params) -> f32` | Same for audio context |

---

### `Filter::apply` and `AudioFilter::apply`

`Filter::apply` builds a `PixelContext`, runs all four channel programs through the VM, clamps outputs to `0–255`, and returns the new `Color`. The alpha program is only run for `RGBA` inputs; `RGB` and `Gray` are promoted to RGBA.

`AudioFilter::apply` builds an `AudioContext`, runs `l_program` and `r_program`, and returns a `(f32, f32)` stereo sample pair.

---

## `engine.rs` — Compiler and Interpreter

### `Value`
The runtime value type:
```rust
pub enum Value { Frame(Frame), Track(Track), Number(f64), String(String) }
```
Every variable, expression result, and function return is one of these.

---

### `EngineError`
```rust
pub enum EngineError {
    Compile(String),    // Error during bytecode compilation
    Eval(String),       // Error during evaluation
    EvalError(String),  // Typed eval error
    Pipeline(PipelineError),
    Io(IOError),
    UndefinedVar(String),
    UndefinedOp(String),
}
```
`From<PipelineError>` and `From<IOError>` are implemented for ergonomic `?` propagation.

---

### `CompileContext` (private)
```rust
enum CompileContext { Image, Audio, Effect }
```
Tells `compile_into` which set of built-in identifiers to resolve. In `Image`/`Effect` context, `r/g/b/a/x/y/width/height` are valid. In `Audio`, `l/r/time/sr` are valid. Effect additionally allows `t`.

---

### Compiler Functions

#### `compile_into(expr, params, param_count, out, context)`
The core recursive compiler. Walks an `Expr` and appends `Instruction`s to `out`. Handles:
- Literals → `PushInt` / `PushFloat`
- Identifiers → built-in loads (`LoadR`, `LoadX`, …), `LoadParam(i)`, or `LoadLocal(i)` depending on whether the name is a parameter or a `let`-local
- `BinOp` → compile both sides, push the operator instruction
- `Call` → multi-arg builtins (`clamp`, `lerp`, `smooth_lerp`, `min`, `max`, `pow`) and single-arg math functions (`abs`, `sin`, `cos`, etc.)
- `Neg` / `Not` → compile inner, push `Neg` / `Not`

#### `compile_stmts_for_channel(stmts, target, …, out)`
Iterates statements and emits instructions only relevant to one output channel:
- `Let` bindings are always emitted (any channel may reference them); they compile the value expression then push `StoreLocal(i)` and extend the local scope.
- `Channel` assignments emit instructions only if the channel matches `target`.
- `IfElse` emits the condition, a `JumpIfFalse` placeholder, the true branch, a `Jump` placeholder, then the false branch. Both placeholders are back-patched with the correct target indices after the branches are emitted.

#### `compile_channel_program(body, target, params, param_count, context)`
Calls `compile_stmts_for_channel` for one channel. If the resulting program is empty (the channel was never assigned), it inserts a single `Load{channel}` instruction so the original value passes through unchanged.

#### `compile_filter_decl(decl) -> Filter`
Compiles an image filter: runs `compile_channel_program` four times (R, G, B, A) under `CompileContext::Image`.

#### `compile_audiofilter_decl(decl) -> AudioFilter`
Same for audio: compiles L and R programs under `CompileContext::Audio`.

#### `compile_kernel_decl(name, matrix) -> Kernel`
Expects an `Expr::Array` of `Expr::Array` rows. Validates the matrix is square, extracts `f32` literals via `const_number`, computes the sum as the divisor (1.0 if zero), and returns a `Kernel`.

#### `const_number(expr) -> f32`
Converts only `Expr::Int`, `Expr::Float`, or `Expr::Neg(literal)` to `f32`. Used for kernel matrix entries which must be compile-time constants.

---

### `Engine`

```rust
pub struct Engine {
    vars: HashMap<String, Value>,
    filters: HashMap<String, Filter>,
    afilters: HashMap<String, AudioFilter>,
    kernels: HashMap<String, Kernel>,
    effects: HashMap<String, Effect>,
    imported_files: HashSet<String>,
}
```

The runtime state. Created with `Engine::new()`, then driven by `Engine::run(&program)`.

#### `run` / `exec_item`
`run` iterates `program.items` and calls `exec_item` on each. `exec_item` dispatches:

| Item | Action |
|---|---|
| `Import::File` | `import_file` — loads, parses, and runs the file; guarded by `imported_files` to prevent cycles |
| `Import::Std` | Resolves to `stdlib/<path>.drive` and calls `import_file` |
| `FilterDecl` | `compile_filter_decl` → inserts into `self.filters` |
| `AudioFilterDecl` | `compile_audiofilter_decl` → inserts into `self.afilters` |
| `KernelDecl` | `compile_kernel_decl` → inserts into `self.kernels` |
| `Assign` | `eval(value)` → inserts into `self.vars` |
| `ForLoop` | Evaluates range, iterates it; inserts loop variable into `vars` each iteration and executes body items |
| `IfElse` | Evaluates condition as `Number`; runs true or false branch items |
| `Print` | Evaluates args, substitutes `{}` placeholders in format string, prints to stdout |
| `Export` | Evaluates value; dispatches `io::encode_image` for `Frame` or `io::encode_wav` for `Track` |

#### `eval(expr) -> Result<Value, EngineError>`
Evaluates an `Expr` at script scope:
- `Ident` → look up in `vars`
- `Int` / `Float` → `Value::Number`
- `Str` → `Value::String`
- `Neg` / `Not` → numeric negation/inversion
- `BinOp` → both sides evaluated as numbers; all comparisons return `1.0` or `0.0`; division by zero returns `0.0`
- `Call` → dispatched to `eval_call`
- `Pipe` → evaluates the base value; if `Frame`, builds an `EffectPipeline` via `compile_stage`; if `Track`, builds an `AudioPipeline` via `compile_audio`; executes and returns the mutated value

#### `eval_call`
Handles built-in functions callable at script scope:

| Function | Returns |
|---|---|
| `frame("path")` | `Value::Frame` — loads image via `io::load_image` |
| `track("path")` | `Value::Track` — decodes audio via `io::decode_audio` |
| `text(str, font, size, r, g, b)` | `Value::Frame` — rasterizes text using `fontdue`, returns it as a frame |
| `blank(w, h)` | `Value::Frame` — creates an empty frame |
| `silence(dur, sr, channels)` | `Value::Track` — creates a silent audio track |

#### `compile_stage(stage) -> Operation`
Resolves a pipeline stage to an `Operation` for the frame pipeline:
- `resize(w, h)` → `Operation::NativeResize`
- `crop(x, y, w, h)` → `Operation::NativeCrop`
- `blend(x, y, frame, alpha)` → `Operation::Blend`
- Named user filter → `Operation::PointFilter { filter, params, mask }`
- `blur(size)` → dynamically generates a box-blur kernel via `Kernel::generate_blur`, bypassing the static kernel dictionary → `Operation::Convolution`
- Named user kernel → `Operation::Convolution { kernel, mask }`

#### `compile_audio(stage) -> AudioOperation`
Resolves a pipeline stage for track pipelines. Looks up the name in `self.afilters` and evaluates numeric arguments. Returns `AudioOperation::PointFilter { filter, params }`.

#### `build_mask` / `expr_to_step_range`
Convert an `Expr::Range { start, end, step }` AST node into a `StepRange` (a `Range<usize>` with a step). A non-range expression is treated as a single-element range `v..(v+1)`. Two ranges (x, y) are combined into a `Mask::Rect` for spatial filter masking.

#### Type-checked eval helpers

| Method | Returns |
|---|---|
| `eval_number(expr)` | `f64` or error if value is not `Number` |
| `eval_frame(expr)` | `Frame` or error |
| `eval_track(expr)` | `Track` or error |
| `eval_string(expr)` | `String` or error |
| `eval_usize(expr)` | `usize`, rejects negative values |
| `eval_export(expr)` | `Frame` or `Track`, rejects numbers and strings |