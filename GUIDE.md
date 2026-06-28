# Drive Language Reference

Drive is a pipeline DSL for media editing. A script loads images or audio, declares filters, and pipes media through them to produce output. This document covers everything currently supported by the language.

The `examples/` folder contains 10 annotated scripts that demonstrate every feature below in working context — reading them alongside this reference is the fastest way to get productive.

---

## Program Structure

A Drive script is a flat sequence of top-level statements executed from top to bottom. There are no classes, no modules beyond `import`, and no entry point — the file itself is the program.

```
import "my_filters.drive";

kernel blur3 = [[1,2,1],[2,4,2],[1,2,1]];

filter my_filter(param) { ... }

af my_audio_filter() { ... }

img = frame("input.png");
result = img -> my_filter(1.5) -> blur3();
export(result, "output.png");
```

---

## Imports

```
import "path/to/file.drive";
```

Loads another Drive file and runs it in the current engine scope. All filters, kernels, and variables defined in that file become available. Circular imports are safe — each file is only executed once.

---

## Values and Variables

Drive has four value types:

| Type | Example |
|---|---|
| Number | `42`, `3.14`, `-0.5` |
| String | `"hello"` |
| Frame | result of `frame()`, `blank()`, `text()` |
| Track | result of `track()`, `silence()` |

Variables are assigned with `=` and live in global script scope. There is no type declaration — types are inferred at runtime.

```
x = 100;
name = "Drive";
img = frame("photo.png");
```

---

## Built-in Functions

### Image

| Call | Returns | Description |
|---|---|---|
| `frame("path")` | Frame | Loads an image file (PNG, JPG, etc.) as RGBA |
| `blank(width, height)` | Frame | Creates a transparent black RGBA frame |
| `text(str, font_path, size, r, g, b)` | Frame | Rasterizes a string into a frame using the given TTF font and RGB colour |

### Audio

| Call | Returns | Description |
|---|---|---|
| `track("path")` | Track | Decodes an audio file (MP3, FLAC, OGG, WAV, etc.) |
| `silence(duration, sample_rate, channels)` | Track | Creates a silent track of the given duration |

### Export

```
export(frame_or_track, "output_path");
```

Exports a Frame as PNG or a Track as WAV (32-bit float). The path must be a string literal.

### Print

```
print("value is {}", x);
```

Writes to stdout. The first argument is a format string; `{}` placeholders are substituted with subsequent arguments in order. Numbers print as integers if they have no fractional part.

---

## Expressions

### Arithmetic
```
x + y    x - y    x * y    x / y
```
Division by zero returns `0`. All arithmetic operates on Numbers.

### Comparison
```
x == y    x != y    x > y    x >= y    x < y    x <= y
```
Return `1.0` for true, `0.0` for false.

### Logic
```
x and y    x or y    not x
```
Any non-zero number is truthy.

### Unary negation
```
-x
```

### Ranges
```
0..10        // 0 to 9
0..30..2     // 0 to 29, step 2
```
Used in `for` loops and spatial masks.

### Built-in math functions

| Single-arg | Multi-arg |
|---|---|
| `abs(x)` | `min(a, b)` |
| `sqrt(x)` | `max(a, b)` |
| `sin(x)` `cos(x)` `tan(x)` | `pow(a, b)` |
| `asin(x)` `acos(x)` `atan(x)` | `clamp(x, min, max)` |
| `exp(x)` `log(x)` `log10(x)` | `lerp(a, b, t)` |
| `floor(x)` `ceil(x)` `round(x)` | `smooth_lerp(a, b, t)` |

`lerp(a, b, t)` computes `a + t * (b - a)`. `smooth_lerp` applies a smoothstep curve to `t` first.

---

## The Pipeline Operator `->`

```
result = base -> filter_one(args) -> filter_two(args);
```

Pipes a Frame or Track through a sequence of operations left to right. The base value is mutated in place through each stage and the final result is returned. Pipelines on Frames and Tracks are separate — you cannot mix them.

### Native Frame operations (no filter declaration needed)

| Stage | Args | Description |
|---|---|---|
| `resize(w, h)` | width, height | Nearest-neighbour resize |
| `crop(x, y, w, h)` | x, y, width, height | Extract a rectangular region |
| `blend(x, y, frame2, alpha)` | position, frame, 0–1 | Alpha-composite `frame2` onto the base at `(x, y)` |
| `blur(size)` | radius | Dynamically generated box-blur kernel |

---

## Spatial Masks

A filter stage can be restricted to a pixel region:

```
result = img -> my_filter(args)[x_range, y_range];
```

Pixels outside the mask are left unchanged. Ranges follow the same `start..end` or `start..end..step` syntax as everywhere else.

```
// Only apply to a 300x250 region starting at (100, 50)
result = img -> redtint()[100..400, 50..300];
```

---

## Filter Declarations

```
filter name(param1, param2) {
    r = expr;
    g = expr;
    b = expr;
    a = expr;
}
```

A `filter` is a per-pixel operation. The body assigns new values to the output channels `r`, `g`, `b`, `a`. Any channel not assigned passes through unchanged.

### Available identifiers inside a filter

| Name | Meaning |
|---|---|
| `r` `g` `b` `a` | Current pixel channel values (0–255) |
| `x` `y` | Pixel coordinates |
| `width` `height` | Frame dimensions |
| `param1`, `param2`, … | Call-site arguments |

### `let` bindings

```
filter example(factor) {
    let luma = 0.299 * r + 0.587 * g + 0.114 * b;
    r = clamp(luma * factor, 0, 255);
    g = clamp(luma * factor, 0, 255);
    b = clamp(luma * factor, 0, 255);
}
```

`let` declares a local intermediate value. Locals are scoped to their enclosing block.

### `if / else / elif` inside filters

```
filter threshold(level) {
    let luma = 0.299 * r + 0.587 * g + 0.114 * b;
    if luma > level {
        r = 255; g = 255; b = 255;
    } else {
        r = 0; g = 0; b = 0;
    }
}
```

---

## Audio Filter Declarations (`af`)

```
af name(param1) {
    l = expr;
    r = expr;
}
```

An `af` is a per-sample operation on a Track. The body assigns new values to `l` (left channel) and `r` (right channel).

### Available identifiers inside an `af`

| Name | Meaning |
|---|---|
| `l` | Current left channel sample (-1.0 to 1.0) |
| `r` | Current right channel sample |
| `time` | Time of this sample in seconds |
| `sr` | Sample rate |
| `param1`, … | Call-site arguments |

All math functions and `let` / `if` are supported inside `af` bodies.

---

## Kernel Declarations

```
kernel name = [
    [a, b, c],
    [d, e, f],
    [g, h, i]
];
```

A `kernel` defines a convolution matrix. The matrix must be square. All entries must be numeric literals. The kernel is automatically normalised by dividing by the sum of its weights (if the sum is zero, no normalisation is applied). Once declared, a kernel is used as a filter stage in a pipeline:

```
result = img -> name();
```

---

## Control Flow

### `for` loop

```
for i in 0..10 {
    print("frame {}", i);
}
```

Iterates `i` over a range. The loop variable is a Number available inside the block. Body items are full top-level statements — assignments, exports, nested loops, and if/else are all valid inside a `for`.

### `if / else` at script level

```
if quality == 1 {
    result = img -> blur(3) -> sharpen();
} else {
    result = img -> sharpen();
}
```

Condition must evaluate to a Number. `elif` is supported and desugared into nested `if/else`.

---

## Comments

```
// This is a line comment. Everything after // is ignored.
```

---

## What is NOT supported yet

- String interpolation in paths (batch export requires fixed filenames or a host script)
- The `effect` keyword (defined in the parser but not yet active — intended for time-based per-pixel operations using `t`)
- Video decode/encode (the video IO layer exists in the codebase but is not yet wired into the engine)
- Arithmetic expressions in `import` paths
- User-defined functions beyond `filter` and `af`

---

## Examples

The `examples/` folder has 10 scripts that cover the full language surface:

| File | What to learn from it |
|---|---|
| `hello.drive` | Minimal load and export — start here |
| `basic_filters.drive` | Declaring and chaining filters |
| `grayscale.drive` | `let` bindings and luminance math inside a filter |
| `kernels.drive` | `kernel` declaration and the built-in `blur(n)` |
| `masked_filter.drive` | Spatial masks, pixel coordinate access |
| `intro.drive` | This is the intro deck of DriveLang
| `batch_loop.drive` | `for` loop and `print` formatting |
| `audio.drive` | `af` declaration, `track()`, `time` and `sr` builtins |
| `conditionals.drive` | `if/else` inside filter bodies and at script level |
| `full_pipeline.drive` | Everything together: import, kernel, audio, compositing |