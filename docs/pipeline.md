# Pipeline — `pipeline.rs`

The pipeline is the execution layer between the engine and the media types. When the engine evaluates a `->` pipe expression, it compiles each stage into an `Operation` or `AudioOperation`, assembles them into a pipeline struct, and calls `execute`. The pipeline then applies every operation in order, mutating the `Frame` or `Track` in place.

---

## Image Pipeline

### `Operation`
Each variant represents one stage a frame can pass through:

| Variant | Fields | What it does |
|---|---|---|
| `PointFilter` | `filter: Filter`, `params: Vec<f32>`, `mask: Option<Mask>` | Runs the compiled bytecode filter per pixel; skips pixels outside the mask if one is set |
| `Convolution` | `kernel: Kernel`, `mask: Option<Mask>` | Applies a convolution kernel; reads from a snapshot clone of the frame to avoid accumulation artefacts |
| `Blend` | `x, y: u32`, `frame2: Frame`, `alpha: f64` | Alpha-composites `frame2` onto the frame at position `(x, y)` |
| `NativeResize` | `width, height: u32` | Resizes the frame to the given dimensions; replaces `*frame` with the result |
| `NativeCrop` | `x, y, width, height: u32` | Crops the frame to the given rect; replaces `*frame` with the result |

---

### `Pipeline` trait
```rust
pub trait Pipeline {
    fn execute(&self, frame: &mut Frame) -> Result<(), PipelineError>;
}
```
Defines the single entry point. `EffectPipeline` implements this.

---

### `EffectPipeline`
```rust
pub struct EffectPipeline {
    pub operations: Vec<Operation>,
}
```
Iterates operations sequentially. Frame dimensions are re-read on each operation — not cached up front — because `NativeResize` and `NativeCrop` change the frame size mid-pipeline.

**Per-operation behaviour:**

- `PointFilter`: creates a fresh `FilterVM`, iterates every `(x, y)` pixel, checks the mask, reads the current color, calls `Filter::apply`, writes the result back.
- `Convolution`: clones the current frame into a `snapshot` first, then iterates pixels and calls `kernel.apply_to_pixel(x, y, &snapshot)`. The snapshot ensures each pixel is computed from the unmodified input, not a partially-written output.
- `Blend`: delegates directly to `frame.blend_on(pos, frame2, alpha)`.
- `NativeResize` / `NativeCrop`: call the frame's own method and assign the returned frame back via `*frame = new_frame`.

---

## Audio Pipeline

### `AudioOperation`

| Variant | Fields | What it does |
|---|---|---|
| `PointFilter` | `filter: AudioFilter`, `params: Vec<f32>` | Runs the compiled audio bytecode per sample across the track buffer |
| `Gain` | `f32` (dB) | Delegates to `track.gain(db)` for a flat volume adjustment |

---

### `AudioPipeline`
```rust
pub struct AudioPipeline {
    pub operations: Vec<AudioOperation>,
}
```
Not a `Pipeline` implementor — has its own `execute(&self, track: &mut Track)` because the signature differs (`Track` not `Frame`).

**`PointFilter` execution:** for each audio frame in the track buffer, computes a per-sample time value `t = frame.time + i * (1/sr)` and calls `filter.apply(l, r, t, sr, params, vm)` sample by sample. Mono tracks (`data.len() == 1`) use the left channel value for both `l_in` and `r_in`; only `data[0]` is written back.

---

### `PipelineError`

| Variant | Triggered by |
|---|---|
| `InvalidData` | Defined, not yet emitted |
| `PixelError` | `frame.set_pixel` failure in `PointFilter` or `Convolution` |
| `NotFeasible` | `NativeResize`, `NativeCrop`, or `Blend` failure |