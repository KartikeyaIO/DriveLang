# Editron

Editron is an experimental media processing engine and domain-specific language for building composable, declarative media pipelines.

The project is built around a philosophy of **infrastructure over individual filters** — new processing operations should require only new DSL expressions, never new Rust code.

---

## Project Goals

1. Provide a simple, readable syntax for defining media processing pipelines
2. Compile DSL filter declarations into efficient bytecode executed by a stack VM
3. Support performance-critical processing with minimal per-filter overhead
4. Handle multiple media formats through a unified RGBA-normalized interface
5. Build a complete standard library of filters expressed entirely in the DSL

---

## Architecture Overview

### Lexer
Converts `.edt` source into a flat token stream.

- Implemented as a single-pass state machine over raw bytes
- Handles identifiers, numbers (int/float), strings, operators, and keywords
- Produces `Vec<Token>` with kind, value, and line number
- Keywords: `filter`, `kernel`, `import`, `export`, `load`, `blank`, `let`, `as`

Details: [Lexer Architecture](docs/Lexer.md)

---

### Parser
Consumes tokens and emits a typed AST.

- Recursive descent, AST-first design
- Supports binary arithmetic (`+`, `-`, `*`, `/`), unary negation, and precedence climbing
- `Pipe` expressions (`base -> stage(args)[mask]`) for chaining operations
- `Range` expressions (`start..end..step`) for spatial masks
- `Array` literals for kernel matrix declarations (`[[1, 2, 1], ...]`)
- `let` bindings inside filter bodies for named intermediate values

Details: [Parser](docs/Parser.md)

---

### Engine
The execution core. Drives the full parse → compile → run cycle.

Responsibilities:
- Compiling `FilterDecl` AST nodes into `Filter` bytecode structs
- Compiling `KernelDecl` AST nodes into `Kernel` convolution structs
- Evaluating top-level assignments, imports, and exports
- Building `EffectPipeline` instances from pipe expressions
- Resolving import paths for both file imports and stdlib imports

#### Filter Compilation
Each `filter` declaration compiles into four independent bytecode programs — one per RGBA channel. Local variables declared with `let` inside a filter body are compiled into `StoreLocal`/`LoadLocal` instructions, with setup instructions prepended to every channel program so locals are available regardless of which channel is executing.

Unassigned channels default to pass-through (`LoadR`, `LoadG`, `LoadB`, `LoadA`).

#### Kernel Compilation
`kernel` declarations compile matrix literals into flat `Vec<f32>` with an automatic divisor computed from the sum of weights (or `1.0` for zero-sum kernels like edge detectors).

---

### Filter VM
A lightweight stack-based virtual machine that executes compiled filter programs per-pixel.

Supported operations:
- Channel access: `r`, `g`, `b`, `a`
- Frame metadata: `x`, `y`, `width`, `height`
- Parameters: indexed by declaration order
- Local variables: `StoreLocal` / `LoadLocal`
- Arithmetic: `+`, `-`, `*`, `/`, `%`, `pow`
- Math: `abs`, `min`, `max`, `clamp`, `sqrt`, `exp`, `log`, `log10`
- Trig: `sin`, `cos`, `tan`, `asin`, `acos`, `atan`
- Rounding: `floor`, `ceil`, `round`
- Comparison and logic: `==`, `!=`, `>`, `>=`, `<`, `<=`, `and`, `or`, `not`

---

### Pipeline
Applies a sequence of operations to a frame in declaration order.

```
EffectPipeline
  ├── PointFilter  →  runs FilterVM per-pixel (with optional spatial Mask)
  └── Convolution  →  runs Kernel against a pre-snapshot of the frame
```

Convolution operations snapshot the frame before the pass so kernel reads are always from the unmodified source — no bleed between filter stages.

Spatial masks restrict any operation to a `Rect` region, built from `Range` expressions in pipe stage brackets: `-> filter(args)[x_range, y_range]`.

Dynamic kernels (e.g. `blur(15)`) are generated at runtime without requiring a static `kernel` declaration.

---

## Core Data Types

### Frame
Represents a single image or video frame.

- Planar channel layout: separate `Vec<u8>` per channel
- Formats: `RGBA`, `RGB`, `GRAY`, `YUV420`
- All pipeline processing normalizes to `RGBA` at entry
- YUV remains only as a decode/encode boundary format handled by FFmpeg
- Operations: `get_pixel`, `set_pixel`, `blit`, `blend`, `blend_on`, `crop`, `brightness`, `contrast`, `saturation`, `opacity`

Details: [Frame](docs/Frame.md)

---

### Track
Represents decoded audio data.

- Planar `f32` samples: `Vec<Vec<f32>>` (channel × sample)
- `AudioFrame` carries a `TimeStamp` and one chunk of planar sample data
- Operations: `gain`, `mix`, `merge`, `slice`, `normalize`, `silence`, `to_pcm_f32`, `to_pcm_i16`

Details: [Track](docs/Track.md)

---

### Video
Provides frame-by-frame access to video streams via FFmpeg.

- Sequential decode with `decode_next()`
- Seek + forward-decode with `decode_frame(index)`
- Embedded audio extraction with `decode_audio()`
- All decoded frames are converted to planar RGBA, with FFmpeg linesize padding stripped

Details: [Video](docs/Video.md)

---

### VideoEncoder
Encodes a sequence of `Frame`s (and optionally a `Track`) into a container file.

- RGBA input → YUV420P via swscale → MPEG4 video stream
- Planar f32 audio → AAC stream (optional, registered before header write)
- Monotonic PTS assignment; caller never touches time-bases

---

## DSL

Editron's DSL is the primary interface for defining pipelines. Filters and kernels declared in `.edt` files are the only extension point — no Rust changes are needed to add new processing operations.

### Syntax

```edt
// Import a stdlib module
import std::color;

// Import a file
import "my_filters.edt" as custom;

// Declare a filter with parameters and local variables
filter brightness(amount) {
    let shift = amount * 2.55;
    r = clamp(r + shift, 0, 255);
    g = clamp(g + shift, 0, 255);
    b = clamp(b + shift, 0, 255);
}

// Declare a convolution kernel
kernel sharpen = [
    [ 0, -1,  0],
    [-1,  5, -1],
    [ 0, -1,  0]
];

// Load, process, and export
image = load("input.jpg");

result = image
    -> brightness(10)
    -> sharpen()
    -> custom::vignette(0.4)[100..900, 50..700];

export(result, "output.png");
```


### Filter Body

Inside a `filter` block:
- `r`, `g`, `b`, `a` — channel assignment targets and source values
- `x`, `y`, `width`, `height` — pixel position and frame metadata
- `let name = expr;` — local intermediate variable
- All standard math and trig functions
- Params are referenced by their declared names

### Pipe Stages

```edt
frame -> filter_name(arg1, arg2)[x_start..x_end, y_start..y_end]
```

The `[x_range, y_range]` bracket suffix is optional. Ranges support an optional step: `0..width..2` for every other column.

### Imports

```edt
import std::color;          
import "path/to/file.edt" as alias;
```

Imported filters and kernels are merged into the current engine scope. Circular imports are detected and skipped.

---

## IO

| Function                                  | Description                                          |
| ----------------------------------------- | ---------------------------------------------------- |
| `load_image(path, fmt)`                   | Loads an image as `Frame` in the specified format    |
| `encode_image(frame, path)`               | Writes a `Frame` to a PNG file                       |
| `decode_audio(path)`                      | Decodes an audio file into a `Track` using Symphonia |
| `encode_wav(track, path)`                 | Writes a `Track` to a WAV file (f32 or i16)          |
| `Video::open(path)`                       | Opens a video file for frame-by-frame decode         |
| `VideoEncoder::open(path, source, audio)` | Opens an output container for encoding               |

---

## Dependencies

| Crate         | Role                                                     |
| ------------- | -------------------------------------------------------- |
| `ffmpeg-next` | Video decode/encode, pixel format conversion via swscale |
| `image`       | Still image load/save                                    |
| `symphonia`   | Audio file decoding (MP3, FLAC, WAV, AAC, ...)           |
| `hound`       | WAV encoding                                             |
| `fontdue`     | Font rasterization for text overlays                     |

---

## Roadmap

- `ChannelOp` linear coefficient representation for `Filter`
- Non-square `Kernel` support
- Fused `PipelineOp` pass (stacking point filters in a single pixel traversal)
- `SequencePipeline` and `Effect` types for frame-level temporal operations
- Parameterized composite filters via `Expr` substitution pass
- `ffmpeg-the-third` migration (replacing `ffmpeg-next`)
- Reel binary intermediate format integration
- Audio pipeline: parallel `AudioPipeline` with `AudioFilter`, `AudioKernel`, `AudioEffect`