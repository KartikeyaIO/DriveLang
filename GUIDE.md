# Drive Language Reference

Drive is a pipeline DSL for media editing and digital signal processing. A script loads images, folders, or audio tracks, declares filters and functions, and pipes media through them to produce rendered output. This document covers everything supported by the language.

The `examples/` folder contains annotated scripts demonstrating the language features. Scripts that read media expect the input files or folders to exist at the paths shown; adjust those paths for your assets.

---

## Program Structure

A Drive script is a flat sequence of top-level statements executed from top to bottom. There are no classes, no modules beyond `import`, and no required entry point — the file itself is the program.

```
import "my_filters.drive";

kernel blur3 = [[1,2,1],[2,4,2],[1,2,1]];

filter my_filter(param) { ... }

af my_audio_filter() { ... }

fn compute_scale(base, factor) {
    return base * factor;
}

img = frame("input.png");
result = img -> my_filter(compute_scale(1.0, 1.5)) -> blur3();
export(result, "output.png");
```

---

## Imports

```
import "path/to/file.drive";
import "std::color";
```

Loads another Drive file and runs it in the current engine scope. All filters, kernels, user functions, and variables defined in that file become available. Circular imports are safe — each file is only executed once.

Standard library modules (`std::core`, `std::color`, `std::audio`, `std::kernels`, `std::advanced`) are automatically resolved from standard system and library paths.

---

## Values and Variables

Drive supports five runtime value types:

| Type | Description | Example |
|---|---|---|
| **Number** | 64-bit floating point number | `42`, `3.14159`, `-0.5` |
| **String** | Text string with escape sequence support | `"hello\nworld"`, `"C:\\data\\"` |
| **Frame** | RGBA image bitmap buffer | Result of `frame()`, `blank()`, `text()`, `folder[name]` |
| **Track** | Multi-channel audio sample buffer | Result of `track()`, `silence()`, `merge()`, `slice()` |
| **Folder** | Filtered image directory container | Result of `folder("path/to/dir")` |

Variables are assigned with `=` and live in script scope. Types are inferred dynamically at runtime.

```
x = 100;
name = "DriveLang";
img = frame("photo.png");
photos = folder("assets/images");
sound = track("music.wav");
```

---

## Escape Sequences in String Literals

Drive strings support standard escape sequences in regular string literals (`"..."`) and formatted strings (`f"..."`):

| Sequence | Character |
|---|---|
| `\n` | Line feed (newline) |
| `\t` | Horizontal tab |
| `\r` | Carriage return |
| `\\` | Literal backslash |
| `\"` | Literal double quote |
| `\0` | Null byte |

```
print("First line\nSecond line\t(indented)");
path = "C:\\projects\\media\\";
quote = "She said \"Drive is fast!\"";
```

---

## Formatted Strings (F-strings)

Prefixing a string with `f` enables Python-style formatted string interpolation with `{}`:

```
name = "frame";
for i in 0..10 {
    // Zero-padding integers to 4 digits: :04
    export(img, f"output/{name}_{i:04}.png");
}

score = 98.7654;
// Precision formatting: :.2f
print(f"Current score: {score:.2f}%");

// Embedding Frame metadata:
print(f"Loaded: {img.name} ({img.width}x{img.height})");
```

### Supported Format Specifiers
- `:0N` — Format a number as a zero-padded whole number of at least `N` digits (e.g. `{i:04}` produces `0007`). Fractional values are converted to integers first.
- `:.Nf` — Format floating point numbers to `N` decimal places (e.g. `{val:.2f}` produces `3.14`).
- Standard `{expr}` without specifiers formats integers as whole numbers and floats as standard decimals.

---

## Built-in Functions

### Image & Media Loading

| Call | Returns | Description |
|---|---|---|
| `frame("path")` | Frame | Loads an image file (PNG, JPG, BMP, WebP, TIFF, etc.) as RGBA |
| `blank(width, height)` | Frame | Creates an opaque black RGBA frame `(width x height)` |
| `text(str, font_path, size, r, g, b)` | Frame | Rasterizes text into a transparent frame using a TTF font and RGB color |

### Folder Operations (Batch Processing)

The `folder` type scans a directory on disk and exposes a container of image files. Non-image files (such as `.txt`, `.tmp`, `.mp3`) are automatically excluded.

| Call / Syntax | Returns | Description |
|---|---|---|
| `folder("dir_path")` | Folder | Loads and indexes all image files in `dir_path` |
| `folder["filename.png"]` | Frame | Directly loads and returns a specific image frame by file name |
| `folder.len()` | Number | Returns the count of image files inside the folder |
| `for img in folder { ... }` | — | Iterates over each image frame in the folder |

```
photos = folder("assets/gallery");
print(f"Found {photos.len()} valid images in folder.\n");

// Direct access by file name:
hero = photos["hero_banner.jpg"];

// Batch loop over images:
for img in photos {
    processed = img -> blur(2) -> resize(1280, 720);
    export(processed, f"out/{img.name}.png");
}
```

### Audio Built-in Functions

Drive provides built-in audio signal composition and editing functions:

| Call | Returns | Description |
|---|---|---|
| `track("path")` | Track | Decodes an audio file (MP3, WAV, FLAC, OGG, etc.) into a 32-bit float Track |
| `silence(duration_s, sample_rate, channels)` | Track | Creates a silent audio track |
| `merge(track_a, track_b)` | Track | Mixes two tracks together (summing sample amplitudes) |
| `slice(track, start_s, end_s)` | Track | Selects complete audio frames whose timestamps fall in the interval from `start_s` (inclusive) to `end_s` (exclusive); it does not trim samples at the boundaries |
| `concat(track_a, track_b, ...)` | Track | Joins multiple tracks sequentially end-to-end |
| `channel_split(track, channel_idx)` | Track | Extracts a single channel as a mono track (`0` = left, `1` = right) |

```
t1 = track("vocals.wav");
t2 = track("guitar.wav");

// Mix both together:
song = merge(t1, t2);

// Extract the first 30 seconds:
intro = slice(song, 0.0, 30.0);

// Chain tracks sequentially:
outro = track("outro.wav");
full_song = concat(intro, song, outro);

// Extract left mono channel:
left_mono = channel_split(full_song, 0);
```

### Media Export

```
export(media, "output_path");
export_mp3(track, "output.mp3", bitrate_kbps);
```

- **Frame export**: Exports as PNG (e.g. `export(frame, "output.png")`).
- **Track export**:
  - If output path ends in `.wav`, exports 32-bit float stereo/mono WAV.
  - If output path ends in `.mp3`, exports natively as MP3 via pure Rust encoder **without needing external FFmpeg installed**.
- **`export_mp3(track, path, kbps)`**: Explicit MP3 export with target bitrate (e.g. `128`, `192`, `320`).
- The path argument can be a formatted string (`f"output/frame_{i:04}.png"`).

### Console Printing

```
print("Message with {} and {}", x, y);
print(f"Or use f-strings directly: {x}\n");
```

---

## Mathematical Functions Everywhere

Drive provides a comprehensive mathematical library. These functions are available in top-level expressions, user functions (`fn`), filter (`filter`) and audio-filter (`af`) bodies, and conditionals. Script-level range bounds can use evaluated expressions. Loops inside filter bodies are unrolled at compile time and have more limited bound evaluation; see the filter-loop section below.

| Category | Functions |
|---|---|
| **Trigonometry** | `sin(x)`, `cos(x)`, `tan(x)`, `asin(x)`, `acos(x)`, `atan(x)`, `atan2(y, x)` |
| **Exponential & Log** | `exp(x)`, `log(x)` (natural ln), `log10(x)`, `pow(base, exp)` |
| **Rounding & Absolute** | `floor(x)`, `ceil(x)`, `round(x)`, `abs(x)` |
| **Roots & Modulo** | `sqrt(x)`, `mod(a, b)` |
| **Clamping & Bounds** | `min(a, b)`, `max(a, b)`, `clamp(x, min, max)` |
| **Interpolation** | `lerp(a, b, t)` (`a + t * (b - a)`), `smooth_lerp(a, b, t)` (smoothstep curve) |

```
// Script level:
angle = sin(3.14159 / 4.0);
dist = sqrt(pow(3.0, 2.0) + pow(4.0, 2.0)); // 5.0
bounded = clamp(dist * 10.0, 0.0, 255.0);

// In loop bounds:
limit = round(max(5, 10));
for i in 0..limit {
    // ...
}
```

---

## Expressions & Operators

### Arithmetic & Modulo
```
x + y    x - y    x * y    x / y    mod(x, y)
```
Division by zero returns `0.0`.

### Comparisons
```
x == y    x != y    x > y    x >= y    x < y    x <= y
```
Returns `1.0` for true, `0.0` for false.

### Logic
```
x and y    x or y    not x
```
Any non-zero number is treated as truthy.

### Ranges
```
0..10        // 0 to 9 (inclusive start, exclusive end)
0..30..2     // 0 to 28, stepping by 2
```

---

## The Pipeline Operator `->`

```
result = media -> stage1(args) -> stage2(args);
```

Pipes a `Frame` or `Track` through a sequential chain of operations from left to right.

### Native Frame Pipeline Operations

| Stage | Arguments | Description |
|---|---|---|
| `resize(w, h)` | width, height | Nearest-neighbour pixel resize |
| `crop(x, y, w, h)` | x, y, width, height | Extract a sub-rectangle |
| `blend(x, y, frame2, alpha)` | x, y, frame2, blend factor | Linearly blends the RGBA channel values of `frame2` with the base at `(x, y)`. Use a factor from 0.0 to 1.0 for interpolation. |
| `blur(size)` | kernel size | Dynamic box-blur spatial convolution; an even size is rounded up to the next odd size |

### Frame Metadata Properties
Frames loaded from disk or created dynamically provide accessible properties:
- `img.width`: pixel width (Number)
- `img.height`: pixel height (Number)
- `img.name`: source filename including its extension (String), or an empty string if the frame has no source name

### Native Audio Pipeline Operations

| Stage | Arguments | Description |
|---|---|---|
| `fade_in(duration_s)` | seconds | Applies a linear volume fade-in over the first `duration_s` seconds |
| `fade_out(duration_s)` | seconds | Applies a linear volume fade-out over the trailing `duration_s` seconds |
| `gain(db)` | decibels | Applies a gain adjustment in decibels |
| `reverse()` | none | Reverses the audio track in time |
| `speed(factor)` | factor (> 0.0) | Resamples audio by `factor` (e.g. `2.0` doubles playback speed) |

```
sound = track("clip.wav");
processed = sound -> reverse() -> speed(1.5) -> fade_in(1.0) -> fade_out(2.0);
export(processed, "processed.mp3");
```

---

## Spatial Masks

Restrict an image filter or kernel stage to a rectangular pixel region:

```
result = img -> my_filter(args)[x_range, y_range];
```

Pixels outside the selected region remain unchanged. The range start is included and the end is excluded, following the same range rules as `for` loops.

```
// Apply sepia only to a 200x200 window starting at (50, 50):
result = img -> sepia(0.8)[50..250, 50..250];
```

---

## Filter Declarations (`filter`)

A `filter` represents a per-pixel image shader executed via a stack-based virtual machine:

```
filter my_filter(param1, param2) {
    let luma = 0.299 * r + 0.587 * g + 0.114 * b;
    r = clamp(luma * param1, 0, 255);
    g = clamp(luma * param1, 0, 255);
    b = clamp(luma * param2, 0, 255);
}
```

### Available Identifiers Inside Filters
- `r`, `g`, `b`, `a`: Current pixel channel values (0.0 to 255.0).
- `x`, `y`: Integer pixel coordinates.
- `width`, `height`: Total frame dimensions.
- `param1`, `param2`, …: Arguments passed at the call site.

### Filter Composition (Calling Filters Inside Filters)
Filters can call other declared filters inside their body like function calls. Intermediate channel assignments are preserved cumulatively across nested calls:

```
filter brighten(amount) {
    r = clamp(r + amount, 0, 255);
    g = clamp(g + amount, 0, 255);
    b = clamp(b + amount, 0, 255);
}

filter warm_vintage(amount, warmth) {
    // Call another filter inline:
    brighten(amount);

    // Add additional color shifts on top:
    r = clamp(r + warmth, 0, 255);
    b = clamp(b - warmth * 0.5, 0, 255);
}
```

### Loops Inside Filters (Compile-Time Unrolling)
Loops with compile-time numeric bounds can be written directly inside filter bodies to perform accumulations. Bounds can use literals, known numeric variables, arithmetic, and a limited set of compile-time math functions; dynamic per-pixel bounds are not supported:

```
filter stepped_tint(steps) {
    let acc = 0;
    for i in 1..4 {
        acc = acc + i * 15;
    }
    r = clamp(r + acc, 0, 255);
}
```

### Conditionals (`if / elif / else`) Inside Filters
```
filter classify(low, high) {
    let luma = 0.299 * r + 0.587 * g + 0.114 * b;
    if luma < low {
        r = 30; g = 30; b = 80;
    } elif luma < high {
        r = r; g = g; b = b;
    } else {
        r = 255; g = 255; b = 255;
    }
}
```

---

## Audio Filter Declarations (`af`)

An `af` operates per-sample on a `Track`. The body assigns new values to `l` (left channel) and `r` (right channel):

```
af tremolo(freq, depth) {
    let mod = 1.0 - depth * (0.5 + 0.5 * sin(2.0 * 3.14159 * freq * time));
    l = l * mod;
    r = r * mod;
}
```

### Available Identifiers Inside `af`
- `l`, `r`: Sample amplitudes (-1.0 to 1.0).
- `time`: Current sample timestamp in seconds.
- `sr`: Audio sample rate in Hz (e.g. `44100` or `48000`).
- `param1`, …: Call site parameters.
- Supports all math functions, `let` bindings, and `if / elif / else`.

---

## Convolution Kernels (`kernel`)

A `kernel` defines a spatial convolution matrix for 2D filtering (e.g. edge detection, sharpening, embossing):

```
kernel sharpen = [
    [ 0, -1,  0],
    [-1,  5, -1],
    [ 0, -1,  0]
];

result = img -> sharpen();
```

The matrix must be square and odd-sized, with numeric literal weights. The engine divides the convolution result by the sum of the weights when that sum is nonzero; a zero sum is left unnormalized.

---

## User-Defined Functions (`fn`)

DriveLang allows defining general-purpose mathematical helper functions with `fn`:

```
fn clamp_and_scale(val, scale, max_val) {
    let scaled = val * scale;
    if scaled > max_val {
        return max_val;
    } elif scaled < 0 {
        return 0;
    } else {
        return scaled;
    }
}

// Functions can contain loops:
fn sum_factors(n) {
    let total = 0;
    for i in 1..n {
        total = total + i;
    }
    return total;
}

gain = clamp_and_scale(15, 2.0, 50);
factor = sum_factors(5);
```

---

## Control Flow

### `for` Loops over Ranges
```
for i in 0..10 {
    // Loop variable 'i' is accessible in script scope:
    result = img -> warm_vintage(i * 5, 10);
    export(result, f"frames/frame_{i:04}.png");
}
```

### `for` Loops over Folders
```
photos = folder("assets/photos");
for img in photos {
    out = img -> blur(1);
    export(out, f"processed/{img.name}.png");
}
```

### Conditionals (`if / elif / else`)
Available at top-level script, inside `for` loops, inside `fn`, and inside filter bodies:
```
if quality == 2 {
    out = img -> blur(4);
} elif quality == 1 {
    out = img -> blur(2);
} else {
    out = img;
}
```

---

## What is NOT Supported Yet

- The `effect` keyword (video time-based shader syntax reserved for future video engine integration).
- Full video container muxing/demuxing (foundation code in `src/io/video_io.rs` is awaiting pipeline wiring).
- Dynamic non-constant loop bounds inside per-pixel `filter` bytecode shaders (constant bounds are unrolled).

---

## Example Scripts

The `examples/` directory contains scripts demonstrating the language features. Scripts that read media expect their input files and folders to exist at the paths written in the script; change those paths to match your assets.

| File | What to learn from it |
|---|---|
| `hello.drive` | Minimal load and export |
| `basic_filters.drive` | Declaring and chaining custom pixel filters |
| `grayscale.drive` | Luminance formulas and `let` bindings |
| `kernels.drive` | `kernel` declarations and built-in `blur(n)` |
| `masked_filters.drive` | Spatial sub-region masks `[x1..x2, y1..y2]` |
| `intro.drive` | Comprehensive language feature showcase |
| `batch_loop.drive` | `for` loops and print formatting |
| `audio.drive` | Audio DSP with `af`, `time`, and `sr` |
| `conditionals.drive` | `if / elif / else` inside filters and scripts |
| `full_pipeline.drive` | Full media workflow with kernels and audio |
| `filter_composition.drive` | Calling filters inside filters, `if/elif/else`, and math |
| `math_and_functions.drive` | User functions `fn`, math built-ins, and loops in functions |
| `folder_batch_processing.drive` | `Folder` type, image filtering, f-string padding, looped exports |
| `audio_composition_mp3.drive` | Native MP3 export, `merge`, `slice`, `concat`, `fade_in/out`, `reverse` |
| `filter_loops_and_accumulation.drive` | Loops inside filters and passing loop variables to filters |
