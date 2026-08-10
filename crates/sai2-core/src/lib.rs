//! Byte-oriented parsing primitives for `PaintTool SAI Ver.2` (`.sai2`) files.
//!
//! This crate is experimental and unofficial. It intentionally does not
//! perform filesystem I/O so that the parser can later be used from WASM and
//! other environments.

mod error;
mod header;

pub use error::ParseError;
pub use header::{FourCc, HEADER_LEN, SAI2_MAGIC, Sai2Header};
