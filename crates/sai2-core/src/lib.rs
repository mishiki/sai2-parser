//! Byte-oriented parsing primitives for `PaintTool SAI Ver.2` (`.sai2`) files.
//!
//! This crate is experimental and unofficial. It intentionally does not
//! perform filesystem I/O so that the parser can later be used from WASM and
//! other environments.

mod chunk;
mod document;
mod error;
mod header;
mod image;
mod layer;

pub use chunk::{CHUNK_ENTRY_LEN, Chunk};
pub use document::Sai2Document;
pub use error::ParseError;
pub use header::{FourCc, HEADER_LEN, SAI2_MAGIC, Sai2Header};
pub use image::{DecodeLimits, RgbaImage, decode_integrated_image};
pub use layer::{Sai2Layer, decode_layers};
