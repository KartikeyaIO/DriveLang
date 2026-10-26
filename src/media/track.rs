use crate::media::video::TimeStamp;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub time: TimeStamp,
    pub data: Vec<Vec<f32>>, // [channel_index][sample_index]
}

#[derive(Debug, Clone)]
pub struct Track {
    sample_rate: u32,
    channels: u16,
    buffer: Vec<AudioFrame>,
}

#[derive(Debug)]
pub enum TrackError {
    MixingIsNotPossible,
}

impl std::fmt::Display for TrackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackError::MixingIsNotPossible => {
                write!(f, "Mixing is not possible: sample rates or channels do not match")
            }
        }
    }
}

impl std::error::Error for TrackError {}

// ── Constructors & Accessors ─────────────────────────────────────────────────

impl Track {
    pub fn new(sample_rate: u32, channels: u16, buffer: Vec<AudioFrame>) -> Self {
        Self {
            sample_rate,
            channels,
            buffer,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    pub fn channels(&self) -> u16 {
        self.channels
    }
    pub fn buffer(&self) -> &[AudioFrame] {
        &self.buffer
    }
}

impl Track {
    /// Apply a gain (in dB) to every sample in every frame.
    pub fn gain(&mut self, db: f32) {
        let factor = 10.0_f32.powf(db / 20.0);
        for frame in &mut self.buffer {
            for channel in &mut frame.data {
                for sample in channel {
                    *sample *= factor;
                }
            }
        }
    }

    /// Mix two tracks sample-by-sample. Both must have identical shape.
    pub fn mix(a: &Track, b: &Track) -> Result<Track, TrackError> {
        if a.channels != b.channels
            || a.sample_rate != b.sample_rate
            || a.buffer.len() != b.buffer.len()
        {
            return Err(TrackError::MixingIsNotPossible);
        }

        let buffer = a
            .buffer
            .iter()
            .zip(b.buffer.iter())
            .map(|(fa, fb)| {
                let data = fa
                    .data
                    .iter()
                    .zip(fb.data.iter())
                    .map(|(ca, cb)| ca.iter().zip(cb.iter()).map(|(sa, sb)| sa + sb).collect())
                    .collect();

                AudioFrame {
                    time: fa.time.clone(),
                    data,
                }
            })
            .collect();

        Ok(Track::new(a.sample_rate, a.channels, buffer))
    }

    /// Flatten all frames/channels into an interleaved f32 PCM stream.
    pub fn to_pcm_f32(&self) -> Vec<f32> {
        self.interleaved().map(|s| s.clamp(-1.0, 1.0)).collect()
    }

    /// Flatten all frames/channels into an interleaved i16 PCM stream.
    pub fn to_pcm_i16(&self) -> Vec<i16> {
        self.interleaved()
            .map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect()
    }

    /// Normalize the track in-place so the loudest sample hits 1.0 or -1.0.
    /// Returns false if the buffer is empty or already silent.
    pub fn normalize(&mut self) -> bool {
        let max = self
            .buffer
            .iter()
            .flat_map(|f| f.data.iter().flat_map(|ch| ch.iter()))
            .map(|s| s.abs())
            .max_by(f32::total_cmp);

        match max {
            Some(peak) if peak > 0.0 => {
                for frame in &mut self.buffer {
                    for channel in &mut frame.data {
                        for sample in channel {
                            *sample /= peak;
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Iterator over every sample, interleaved by channel across all frames.
    fn interleaved(&self) -> impl Iterator<Item = f32> + '_ {
        self.buffer.iter().flat_map(|frame| {
            // For each frame, yield samples interleaved
            let len = frame.data.first().map_or(0, |ch| ch.len());
            (0..len).flat_map(move |i| frame.data.iter().map(move |ch| ch[i]))
        })
    }
    pub fn slice(&self, start: TimeStamp, end: TimeStamp) -> Self {
        assert!(start < end);

        let buffer = self
            .buffer
            .iter()
            .filter(|frame| frame.time >= start && frame.time < end)
            .map(|frame| AudioFrame {
                time: frame.time.sub(start),
                data: frame.data.clone(),
            })
            .collect();

        Track::new(self.sample_rate, self.channels, buffer)
    }
    pub fn merge(a: &Track, b: &Track) -> Result<Track, TrackError> {
        if a.channels != b.channels || a.sample_rate != b.sample_rate {
            return Err(TrackError::MixingIsNotPossible);
        }

        let offset = a.duration();

        let mut buffer = a.buffer.clone();

        buffer.extend(b.buffer.iter().map(|frame| AudioFrame {
            time: frame.time.add(offset),
            data: frame.data.clone(),
        }));

        Ok(Track::new(a.sample_rate, a.channels, buffer))
    }
    pub fn merge_many(tracks: &[Track]) -> Result<Track, TrackError> {
        if tracks.is_empty() {
            return Ok(Track::new(44100, 2, vec![]));
        }

        let sample_rate = tracks[0].sample_rate;
        let channels = tracks[0].channels;

        let mut offset = TimeStamp::zero_like(tracks[0].buffer[0].time);
        let mut buffer = Vec::new();

        for track in tracks {
            if track.sample_rate != sample_rate || track.channels != channels {
                return Err(TrackError::MixingIsNotPossible);
            }

            for frame in &track.buffer {
                buffer.push(AudioFrame {
                    time: frame.time.add(offset),
                    data: frame.data.clone(),
                });
            }

            offset = offset.add(track.duration());
        }

        Ok(Track::new(sample_rate, channels, buffer))
    }
    pub fn fade_in(&mut self, duration: TimeStamp) {
        let fade_secs = duration.to_seconds();

        for frame in &mut self.buffer {
            let start = frame.time.to_seconds();

            for i in 0..frame.data[0].len() {
                let t = start + i as f64 / self.sample_rate as f64;

                let gain = if t >= fade_secs {
                    1.0
                } else {
                    (t / fade_secs) as f32
                };

                for channel in &mut frame.data {
                    channel[i] *= gain;
                }
            }
        }
    }
    pub fn fade_out(&mut self, duration: TimeStamp) {
        let total = self.duration().to_seconds();
        let fade_secs = duration.to_seconds();

        for frame in &mut self.buffer {
            let start = frame.time.to_seconds();

            for i in 0..frame.data[0].len() {
                let t = start + i as f64 / self.sample_rate as f64;

                let gain = if t <= total - fade_secs {
                    1.0
                } else {
                    ((total - t) / fade_secs).clamp(0.0, 1.0) as f32
                };

                for channel in &mut frame.data {
                    channel[i] *= gain;
                }
            }
        }
    }
    pub fn silence(duration: TimeStamp, sample_rate: u32, channels: u16) -> Self {
        const FRAME_SIZE: usize = 1024;

        let frame_duration = TimeStamp::from_seconds(
            FRAME_SIZE as f64 / sample_rate as f64,
            duration.num,
            duration.den,
        );

        let mut current = TimeStamp::zero_like(duration);
        let mut buffer = Vec::new();

        while current < duration {
            buffer.push(AudioFrame {
                time: current,
                data: vec![vec![0.0; FRAME_SIZE]; channels as usize],
            });

            current = current.add(frame_duration);
        }

        Track::new(sample_rate, channels, buffer)
    }
}
impl Track {
    pub fn buffer_mut(&mut self) -> &mut [AudioFrame] {
        &mut self.buffer
    }
    pub fn duration(&self) -> TimeStamp {
        let Some(last) = self.buffer.last() else {
            return TimeStamp::default();
        };

        let samples = last.data.first().map(|c| c.len()).unwrap_or(0);

        let frame_duration_secs = samples as f64 / self.sample_rate as f64;

        let frame_duration =
            TimeStamp::from_seconds(frame_duration_secs, last.time.num, last.time.den);

        last.time.add(frame_duration)
    }
}

// ── Feature 10: Additional audio manipulation methods ────────────────────────
impl Track {
    /// Reverse the entire track in place (sample-level reversal within each frame,
    /// then reverse the frame order).
    pub fn reverse(&self) -> Self {
        // Collect all samples per channel
        let n_ch = self.channels as usize;
        let mut all_samples: Vec<Vec<f32>> = vec![Vec::new(); n_ch];

        for frame in &self.buffer {
            for (ch_idx, ch) in frame.data.iter().enumerate() {
                if ch_idx < n_ch {
                    all_samples[ch_idx].extend_from_slice(ch);
                }
            }
        }

        // Reverse each channel
        for ch in &mut all_samples {
            ch.reverse();
        }

        // Re-pack into frames of the same sizes as originals (but reversed order)
        const FRAME_SIZE: usize = 1024;
        let total_samples = all_samples.first().map_or(0, |c| c.len());
        let mut out_buffer = Vec::new();
        let mut offset = 0usize;
        let mut ts = if let Some(first) = self.buffer.first() {
            TimeStamp::zero_like(first.time)
        } else {
            TimeStamp::default()
        };

        while offset < total_samples {
            let end = (offset + FRAME_SIZE).min(total_samples);
            let data: Vec<Vec<f32>> = all_samples
                .iter()
                .map(|ch| ch[offset..end].to_vec())
                .collect();
            out_buffer.push(AudioFrame { time: ts, data });
            let dt = (end - offset) as f64 / self.sample_rate as f64;
            if let Some(f) = self.buffer.first() {
                ts = ts.add(TimeStamp::from_seconds(dt, f.time.num, f.time.den));
            }
            offset = end;
        }

        Track::new(self.sample_rate, self.channels, out_buffer)
    }

    /// Change playback speed by `factor` (>1 speeds up, <1 slows down).
    /// Uses linear interpolation for quality resampling.
    pub fn speed(&self, factor: f64) -> Self {
        if factor <= 0.0 {
            return Track::new(self.sample_rate, self.channels, vec![]);
        }

        let n_ch = self.channels as usize;
        let mut all_samples: Vec<Vec<f32>> = vec![Vec::new(); n_ch];
        for frame in &self.buffer {
            for (ch_idx, ch) in frame.data.iter().enumerate() {
                if ch_idx < n_ch {
                    all_samples[ch_idx].extend_from_slice(ch);
                }
            }
        }

        let original_len = all_samples.first().map_or(0, |c| c.len());
        let new_len = (original_len as f64 / factor).round() as usize;

        let mut resampled: Vec<Vec<f32>> = vec![Vec::with_capacity(new_len); n_ch];
        for out_i in 0..new_len {
            let src_pos = out_i as f64 * factor;
            let src_i = src_pos.floor() as usize;
            let frac = (src_pos - src_i as f64) as f32;
            for (ch_idx, ch) in all_samples.iter().enumerate() {
                let a = ch.get(src_i).copied().unwrap_or(0.0);
                let b = ch.get(src_i + 1).copied().unwrap_or(a);
                resampled[ch_idx].push(a + frac * (b - a));
            }
        }

        // Pack into frames
        const FRAME_SIZE: usize = 1024;
        let mut out_buffer = Vec::new();
        let mut offset = 0usize;
        let mut ts = if let Some(f) = self.buffer.first() {
            TimeStamp::zero_like(f.time)
        } else {
            TimeStamp::default()
        };

        while offset < new_len {
            let end = (offset + FRAME_SIZE).min(new_len);
            let data: Vec<Vec<f32>> = resampled
                .iter()
                .map(|ch| ch[offset..end].to_vec())
                .collect();
            out_buffer.push(AudioFrame { time: ts, data });
            let dt = (end - offset) as f64 / self.sample_rate as f64;
            if let Some(f) = self.buffer.first() {
                ts = ts.add(TimeStamp::from_seconds(dt, f.time.num, f.time.den));
            }
            offset = end;
        }

        Track::new(self.sample_rate, self.channels, out_buffer)
    }

    /// Concatenate multiple tracks sequentially. All must have the same sample
    /// rate and channel count.
    pub fn concat_tracks(tracks: &[&Track]) -> Result<Track, TrackError> {
        if tracks.is_empty() {
            return Ok(Track::new(44100, 2, vec![]));
        }
        let sr = tracks[0].sample_rate;
        let ch = tracks[0].channels;
        for t in tracks.iter().skip(1) {
            if t.sample_rate != sr || t.channels != ch {
                return Err(TrackError::MixingIsNotPossible);
            }
        }
        let mut merged = Track::new(sr, ch, vec![]);
        for t in tracks {
            merged = Track::merge(&merged, t)?;
        }
        Ok(merged)
    }

    /// Extract a single audio channel as a mono track.
    /// `channel_index` is 0-based (0 = left, 1 = right, etc.).
    pub fn channel_split(&self, channel_index: usize) -> Self {
        let buffer = self
            .buffer
            .iter()
            .map(|frame| {
                let ch_data = frame
                    .data
                    .get(channel_index)
                    .cloned()
                    .unwrap_or_else(|| vec![0.0; frame.data.first().map_or(0, |c| c.len())]);
                AudioFrame {
                    time: frame.time,
                    data: vec![ch_data],
                }
            })
            .collect();
        Track::new(self.sample_rate, 1, buffer)
    }

    /// Slice by seconds (convenience wrapper around the TimeStamp-based `slice`).
    pub fn slice_seconds(&self, start_s: f64, end_s: f64) -> Self {
        let tb_num = self
            .buffer
            .first()
            .map_or(1, |f| f.time.num);
        let tb_den = self
            .buffer
            .first()
            .map_or(self.sample_rate, |f| f.time.den);
        let start = TimeStamp::from_seconds(start_s, tb_num, tb_den);
        let end = TimeStamp::from_seconds(end_s, tb_num, tb_den);
        if start >= end {
            return Track::new(self.sample_rate, self.channels, vec![]);
        }
        self.slice(start, end)
    }

    /// Mix (sum) two tracks together. They must have the same sample rate and
    /// channel count. If their durations differ, the shorter one is padded with silence.
    pub fn mix_sum(a: &Track, b: &Track) -> Result<Track, TrackError> {
        if a.sample_rate != b.sample_rate || a.channels != b.channels {
            return Err(TrackError::MixingIsNotPossible);
        }
        let n_ch = a.channels as usize;
        let sr = a.sample_rate;

        // Flatten both tracks
        let flatten = |t: &Track| -> Vec<Vec<f32>> {
            let mut out: Vec<Vec<f32>> = vec![Vec::new(); n_ch];
            for frame in t.buffer() {
                for (i, ch) in frame.data.iter().enumerate() {
                    if i < n_ch {
                        out[i].extend_from_slice(ch);
                    }
                }
            }
            out
        };

        let mut a_flat = flatten(a);
        let b_flat = flatten(b);

        let max_len = a_flat
            .first()
            .map_or(0, |c| c.len())
            .max(b_flat.first().map_or(0, |c| c.len()));

        for ch_idx in 0..n_ch {
            a_flat[ch_idx].resize(max_len, 0.0);
            let b_len = b_flat.get(ch_idx).map_or(0, |c| c.len());
            for i in 0..b_len {
                a_flat[ch_idx][i] += b_flat[ch_idx][i];
            }
        }

        // Pack
        const FRAME_SIZE: usize = 1024;
        let mut out_buffer = Vec::new();
        let mut offset = 0usize;
        let tb_num = a.buffer.first().map_or(1, |f| f.time.num);
        let tb_den = a.buffer.first().map_or(sr, |f| f.time.den);
        let mut ts = TimeStamp { value: 0, num: tb_num, den: tb_den };

        while offset < max_len {
            let end = (offset + FRAME_SIZE).min(max_len);
            let data: Vec<Vec<f32>> = a_flat.iter().map(|ch| ch[offset..end].to_vec()).collect();
            out_buffer.push(AudioFrame { time: ts, data });
            let dt = (end - offset) as f64 / sr as f64;
            ts = ts.add(TimeStamp::from_seconds(dt, tb_num, tb_den));
            offset = end;
        }

        Ok(Track::new(sr, a.channels, out_buffer))
    }
}

