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
        offset: u32,
        table_end: usize,
        file_len: usize,
    },
    /// Chunk offsets cannot be used to derive bounded chunk sizes.
    ChunkOffsetsOutOfOrder {
        previous_index: usize,
        previous_offset: u32,
        index: usize,
        offset: u32,
    },
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
        }
    }
}

impl std::error::Error for ParseError {}
