# Media Primitives — `frame.rs` + `track.rs`

These two files define the core data types that flow through the entire Drive pipeline. Everything the engine loads, transforms, and exports is ultimately a `Frame` or a `Track`.

---

## `frame.rs` — Image Data

### Architecture

A `Frame` is a 2D image of fixed dimensions backed by a `PixelData` buffer. The pixel data is stored **planar** — each channel is a separate `Vec<u8>` — rather than interleaved. This means for an RGBA frame of N pixels, there are four independent `Vec<u8>` each of length N, indexed by `y * width + x`.

```
Frame { width, height, data: PixelData }

PixelData::RGBA(r: Vec<u8>, g: Vec<u8>, b: Vec<u8>, a: Vec<u8>)
                 └── all length = width * height
```

The planar layout is what makes the filter VM work: each channel program reads and writes one plane at a time without touching the others.

### `PixelData`
Four formats are supported:

| Variant | Layout | Use |
|---|---|---|
| `RGB(r, g, b)` | 3 planes | Standard colour |
| `RGBA(r, g, b, a)` | 4 planes | Standard colour with alpha — the primary working format |
| `GRAY(l)` | 1 plane | Grayscale |
| `YUV420(y, u, v)` | 3 planes, u/v subsampled | Video decode output; most frame operations reject it with `YUVNotApplied` |

`YUV420` is a transport format — it arrives from the video decoder and must be converted to RGBA via `to_rgba8()` before any filter or pipeline operation can touch it. `to_rgba8` applies integer BT.601 YCbCr→RGB conversion.

`interleave()` produces a packed byte stream (RGBRGB… or RGBARGBA…) for IO; `ffmpeg_fmt()` returns the FFmpeg pixel format string for the encoder.

### `Color` and `Pos`
`Color` is the per-pixel value type used at the API boundary — `get_pixel` returns one, `set_pixel` takes one. It matches `PixelData` in format (RGB, RGBA, Gray) and the two must agree or `set_pixel` returns `InvalidPixelFormat`. `Pos(x, y)` is a plain coordinate pair.

### `Frame` operations
`Frame::new` validates that `data.len() == width * height` before constructing. Pixel access uses `pixel_index(pos)` which computes `y * width + x` and bounds-checks it.

Beyond get/set, Frame supports: `crop` (copies a rect into a new Frame), `resize` (nearest-neighbour scaling), `blit` (hard copy of one frame onto another at a position), `blend_on` (alpha composite at a position), `blend` (full-frame linear interpolation between two same-size frames), and `normalize` (pads two frames to the same size by centering them on a black canvas so they can be blended).

---

## `track.rs` — Audio Data

### Architecture

A `Track` is a sequence of `AudioFrame`s — fixed-size chunks of PCM samples stored channel-by-channel.

```
Track { sample_rate, channels, buffer: Vec<AudioFrame> }

AudioFrame { time: TimeStamp, data: Vec<Vec<f32>> }
                               └── [channel_index][sample_index]
```

Each `AudioFrame` holds `channels` inner `Vec<f32>` of equal length (typically 1024 samples). This is the standard non-interleaved audio buffer layout. `time` is a `TimeStamp` recording when this chunk begins in the track.

Samples are floating-point and expected to stay in the `[-1.0, 1.0]` range for most operations, though no hard constraint is enforced internally.

### `TimeStamp`
Defined in `video.rs`, it is a rational time value (`num / den`) used for both audio frame timestamps and duration arithmetic. `Track::duration()` computes it by taking the last frame's timestamp and adding one frame's worth of time (`frame_size / sample_rate`).

### Track operations
The key operations are:

- **`gain(db)`** — multiplies every sample by `10^(db/20)`, the standard dB-to-linear conversion.
- **`normalize()`** — finds the peak absolute sample across the entire buffer and divides everything by it, so the loudest point hits exactly ±1.0.
- **`mix(a, b)`** — sample-wise addition of two tracks; both must have identical channel count, sample rate, and frame count.
- **`merge(a, b)`** — concatenates `b` after `a` in time by offsetting `b`'s frame timestamps by `a.duration()`.
- **`merge_many(tracks)`** — generalises merge to a slice of tracks.
- **`slice(start, end)`** — extracts a time range from the buffer, re-zeroing timestamps relative to `start`.
- **`silence(duration, sr, channels)`** — constructs a buffer of zeroed 1024-sample frames covering the given duration.
- **`to_pcm_f32()` / `to_pcm_i16()`** — flatten the entire buffer into a single interleaved stream, clamped to `[-1.0, 1.0]`, for WAV export.

The private `interleaved()` iterator powers both PCM export methods. It walks frame by frame and within each frame interleaves samples by channel index (`ch[0][i], ch[1][i], ch[0][i+1], …`).