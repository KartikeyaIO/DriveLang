use std::fmt;

use std::cmp::max;

// CONFIGS

#[derive(Debug, Clone, Copy)]
pub enum Color {
    RGB(u8, u8, u8),
    RGBA(u8, u8, u8, u8),
    Gray(u8),
}
impl Color {
    pub fn size(&self) -> usize {
        match *self {
            Self::RGB(..) => 3,
            Self::Gray(_) => 1,
            Self::RGBA(..) => 4,
        }
    }
    pub fn r(&self) -> u8 {
        match self {
            Self::RGB(r, _, _) => *r,
            Self::RGBA(r, _, _, _) => *r,
            _ => 0,
        }
    }
    pub fn g(&self) -> u8 {
        match self {
            Self::RGB(_, g, _) => *g,
            Self::RGBA(_, g, _, _) => *g,
            _ => 0,
        }
    }
    pub fn b(&self) -> u8 {
        match self {
            Self::RGB(_, _, b) => *b,
            Self::RGBA(_, _, b, _) => *b,
            _ => 0,
        }
    }
    pub fn to_rgba(self) -> Color {
        match self {
            Color::Gray(v) => Color::RGBA(v, v, v, v),
            Color::RGB(r, g, b) => Color::RGBA(r, g, b, 255),
            Color::RGBA(r, g, b, a) => Color::RGBA(r, g, b, a),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub struct Pos(pub u32, pub u32);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// PixelData store the actual Data of a Frame
pub enum PixelData {
    RGB(Vec<u8>, Vec<u8>, Vec<u8>),
    RGBA(Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>),
    GRAY(Vec<u8>),
    YUV420(Vec<u8>, Vec<u8>, Vec<u8>),
}

impl PixelData {
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            PixelData::RGB(..) => 3,
            PixelData::GRAY(_) => 1,
            PixelData::RGBA(..) => 4,
            _ => 0,
        }
    }
    pub fn ffmpeg_fmt(&self) -> &str {
        match *self {
            PixelData::RGB(..) => "rgb24",
            PixelData::GRAY(_) => "gray",
            PixelData::RGBA(..) => "rgba",
            PixelData::YUV420(..) => "yuv420p",
        }
    }
    pub fn len(&self) -> usize {
        match self {
            PixelData::RGB(d, _, _) => d.len(),
            PixelData::GRAY(d) => d.len(),
            PixelData::RGBA(d, _, _, _) => d.len(),
            PixelData::YUV420(y, _, _) => y.len(),
        }
    }
    pub fn interleave(&self) -> Vec<u8> {
        match self {
            PixelData::GRAY(v) => v.clone(),
            PixelData::RGB(r, g, b) => {
                let mut v = Vec::new();
                for i in 0..r.len() {
                    v.push(r[i]);
                    v.push(g[i]);
                    v.push(b[i]);
                }
                v
            }
            PixelData::RGBA(r, g, b, a) => {
                let mut v = Vec::new();
                for i in 0..r.len() {
                    v.push(r[i]);
                    v.push(g[i]);
                    v.push(b[i]);
                    v.push(a[i]);
                }
                v
            }
            PixelData::YUV420(y, _, _) => {
                println!(
                    "The Function is not implmented for YUV420\n The Returned slice is the luma values!"
                );
                return y.clone();
            }
        }
    }
    pub fn to_rgba8(&self, width: u32, height: u32) -> Result<PixelData, FrameError> {
        match self {
            PixelData::GRAY(v) => {
                let l = v.len();
                let a = vec![255u8; l];
                Ok(PixelData::RGBA(v.clone(), v.clone(), v.clone(), a))
            }
            PixelData::RGB(r, g, b) => {
                let l = r.len();
                let a = vec![255u8; l];
                Ok(PixelData::RGBA(r.clone(), g.clone(), b.clone(), a))
            }
            PixelData::YUV420(y_plane, u_plane, v_plane) => {
                if width % 2 != 0 || height % 2 != 0 {
                    return Err(FrameError::InvalidFrameSize);
                }

                let mut r = Vec::with_capacity((width * height) as usize);
                let mut g = Vec::with_capacity((width * height) as usize);
                let mut b = Vec::with_capacity((width * height) as usize);
                let a = vec![255u8; (width * height) as usize];

                for y in 0..height {
                    for x in 0..width {
                        let yidx = (y * width + x) as usize;
                        let uvidx = ((y / 2) * (width / 2) + (x / 2)) as usize;

                        // ---- fetch values ----
                        let y_raw = y_plane[yidx] as i32;
                        let u_raw = u_plane[uvidx] as i32;
                        let v_raw = v_plane[uvidx] as i32;

                        // ---- convert limited range → full ----
                        let c = (y_raw - 16).max(0);
                        let d = u_raw - 128;
                        let e = v_raw - 128;

                        // ---- integer BT.601 ----
                        let r_val = (298 * c + 409 * e + 128) >> 8;
                        let g_val = (298 * c - 100 * d - 208 * e + 128) >> 8;
                        let b_val = (298 * c + 516 * d + 128) >> 8;

                        r.push(r_val.clamp(0, 255) as u8);
                        g.push(g_val.clamp(0, 255) as u8);
                        b.push(b_val.clamp(0, 255) as u8);
                    }
                }

                Ok(PixelData::RGBA(r, g, b, a))
            }
            PixelData::RGBA(r, g, b, a) => {
                Ok(PixelData::RGBA(r.clone(), g.clone(), b.clone(), a.clone()))
            }
        }
    }
}

#[derive(Debug, Clone)]
/// Frame Type is used to store an image or a single frame from a video.
pub struct Frame {
    width: u32,
    height: u32,
    data: PixelData,
}

#[derive(Debug)]
pub enum FrameError {
    InvalidFrameSize,
    InvalidPixel,
    InvalidPixelFormat,
    BlitFailed,
    InvalidOpacityValue,
    EmptyFrame,
    YUVNotApplied,
    BlendingFailed,
}
impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FrameError::InvalidFrameSize => {
                write!(f, "Invalid frame buffer size")
            }
            FrameError::InvalidPixel => {
                write!(f, "Unable to find the pixel!")
            }
            FrameError::InvalidPixelFormat => {
                write!(f, "The PixelFormat for Frame and Color Does not match!")
            }
            FrameError::BlitFailed => {
                write!(f, "Overlapping Failed! Check Dimensions!")
            }
            FrameError::InvalidOpacityValue => {
                write!(f, "Opacity Value must be between 0 and 100")
            }
            FrameError::EmptyFrame => {
                write!(
                    f,
                    "The data in Frame is Empty! The Image might be corrupted."
                )
            }
            FrameError::YUVNotApplied => {
                write!(f, "YUV Format is Not Implemented for this function.")
            }
            FrameError::BlendingFailed => {
                write!(f, "The two frames are not compatible!")
            }
        }
    }
}

fn pad_to(frame: &Frame, target_w: usize, target_h: usize) -> Result<Frame, FrameError> {
    let offset_x = (target_w as u32 - frame.width) / 2;
    let offset_y = (target_h as u32 - frame.height) / 2;

    match &frame.data {
        PixelData::GRAY(v) => {
            let mut out = vec![0u8; target_w * target_h];

            for y in 0..frame.height {
                for x in 0..frame.width {
                    let src_idx = (y * frame.width + x) as usize;
                    let dst_idx = (y + offset_y) as usize * target_w + (x + offset_x) as usize;
                    out[dst_idx] = v[src_idx];
                }
            }

            Frame::new(target_w as u32, target_h as u32, PixelData::GRAY(out))
        }

        PixelData::RGB(r, g, b) => {
            let mut r_out = vec![0u8; target_w * target_h];
            let mut g_out = vec![0u8; target_w * target_h];
            let mut b_out = vec![0u8; target_w * target_h];

            for y in 0..frame.height {
                for x in 0..frame.width {
                    let src = (y * frame.width + x) as usize;
                    let dst = (y + offset_y) as usize * target_w + (x + offset_x) as usize;

                    r_out[dst] = r[src];
                    g_out[dst] = g[src];
                    b_out[dst] = b[src];
                }
            }

            Frame::new(
                target_w as u32,
                target_h as u32,
                PixelData::RGB(r_out, g_out, b_out),
            )
        }

        PixelData::RGBA(r, g, b, a) => {
            let mut r_out = vec![0u8; target_w * target_h];
            let mut g_out = vec![0u8; target_w * target_h];
            let mut b_out = vec![0u8; target_w * target_h];
            let mut a_out = vec![0u8; target_w * target_h];

            for y in 0..frame.height {
                for x in 0..frame.width {
                    let src = (y * frame.width + x) as usize;
                    let dst = (y + offset_y) as usize * target_w + (x + offset_x) as usize;

                    r_out[dst] = r[src];
                    g_out[dst] = g[src];
                    b_out[dst] = b[src];
                    a_out[dst] = a[src];
                }
            }

            Frame::new(
                target_w as u32,
                target_h as u32,
                PixelData::RGBA(r_out, g_out, b_out, a_out),
            )
        }

        _ => Err(FrameError::YUVNotApplied),
    }
}

impl std::error::Error for FrameError {}

impl Frame {
    pub fn new(width: u32, height: u32, data: PixelData) -> Result<Self, FrameError> {
        let expected_len = width as usize * height as usize;

        if data.len() == expected_len {
            Ok(Self {
                width,
                height,

                data,
            })
        } else {
            Err(FrameError::InvalidFrameSize)
        }
    }
    /// The brightness function is used to adjust the brightness of a pixel
    pub fn brightness(&mut self, pos: &Pos, delta: i16) -> bool {
        let clamp = |v: u8| (v as i16 + delta).clamp(0, 255) as u8;
        let index = match self.pixel_index(pos) {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Invalid Pixel position");
                return false;
            }
        };

        match &mut self.data {
            PixelData::RGB(r, g, b) | PixelData::RGBA(r, g, b, _) => {
                r[index] = clamp(r[index]);
                g[index] = clamp(g[index]);
                b[index] = clamp(b[index]);
                true
            }
            PixelData::GRAY(l) => {
                l[index] = clamp(l[index]);
                true
            }
            PixelData::YUV420(y, _, _) => {
                y[index] = clamp(y[index]);
                true
            }
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn data(&self) -> &PixelData {
        &self.data
    }
    pub fn data_mut(&mut self) -> &mut PixelData {
        &mut self.data
    }
}
impl Frame {
    pub fn pixel_index(&self, pos: &Pos) -> Result<usize, FrameError> {
        let Pos(x, y) = *pos;
        if x >= self.width || y >= self.height {
            return Err(FrameError::InvalidPixel);
        }
        Ok(y as usize * self.width as usize + x as usize)
    }

    /// It  changes the colour of a single pixel and returns the colour of that pixel
    pub fn replace_pixel(&mut self, pos: &Pos, color: &Color) -> Result<Color, FrameError> {
        let pixel = self.get_pixel(pos)?;
        self.set_pixel(pos, color)?;
        Ok(pixel)
    }
    /// The get_pixel function returns the color of a pixel at a certain position
    pub fn get_pixel(&self, pos: &Pos) -> Result<Color, FrameError> {
        let index = self.pixel_index(pos)?;
        let data = self.data();

        match data {
            PixelData::RGB(r, g, b) => Ok(Color::RGB(r[index], g[index], b[index])),
            PixelData::RGBA(r, g, b, a) => Ok(Color::RGBA(r[index], g[index], b[index], a[index])),
            PixelData::GRAY(l) => Ok(Color::Gray(l[index])),
            _ => return Err(FrameError::YUVNotApplied),
        }
    }

    /// The set_pixel() method allows us to set the color of a pixel at a specific position
    pub fn set_pixel(&mut self, pos: &Pos, color: &Color) -> Result<(), FrameError> {
        let index = self.pixel_index(pos)?;

        if self.data.bytes_per_pixel() != color.size() {
            return Err(FrameError::InvalidPixelFormat);
        }

        match (&mut self.data, color) {
            (PixelData::RGB(r, g, b), Color::RGB(rv, gv, bv)) => {
                r[index] = *rv;
                g[index] = *gv;
                b[index] = *bv;
            }
            (PixelData::RGBA(r, g, b, a), Color::RGBA(rv, gv, bv, av)) => {
                r[index] = *rv;
                g[index] = *gv;
                b[index] = *bv;
                a[index] = *av;
            }
            (PixelData::GRAY(l), Color::Gray(v)) => {
                l[index] = *v;
            }
            _ => return Err(FrameError::InvalidPixelFormat),
        }

        Ok(())
    }
    pub fn set_alpha(&mut self, pos: &Pos, value: u8) -> Result<(), FrameError> {
        if self.data.bytes_per_pixel() != 4 {
            return Err(FrameError::InvalidPixelFormat);
        }
        let index = self.pixel_index(pos)?;
        let data = &mut self.data;
        match data {
            PixelData::RGBA(_, _, _, a) => {
                a[index] = value;
            }
            _ => return Err(FrameError::InvalidPixelFormat),
        }
        Ok(())
    }
    pub fn to_rgba(&self) -> Result<Frame, FrameError> {
        let data = self.data.to_rgba8(self.width, self.height)?;
        Frame::new(self.width, self.height, data)
    }
    pub fn opacity(&mut self, pos: &Pos, value: u8) -> Result<(), FrameError> {
        if self.data.bytes_per_pixel() != 4 {
            return Err(FrameError::InvalidPixelFormat);
        }
        if value > 100 {
            return Err(FrameError::InvalidOpacityValue);
        }
        let index = self.pixel_index(pos)?;
        let data = &mut self.data;

        match data {
            PixelData::RGBA(_, _, _, a) => {
                a[index] = ((a[index] as u16 * value as u16) / 100) as u8;
            }
            _ => return Err(FrameError::InvalidPixelFormat),
        }
        Ok(())
    }
    pub fn contrast(&mut self, pos: &Pos, factor: f32) -> Result<(), FrameError> {
        let index = self.pixel_index(pos)?;
        let data = &mut self.data;
        match data {
            PixelData::GRAY(l) => {
                l[index] = (128.0 + factor * (l[index] as f32 - 128.0)).clamp(0.0, 255.0) as u8;
            }
            PixelData::RGB(r, g, b) | PixelData::RGBA(r, g, b, _) => {
                for channel in [r, g, b] {
                    channel[index] =
                        (128.0 + factor * (channel[index] as f32 - 128.0)).clamp(0.0, 255.0) as u8;
                }
            }
            PixelData::YUV420(l, _, _) => {
                l[index] = (128.0 + factor * (l[index] as f32 - 128.0)).clamp(0.0, 255.0) as u8;
            }
        }
        Ok(())
    }
    pub fn blend(&self, frame: &Frame, alpha: f32) -> Result<Frame, FrameError> {
        let mut data = self.data.clone();
        let data2 = frame.data();

        let alpha = alpha.clamp(0.0, 1.0);

        if std::mem::discriminant(&data) != std::mem::discriminant(&data2) {
            return Err(FrameError::BlendingFailed);
        }

        if self.width.abs_diff(frame.width) != 0 || self.height.abs_diff(frame.height) != 0 {
            return Err(FrameError::BlendingFailed);
        }

        match data {
            PixelData::GRAY(ref mut v) => {
                let v2 = match data2 {
                    PixelData::GRAY(v2) => v2,
                    _ => return Err(FrameError::BlendingFailed),
                };

                for (p, p2) in v.iter_mut().zip(v2.iter()) {
                    *p = (*p as f32 * (1.0 - alpha) + *p2 as f32 * alpha) as u8;
                }
            }

            PixelData::RGB(ref mut r, ref mut g, ref mut b) => {
                let (r1, g1, b1) = match data2 {
                    PixelData::RGB(r1, g1, b1) => (r1, g1, b1),
                    _ => return Err(FrameError::BlendingFailed),
                };

                for i in 0..r.len() {
                    r[i] = (r[i] as f32 * (1.0 - alpha) + r1[i] as f32 * alpha) as u8;
                    g[i] = (g[i] as f32 * (1.0 - alpha) + g1[i] as f32 * alpha) as u8;
                    b[i] = (b[i] as f32 * (1.0 - alpha) + b1[i] as f32 * alpha) as u8;
                }
            }

            PixelData::RGBA(ref mut r, ref mut g, ref mut b, ref mut a) => {
                let (r1, g1, b1, a1) = match data2 {
                    PixelData::RGBA(r1, g1, b1, a1) => (r1, g1, b1, a1),
                    _ => return Err(FrameError::BlendingFailed),
                };

                for i in 0..r.len() {
                    r[i] = (r[i] as f32 * (1.0 - alpha) + r1[i] as f32 * alpha) as u8;
                    g[i] = (g[i] as f32 * (1.0 - alpha) + g1[i] as f32 * alpha) as u8;
                    b[i] = (b[i] as f32 * (1.0 - alpha) + b1[i] as f32 * alpha) as u8;
                    a[i] = (a[i] as f32 * (1.0 - alpha) + a1[i] as f32 * alpha) as u8;
                }
            }

            _ => return Err(FrameError::YUVNotApplied),
        }

        Frame::new(self.width, self.height, data)
    }

    pub fn normalize(a: &Frame, b: &Frame) -> Result<(Frame, Frame), FrameError> {
        if std::mem::discriminant(&a.data) != std::mem::discriminant(&b.data) {
            return Err(FrameError::BlendingFailed);
        }

        let target_w = max(a.width, b.width);
        let target_h = max(a.height, b.height);

        let a_norm = pad_to(a, target_w as usize, target_h as usize)?;
        let b_norm = pad_to(b, target_w as usize, target_h as usize)?;
        Ok((a_norm, b_norm))
    }
}

impl Frame {
    pub fn saturation(&mut self, pos: &Pos, s_value: f32) -> bool {
        let index = match self.pixel_index(pos) {
            Ok(val) => val,
            Err(_) => {
                eprintln!("Invalid Pixel Index!");
                return false;
            }
        };

        let data = &mut self.data;
        match data {
            PixelData::RGB(r, g, b) | PixelData::RGBA(r, g, b, _) => {
                let y = 0.299 * r[index] as f32 + 0.587 * g[index] as f32 + 0.114 * b[index] as f32;

                r[index] = (y + s_value * (r[index] as f32 - y).clamp(0.0, 255.0)) as u8;
                g[index] = (y + s_value * (g[index] as f32 - y).clamp(0.0, 255.0)) as u8;
                b[index] = (y + s_value * (b[index] as f32 - y).clamp(0.0, 255.0)) as u8;
                true
            }
            _ => {
                eprintln!("Saturation is only implemented for RGB or RGBA!");
                return false;
            }
        }
    }
    pub fn blit(&mut self, pos: &Pos, frame: &Frame) -> Result<(), FrameError> {
        let Pos(x, y) = *pos;

        if x + frame.width > self.width || y + frame.height > self.height {
            return Err(FrameError::BlitFailed);
        }

        let (r, g, b, a) = match &mut self.data {
            PixelData::RGBA(r, g, b, a) => (r, g, b, a),
            _ => return Err(FrameError::BlitFailed),
        };

        let (r2, g2, b2, a2) = match &frame.data {
            PixelData::RGBA(r, g, b, a) => (r, g, b, a),
            _ => return Err(FrameError::BlitFailed),
        };

        for row in 0..frame.height {
            for col in 0..frame.width {
                let dst_idx = ((y + row) * self.width + (x + col)) as usize;
                let src_idx = (row * frame.width + col) as usize;

                r[dst_idx] = r2[src_idx];
                g[dst_idx] = g2[src_idx];
                b[dst_idx] = b2[src_idx];
                a[dst_idx] = a2[src_idx];
            }
        }

        Ok(())
    }
    pub fn blend_on(&mut self, pos: &Pos, frame: &Frame, alpha: f64) -> Result<(), FrameError> {
        let Pos(x, y) = *pos;

        if x + frame.width > self.width || y + frame.height > self.height {
            return Err(FrameError::BlitFailed);
        }

        let (r, g, b, a) = match &mut self.data {
            PixelData::RGBA(r, g, b, a) => (r, g, b, a),
            _ => return Err(FrameError::BlitFailed),
        };

        let (r2, g2, b2, a2) = match &frame.data {
            PixelData::RGBA(r, g, b, a) => (r, g, b, a),
            _ => return Err(FrameError::BlitFailed),
        };

        for row in 0..frame.height {
            for col in 0..frame.width {
                let j = ((y + row) * self.width + (x + col)) as usize;
                let i = (row * frame.width + col) as usize;

                r[j] = (r[j] as f64 * (1.0 - alpha) + r2[i] as f64 * alpha) as u8;
                g[j] = (g[j] as f64 * (1.0 - alpha) + g2[i] as f64 * alpha) as u8;
                b[j] = (b[j] as f64 * (1.0 - alpha) + b2[i] as f64 * alpha) as u8;
                a[j] = (a[j] as f64 * (1.0 - alpha) + a2[i] as f64 * alpha) as u8;
            }
        }

        Ok(())
    }
}
impl Frame {
    pub fn blank(width: u32, height: u32) -> Self {
        let len = (width * height) as usize;

        Self {
            width,
            height,
            data: PixelData::RGBA(
                vec![0u8; len],
                vec![0u8; len],
                vec![0u8; len],
                vec![255u8; len],
            ),
        }
    }
    pub fn crop(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Frame, FrameError> {
        if x + width > self.width || y + height > self.height {
            return Err(FrameError::InvalidFrameSize);
        }

        let len = (width * height) as usize;

        match &self.data {
            PixelData::GRAY(src) => {
                let mut dst = vec![0u8; len];

                for row in 0..height {
                    let src_start = ((y + row) * self.width + x) as usize;
                    let src_end = src_start + width as usize;

                    let dst_start = (row * width) as usize;

                    dst[dst_start..dst_start + width as usize]
                        .copy_from_slice(&src[src_start..src_end]);
                }

                Frame::new(width, height, PixelData::GRAY(dst))
            }

            PixelData::RGB(r, g, b) => {
                let mut r_out = vec![0u8; len];
                let mut g_out = vec![0u8; len];
                let mut b_out = vec![0u8; len];

                for row in 0..height {
                    let src_start = ((y + row) * self.width + x) as usize;
                    let src_end = src_start + width as usize;

                    let dst_start = (row * width) as usize;

                    r_out[dst_start..dst_start + width as usize]
                        .copy_from_slice(&r[src_start..src_end]);

                    g_out[dst_start..dst_start + width as usize]
                        .copy_from_slice(&g[src_start..src_end]);

                    b_out[dst_start..dst_start + width as usize]
                        .copy_from_slice(&b[src_start..src_end]);
                }

                Frame::new(width, height, PixelData::RGB(r_out, g_out, b_out))
            }

            PixelData::RGBA(r, g, b, a) => {
                let mut r_out = vec![0u8; len];
                let mut g_out = vec![0u8; len];
                let mut b_out = vec![0u8; len];
                let mut a_out = vec![0u8; len];

                for row in 0..height {
                    let src_start = ((y + row) * self.width + x) as usize;
                    let src_end = src_start + width as usize;

                    let dst_start = (row * width) as usize;

                    r_out[dst_start..dst_start + width as usize]
                        .copy_from_slice(&r[src_start..src_end]);

                    g_out[dst_start..dst_start + width as usize]
                        .copy_from_slice(&g[src_start..src_end]);

                    b_out[dst_start..dst_start + width as usize]
                        .copy_from_slice(&b[src_start..src_end]);

                    a_out[dst_start..dst_start + width as usize]
                        .copy_from_slice(&a[src_start..src_end]);
                }

                Frame::new(width, height, PixelData::RGBA(r_out, g_out, b_out, a_out))
            }

            PixelData::YUV420(..) => Err(FrameError::YUVNotApplied),
        }
    }
    pub fn resize(&self, target_w: u32, target_h: u32) -> Result<Frame, FrameError> {
        if target_w == 0 || target_h == 0 {
            return Err(FrameError::InvalidFrameSize);
        }

        let scale_x = self.width as f32 / target_w as f32;
        let scale_y = self.height as f32 / target_h as f32;
        let len = (target_w * target_h) as usize;

        match &self.data {
            PixelData::GRAY(l) => {
                let mut out = vec![0u8; len];
                for y in 0..target_h {
                    for x in 0..target_w {
                        let src_x = ((x as f32 * scale_x) as u32).min(self.width - 1);
                        let src_y = ((y as f32 * scale_y) as u32).min(self.height - 1);
                        out[(y * target_w + x) as usize] = l[(src_y * self.width + src_x) as usize];
                    }
                }
                Frame::new(target_w, target_h, PixelData::GRAY(out))
            }
            PixelData::RGB(r, g, b) => {
                let mut r_out = vec![0u8; len];
                let mut g_out = vec![0u8; len];
                let mut b_out = vec![0u8; len];

                for y in 0..target_h {
                    for x in 0..target_w {
                        let src_x = ((x as f32 * scale_x) as u32).min(self.width - 1);
                        let src_y = ((y as f32 * scale_y) as u32).min(self.height - 1);
                        let src_idx = (src_y * self.width + src_x) as usize;
                        let dst_idx = (y * target_w + x) as usize;

                        r_out[dst_idx] = r[src_idx];
                        g_out[dst_idx] = g[src_idx];
                        b_out[dst_idx] = b[src_idx];
                    }
                }
                Frame::new(target_w, target_h, PixelData::RGB(r_out, g_out, b_out))
            }
            PixelData::RGBA(r, g, b, a) => {
                let mut r_out = vec![0u8; len];
                let mut g_out = vec![0u8; len];
                let mut b_out = vec![0u8; len];
                let mut a_out = vec![0u8; len];

                for y in 0..target_h {
                    for x in 0..target_w {
                        let src_x = ((x as f32 * scale_x) as u32).min(self.width - 1);
                        let src_y = ((y as f32 * scale_y) as u32).min(self.height - 1);
                        let src_idx = (src_y * self.width + src_x) as usize;
                        let dst_idx = (y * target_w + x) as usize;

                        r_out[dst_idx] = r[src_idx];
                        g_out[dst_idx] = g[src_idx];
                        b_out[dst_idx] = b[src_idx];
                        a_out[dst_idx] = a[src_idx];
                    }
                }
                Frame::new(target_w, target_h, PixelData::RGBA(r_out, g_out, b_out, a_out))
            }
            PixelData::YUV420(..) => Err(FrameError::YUVNotApplied),
        }
    }
}

