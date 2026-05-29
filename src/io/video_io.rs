use crate::{
    io::io::IOError,
    media::{
        frame::{Frame, PixelData},
        track::{AudioFrame, Track},
        video::{TimeStamp, VideoFrame},
    },
};
use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::format::pixel::Pixel;
use ffmpeg_next as ffmpeg;

// ─── Video (Decoder) ──────────────────────────────────────────────────────────
//
// Holds the open demuxer + decoder and a cursor that advances on every
// decode_next() call.  The caller never touches PTS, time-bases, or rationals.
//
// Typical usage:
//
//   let mut video = Video::open("input.mp4")?;
//   while let Some(frame) = video.decode_next()? {
//       // work on frame.frame  (a plain Frame)
//   }
//
//   // — or — jump to an arbitrary frame:
//   let frame = video.decode_frame(120)?;   // frame index, not PTS

pub struct Video {
    // FFmpeg internals — never exposed
    ictx: ffmpeg::format::context::Input,
    decoder: ffmpeg::decoder::Video,
    video_index: usize,
    time_base: ffmpeg::Rational,  // kept for PTS ↔ index math only
    frame_rate: ffmpeg::Rational, // kept for PTS ↔ index math only

    // Public metadata (read via accessors)
    width: u32,
    height: u32,
    frame_count: u64, // 0 if container doesn't report duration
    fps: f64,
}

// Convert a raw fps float into a clean ffmpeg::Rational with a denominator
// that MPEG4 (and every other codec) can handle — max denominator 1001.
// Recognises NTSC drop-frame rates (23.976, 29.97, 59.94) and all integer
// frame rates.  Anything unrecognised is rounded to the nearest integer fps.

fn normalize_frame_rate(fps: f64) -> ffmpeg::Rational {
    if fps <= 0.0 || fps.is_nan() || fps.is_infinite() {
        return ffmpeg::Rational::new(30, 1);
    }

    // Clamp absurd values
    let fps = fps.clamp(1.0, 240.0);

    // Common exact NTSC rates
    const NTSC: &[(f64, i32, i32)] = &[
        (23.976023976, 24000, 1001),
        (29.97002997, 30000, 1001),
        (47.95204795, 48000, 1001),
        (59.94005994, 60000, 1001),
        (119.88011988, 120000, 1001),
    ];

    for &(target, num, den) in NTSC {
        if (fps - target).abs() < 0.0005 {
            return ffmpeg::Rational::new(num, den);
        }
    }

    // Exact integers
    if (fps - fps.round()).abs() < 0.00001 {
        return ffmpeg::Rational::new(fps.round() as i32, 1);
    }

    // Preserve precise common fractional rates
    // Use denominator 1000 for sane precision
    let num = (fps * 1000.0).round() as i32;
    let den = 1000;

    // Reduce fraction
    fn gcd(mut a: i32, mut b: i32) -> i32 {
        while b != 0 {
            let t = b;
            b = a % b;
            a = t;
        }
        a.abs()
    }

    let g = gcd(num, den);

    ffmpeg::Rational::new(num / g, den / g)
}

impl Video {
    /// Open a video file. Returns `IOError::FFmpegError` if the file can't be
    /// read or has no video stream.
    pub fn open(path: &str) -> Result<Self, IOError> {
        ffmpeg_next::init().map_err(|_| IOError::FFmpegError)?;

        let ictx = ffmpeg::format::input(&path).map_err(|_| IOError::FFmpegError)?;

        let stream = ictx
            .streams()
            .best(ffmpeg::media::Type::Video)
            .ok_or(IOError::FFmpegError)?;

        let video_index = stream.index();
        let time_base = stream.time_base();
        let avg_rate = stream.avg_frame_rate();

        let fps_raw = if avg_rate.denominator() != 0 {
            avg_rate.numerator() as f64 / avg_rate.denominator() as f64
        } else {
            30.0
        };
        let frame_rate = normalize_frame_rate(fps_raw);
        let fps = frame_rate.numerator() as f64 / frame_rate.denominator().max(1) as f64;

        // frame_count from stream duration when available
        let frame_count = if stream.duration() > 0 && frame_rate.denominator() > 0 {
            // duration is in stream time_base units; convert to seconds then to frames
            let dur_secs = stream.duration() as f64 * time_base.numerator() as f64
                / time_base.denominator().max(1) as f64;
            (dur_secs * fps).round() as u64
        } else {
            0
        };

        let decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            .map_err(|_| IOError::FFmpegError)?
            .decoder()
            .video()
            .map_err(|_| IOError::FFmpegError)?;

        let width = decoder.width();
        let height = decoder.height();

        Ok(Self {
            ictx,
            decoder,
            video_index,
            time_base,
            frame_rate,
            width,
            height,
            frame_count,
            fps,
        })
    }

    // ── Metadata ──────────────────────────────────────────────────────────────

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn fps(&self) -> f64 {
        self.fps
    }
    /// Total frame count. 0 means the container didn't report duration.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    // ── Decoding ──────────────────────────────────────────────────────────────

    /// Decode and return the next frame in display order, advancing the internal
    /// cursor.  Returns `None` at end of stream.
    ///
    /// You can call this, do other work, then call it again — the cursor is
    /// preserved inside the `Video` struct.
    pub fn decode_next(&mut self) -> Result<Option<Frame>, IOError> {
        let mut ff_frame = ffmpeg::util::frame::Video::empty();

        loop {
            match self.decoder.receive_frame(&mut ff_frame) {
                Ok(()) => return Ok(Some(self.convert_to_frame(&ff_frame)?)),

                Err(e)
                    if e == ffmpeg_next::Error::Other {
                        errno: ffmpeg::error::EAGAIN,
                    } =>
                {
                    // Need more data — feed one video packet
                    if !self.feed_next_video_packet()? {
                        // Demuxer exhausted; flush decoder
                        self.decoder.send_eof().map_err(|_| IOError::FFmpegError)?;
                        return match self.decoder.receive_frame(&mut ff_frame) {
                            Ok(()) => Ok(Some(self.convert_to_frame(&ff_frame)?)),
                            Err(_) => Ok(None),
                        };
                    }
                }

                Err(_) => return Ok(None),
            }
        }
    }

    /// Seek to the nearest keyframe at or before `frame_index`, then decode
    /// forward until the exact frame is reached.  Returns `None` if the index
    /// is beyond the end of the video.
    ///
    /// After this call the cursor sits right after the returned frame, so
    /// subsequent `decode_next()` calls continue from there.
    pub fn decode_frame(&mut self, frame_index: u64) -> Result<Option<Frame>, IOError> {
        let target_pts = self.index_to_pts(frame_index);

        self.ictx
            .seek(target_pts, ..target_pts)
            .map_err(|_| IOError::FFmpegError)?;
        self.decoder.flush();

        loop {
            match self.decode_next()? {
                None => return Ok(None),
                Some(frame) => {
                    // We don't expose PTS on Frame, so we just return the first
                    // frame that arrives after the seek — which is the keyframe
                    // at or just before the target.  For exact frame-accurate
                    // seeks, the caller can use decode_next() to step forward
                    // the remaining few frames from the keyframe.
                    return Ok(Some(frame));
                }
            }
        }
    }

    /// Decode the entire audio track embedded in this video file.
    /// Returns a `Track` with planar f32 channel data.
    ///
    /// Note: Call this *before* any `decode_next()` / `decode_frame()` calls,
    /// or *after* you're done decoding video — it iterates all packets in the
    /// demuxer independently.
    pub fn decode_audio(&mut self) -> Result<Track, IOError> {
        let stream = self
            .ictx
            .streams()
            .best(ffmpeg::media::Type::Audio)
            .ok_or(IOError::FFmpegError)?;

        let audio_index = stream.index();
        let audio_tb = stream.time_base();

        let mut audio_decoder =
            ffmpeg::codec::context::Context::from_parameters(stream.parameters())
                .map_err(|_| IOError::FFmpegError)?
                .decoder()
                .audio()
                .map_err(|_| IOError::FFmpegError)?;

        let sample_rate = audio_decoder.rate();
        let mut buffer: Vec<AudioFrame> = Vec::new();
        let mut decoded = ffmpeg::util::frame::Audio::empty();

        for (stream, packet) in self.ictx.packets() {
            if stream.index() != audio_index {
                continue;
            }
            audio_decoder
                .send_packet(&packet)
                .map_err(|_| IOError::FFmpegError)?;

            loop {
                match audio_decoder.receive_frame(&mut decoded) {
                    Ok(()) => {
                        let ts = TimeStamp {
                            value: decoded.pts().unwrap_or(0),
                            num: audio_tb.numerator() as u32,
                            den: audio_tb.denominator() as u32,
                        };
                        let n_ch = decoded.channels() as usize;
                        let data: Vec<Vec<f32>> = (0..n_ch)
                            .map(|ch| decoded.plane::<f32>(ch).to_vec())
                            .collect();
                        buffer.push(AudioFrame { time: ts, data });
                    }
                    Err(_) => break,
                }
            }
        }

        // Flush
        audio_decoder.send_eof().map_err(|_| IOError::FFmpegError)?;
        loop {
            match audio_decoder.receive_frame(&mut decoded) {
                Ok(()) => {
                    let ts = TimeStamp {
                        value: decoded.pts().unwrap_or(0),
                        num: audio_tb.numerator() as u32,
                        den: audio_tb.denominator() as u32,
                    };
                    let n_ch = decoded.channels() as usize;
                    let data: Vec<Vec<f32>> = (0..n_ch)
                        .map(|ch| decoded.plane::<f32>(ch).to_vec())
                        .collect();
                    buffer.push(AudioFrame { time: ts, data });
                }
                Err(_) => break,
            }
        }

        let channels = buffer.first().map_or(0, |f| f.data.len()) as u16;
        self.ictx.seek(0, ..0).map_err(|_| IOError::FFmpegError)?;
        self.decoder.flush();
        Ok(Track::new(sample_rate, channels, buffer))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Convert a frame index (0-based) to a PTS value in the stream time-base.
    fn index_to_pts(&self, index: u64) -> i64 {
        // pts = index * (time_base_den / time_base_num) / fps
        //     = index * time_base_den / (time_base_num * fps_num / fps_den)
        let tb_num = self.time_base.numerator() as f64;
        let tb_den = self.time_base.denominator() as f64;
        let fps = self.fps;
        if fps == 0.0 || tb_num == 0.0 {
            return 0;
        }
        // seconds = index / fps
        // pts = seconds / (tb_num / tb_den) = seconds * tb_den / tb_num
        ((index as f64 / fps) * tb_den / tb_num).round() as i64
    }

    /// Pull one video packet from the demuxer into the decoder.
    /// Returns false when the demuxer is exhausted.
    fn feed_next_video_packet(&mut self) -> Result<bool, IOError> {
        for (stream, packet) in self.ictx.packets() {
            if stream.index() == self.video_index {
                self.decoder
                    .send_packet(&packet)
                    .map_err(|_| IOError::FFmpegError)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Convert an FFmpeg video frame to Editron's planar RGBA `Frame`.
    /// Stride padding is stripped here — `Frame` always holds exactly
    /// `width * height` samples per channel, no padding.
    fn convert_to_frame(&self, src: &ffmpeg::util::frame::Video) -> Result<Frame, IOError> {
        let width = src.width();
        let height = src.height();

        let mut scaler = Context::get(
            src.format(),
            width,
            height,
            Pixel::RGBA,
            width,
            height,
            Flags::BILINEAR,
        )
        .map_err(|_| IOError::FFmpegError)?;

        let mut rgba_frame = ffmpeg::util::frame::Video::new(Pixel::RGBA, width, height);
        scaler
            .run(src, &mut rgba_frame)
            .map_err(|_| IOError::FFmpegError)?;

        let stride = rgba_frame.stride(0);
        let row_bytes = width as usize * 4;
        let data = rgba_frame.data(0);

        let pixel_count = (width * height) as usize;
        let mut r = Vec::with_capacity(pixel_count);
        let mut g = Vec::with_capacity(pixel_count);
        let mut b = Vec::with_capacity(pixel_count);
        let mut a = Vec::with_capacity(pixel_count);

        for row in 0..height as usize {
            let row_data = &data[row * stride..row * stride + row_bytes];
            for px in row_data.chunks_exact(4) {
                r.push(px[0]);
                g.push(px[1]);
                b.push(px[2]);
                a.push(px[3]);
            }
        }

        Frame::new(width, height, PixelData::RGBA(r, g, b, a)).map_err(|_| IOError::InvalidData)
    }
}

// ─── VideoEncoder ─────────────────────────────────────────────────────────────
//
// Encodes a sequence of `Frame`s into a video file.  Optionally muxes a
// `Track` as an AAC audio stream.  The caller never touches time-bases, PTS
// values, or stream indices — all of that is managed internally.
//
// Typical usage:
//
//   let source = Video::open("input.mp4")?;
//   let mut enc = VideoEncoder::open("output.mp4", &source)?;
//
//   while let Some(frame) = source.decode_next()? {
//       let frame = apply_filters(frame);
//       enc.encode_frame(&frame)?;
//   }
//
//   // Optional — if the source had audio:
//   let track = source.decode_audio()?;
//   enc.encode_audio(&track)?;
//
//   enc.finish()?;

pub struct VideoEncoder {
    octx: ffmpeg::format::context::Output,
    vid_encoder: ffmpeg::encoder::Video,
    vid_stream: usize,

    // Audio — only present when encode_audio() is called
    aud_encoder: Option<ffmpeg::encoder::Audio>,
    aud_stream: Option<usize>,

    // Internals
    frame_rate: ffmpeg::Rational,
    frame_index: i64, // monotonic counter, becomes PTS
}

impl VideoEncoder {
    /// Open an output file, copying all encoding parameters from a source
    /// `Video`.
    ///
    /// Pass `Some(&track)` if the output should contain audio — the audio
    /// stream is registered here, before `write_header`, which is the correct
    /// FFmpeg sequence.  Pass `None` for video-only output.
    ///
    /// The `Track` is only used here to read `sample_rate` and `channels`.
    /// The actual samples are sent later via `encode_audio(&track)`.
    pub fn open(path: &str, source: &Video, audio: Option<&Track>) -> Result<Self, IOError> {
        ffmpeg_next::init().map_err(|_| IOError::FFmpegError)?;

        let mut octx = ffmpeg::format::output(&path).map_err(|_| IOError::FFmpegError)?;

        let global_header = octx
            .format()
            .flags()
            .contains(ffmpeg::format::flag::Flags::GLOBAL_HEADER);

        // ── Video stream ──────────────────────────────────────────────────────
        // Standard: mpeg4 + YUV420P.
        // mpeg4 is present in every FFmpeg build — no external libs, no
        // platform-specific encoder selection (avoids h264_mf on Windows).
        let vid_codec =
            ffmpeg::encoder::find(ffmpeg::codec::Id::MPEG4).ok_or(IOError::FFmpegError)?;

        let mut enc_ctx = ffmpeg::codec::context::Context::new_with_codec(vid_codec)
            .encoder()
            .video()
            .map_err(|_| IOError::FFmpegError)?;

        let frame_rate = source.frame_rate;
        let enc_tb = ffmpeg::Rational::new(frame_rate.denominator(), frame_rate.numerator());

        enc_ctx.set_width(source.width);
        enc_ctx.set_height(source.height);
        enc_ctx.set_format(Pixel::YUV420P);
        enc_ctx.set_time_base(enc_tb);
        enc_ctx.set_frame_rate(Some(frame_rate));
        enc_ctx.set_bit_rate(8_000_000); // 8 Mbps — good quality for mpeg4
        if global_header {
            enc_ctx.set_flags(ffmpeg::codec::flag::Flags::GLOBAL_HEADER);
        }

        let vid_encoder = enc_ctx
            .open_as(vid_codec)
            .map_err(|_| IOError::FFmpegError)?;

        let mut vid_stream = octx
            .add_stream(vid_codec)
            .map_err(|_| IOError::FFmpegError)?;
        let vid_idx = vid_stream.index();
        vid_stream.set_parameters(&vid_encoder);

        // ── Audio stream (optional) ───────────────────────────────────────────
        // Standard: AAC + FLTP (planar f32).
        // AAC is built into FFmpeg — no external libs.
        // FLTP matches Track's internal Vec<Vec<f32>> directly, no conversion.
        // Must be added BEFORE write_header.
        let (aud_encoder, aud_stream) = if let Some(track) = audio {
            let aud_codec =
                ffmpeg::encoder::find(ffmpeg::codec::Id::AAC).ok_or(IOError::FFmpegError)?;

            let mut aud_ctx = ffmpeg::codec::context::Context::new_with_codec(aud_codec)
                .encoder()
                .audio()
                .map_err(|_| IOError::FFmpegError)?;

            let n_ch = track.channels() as i32;
            aud_ctx.set_rate(track.sample_rate() as i32);
            aud_ctx.set_format(ffmpeg::format::Sample::F32(
                ffmpeg::format::sample::Type::Planar,
            ));
            aud_ctx.set_channel_layout(ffmpeg::channel_layout::ChannelLayout::default(n_ch));
            aud_ctx.set_time_base(ffmpeg::Rational::new(1, track.sample_rate() as i32));

            let aud_encoder = aud_ctx
                .open_as(aud_codec)
                .map_err(|_| IOError::FFmpegError)?;

            let mut aud_stream = octx
                .add_stream(aud_codec)
                .map_err(|_| IOError::FFmpegError)?;
            let aud_idx = aud_stream.index();
            aud_stream.set_parameters(&aud_encoder);

            (Some(aud_encoder), Some(aud_idx))
        } else {
            (None, None)
        };

        // All streams registered — safe to write header now
        octx.write_header().map_err(|_| IOError::FFmpegError)?;

        Ok(Self {
            octx,
            vid_encoder,
            vid_stream: vid_idx,
            aud_encoder,
            aud_stream,
            frame_rate,
            frame_index: 0,
        })
    }

    /// Encode a single `Frame` into the video stream.
    ///
    /// The frame must be RGBA.  YUV420P conversion is done internally.
    /// PTS is assigned automatically from an internal monotonic counter.
    pub fn encode_frame(&mut self, frame: &Frame) -> Result<(), IOError> {
        let width = frame.width();
        let height = frame.height();

        if width % 2 != 0 || height % 2 != 0 {
            return Err(IOError::InvalidData);
        }

        let (r, g, b) = match frame.data() {
            PixelData::RGBA(r, g, b, _) => (r, g, b),
            _ => return Err(IOError::InvalidData),
        };

        let pixel_count = (width * height) as usize;
        if r.len() != pixel_count || g.len() != pixel_count || b.len() != pixel_count {
            return Err(IOError::InvalidData);
        }

        // Build RGBA src frame, respecting FFmpeg's stride padding
        let mut src = ffmpeg::util::frame::Video::new(Pixel::RGBA, width, height);
        {
            let stride = src.stride(0);
            let data = src.data_mut(0);
            for row in 0..height as usize {
                let row_start = row * stride;
                for col in 0..width as usize {
                    let i = row * width as usize + col;
                    let off = row_start + col * 4;
                    data[off] = r[i];
                    data[off + 1] = g[i];
                    data[off + 2] = b[i];
                    data[off + 3] = 255;
                }
            }
        }

        // Convert RGBA → YUV420P via swscale
        let mut scaler = Context::get(
            Pixel::RGBA,
            width,
            height,
            Pixel::YUV420P,
            width,
            height,
            Flags::BILINEAR,
        )
        .map_err(|_| IOError::FFmpegError)?;

        let mut yuv = ffmpeg::util::frame::Video::new(Pixel::YUV420P, width, height);
        scaler
            .run(&src, &mut yuv)
            .map_err(|_| IOError::FFmpegError)?;

        yuv.set_pts(Some(self.frame_index));
        self.frame_index += 1;

        self.vid_encoder
            .send_frame(&yuv)
            .map_err(|_| IOError::FFmpegError)?;

        self.drain_video_packets()
    }

    /// Encode an entire `Track` as an AAC audio stream and mux it into the
    /// output file.
    ///
    /// Call this after all `encode_frame()` calls and before `finish()`.
    /// The track's planar f32 samples are chunked into AAC frame-sized
    /// batches (1024 samples) automatically.
    ///
    /// Requires that `open()` was called with `Some(&track)` — the audio
    /// stream must be registered before `write_header`.

    pub fn encode_audio(&mut self, track: &Track) -> Result<(), IOError> {
        if self.aud_encoder.is_none() {
            return Err(IOError::FFmpegError);
        }

        let n_ch = track.channels() as usize;

        // ── Flatten AudioFrames into contiguous planar buffers ────────────────
        let mut flat_channels: Vec<Vec<f32>> = vec![Vec::new(); n_ch];

        for af in track.buffer() {
            for (ch, ch_data) in af.data.iter().enumerate() {
                if ch < n_ch {
                    flat_channels[ch].extend_from_slice(ch_data);
                }
            }
        }

        let total_samples = flat_channels.first().map_or(0, |c| c.len());

        if total_samples == 0 {
            return Ok(());
        }

        let aud_encoder = self.aud_encoder.as_mut().unwrap();
        let aud_stream_idx = self.aud_stream.unwrap();

        let frame_size = aud_encoder.frame_size() as usize;

        let sample_rate = track.sample_rate();

        let enc_tb = ffmpeg::Rational::new(1, sample_rate as i32);

        let out_tb = self.octx.stream(aud_stream_idx).unwrap().time_base();

        let mut pts: i64 = 0;
        let mut offset = 0usize;

        while offset < total_samples {
            let remaining = total_samples - offset;
            let chunk_len = remaining.min(frame_size);

            let mut ff_audio = ffmpeg::util::frame::Audio::new(
                ffmpeg::format::Sample::F32(ffmpeg::format::sample::Type::Planar),
                frame_size,
                ffmpeg::channel_layout::ChannelLayout::default(n_ch as i32),
            );

            ff_audio.set_rate(sample_rate);

            // ── Fill channel planes ────────────────────────────────────────────
            for ch in 0..n_ch {
                let plane: &mut [f32] = ff_audio.plane_mut(ch);

                // Copy actual audio samples
                plane[..chunk_len].copy_from_slice(&flat_channels[ch][offset..offset + chunk_len]);

                // Zero-pad remaining samples
                for sample in &mut plane[chunk_len..frame_size] {
                    *sample = 0.0;
                }
            }

            ff_audio.set_pts(Some(pts));

            pts += frame_size as i64;

            aud_encoder
                .send_frame(&ff_audio)
                .map_err(|_| IOError::FFmpegError)?;

            // ── Drain packets immediately ─────────────────────────────────────
            let mut pkt = ffmpeg::codec::packet::Packet::empty();

            loop {
                match aud_encoder.receive_packet(&mut pkt) {
                    Ok(()) => {
                        pkt.set_stream(aud_stream_idx);

                        pkt.rescale_ts(enc_tb, out_tb);

                        pkt.write_interleaved(&mut self.octx)
                            .map_err(|_| IOError::FFmpegError)?;
                    }

                    Err(ffmpeg::Error::Other {
                        errno: ffmpeg::error::EAGAIN,
                    }) => {
                        break;
                    }

                    Err(_) => {
                        break;
                    }
                }
            }

            offset += chunk_len;
        }

        Ok(())
    }

    /// Flush both encoders and write the container trailer.
    /// Must be called exactly once, after all frames and audio have been sent.
    pub fn finish(&mut self) -> Result<(), IOError> {
        // Flush video
        self.vid_encoder
            .send_eof()
            .map_err(|_| IOError::FFmpegError)?;
        self.drain_video_packets()?;

        // Flush audio if it was used
        if let Some(aud_enc) = self.aud_encoder.as_mut() {
            aud_enc.send_eof().map_err(|_| IOError::FFmpegError)?;

            let aud_stream_idx = self.aud_stream.unwrap();
            let aud_tb = self.octx.stream(aud_stream_idx).unwrap().time_base();
            let mut pkt = ffmpeg::codec::packet::Packet::empty();
            loop {
                match aud_enc.receive_packet(&mut pkt) {
                    Ok(()) => {
                        pkt.set_stream(aud_stream_idx);

                        let out_tb = self.octx.stream(aud_stream_idx).unwrap().time_base();

                        pkt.rescale_ts(aud_tb, out_tb);

                        // already in output tb
                        pkt.write_interleaved(&mut self.octx)
                            .map_err(|_| IOError::FFmpegError)?;
                    }
                    Err(_) => break,
                }
            }
        }

        self.octx
            .write_trailer()
            .map_err(|_| IOError::FFmpegError)?;
        Ok(())
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn drain_video_packets(&mut self) -> Result<(), IOError> {
        let enc_tb =
            ffmpeg::Rational::new(self.frame_rate.denominator(), self.frame_rate.numerator());
        let out_tb = self.octx.stream(self.vid_stream).unwrap().time_base();
        let mut pkt = ffmpeg::codec::packet::Packet::empty();

        loop {
            match self.vid_encoder.receive_packet(&mut pkt) {
                Ok(()) => {
                    pkt.set_stream(self.vid_stream);
                    pkt.rescale_ts(enc_tb, out_tb);
                    pkt.write_interleaved(&mut self.octx)
                        .map_err(|_| IOError::FFmpegError)?;
                }
                Err(_) => break,
            }
        }
        Ok(())
    }
}
