//! Small streaming RGBA PNG serializer used by the command-line tools.

use std::io::Write;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const STORED_BLOCK_MAX: usize = u16::MAX as usize;

/// Writes an eight-bit straight-alpha RGBA image as a PNG stream.
///
/// # Errors
///
/// Returns an error for invalid dimensions, inconsistent pixel-buffer length,
/// arithmetic overflow, or a failure while writing the destination stream.
pub fn write_rgba(
    mut output: impl Write,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("PNG dimensions must be nonzero".to_owned());
    }
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| "PNG row size overflow".to_owned())?;
    let height_usize = usize::try_from(height).map_err(|_| "PNG height overflow")?;
    let expected = row_bytes
        .checked_mul(height_usize)
        .ok_or_else(|| "PNG image size overflow".to_owned())?;
    if pixels.len() != expected {
        return Err(format!(
            "PNG pixel length mismatch: expected {expected}, found {}",
            pixels.len()
        ));
    }

    write_all(&mut output, PNG_SIGNATURE)?;
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(&mut output, *b"IHDR", &ihdr)?;
    write_idat(&mut output, pixels, row_bytes, height_usize)?;
    write_chunk(&mut output, *b"IEND", &[])?;
    Ok(())
}

fn write_idat(
    output: &mut impl Write,
    pixels: &[u8],
    row_bytes: usize,
    height: usize,
) -> Result<(), String> {
    let raw_len = row_bytes
        .checked_add(1)
        .and_then(|stride| stride.checked_mul(height))
        .ok_or_else(|| "PNG scanline size overflow".to_owned())?;
    let block_count = raw_len.div_ceil(STORED_BLOCK_MAX);
    let idat_len = raw_len
        .checked_add(
            block_count
                .checked_mul(5)
                .ok_or_else(|| "PNG block count overflow".to_owned())?,
        )
        .and_then(|length| length.checked_add(6)) // zlib header + Adler-32
        .ok_or_else(|| "PNG IDAT size overflow".to_owned())?;
    let idat_len = u32::try_from(idat_len).map_err(|_| "PNG IDAT exceeds u32 length")?;

    write_all(output, &idat_len.to_be_bytes())?;
    write_all(output, b"IDAT")?;
    let mut crc = Crc32::new();
    crc.update(b"IDAT");
    write_crc(output, &mut crc, &[0x78, 0x01])?;

    let mut zlib = StoredZlibWriter::new(output, &mut crc, raw_len);
    for row in pixels.chunks_exact(row_bytes) {
        zlib.write_raw(&[0])?; // PNG filter type: None
        zlib.write_raw(row)?;
    }
    let adler = zlib.finish()?;
    write_crc(output, &mut crc, &adler.to_be_bytes())?;
    write_all(output, &crc.finish().to_be_bytes())?;
    Ok(())
}

struct StoredZlibWriter<'a, W: Write> {
    output: &'a mut W,
    crc: &'a mut Crc32,
    block: Vec<u8>,
    raw_written: usize,
    raw_len: usize,
    adler: Adler32,
}

impl<'a, W: Write> StoredZlibWriter<'a, W> {
    fn new(output: &'a mut W, crc: &'a mut Crc32, raw_len: usize) -> Self {
        Self {
            output,
            crc,
            block: Vec::with_capacity(STORED_BLOCK_MAX),
            raw_written: 0,
            raw_len,
            adler: Adler32::new(),
        }
    }

    fn write_raw(&mut self, mut input: &[u8]) -> Result<(), String> {
        while !input.is_empty() {
            let available = STORED_BLOCK_MAX - self.block.len();
            let take = available.min(input.len());
            let (part, remaining) = input.split_at(take);
            self.block.extend_from_slice(part);
            self.adler.update(part);
            self.raw_written = self
                .raw_written
                .checked_add(part.len())
                .ok_or_else(|| "PNG raw byte count overflow".to_owned())?;
            input = remaining;
            if self.block.len() == STORED_BLOCK_MAX {
                self.flush_block()?;
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<u32, String> {
        if !self.block.is_empty() {
            self.flush_block()?;
        }
        if self.raw_written != self.raw_len {
            return Err("PNG raw byte count mismatch".to_owned());
        }
        Ok(self.adler.finish())
    }

    fn flush_block(&mut self) -> Result<(), String> {
        let final_block = self.raw_written == self.raw_len;
        write_crc(self.output, self.crc, &[u8::from(final_block)])?;
        let length = u16::try_from(self.block.len()).map_err(|_| "PNG block size overflow")?;
        write_crc(self.output, self.crc, &length.to_le_bytes())?;
        write_crc(self.output, self.crc, &(!length).to_le_bytes())?;
        write_crc(self.output, self.crc, &self.block)?;
        self.block.clear();
        Ok(())
    }
}

fn write_chunk(output: &mut impl Write, kind: [u8; 4], data: &[u8]) -> Result<(), String> {
    let length = u32::try_from(data.len()).map_err(|_| "PNG chunk exceeds u32 length")?;
    write_all(output, &length.to_be_bytes())?;
    write_all(output, &kind)?;
    write_all(output, data)?;

    let mut crc = Crc32::new();
    crc.update(&kind);
    crc.update(data);
    write_all(output, &crc.finish().to_be_bytes())?;
    Ok(())
}

fn write_crc(output: &mut impl Write, crc: &mut Crc32, data: &[u8]) -> Result<(), String> {
    write_all(output, data)?;
    crc.update(data);
    Ok(())
}

fn write_all(output: &mut impl Write, data: &[u8]) -> Result<(), String> {
    output
        .write_all(data)
        .map_err(|error| format!("could not write PNG: {error}"))
}

struct Adler32 {
    a: u32,
    b: u32,
}

impl Adler32 {
    const MODULUS: u32 = 65_521;

    const fn new() -> Self {
        Self { a: 1, b: 0 }
    }

    fn update(&mut self, input: &[u8]) {
        for byte in input {
            self.a = (self.a + u32::from(*byte)) % Self::MODULUS;
            self.b = (self.b + self.a) % Self::MODULUS;
        }
    }

    const fn finish(&self) -> u32 {
        (self.b << 16) | self.a
    }
}

struct Crc32(u32);

impl Crc32 {
    const fn new() -> Self {
        Self(u32::MAX)
    }

    fn update(&mut self, input: &[u8]) {
        for byte in input {
            self.0 ^= u32::from(*byte);
            for _ in 0..8 {
                let low_bit_mask = 0_u32.wrapping_sub(self.0 & 1);
                self.0 = (self.0 >> 1) ^ (0xedb8_8320 & low_bit_mask);
            }
        }
    }

    const fn finish(&self) -> u32 {
        !self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_minimal_rgba_png() {
        let mut png = Vec::new();
        write_rgba(&mut png, 1, 1, &[255, 0, 0, 255]).expect("PNG should encode");

        assert_eq!(&png[..8], PNG_SIGNATURE);
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 1);
        assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 1);
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
    }

    #[test]
    fn writes_multiple_stored_blocks() {
        let pixels = vec![0_u8; 256 * 256 * 4];
        let mut png = Vec::new();

        write_rgba(&mut png, 256, 256, &pixels).expect("large PNG should encode");

        assert_eq!(&png[..8], PNG_SIGNATURE);
        assert!(png.len() > pixels.len());
    }

    #[test]
    fn rejects_an_incorrect_pixel_length() {
        let mut png = Vec::new();
        assert_eq!(
            write_rgba(&mut png, 1, 1, &[0, 0, 0]),
            Err("PNG pixel length mismatch: expected 4, found 3".to_owned())
        );
    }
}
