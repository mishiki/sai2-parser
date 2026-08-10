use std::fmt;

use crate::ParseError;

/// Size of the fixed SAI2 header in bytes.
pub const HEADER_LEN: usize = 64;

/// Signature at the beginning of a SAI2 document.
pub const SAI2_MAGIC: [u8; 16] = *b"SAI-CANVAS-TYPE0";

/// A four-byte ASCII-oriented identifier stored without interpretation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FourCc([u8; 4]);

impl FourCc {
    /// Creates an identifier from its exact bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    /// Returns the exact bytes found in the file.
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 4] {
        self.0
    }
}

impl fmt::Display for FourCc {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            if byte.is_ascii_graphic() || byte == b' ' {
                write!(formatter, "{}", char::from(byte))?;
            } else {
                write!(formatter, "\\x{byte:02x}")?;
            }
        }
        Ok(())
    }
}

/// Parsed representation of the fixed 64-byte SAI2 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sai2Header {
    flags: u32,
    width: u32,
    height: u32,
    unknown_0: u32,
    chunk_count: u32,
    unknown_1: u32,
    reserved: [u8; 16],
    background_color: u32,
    format_tag: FourCc,
}

impl Sai2Header {
    /// Parses the fixed header from the start of `input`.
    ///
    /// Extra bytes after the header are left untouched for later parsing
    /// phases. Unknown and reserved values are preserved instead of rejected.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::Truncated`] when fewer than 64 bytes are
    /// available, or [`ParseError::InvalidSignature`] when the input does not
    /// begin with [`SAI2_MAGIC`].
    pub fn parse(input: &[u8]) -> Result<Self, ParseError> {
        if input.len() < HEADER_LEN {
            return Err(ParseError::Truncated {
                expected: HEADER_LEN,
                actual: input.len(),
            });
        }

        let signature = bytes_at::<16>(input, 0)?;
        if signature != SAI2_MAGIC {
            return Err(ParseError::InvalidSignature { found: signature });
        }

        Ok(Self {
            flags: u32_at(input, 16)?,
            width: u32_at(input, 20)?,
            height: u32_at(input, 24)?,
            unknown_0: u32_at(input, 28)?,
            chunk_count: u32_at(input, 32)?,
            unknown_1: u32_at(input, 36)?,
            reserved: bytes_at::<16>(input, 40)?,
            background_color: u32_at(input, 56)?,
            format_tag: FourCc::from_bytes(bytes_at::<4>(input, 60)?),
        })
    }

    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// Reports whether the saved integrated image uses an alpha channel.
    ///
    /// This bit interpretation is verified against the currently owned opaque
    /// and transparent fixtures. Unknown flag combinations remain accepted.
    #[must_use]
    pub const fn integrated_image_has_alpha(&self) -> bool {
        self.flags.to_le_bytes()[1].trailing_zeros() >= 3
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub const fn unknown_0(&self) -> u32 {
        self.unknown_0
    }

    #[must_use]
    pub const fn chunk_count(&self) -> u32 {
        self.chunk_count
    }

    #[must_use]
    pub const fn unknown_1(&self) -> u32 {
        self.unknown_1
    }

    #[must_use]
    pub const fn reserved(&self) -> [u8; 16] {
        self.reserved
    }

    #[must_use]
    pub const fn background_color(&self) -> u32 {
        self.background_color
    }

    #[must_use]
    pub const fn format_tag(&self) -> FourCc {
        self.format_tag
    }
}

fn u32_at(input: &[u8], offset: usize) -> Result<u32, ParseError> {
    Ok(u32::from_le_bytes(bytes_at::<4>(input, offset)?))
}

fn bytes_at<const N: usize>(input: &[u8], offset: usize) -> Result<[u8; N], ParseError> {
    let end = offset.checked_add(N).ok_or(ParseError::Truncated {
        expected: usize::MAX,
        actual: input.len(),
    })?;
    let bytes = input.get(offset..end).ok_or(ParseError::Truncated {
        expected: end,
        actual: input.len(),
    })?;
    bytes.try_into().map_err(|_| ParseError::Truncated {
        expected: end,
        actual: input.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_header() -> [u8; HEADER_LEN] {
        let mut bytes = [0_u8; HEADER_LEN];
        bytes[0..16].copy_from_slice(&SAI2_MAGIC);
        bytes[16..20].copy_from_slice(&0x0100_u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&4096_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&2048_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&0x1122_3344_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&12_u32.to_le_bytes());
        bytes[36..40].copy_from_slice(&0x5566_7788_u32.to_le_bytes());
        bytes[56..60].copy_from_slice(&0xff80_8080_u32.to_le_bytes());
        bytes[60..64].copy_from_slice(b"norm");
        bytes
    }

    #[test]
    fn parses_documented_header_fields() {
        let header = Sai2Header::parse(&valid_header()).expect("valid header should parse");

        assert_eq!(header.flags(), 0x0100);
        assert!(!header.integrated_image_has_alpha());
        assert_eq!(header.width(), 4096);
        assert_eq!(header.height(), 2048);
        assert_eq!(header.unknown_0(), 0x1122_3344);
        assert_eq!(header.chunk_count(), 12);
        assert_eq!(header.unknown_1(), 0x5566_7788);
        assert_eq!(header.reserved(), [0; 16]);
        assert_eq!(header.background_color(), 0xff80_8080);
        assert_eq!(header.format_tag(), FourCc::from_bytes(*b"norm"));
    }

    #[test]
    fn identifies_the_observed_transparent_integrated_image_flag() {
        let mut bytes = valid_header();
        bytes[16..20].copy_from_slice(&0x2000_u32.to_le_bytes());

        let header = Sai2Header::parse(&bytes).unwrap();

        assert!(header.integrated_image_has_alpha());
    }

    #[test]
    fn rejects_every_truncated_header_length() {
        let bytes = valid_header();

        for length in 0..HEADER_LEN {
            assert_eq!(
                Sai2Header::parse(&bytes[..length]),
                Err(ParseError::Truncated {
                    expected: HEADER_LEN,
                    actual: length,
                })
            );
        }
    }

    #[test]
    fn rejects_an_invalid_signature() {
        let mut bytes = valid_header();
        bytes[0] = b'X';

        assert!(matches!(
            Sai2Header::parse(&bytes),
            Err(ParseError::InvalidSignature { .. })
        ));
    }

    #[test]
    fn preserves_undocumented_and_unknown_values() {
        let mut bytes = valid_header();
        bytes[40..56].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
        bytes[60..64].copy_from_slice(&[0, b'X', 0xff, b' ']);

        let header = Sai2Header::parse(&bytes).expect("unknown values should be preserved");

        assert_eq!(
            header.reserved(),
            [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
        assert_eq!(header.format_tag().as_bytes(), [0, b'X', 0xff, b' ']);
        assert_eq!(header.format_tag().to_string(), "\\x00X\\xff ");
    }

    #[test]
    fn ignores_bytes_after_the_fixed_header() {
        let mut bytes = valid_header().to_vec();
        bytes.extend_from_slice(b"future chunk data");

        let header = Sai2Header::parse(&bytes).expect("trailing data belongs to later phases");

        assert_eq!(header.chunk_count(), 12);
    }
}
