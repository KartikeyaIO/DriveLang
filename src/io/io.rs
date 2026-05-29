use crate::media::frame::Frame;
use crate::media::frame::{FrameError, PixelData};
use crate::media::track::{AudioFrame, Track};
use crate::media::video::TimeStamp;
use crate::media::video::VideoFrame;

use ffmpeg::software::scaling::{context::Context, flag::Flags};
use ffmpeg::util::format::pixel::Pixel;
use ffmpeg_next as ffmpeg;

use ffmpeg_next::format::context::Input as FfmpegInput;
use hound::{SampleFormat, WavSpec, WavWriter};
use image::GenericImageView;
use image::codecs::png::PngEncoder;
use image::io::Reader;
use image::{ExtendedColorType, ImageEncoder};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::default;

#[derive(Debug)]
pub enum IOError {
    FileNotFound,
    InvalidData,
    EncodingFailed,
    FFmpegError,
    FFmpegDecodingFailed,
    ReelError,
}

pub fn load_image(path: &str, fmt: &str) -> Result<Frame, FrameError> {
    let img = Reader::open(path)
        .map_err(|_| FrameError::EmptyFrame)?
        .decode()
        .map_err(|_| FrameError::EmptyFrame)?;

    let (width, height) = img.dimensions();
    let pixel_count = (width * height) as usize;

    let data = match fmt.to_lowercase().as_str() {
        "rgb" => {
            let rgb = img.to_rgb8();
            let raw = rgb.as_raw();

            let mut r = Vec::with_capacity(pixel_count);
            let mut g = Vec::with_capacity(pixel_count);
            let mut b = Vec::with_capacity(pixel_count);

            for chunk in raw.chunks_exact(3) {
                r.push(chunk[0]);
                g.push(chunk[1]);
                b.push(chunk[2]);
            }

            PixelData::RGB(r, g, b)
        }

        "rgba" => {
            let rgba = img.to_rgba8();
            let raw = rgba.as_raw();

            let mut r = Vec::with_capacity(pixel_count);
            let mut g = Vec::with_capacity(pixel_count);
            let mut b = Vec::with_capacity(pixel_count);
            let mut a = Vec::with_capacity(pixel_count);

            for chunk in raw.chunks_exact(4) {
                r.push(chunk[0]);
                g.push(chunk[1]);
                b.push(chunk[2]);
                a.push(chunk[3]);
            }

            PixelData::RGBA(r, g, b, a)
        }

        "gray" | "l8" => {
            let gray = img.to_luma8();
            PixelData::GRAY(gray.into_raw())
        }

        "yuv420" => {
            // Enforce even dimensions
            if width % 2 != 0 || height % 2 != 0 {
                return Err(FrameError::InvalidFrameSize);
            }

            let rgb = img.to_rgb8();
            let raw = rgb.as_raw();

            let mut y_plane = vec![0u8; (width * height) as usize];
            let mut u_plane = vec![0u8; ((width / 2) * (height / 2)) as usize];
            let mut v_plane = vec![0u8; ((width / 2) * (height / 2)) as usize];

            // --- Y PLANE ---
            for y in 0..height {
                for x in 0..width {
                    let idx = ((y * width + x) * 3) as usize;

                    let r = raw[idx] as f32;
                    let g = raw[idx + 1] as f32;
                    let b = raw[idx + 2] as f32;

                    let y_val = (0.299 * r + 0.587 * g + 0.114 * b).clamp(0.0, 255.0) as u8;

                    y_plane[(y * width + x) as usize] = y_val;
                }
            }

            // --- U & V (4:2:0 subsampling) ---
            for y in (0..height).step_by(2) {
                for x in (0..width).step_by(2) {
                    let mut u_sum = 0.0;
                    let mut v_sum = 0.0;

                    // 2x2 block
                    for dy in 0..2 {
                        for dx in 0..2 {
                            let px = x + dx;
                            let py = y + dy;

                            let idx = ((py * width + px) * 3) as usize;

                            let r = raw[idx] as f32;
                            let g = raw[idx + 1] as f32;
                            let b = raw[idx + 2] as f32;

                            u_sum += -0.169 * r - 0.331 * g + 0.5 * b + 128.0;
                            v_sum += 0.5 * r - 0.419 * g - 0.081 * b + 128.0;
                        }
                    }

                    let index = ((y / 2) * (width / 2) + (x / 2)) as usize;

                    u_plane[index] = (u_sum * 0.25).clamp(0.0, 255.0) as u8;
                    v_plane[index] = (v_sum * 0.25).clamp(0.0, 255.0) as u8;
                }
            }

            PixelData::YUV420(y_plane, u_plane, v_plane)
        }

        _ => return Err(FrameError::InvalidPixelFormat),
    };

    Frame::new(width, height, data)
}

pub fn encode_image(frame: &Frame, path: &str) -> Result<(), IOError> {
    let file = match File::create(path) {
        Ok(val) => val,
        Err(_) => return Err(IOError::FileNotFound),
    };
    let writer = BufWriter::new(file);
    let encoder = PngEncoder::new(writer);
    let width = frame.width();
    let height = frame.height();
    let data = match frame.data().to_rgba8(width, height) {
        Ok(val) => val,
        Err(_) => return Err(IOError::InvalidData),
    };

    encoder
        .write_image(
            &data.interleave()[..],
            width,
            height,
            ExtendedColorType::Rgba8,
        )
        .map_err(|_| IOError::EncodingFailed)
}

// fn copy_plane(
//     frame: &ffmpeg::util::frame::Video,
//     plane: usize,
//     width: usize,
//     height: usize,
// ) -> Vec<u8> {
//     let stride = frame.stride(plane);
//     let data = frame.data(plane);
//     let mut out = Vec::with_capacity(width * height);
//     for row in 0..height {
//         let start = row * stride;
//         out.extend_from_slice(&data[start..start + width]);
//     }
//     out
// }

// Audio Handling

#[derive(Debug)]
pub enum AudioDecodeError {
    Io(std::io::Error),
    NoAudioTrack,
    UnsupportedFormat,
    Symphonia(SymphoniaError),
}

impl From<std::io::Error> for AudioDecodeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<SymphoniaError> for AudioDecodeError {
    fn from(e: SymphoniaError) -> Self {
        Self::Symphonia(e)
    }
}

pub fn decode_audio(path: &Path) -> Result<Track, AudioDecodeError> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL)
        .ok_or(AudioDecodeError::NoAudioTrack)?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params.sample_rate.unwrap_or(44100);
    let tb_num = codec_params.time_base.map_or(1, |tb| tb.numer);
    let tb_den = codec_params.time_base.map_or(1, |tb| tb.denom);

    let mut decoder = default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|_| AudioDecodeError::UnsupportedFormat)?;

    let mut frames: Vec<AudioFrame> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(_)) | Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(AudioDecodeError::Symphonia(e)),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(buf) => buf,
            Err(SymphoniaError::DecodeError(_)) => continue, // skip broken packets
            Err(e) => return Err(AudioDecodeError::Symphonia(e)),
        };

        // Timestamp for this packet using the stream's timebase
        let ts = TimeStamp {
            value: packet.ts() as i64,
            num: tb_num,
            den: tb_den,
        };

        let channel_data = extract_planar_f32(&decoded);

        frames.push(AudioFrame {
            time: ts,
            data: channel_data,
        });
    }

    let channels = frames.first().map_or(0, |f| f.data.len()) as u16;

    Ok(Track::new(sample_rate, channels, frames))
}

fn extract_planar_f32(buf: &AudioBufferRef<'_>) -> Vec<Vec<f32>> {
    use symphonia::core::audio::SampleBuffer;

    if let AudioBufferRef::F32(b) = buf {
        let planes = b.planes();
        return planes.planes().iter().map(|ch| ch.to_vec()).collect();
    }

    let frames = buf.frames();
    let spec = *buf.spec();
    let n_ch = spec.channels.count();

    let mut sample_buf = SampleBuffer::<f32>::new(frames as u64, spec);
    sample_buf.copy_planar_ref(buf.clone());
    let flat = sample_buf.samples();

    flat.chunks(frames)
        .map(|ch| ch.to_vec())
        .collect::<Vec<_>>()
        .into_iter()
        .take(n_ch)
        .collect()
}
// src/media/audio/encoder.rs

#[derive(Debug)]
pub enum WavEncodeError {
    Io(std::io::Error),
    Hound(hound::Error),
    EmptyTrack,
}

impl From<std::io::Error> for WavEncodeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<hound::Error> for WavEncodeError {
    fn from(e: hound::Error) -> Self {
        Self::Hound(e)
    }
}

pub fn encode_wav(track: &Track, path: &Path) -> Result<(), WavEncodeError> {
    if track.buffer().is_empty() {
        return Err(WavEncodeError::EmptyTrack);
    }

    let spec = WavSpec {
        channels: track.channels(),
        sample_rate: track.sample_rate(),
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    let mut writer = WavWriter::create(path, spec)?;

    for frame in track.buffer() {
        // Determine how many samples are in each channel for this frame.
        let n_samples = frame.data.first().map_or(0, |ch| ch.len());

        // Interleave: emit one sample per channel before advancing to next sample.
        for i in 0..n_samples {
            for channel in &frame.data {
                // Guard against ragged channels — pad with silence rather than panic.
                let sample = channel.get(i).copied().unwrap_or(0.0);
                writer.write_sample(sample)?;
            }
        }
    }

    writer.finalize()?;
    Ok(())
}

pub fn encode_wav_i16(track: &Track, path: &Path) -> Result<(), WavEncodeError> {
    if track.buffer().is_empty() {
        return Err(WavEncodeError::EmptyTrack);
    }

    let spec = WavSpec {
        channels: track.channels(),
        sample_rate: track.sample_rate(),
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec)?;

    for frame in track.buffer() {
        let n_samples = frame.data.first().map_or(0, |ch| ch.len());
        for i in 0..n_samples {
            for channel in &frame.data {
                let s = channel.get(i).copied().unwrap_or(0.0);
                let pcm = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                writer.write_sample(pcm)?;
            }
        }
    }

    writer.finalize()?;
    Ok(())
}
