# Editron DSL Reference

The Editron DSL (`.drive` files) is a declarative language for defining image and video processing pipelines. Programs are sequences of top-level items executed in order.

---

## Top-Level Items

These are the statements valid at the top level of a `.drive` file.

### Import

```
import std::core;
import "path/to/file.drive" as alias;
```

- `import std::X` loads `stdlib/X.drive` relative to the working directory.
- `import "path" as alias` loads an external file. The alias is currently registered but imports are run immediately — the alias is not used for namespacing yet.
- Circular imports are safe; already-imported files are skipped.

### Variable Assignment

```
img = load("photo.png");
w = 1920;
h = 1080;
```

Variables hold one of three value types: `Frame`, `Number`, or `String`. Type errors surface at runtime.

### `load(path)`

```
img = load("input.png");
```

Loads an image from disk as an RGBA frame. This is a built-in call, not a user-defined filter.

### `blank(width, height)`

```
canvas = blank(1920, 1080);
```

Creates a fully transparent RGBA frame of the given dimensions.

### `text(content, font_path, size, r, g, b)`

```
label = text("Hello", "fonts/Inter.ttf", 48, 255, 255, 255);
```

Rasterizes a string to an RGBA frame using the given font file and color.

### `export(frame, path)`

```
export(result, "output.png");
```

Encodes a frame to disk as a PNG. The path must be a string literal.

### `print(format_string, ...args)`

```
print("Width: {}, Height: {}", 1920, 1080);
```

Prints a formatted string to stdout. `{}` is the only placeholder. The first argument must be a string literal; subsequent arguments are numbers, strings, or literals.

### Filter Declaration

See the Filter Declaration section below.

### Effect Declaration

See the Effect Declaration section below.

### Kernel Declaration

```
kernel sharpen = [
    [ 0, -1,  0],
    [-1,  5, -1],
    [ 0, -1,  0]
];
```

Declares a named convolution kernel. The matrix must be a square 2D array of numeric literals. The divisor is computed automatically as the sum of all weights (or 1.0 if the sum is zero, for edge-detection kernels).

### For Loop

```
for i in 0..10 {
    export(img, "frame_{}.png");
}
```

Iterates over a range. The loop variable `i` is available as a `Number` inside the body. The body contains top-level items (assignments, exports, prints, filter applications). Nested for loops are allowed.

### If / Else (top-level)

```
if mode == 1 {
    export(img -> grayscale(), "gray.png");
} else {
    export(img, "color.png");
}
```

Evaluates the condition as a number (0.0 = false, anything else = true). Both branches contain top-level items. `elif` is not supported at the top level (only inside filter bodies).

---

## Pipe Expressions

The pipe operator `->` chains filter operations on a frame. Each stage is applied sequentially.

```
result = img -> brightness(20) -> contrast(1.2) -> blur(5);
```

### Pipe Stage Syntax

```
frame -> filter_name(arg1, arg2)
frame -> filter_name(arg1)[x_range, y_range]
```

The optional `[x_range, y_range]` mask restricts the operation to a rectangular region:

```
img -> brightness(30)[100..500, 200..400]
```

Ranges use `start..end` or `start..end..step`.

### Built-in Pipe Operations

These are handled natively by the pipeline executor and are not DSL-defined filters:

| Name | Syntax | Description |
|------|--------|-------------|
| `resize` | `resize(width, height)` | Nearest-neighbor resize |
| `crop` | `crop(x, y, width, height)` | Crop a sub-region |
| `blend` | `blend(x, y, frame2, alpha)` | Composite `frame2` onto the frame at position `(x, y)` with `alpha` in [0.0, 1.0] |
| `blur` | `blur(size)` | Box blur with a dynamically-sized kernel (size is auto-rounded to odd) |

Any declared `kernel` can also be used as a pipe stage by its name (no arguments):

```
img -> sharpen()
```

---

## Filter Declaration

Filters are pure, stateless, per-pixel transformations. They are the primary way to define new operations.

```
filter brightness(amount) {
    r = r + amount;
    g = g + amount;
    b = b + amount;
}
```

### Structure

```
filter name(param1, param2, ...) {
    // statements
}
```

### Statements Valid Inside a Filter Body

#### Channel Assignment

```
r = <expr>;
g = <expr>;
b = <expr>;
a = <expr>;
```

Each channel is assigned independently. Unassigned channels pass through unchanged. You can assign the same channel multiple times; the last assignment wins.

**`t` (time) is not valid inside `filter` bodies.** reserved for  `effect` .

#### Let Binding

```
let luma = 0.299 * r + 0.587 * g + 0.114 * b;
r = luma;
g = luma;
b = luma;
```

Let bindings compute a value once and store it in a local slot. The value is re-computed per pixel (locals are per-pixel VM state, not global constants). Let bindings are available to all subsequent statements including channel assignments and other let bindings.

#### If / Else / Elif

```
filter threshold(cutoff) {
    let luma = 0.299 * r + 0.587 * g + 0.114 * b;
    if luma > cutoff {
        r = 255;
        g = 255;
        b = 255;
    } else {
        r = 0;
        g = 0;
        b = 0;
    }
}
```

```
filter grade(level) {
    if level == 1 {
        r = r * 1.2;
    } elif level == 2 {
        r = r * 0.8;
    } else {
        r = r;
    }
}
```

- `elif` chains are supported inside filter body.
- Condition is a number expression: 0.0 is false, anything else is true.
- Both branches can contain channel assignments and let bindings.
- Let bindings declared inside a branch are scoped to that branch only.

### Expressions Valid in Filter Bodies

#### Built-in Identifiers

| Name | Meaning |
|------|---------|
| `r` | Red channel of the current pixel (0–255) |
| `g` | Green channel |
| `b` | Blue channel |
| `a` | Alpha channel |
| `x` | X coordinate of the current pixel |
| `y` | Y coordinate of the current pixel |
| `width` | Frame width in pixels |
| `height` | Frame height in pixels |

#### Arithmetic Operators

`+`, `-`, `*`, `/`, `%` (modulo), `^` is not syntax — use `pow(a, b)`.

#### Comparison Operators (produce 1.0 or 0.0)

`==`, `!=`, `>`, `>=`, `<`, `<=`

#### Logical Operators (produce 1.0 or 0.0)

`and`, `or`, `not`

#### Unary

`-expr` (negation), `not expr`

#### Built-in Math Functions

| Function | Args | Description |
|----------|------|-------------|
| `abs(x)` | 1 | Absolute value |
| `sin(x)` | 1 | Sine (radians) |
| `cos(x)` | 1 | Cosine (radians) |
| `tan(x)` | 1 | Tangent (radians) |
| `asin(x)` | 1 | Arc sine |
| `acos(x)` | 1 | Arc cosine |
| `atan(x)` | 1 | Arc tangent |
| `sqrt(x)` | 1 | Square root |
| `exp(x)` | 1 | e^x |
| `log(x)` | 1 | Natural log |
| `log10(x)` | 1 | Base-10 log |
| `floor(x)` | 1 | Round down |
| `ceil(x)` | 1 | Round up |
| `round(x)` | 1 | Round to nearest |
| `min(a, b)` | 2 | Minimum |
| `max(a, b)` | 2 | Maximum |
| `pow(a, b)` | 2 | a raised to b |
| `clamp(v, lo, hi)` | 3 | Clamp v to [lo, hi] |
| `lerp(a, b, t)` | 3 | Linear interpolate between a and b |
| `smooth_lerp(a, b, t)` | 3 | Smoothstep interpolation (t clamped to [0,1]) |

#### Literals

Integer: `42`, `-1`
Float: `3.14`, `0.5`
String literals are not valid in filter expression context.

### What is NOT Valid Inside a Filter Body

- Top-level items (`load`, `export`, `import`, `for`, `print`)
- Pipe expressions (`->`)
- Frame values or calls that return frames (`load`, `blank`, `text`)
- Array literals
- The `t` channel (reserved for  `effect` )


---

## Ranges

Used in for loops and pipe stage masks.

```
0..10        // 0, 1, 2, ..., 9 (exclusive end)
0..100..5    // 0, 5, 10, ..., 95 (with step)
```

---

## Type System Summary

The DSL has three runtime value types at the engine level:

| Type | Created by |
|------|------------|
| `Frame` | `load()`, `blank()`, `text()`, pipe expressions |
| `Number` | Integer/float literals, arithmetic expressions, for loop variables |
| `String` | String literals (`"..."`) |

Filter  parameters are always `Number` (passed as `f32` to the VM).

---

## Known Limitations (V1)

- `import "file" as alias` — the alias is ignored; the file's definitions are merged into the global scope.
- Kernel `divisor` and `bias` cannot be set from the DSL; divisor is auto-computed from weight sum.
- `Mask::Circle` is not reachable from pipe stage syntax (only `Rect` masks via `[x..y, x..y]`).
- Composite filters (a filter body that calls other filters) are not yet supported.

