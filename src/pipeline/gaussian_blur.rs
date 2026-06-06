// use crate::{
//     filter::Filter,
//     media::frame::{Frame, PixelData},
// };

// pub struct GaussianBlur {
//     pub sigma: f32,
//     kernel: Vec<f32>,
// }

// impl GaussianBlur {
//     pub fn new(sigma: f32) -> Self {
//         let kernel = Self::build_kernel(sigma);
//         Self { sigma, kernel }
//     }

//     fn build_kernel(sigma: f32) -> Vec<f32> {
//         if sigma <= 0.0 {
//             return vec![1.0];
//         }
//         let radius = (3.0 * sigma).ceil() as i32;
//         let mut kernel: Vec<f32> = (-radius..=radius)
//             .map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp())
//             .collect();
//         let sum: f32 = kernel.iter().sum();
//         kernel.iter_mut().for_each(|k| *k /= sum);
//         kernel
//     }
// }

// // ── Channel blur helpers ──────────────────────────────────────────────────────

// fn blur_horizontal(channel: &[u8], width: u32, height: u32, kernel: &[f32]) -> Vec<u8> {
//     let radius = (kernel.len() / 2) as i32;
//     let w = width as usize;
//     let h = height as usize;
//     let mut out = vec![0u8; w * h];

//     for y in 0..h {
//         for x in 0..w {
//             let mut sum = 0.0f32;
//             for (k, &weight) in kernel.iter().enumerate() {
//                 let nx = (x as i32 + k as i32 - radius).clamp(0, (w - 1) as i32) as usize;
//                 sum += channel[y * w + nx] as f32 * weight;
//             }
//             out[y * w + x] = sum.clamp(0.0, 255.0) as u8;
//         }
//     }
//     out
// }

// fn blur_vertical(channel: &[u8], width: u32, height: u32, kernel: &[f32]) -> Vec<u8> {
//     let radius = (kernel.len() / 2) as i32;
//     let w = width as usize;
//     let h = height as usize;
//     let mut out = vec![0u8; w * h];

//     for y in 0..h {
//         for x in 0..w {
//             let mut sum = 0.0f32;
//             for (k, &weight) in kernel.iter().enumerate() {
//                 let ny = (y as i32 + k as i32 - radius).clamp(0, (h - 1) as i32) as usize;
//                 sum += channel[ny * w + x] as f32 * weight;
//             }
//             out[y * w + x] = sum.clamp(0.0, 255.0) as u8;
//         }
//     }
//     out
// }

// fn blur_channel(channel: &[u8], width: u32, height: u32, kernel: &[f32]) -> Vec<u8> {
//     let temp = blur_horizontal(channel, width, height, kernel);
//     blur_vertical(&temp, width, height, kernel)
// }

// // ── Filter impl ───────────────────────────────────────────────────────────────

// impl Filter for GaussianBlur {
//     fn apply(&self, mut frame: Frame) -> Frame {
//         let w = frame.width();
//         let h = frame.height();
//         let kernel = &self.kernel;

//         match &mut frame.data_mut() {
//             PixelData::RGB(r, g, b) => {
//                 *r = blur_channel(r, w, h, kernel);
//                 *g = blur_channel(g, w, h, kernel);
//                 *b = blur_channel(b, w, h, kernel);
//             }
//             PixelData::RGBA(r, g, b, _a) => {
//                 *r = blur_channel(r, w, h, kernel);
//                 *g = blur_channel(g, w, h, kernel);
//                 *b = blur_channel(b, w, h, kernel);
//                 // alpha intentionally untouched
//             }
//             PixelData::GRAY(l) => {
//                 *l = blur_channel(l, w, h, kernel);
//             }
//             PixelData::YUV420(y, u, v) => {
//                 *y = blur_channel(y, w, h, kernel);
//                 *u = blur_channel(u, w / 2, h / 2, kernel);
//                 *v = blur_channel(v, w / 2, h / 2, kernel);
//             }
//         }

//         frame
//     }
// }
