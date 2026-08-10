use crate::{FourCc, ParseError, Sai2Document};

const TILE_SIZE: usize = 256;
const OUTPUT_CHANNELS: usize = 4;

/// Resource limits applied before integrated-image decoding allocates output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    /// Maximum number of output pixels.
    pub max_pixels: u64,
}

impl Default for DecodeLimits {
    fn default() -> Self {
        Self {
            max_pixels: 64 * 1024 * 1024,
        }
    }
}

/// An eight-bit RGBA image decoded from the document's `intg` chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    pub(crate) fn from_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }

    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns row-major pixels in red, green, blue, alpha byte order.
    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// Decodes the merged image stored in the first `intg` chunk.
///
/// # Errors
///
/// Returns [`ParseError`] for an invalid top-level document, a missing or
/// unsupported integrated image, dimensions above `limits`, or malformed tile
/// framing, RLE, and delta data.
#[allow(clippy::too_many_lines)]
pub fn decode_integrated_image(
    input: &[u8],
    limits: DecodeLimits,
) -> Result<RgbaImage, ParseError> {
    let document = Sai2Document::parse(input)?;
    let header = document.header();
    let width = usize::try_from(header.width()).map_err(|_| invalid_dimensions(header))?;
    let height = usize::try_from(header.height()).map_err(|_| invalid_dimensions(header))?;
    if width == 0 || height == 0 {
        return Err(invalid_dimensions(header));
    }

    let pixel_count = u64::from(header.width())
        .checked_mul(u64::from(header.height()))
        .ok_or_else(|| invalid_dimensions(header))?;
    if pixel_count > limits.max_pixels {
        return Err(ParseError::ImageTooLarge {
            pixels: pixel_count,
            max_pixels: limits.max_pixels,
        });
    }
    let output_len = usize::try_from(pixel_count)
        .ok()
        .and_then(|pixels| pixels.checked_mul(OUTPUT_CHANNELS))
        .ok_or_else(|| invalid_dimensions(header))?;

    let chunk = document
        .chunks()
        .iter()
        .find(|chunk| chunk.kind() == FourCc::from_bytes(*b"intg"))
        .ok_or(ParseError::MissingIntegratedImage)?;
    let chunk_start = usize::try_from(chunk.offset()).map_err(|_| malformed("chunk offset"))?;
    let chunk_end = chunk_start
        .checked_add(chunk.size())
        .ok_or_else(|| malformed("chunk range"))?;
    let body = input
        .get(chunk_start..chunk_end)
        .ok_or_else(|| malformed("chunk range"))?;
    let encoding = read_array::<4>(body, 0)?;
    if encoding != *b"dpcm" {
        return Err(ParseError::UnsupportedIntegratedImage { found: encoding });
    }

    let tiles_x = width.div_ceil(TILE_SIZE);
    let tiles_y = height.div_ceil(TILE_SIZE);
    if tiles_x > usize::from(u8::MAX) {
        return Err(malformed("tile row has more than 255 columns"));
    }
    let tile_count = tiles_x
        .checked_mul(tiles_y)
        .ok_or_else(|| malformed("tile count overflow"))?;
    let table_len = tile_count
        .checked_mul(4)
        .ok_or_else(|| malformed("tile-size table overflow"))?;
    let table_end = 4_usize
        .checked_add(table_len)
        .ok_or_else(|| malformed("tile-size table overflow"))?;
    if body.len() < table_end {
        return Err(malformed("truncated tile-size table"));
    }

    let mut tile_sizes = Vec::with_capacity(tile_count);
    for index in 0..tile_count {
        let offset = 4 + index * 4;
        let size = usize::try_from(u32::from_le_bytes(read_array::<4>(body, offset)?))
            .map_err(|_| malformed("tile size does not fit in memory"))?;
        tile_sizes.push(size);
    }

    let input_channels = if header.integrated_image_has_alpha() {
        4
    } else {
        3
    };
    let mut pixels = vec![0_u8; output_len];
    let mut body_offset = table_end;

    for tile_y in 0..tiles_y {
        let tile_height = (height - tile_y * TILE_SIZE).min(TILE_SIZE);
        for tile_x in 0..tiles_x {
            let table_index = tile_y * tiles_x + tile_x;
            let tile_size = tile_sizes[table_index];
            let tile_end = body_offset
                .checked_add(tile_size)
                .ok_or_else(|| malformed("tile range overflow"))?;
            let tile = body
                .get(body_offset..tile_end)
                .ok_or_else(|| malformed("truncated tile data"))?;
            body_offset = tile_end;
            if tile.len() < 2 {
                return Err(malformed("tile is missing its marker"));
            }
            validate_marker(u16::from_le_bytes([tile[0], tile[1]]), tile_x)?;

            let tile_width = (width - tile_x * TILE_SIZE).min(TILE_SIZE);
            let mut compressed = &tile[2..];
            let mut previous_row = [[0_u8; OUTPUT_CHANNELS]; TILE_SIZE];

            for row in 0..tile_height {
                let mut deltas = [0_i16; TILE_SIZE * OUTPUT_CHANNELS];
                let consumed = decode_delta_row(
                    compressed,
                    &mut deltas[..tile_width * OUTPUT_CHANNELS],
                    tile_width,
                    input_channels,
                    OUTPUT_CHANNELS,
                )?;
                compressed = compressed
                    .get(consumed..)
                    .ok_or_else(|| malformed("row consumed beyond tile data"))?;

                let current_row = reconstruct_row(
                    &deltas[..tile_width * OUTPUT_CHANNELS],
                    &previous_row[..tile_width],
                    tile_width,
                    input_channels,
                );
                let image_y = tile_y * TILE_SIZE + row;
                let image_x = tile_x * TILE_SIZE;
                let destination = (image_y * width + image_x) * OUTPUT_CHANNELS;
                for (x, bgra) in current_row[..tile_width].iter().enumerate() {
                    let pixel = destination + x * OUTPUT_CHANNELS;
                    pixels[pixel..pixel + OUTPUT_CHANNELS]
                        .copy_from_slice(&[bgra[2], bgra[1], bgra[0], bgra[3]]);
                }
                previous_row = current_row;
            }

            if !compressed.is_empty() {
                return Err(malformed("tile has unused compressed bytes"));
            }
        }

        let marker_end = body_offset
            .checked_add(2)
            .ok_or_else(|| malformed("tile-row marker overflow"))?;
        let marker_bytes = body
            .get(body_offset..marker_end)
            .ok_or_else(|| malformed("missing tile-row marker"))?;
        validate_marker(
            u16::from_le_bytes([marker_bytes[0], marker_bytes[1]]),
            tiles_x,
        )?;
        body_offset = marker_end;
    }

    let padding = body
        .get(body_offset..)
        .ok_or_else(|| malformed("invalid chunk padding"))?;
    if padding.len() > 3 || padding.iter().any(|byte| *byte != 0) {
        return Err(malformed("invalid chunk padding"));
    }

    Ok(RgbaImage {
        width: header.width(),
        height: header.height(),
        pixels,
    })
}

pub(crate) fn decode_delta_row(
    compressed: &[u8],
    deltas: &mut [i16],
    pixel_count: usize,
    input_channels: usize,
    output_channels: usize,
) -> Result<usize, ParseError> {
    let expected = pixel_count
        .checked_mul(output_channels)
        .ok_or_else(|| malformed("row size overflow"))?;
    if deltas.len() != expected
        || !(1..=4).contains(&input_channels)
        || !(input_channels..=4).contains(&output_channels)
    {
        return Err(malformed("invalid row buffer or channel count"));
    }

    let mut bits = LsbBits::new(compressed);
    for channel in 0..input_channels {
        let mut written = 0_usize;
        while written < pixel_count {
            let mut zero_prefix = 0_u8;
            while bits.read_bit()? == 0 {
                zero_prefix = zero_prefix
                    .checked_add(1)
                    .ok_or_else(|| malformed("RLE opcode overflow"))?;
                if zero_prefix > 7 {
                    return Err(malformed("RLE opcode exceeds 15"));
                }
            }
            let opcode = usize::from(zero_prefix) * 2 + usize::from(bits.read_bit()?);

            match opcode {
                0 => {
                    deltas[written * output_channels + channel] = 0;
                    written += 1;
                }
                1..=14 => {
                    let low = bits.read_bits(opcode)?;
                    let negative = bits.read_bit()? != 0;
                    let magnitude = ((1_u32 << opcode) | low) - 1;
                    let magnitude = i16::try_from(magnitude)
                        .map_err(|_| malformed("delta exceeds signed 16-bit range"))?;
                    deltas[written * output_channels + channel] =
                        if negative { -magnitude } else { magnitude };
                    written += 1;
                }
                15 => {
                    let run = usize::try_from(bits.read_bits(7)?)
                        .map_err(|_| malformed("zero run does not fit in memory"))?
                        + 8;
                    if run > pixel_count - written {
                        return Err(malformed("zero run exceeds row width"));
                    }
                    written += run;
                }
                _ => return Err(malformed("invalid RLE opcode")),
            }
        }
    }

    Ok(bits.bytes_consumed())
}

fn reconstruct_row(
    deltas: &[i16],
    previous_row: &[[u8; OUTPUT_CHANNELS]],
    pixel_count: usize,
    input_channels: usize,
) -> [[u8; OUTPUT_CHANNELS]; TILE_SIZE] {
    let mut result = [[0_u8; OUTPUT_CHANNELS]; TILE_SIZE];
    let mut sums = [0_u16; OUTPUT_CHANNELS];
    let mut previous_above = [0_u16; OUTPUT_CHANNELS];

    for x in 0..pixel_count {
        for channel in 0..OUTPUT_CHANNELS {
            let above = u16::from(previous_row[x][channel]);
            let predicted = sums[channel]
                .wrapping_add(above)
                .saturating_sub(previous_above[channel])
                .saturating_add(0xff00)
                .saturating_sub(0xff00);
            let delta = u16::from_ne_bytes(deltas[x * OUTPUT_CHANNELS + channel].to_ne_bytes());
            sums[channel] = predicted.wrapping_add(delta);
            result[x][channel] = sums[channel].min(255).to_le_bytes()[0];
            previous_above[channel] = above;
        }
        if input_channels == 3 {
            result[x][3] = 255;
        }
    }

    result
}

fn validate_marker(marker: u16, expected_index: usize) -> Result<(), ParseError> {
    let expected = u16::try_from(expected_index)
        .ok()
        .and_then(|index| index.checked_shl(8))
        .map(|index| index | 0x00ff)
        .ok_or_else(|| malformed("tile marker index overflow"))?;
    if marker != expected {
        return Err(malformed("unexpected tile marker"));
    }
    Ok(())
}

fn read_array<const N: usize>(input: &[u8], offset: usize) -> Result<[u8; N], ParseError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| malformed("byte range overflow"))?;
    input
        .get(offset..end)
        .ok_or_else(|| malformed("truncated integrated-image header"))?
        .try_into()
        .map_err(|_| malformed("truncated integrated-image header"))
}

fn invalid_dimensions(header: &crate::Sai2Header) -> ParseError {
    ParseError::InvalidImageDimensions {
        width: header.width(),
        height: header.height(),
    }
}

const fn malformed(reason: &'static str) -> ParseError {
    ParseError::MalformedDpcm { reason }
}

struct LsbBits<'a> {
    input: &'a [u8],
    bit_offset: usize,
}

impl<'a> LsbBits<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            bit_offset: 0,
        }
    }

    fn read_bit(&mut self) -> Result<u8, ParseError> {
        let byte = self
            .input
            .get(self.bit_offset / 8)
            .ok_or_else(|| malformed("truncated RLE bitstream"))?;
        let bit = (byte >> (self.bit_offset % 8)) & 1;
        self.bit_offset += 1;
        Ok(bit)
    }

    fn read_bits(&mut self, count: usize) -> Result<u32, ParseError> {
        let mut value = 0_u32;
        for shift in 0..count {
            value |= u32::from(self.read_bit()?) << shift;
        }
        Ok(value)
    }

    const fn bytes_consumed(&self) -> usize {
        self.bit_offset.div_ceil(8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SAI2_MAGIC;

    #[test]
    fn decodes_positive_negative_and_zero_deltas() {
        let compressed = encode_row(&[[1, -1, 2, -2, 0]]);
        let mut deltas = [0_i16; 5 * OUTPUT_CHANNELS];

        let consumed = decode_delta_row(&compressed, &mut deltas, 5, 1, OUTPUT_CHANNELS)
            .expect("row should decode");

        assert_eq!(consumed, compressed.len());
        assert_eq!(
            deltas
                .iter()
                .step_by(OUTPUT_CHANNELS)
                .copied()
                .collect::<Vec<_>>(),
            vec![1, -1, 2, -2, 0]
        );
    }

    #[test]
    fn rejects_a_zero_run_beyond_the_row() {
        let mut writer = BitWriter::default();
        writer.write_opcode(15);
        writer.write_bits(1, 7); // run length 9 for an eight-pixel row
        let mut deltas = [0_i16; 8 * OUTPUT_CHANNELS];

        assert_eq!(
            decode_delta_row(&writer.finish(), &mut deltas, 8, 3, OUTPUT_CHANNELS),
            Err(malformed("zero run exceeds row width"))
        );
    }

    #[test]
    fn decodes_a_synthetic_opaque_red_pixel() {
        let row = encode_row(&[[0], [0], [255]]);
        let document = synthetic_document(1, 1, 0x0100, &row);

        let image = decode_integrated_image(&document, DecodeLimits::default())
            .expect("synthetic integrated image should decode");

        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
        assert_eq!(image.pixels(), &[255, 0, 0, 255]);
    }

    #[test]
    fn decodes_a_synthetic_transparent_pixel() {
        let row = encode_row(&[[0], [0], [0], [0]]);
        let document = synthetic_document(1, 1, 0x2000, &row);

        let image = decode_integrated_image(&document, DecodeLimits::default())
            .expect("synthetic integrated image should decode");

        assert_eq!(image.pixels(), &[0, 0, 0, 0]);
    }

    #[test]
    fn enforces_the_pixel_limit_before_output_allocation() {
        let row = encode_row(&[[0], [0], [0]]);
        let document = synthetic_document(1, 1, 0x0100, &row);

        assert_eq!(
            decode_integrated_image(&document, DecodeLimits { max_pixels: 0 }),
            Err(ParseError::ImageTooLarge {
                pixels: 1,
                max_pixels: 0,
            })
        );
    }

    #[test]
    fn rejects_an_unexpected_tile_marker() {
        let row = encode_row(&[[0], [0], [0]]);
        let mut document = synthetic_document(1, 1, 0x0100, &row);
        document[88..90].copy_from_slice(&0_u16.to_le_bytes());

        assert_eq!(
            decode_integrated_image(&document, DecodeLimits::default()),
            Err(malformed("unexpected tile marker"))
        );
    }

    #[test]
    fn decodes_four_tiles_with_partial_right_and_bottom_edges() {
        let document = synthetic_tiled_blank(257, 257);

        let image = decode_integrated_image(&document, DecodeLimits::default())
            .expect("four-tile image should decode");

        assert_eq!(image.width(), 257);
        assert_eq!(image.height(), 257);
        assert_eq!(image.pixels().len(), 257 * 257 * OUTPUT_CHANNELS);
        assert!(image.pixels().iter().all(|byte| *byte == 0));
    }

    fn synthetic_document(width: u32, height: u32, flags: u32, row: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(b"dpcm");
        let tile_size = u32::try_from(2 + row.len()).expect("test row should fit");
        body.extend_from_slice(&tile_size.to_le_bytes());
        body.extend_from_slice(&0x00ff_u16.to_le_bytes());
        body.extend_from_slice(row);
        body.extend_from_slice(&0x01ff_u16.to_le_bytes());
        while body.len() % 4 != 0 {
            body.push(0);
        }

        wrap_integrated_body(width, height, flags, &body)
    }

    fn synthetic_tiled_blank(width: u32, height: u32) -> Vec<u8> {
        let width_usize = usize::try_from(width).expect("test width should fit");
        let height_usize = usize::try_from(height).expect("test height should fit");
        let tiles_x = width_usize.div_ceil(TILE_SIZE);
        let tiles_y = height_usize.div_ceil(TILE_SIZE);
        let mut tiles = Vec::new();

        for tile_y in 0..tiles_y {
            let tile_height = (height_usize - tile_y * TILE_SIZE).min(TILE_SIZE);
            for tile_x in 0..tiles_x {
                let tile_width = (width_usize - tile_x * TILE_SIZE).min(TILE_SIZE);
                let mut tile = Vec::new();
                let marker =
                    (u16::try_from(tile_x).expect("test tile index should fit") << 8) | 0x00ff;
                tile.extend_from_slice(&marker.to_le_bytes());
                let row = encode_zero_row(tile_width, 4);
                for _ in 0..tile_height {
                    tile.extend_from_slice(&row);
                }
                tiles.push(tile);
            }
        }

        let mut body = Vec::new();
        body.extend_from_slice(b"dpcm");
        for tile in &tiles {
            let size = u32::try_from(tile.len()).expect("test tile should fit");
            body.extend_from_slice(&size.to_le_bytes());
        }
        for tile_y in 0..tiles_y {
            for tile_x in 0..tiles_x {
                body.extend_from_slice(&tiles[tile_y * tiles_x + tile_x]);
            }
            let marker =
                (u16::try_from(tiles_x).expect("test tile count should fit") << 8) | 0x00ff;
            body.extend_from_slice(&marker.to_le_bytes());
        }
        while body.len() % 4 != 0 {
            body.push(0);
        }

        wrap_integrated_body(width, height, 0x2000, &body)
    }

    fn wrap_integrated_body(width: u32, height: u32, flags: u32, body: &[u8]) -> Vec<u8> {
        let mut document = vec![0_u8; 80];
        document[0..16].copy_from_slice(&SAI2_MAGIC);
        document[16..20].copy_from_slice(&flags.to_le_bytes());
        document[20..24].copy_from_slice(&width.to_le_bytes());
        document[24..28].copy_from_slice(&height.to_le_bytes());
        document[32..36].copy_from_slice(&1_u32.to_le_bytes());
        document[60..64].copy_from_slice(b"norm");
        document[64..68].copy_from_slice(b"intg");
        document[72..80].copy_from_slice(&80_u64.to_le_bytes());
        document.extend_from_slice(body);
        document
    }

    fn encode_zero_row(pixel_count: usize, channels: usize) -> Vec<u8> {
        let mut writer = BitWriter::default();
        for _ in 0..channels {
            for _ in 0..pixel_count {
                writer.write_opcode(0);
            }
        }
        writer.finish()
    }

    fn encode_row<const CHANNELS: usize, const PIXELS: usize>(
        channels: &[[i16; PIXELS]; CHANNELS],
    ) -> Vec<u8> {
        let mut writer = BitWriter::default();
        for channel in channels {
            for value in channel {
                writer.write_value(*value);
            }
        }
        writer.finish()
    }

    #[derive(Default)]
    struct BitWriter {
        bytes: Vec<u8>,
        bit_offset: usize,
    }

    impl BitWriter {
        fn write_value(&mut self, value: i16) {
            if value == 0 {
                self.write_opcode(0);
                return;
            }

            let magnitude = u32::from(value.unsigned_abs());
            let opcode = (magnitude + 1).ilog2();
            self.write_opcode(usize::try_from(opcode).expect("opcode should fit"));
            let leading = 1_u32 << opcode;
            self.write_bits(
                magnitude + 1 - leading,
                usize::try_from(opcode).expect("opcode should fit"),
            );
            self.write_bits(u32::from(u8::from(value.is_negative())), 1);
        }

        fn write_opcode(&mut self, opcode: usize) {
            let zero_prefix = opcode / 2;
            self.write_bits(0, zero_prefix);
            self.write_bits(1, 1);
            self.write_bits(u32::try_from(opcode % 2).expect("opcode bit should fit"), 1);
        }

        fn write_bits(&mut self, value: u32, count: usize) {
            for shift in 0..count {
                if self.bit_offset / 8 == self.bytes.len() {
                    self.bytes.push(0);
                }
                let bit = u8::try_from((value >> shift) & 1).expect("bit should fit");
                self.bytes[self.bit_offset / 8] |= bit << (self.bit_offset % 8);
                self.bit_offset += 1;
            }
        }

        fn finish(self) -> Vec<u8> {
            self.bytes
        }
    }
}
