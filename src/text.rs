use crate::media::frame::{Color, Frame, FrameError, PixelData, Pos};
use fontdue::{self, Font};

pub struct Text {
    data: String,
    font: Font,
    size: f32,
    pos: Pos,
    color: Color,
}

#[derive(Debug)]
pub enum TextError {
    FontNotFound,
    FrameCreationFailed(FrameError),
    InvalidDimensions,
    EmptyText,
}

impl Text {
    pub fn new(data: &str, font: Font, size: f32, pos: Pos, color: Color) -> Self {
        Self {
            data: data.to_string(),
            font,
            size,
            pos,
            color,
        }
    }
    pub fn set_font(&mut self, font: Font) {
        self.font = font;
    }
    pub fn set_position(&mut self, pos: &Pos) {
        self.pos = *pos;
    }
    pub fn set_size(&mut self, size: &f32) {
        self.size = *size;
    }
    pub fn set_color(&mut self, color: &Color) {
        self.color = *color;
    }
    pub fn set_text(&mut self, content: &str) {
        self.data = content.to_string();
    }

    pub fn picturize(&self) -> Result<Frame, TextError> {
        let font = &self.font;
        let data = &self.data;
        let size = self.size;
        let color = self.color;

        if data.is_empty() {
            return Err(TextError::EmptyText);
        }

        // ── Measure pass ─────────────────────────────────────────────────────
        let mut pen_x = 0.0f32;
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_top = f32::MIN;
        let mut min_bottom = f32::MAX;

        for ch in data.chars() {
            let (metrics, _) = font.rasterize(ch, size);
            let gx_min = pen_x + metrics.xmin as f32;
            let gx_max = gx_min + metrics.width as f32;
            min_x = min_x.min(gx_min);
            max_x = max_x.max(gx_max);
            max_top = max_top.max(metrics.ymin as f32 + metrics.height as f32);
            min_bottom = min_bottom.min(metrics.ymin as f32);
            pen_x += metrics.advance_width;
        }

        let width = (max_x - min_x).ceil() as u32;
        let height = (max_top - min_bottom).ceil() as u32;
        let baseline = max_top;

        if width == 0 || height == 0 {
            return Err(TextError::InvalidDimensions);
        }

        // ── Allocate channel buffers ──────────────────────────────────────────
        let pixel_count = width as usize * height as usize;
        let mut r_buf = vec![0u8; pixel_count];
        let mut g_buf = vec![0u8; pixel_count];
        let mut b_buf = vec![0u8; pixel_count];
        let mut a_buf = vec![0u8; pixel_count]; // only used for RGBA

        // ── Rasterize pass ───────────────────────────────────────────────────
        let mut pen_x = 0.0f32;

        for ch in data.chars() {
            let (metrics, bitmap) = font.rasterize(ch, size);
            let glyph_w = metrics.width as u32;
            let glyph_h = metrics.height as u32;
            let x_off = (pen_x + metrics.xmin as f32 - min_x).round() as i32;
            let y_off = (baseline - (metrics.ymin as f32 + metrics.height as f32)).round() as i32;

            for row in 0..glyph_h {
                for col in 0..glyph_w {
                    let coverage = bitmap[(row * glyph_w + col) as usize];
                    if coverage == 0 {
                        continue;
                    }

                    let dx = x_off + col as i32;
                    let dy = y_off + row as i32;
                    if dx < 0 || dy < 0 || dx >= width as i32 || dy >= height as i32 {
                        continue;
                    }

                    let idx = dy as usize * width as usize + dx as usize;
                    let alpha = coverage as f32 / 255.0;

                    match color {
                        Color::Gray(_) => {
                            r_buf[idx] = r_buf[idx].max(coverage);
                        }
                        Color::RGB(_, _, _) => {
                            r_buf[idx] = (color.r() as f32 * alpha) as u8;
                            g_buf[idx] = (color.g() as f32 * alpha) as u8;
                            b_buf[idx] = (color.b() as f32 * alpha) as u8;
                        }
                        Color::RGBA(_, _, _, _) => {
                            // Store straight RGBA. Coverage belongs in the alpha
                            // channel; premultiplying RGB here would darken
                            // antialiased glyph edges when composited later.
                            r_buf[idx] = color.r();
                            g_buf[idx] = color.g();
                            b_buf[idx] = color.b();
                            a_buf[idx] = coverage;
                        }
                    }
                }
            }

            pen_x += metrics.advance_width;
        }

        // ── Build PixelData from color variant ────────────────────────────────
        let pixel_data = match color {
            Color::Gray(_) => PixelData::GRAY(r_buf),
            Color::RGB(_, _, _) => PixelData::RGB(r_buf, g_buf, b_buf),
            Color::RGBA(_, _, _, _) => PixelData::RGBA(r_buf, g_buf, b_buf, a_buf),
        };

        Frame::new(width, height, pixel_data).map_err(TextError::FrameCreationFailed)
    }
}
