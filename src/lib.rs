#![allow(warnings)]

#[cfg(not(target_arch = "wasm32"))]
pub mod engine;
pub mod experiments;
pub mod filter;
pub mod io;
#[cfg(not(target_arch = "wasm32"))]
pub mod lexer;
pub mod media;

#[cfg(not(target_arch = "wasm32"))]
pub mod parser;
pub mod pipeline;
pub mod range;
pub mod text;
