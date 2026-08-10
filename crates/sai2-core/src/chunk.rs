use crate::{FourCc, HEADER_LEN, ParseError, Sai2Header};

/// Size of one entry in the SAI2 chunk table.
pub const CHUNK_ENTRY_LEN: usize = 16;

/// Metadata for one chunk body in a SAI2 document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    kind: FourCc,
    object_id: u32,
    offset: u64,
    size: usize,
}

impl Chunk {
    pub(crate) fn parse_all(input: &[u8], header: &Sai2Header) -> Result<Vec<Self>, ParseError> {
        let count =
            usize::try_from(header.chunk_count()).map_err(|_| ParseError::InvalidChunkCount {
                count: header.chunk_count(),
            })?;
        let table_len =
            count
                .checked_mul(CHUNK_ENTRY_LEN)
                .ok_or(ParseError::InvalidChunkCount {
                    count: header.chunk_count(),
                })?;
        let table_end = HEADER_LEN
            .checked_add(table_len)
            .ok_or(ParseError::InvalidChunkCount {
                count: header.chunk_count(),
            })?;

        if input.len() < table_end {
            return Err(ParseError::Truncated {
                expected: table_end,
                actual: input.len(),
            });
        }

        // Allocation is delayed until the complete table is known to fit in
        // the input, so an untrusted count cannot cause arbitrary allocation.
        let mut entries = Vec::with_capacity(count);
        for index in 0..count {
            let entry_offset = HEADER_LEN + index * CHUNK_ENTRY_LEN;
            entries.push(RawChunk {
                kind: FourCc::from_bytes(bytes_at::<4>(input, entry_offset)?),
                object_id: u32_at(input, entry_offset + 4)?,
                offset: u64_at(input, entry_offset + 8)?,
            });
        }

        for (index, entry) in entries.iter().enumerate() {
            let offset = usize::try_from(entry.offset)
                .map_err(|_| invalid_offset(index, entry.offset, table_end, input.len()))?;
            if offset < table_end || offset > input.len() {
                return Err(invalid_offset(index, entry.offset, table_end, input.len()));
            }

            if let Some(previous) = index.checked_sub(1).and_then(|i| entries.get(i)) {
                if previous.offset > entry.offset {
                    return Err(ParseError::ChunkOffsetsOutOfOrder {
                        previous_index: index - 1,
                        previous_offset: previous.offset,
                        index,
                        offset: entry.offset,
                    });
                }
            }
        }

        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let offset = usize::try_from(entry.offset)
                    .map_err(|_| invalid_offset(index, entry.offset, table_end, input.len()))?;
                let end = if let Some(next) = entries.get(index + 1) {
                    usize::try_from(next.offset).map_err(|_| {
                        invalid_offset(index + 1, next.offset, table_end, input.len())
                    })?
                } else {
                    input.len()
                };

                Ok(Self {
                    kind: entry.kind,
                    object_id: entry.object_id,
                    offset: entry.offset,
                    size: end - offset,
                })
            })
            .collect()
    }

    /// Returns the exact four-byte chunk type.
    #[must_use]
    pub const fn kind(&self) -> FourCc {
        self.kind
    }

    /// Returns the object identifier associated with this chunk.
    #[must_use]
    pub const fn object_id(&self) -> u32 {
        self.object_id
    }

    /// Returns the absolute byte offset of the chunk body.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Returns the chunk body's derived size in bytes.
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }
}

#[derive(Debug, Clone, Copy)]
struct RawChunk {
    kind: FourCc,
    object_id: u32,
    offset: u64,
}

fn invalid_offset(index: usize, offset: u64, table_end: usize, file_len: usize) -> ParseError {
    ParseError::InvalidChunkOffset {
        index,
        offset,
        table_end,
        file_len,
    }
}

fn u32_at(input: &[u8], offset: usize) -> Result<u32, ParseError> {
    Ok(u32::from_le_bytes(bytes_at::<4>(input, offset)?))
}

fn u64_at(input: &[u8], offset: usize) -> Result<u64, ParseError> {
    Ok(u64::from_le_bytes(bytes_at::<8>(input, offset)?))
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
    use crate::{SAI2_MAGIC, Sai2Document};

    fn document_with_chunks() -> Vec<u8> {
        let mut bytes = vec![0_u8; 106];
        bytes[0..16].copy_from_slice(&SAI2_MAGIC);
        bytes[20..24].copy_from_slice(&32_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&32_u32.to_le_bytes());
        bytes[32..36].copy_from_slice(&2_u32.to_le_bytes());
        bytes[60..64].copy_from_slice(b"norm");

        bytes[64..68].copy_from_slice(b"hist");
        bytes[68..72].copy_from_slice(&7_u32.to_le_bytes());
        bytes[72..76].copy_from_slice(&96_u32.to_le_bytes());

        bytes[80..84].copy_from_slice(&[0xff, 0, b'X', b' ']);
        bytes[84..88].copy_from_slice(&9_u32.to_le_bytes());
        bytes[88..92].copy_from_slice(&100_u32.to_le_bytes());

        bytes[96..100].copy_from_slice(b"1234");
        bytes[100..106].copy_from_slice(b"567890");
        bytes
    }

    #[test]
    fn parses_chunk_metadata_and_derives_sizes() {
        let document =
            Sai2Document::parse(&document_with_chunks()).expect("chunk table should parse");

        assert_eq!(document.chunks().len(), 2);
        assert_eq!(document.chunks()[0].kind(), FourCc::from_bytes(*b"hist"));
        assert_eq!(document.chunks()[0].object_id(), 7);
        assert_eq!(document.chunks()[0].offset(), 96);
        assert_eq!(document.chunks()[0].size(), 4);
        assert_eq!(
            document.chunks()[1].kind().as_bytes(),
            [0xff, 0, b'X', b' ']
        );
        assert_eq!(document.chunks()[1].offset(), 100);
        assert_eq!(document.chunks()[1].size(), 6);
    }

    #[test]
    fn rejects_a_truncated_chunk_table_before_allocating() {
        let mut bytes = document_with_chunks();
        bytes.truncate(95);

        assert_eq!(
            Sai2Document::parse(&bytes),
            Err(ParseError::Truncated {
                expected: 96,
                actual: 95,
            })
        );
    }

    #[test]
    fn rejects_offsets_inside_the_chunk_table() {
        let mut bytes = document_with_chunks();
        bytes[72..76].copy_from_slice(&95_u32.to_le_bytes());

        assert!(matches!(
            Sai2Document::parse(&bytes),
            Err(ParseError::InvalidChunkOffset { index: 0, .. })
        ));
    }

    #[test]
    fn rejects_offsets_after_the_end_of_the_file() {
        let mut bytes = document_with_chunks();
        bytes[88..92].copy_from_slice(&107_u32.to_le_bytes());

        assert!(matches!(
            Sai2Document::parse(&bytes),
            Err(ParseError::InvalidChunkOffset { index: 1, .. })
        ));
    }

    #[test]
    fn rejects_decreasing_offsets() {
        let mut bytes = document_with_chunks();
        bytes[72..76].copy_from_slice(&101_u32.to_le_bytes());

        assert!(matches!(
            Sai2Document::parse(&bytes),
            Err(ParseError::ChunkOffsetsOutOfOrder { index: 1, .. })
        ));
    }

    #[test]
    fn retains_equal_offsets_as_an_empty_chunk() {
        let mut bytes = document_with_chunks();
        bytes[72..76].copy_from_slice(&100_u32.to_le_bytes());

        let document = Sai2Document::parse(&bytes).expect("equal offsets should remain readable");

        assert_eq!(document.chunks()[0].size(), 0);
        assert_eq!(document.chunks()[1].size(), 6);
    }

    #[test]
    fn accepts_an_empty_chunk_table() {
        let mut bytes = document_with_chunks();
        bytes[32..36].copy_from_slice(&0_u32.to_le_bytes());
        bytes.truncate(64);

        let document = Sai2Document::parse(&bytes).expect("empty table should parse");

        assert!(document.chunks().is_empty());
    }
}
