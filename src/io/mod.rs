#[cfg(not(target_arch = "wasm32"))]
pub mod io;

#[cfg(not(target_arch = "wasm32"))]
pub mod video_io;

#[cfg(target_arch = "wasm32")]
pub mod web_io;
