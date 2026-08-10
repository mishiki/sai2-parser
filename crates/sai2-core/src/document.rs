use crate::{Chunk, ParseError, Sai2Header};

/// Parsed top-level metadata for a SAI2 document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sai2Document {
    header: Sai2Header,
    chunks: Vec<Chunk>,
}

impl Sai2Document {
    /// Parses the fixed header and complete chunk table from `input`.
    ///
    /// Chunk bodies are not decoded. Unknown chunk types and nonzero reserved
    /// table fields are retained as metadata.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] when the header/table is truncated, the
    /// signature is invalid, or a chunk offset cannot describe a bounded,
    /// ordered range within the input.
    pub fn parse(input: &[u8]) -> Result<Self, ParseError> {
        let header = Sai2Header::parse(input)?;
        let chunks = Chunk::parse_all(input, &header)?;
        Ok(Self { header, chunks })
    }

    /// Returns the fixed document header.
    #[must_use]
    pub const fn header(&self) -> &Sai2Header {
        &self.header
    }

    /// Returns chunk metadata in table order.
    #[must_use]
    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }
}
