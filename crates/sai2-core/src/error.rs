use std::fmt;

/// Error returned when binary SAI2 data cannot be parsed safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The input ended before a required number of bytes were available.
    Truncated { expected: usize, actual: usize },
    /// The fixed file signature did not identify a SAI2 document.
    InvalidSignature { found: [u8; 16] },
    /// The declared chunk count cannot be represented safely on this system.
    InvalidChunkCount { count: u32 },
    /// A chunk points into the header/table or beyond the end of the file.
    InvalidChunkOffset {
        index: usize,
        offset: u64,
        table_end: usize,
        file_len: usize,
    },
    /// Chunk offsets cannot be used to derive bounded chunk sizes.
    ChunkOffsetsOutOfOrder {
        previous_index: usize,
        previous_offset: u64,
        index: usize,
        offset: u64,
    },
    /// No integrated-image (`intg`) chunk was present.
    MissingIntegratedImage,
    /// The integrated image uses an unsupported body encoding.
    UnsupportedIntegratedImage { found: [u8; 4] },
    /// Canvas dimensions cannot describe a decodable image.
    InvalidImageDimensions { width: u32, height: u32 },
    /// Decoding was stopped before allocating more pixels than allowed.
    ImageTooLarge { pixels: u64, max_pixels: u64 },
    /// The integrated-image stream is malformed or truncated.
    MalformedDpcm { reason: &'static str },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { expected, actual } => write!(
                formatter,
                "truncated SAI2 data: expected at least {expected} bytes, found {actual}"
            ),
            Self::InvalidSignature { found } => write!(
                formatter,
                "invalid SAI2 signature: found {:?}",
                String::from_utf8_lossy(found)
            ),
            Self::InvalidChunkCount { count } => {
                write!(formatter, "invalid SAI2 chunk count: {count}")
            }
            Self::InvalidChunkOffset {
                index,
                offset,
                table_end,
                file_len,
            } => write!(
                formatter,
                "invalid SAI2 chunk offset at index {index}: {offset} is outside {table_end}..={file_len}"
            ),
            Self::ChunkOffsetsOutOfOrder {
                previous_index,
                previous_offset,
                index,
                offset,
            } => write!(
                formatter,
                "SAI2 chunk offsets are out of order: index {previous_index} points to {previous_offset}, but index {index} points to {offset}"
            ),
            Self::MissingIntegratedImage => {
                write!(formatter, "SAI2 document has no integrated-image chunk")
            }
            Self::UnsupportedIntegratedImage { found } => write!(
                formatter,
                "unsupported integrated-image encoding: {:?}",
                String::from_utf8_lossy(found)
            ),
            Self::InvalidImageDimensions { width, height } => {
                write!(
                    formatter,
                    "invalid SAI2 image dimensions: {width} x {height}"
                )
            }
            Self::ImageTooLarge { pixels, max_pixels } => write!(
                formatter,
                "SAI2 image has {pixels} pixels, exceeding the configured limit of {max_pixels}"
            ),
            Self::MalformedDpcm { reason } => {
                write!(formatter, "malformed SAI2 DPCM image: {reason}")
            }
        }
    }
}

impl std::error::Error for ParseError {}
