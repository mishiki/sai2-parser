use std::fmt;

/// Error returned when binary SAI2 data cannot be parsed safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The input ended before a required number of bytes were available.
    Truncated { expected: usize, actual: usize },
    /// The fixed file signature did not identify a SAI2 document.
    InvalidSignature { found: [u8; 16] },
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { expected, actual } => write!(
                formatter,
                "truncated SAI2 header: expected at least {expected} bytes, found {actual}"
            ),
            Self::InvalidSignature { found } => write!(
                formatter,
                "invalid SAI2 signature: found {:?}",
                String::from_utf8_lossy(found)
            ),
        }
    }
}

impl std::error::Error for ParseError {}
