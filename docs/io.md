# IO — `io.rs`

The IO layer is Drive's only contact with the filesystem. It translates between external file formats and the internal `Frame`/`Track` types. Everything else in the pipeline operates purely on those types; this module is where bytes enter and leave.

---

## Image IO

**`load_image(path, fmt) -> Result<Frame, FrameError>`**
Opens any format the `image` crate supports, then de-interleaves the raw pixel bytes into Drive's planar `PixelData` layout. The `fmt` argument selects the target format: `"rgb"`, `"rgba"`, `"gray"`, or `"yuv420"`. For `yuv420`, the image is first loaded as RGB and then converted using BT.601 coefficients with 4:2:0 chroma subsampling (one U/V pair averaged from each 2×2 pixel block).

**`encode_image(frame, path) -> Result<(), IOError>`**
Exports any `Frame` as a PNG. Internally calls `to_rgba8()` on the frame's `PixelData` to normalise all formats to RGBA, then `interleave()` to produce the packed byte stream the PNG encoder expects.

---

## Audio IO

**`decode_audio(path) -> Result<Track, AudioDecodeError>`**
Decodes any audio file Symphonia supports (MP3, FLAC, OGG, WAV, etc.). Uses Symphonia's probe/format/decoder pipeline: probes the file format, finds the first valid audio track, then loops over packets decoding each one into an `AudioFrame`. Timestamps are preserved from the stream's timebase as `TimeStamp { value, num, den }`. Samples are extracted as planar `f32` via `extract_planar_f32`, which handles both native F32 buffers and any other format by converting through a `SampleBuffer<f32>`. Broken packets are skipped; IO/reset errors terminate the loop cleanly.

**`encode_wav(track, path)`** — writes a 32-bit float WAV, interleaving samples across channels per frame. Ragged channels (unequal lengths) are padded with silence rather than panicking.

**`encode_wav_i16(track, path)`** — same structure but writes 16-bit integer PCM, scaling `f32 [-1, 1]` to `i16` range.

---

## Error Types

| Type | Variants |
|---|---|
| `IOError` | `FileNotFound`, `InvalidData`, `EncodingFailed`, `FFmpegError`, `FFmpegDecodingFailed`, `ReelError` |
| `AudioDecodeError` | `Io`, `NoAudioTrack`, `UnsupportedFormat`, `Symphonia` |
| `WavEncodeError` | `Io`, `Hound`, `EmptyTrack` |

`AudioDecodeError` and `WavEncodeError` implement `From` for their underlying error types for `?` propagation.