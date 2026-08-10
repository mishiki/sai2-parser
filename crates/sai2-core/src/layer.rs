use crate::{DecodeLimits, FourCc, ParseError, RgbaImage, Sai2Document};

const LAYER_HEADER_LEN: usize = 56;
const BLOCK_SIZE: usize = 32;
const BLOCK_SIZE_U32: u32 = 32;
const BLOCK_PIXELS: usize = BLOCK_SIZE * BLOCK_SIZE;
const CHANNELS: usize = 4;
const CHANNEL_MAX: i32 = 0x4000;

/// Metadata and, when supported, decoded pixels for one SAI2 layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sai2Layer {
    id: u32,
    layer_type: FourCc,
    blend_mode: FourCc,
    opacity: u8,
    flags: u32,
    name: String,
    tile_count: u32,
    image: Option<RgbaImage>,
}

impl Sai2Layer {
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }
    #[must_use]
    pub const fn layer_type(&self) -> FourCc {
        self.layer_type
    }
    #[must_use]
    pub const fn blend_mode(&self) -> FourCc {
        self.blend_mode
    }
    #[must_use]
    pub const fn opacity(&self) -> u8 {
        self.opacity
    }
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn tile_count(&self) -> u32 {
        self.tile_count
    }
    #[must_use]
    pub const fn image(&self) -> Option<&RgbaImage> {
        self.image.as_ref()
    }
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.flags & 0x0001_0000 != 0
    }
}

/// Parses layer descriptors and decodes the raster layers supported so far.
///
/// Unsupported layer-pixel layouts remain present as metadata with no image.
/// This lets callers preserve layer order while compatibility grows.
///
/// # Errors
///
/// Returns [`ParseError`] when the document or a layer descriptor is malformed,
/// or when the canvas exceeds `limits`.
pub fn decode_layers(input: &[u8], limits: DecodeLimits) -> Result<Vec<Sai2Layer>, ParseError> {
    let document = Sai2Document::parse(input)?;
    let header = document.header();
    let pixels = u64::from(header.width())
        .checked_mul(u64::from(header.height()))
        .ok_or(ParseError::InvalidImageDimensions {
            width: header.width(),
            height: header.height(),
        })?;
    if pixels > limits.max_pixels {
        return Err(ParseError::ImageTooLarge {
            pixels,
            max_pixels: limits.max_pixels,
        });
    }

    let mut layers = Vec::new();
    for chunk in document
        .chunks()
        .iter()
        .filter(|chunk| chunk.kind() == FourCc::from_bytes(*b"layr"))
    {
        let body = chunk_body(input, chunk)?;
        let mut layer = parse_layer(body)?;
        if layer.layer_type == FourCc::from_bytes(*b"norm") {
            if let Some(pixel_chunk) = document.chunks().iter().find(|candidate| {
                candidate.kind() == FourCc::from_bytes(*b"lpix")
                    && candidate.object_id() == layer.id
            }) {
                let pixel_body = chunk_body(input, pixel_chunk)?;
                layer.image = decode_small_raster_layer(
                    pixel_body,
                    layer.tile_count,
                    header.width(),
                    header.height(),
                )?;
            }
        }
        layers.push(layer);
    }
    Ok(layers)
}

fn parse_layer(body: &[u8]) -> Result<Sai2Layer, ParseError> {
    if body.len() < LAYER_HEADER_LEN || read::<4>(body, 0)? != *b"layr" {
        return Err(layer_error("invalid layr header"));
    }
    let id = u32::from_le_bytes(read(body, 4)?);
    let layer_type = FourCc::from_bytes(read(body, 16)?);
    let tile_count = u32::from_le_bytes(read(body, 40)?);
    let blend_mode = FourCc::from_bytes(read(body, 44)?);
    let opacity_raw = u32::from_le_bytes(read(body, 48)?);
    let opacity = u8::try_from(opacity_raw.min(100)).map_err(|_| layer_error("invalid opacity"))?;
    let flags = u32::from_le_bytes(read(body, 52)?);

    let mut name = format!("Layer {id}");
    let mut offset = LAYER_HEADER_LEN;
    while offset + 4 <= body.len() {
        let tag = read::<4>(body, offset)?;
        offset += 4;
        if tag == [0; 4] {
            break;
        }
        let length = usize::try_from(u32::from_le_bytes(read(body, offset)?))
            .map_err(|_| layer_error("layer parameter is too large"))?;
        offset += 4;
        let end = offset
            .checked_add(length)
            .ok_or_else(|| layer_error("layer parameter overflow"))?;
        let value = body
            .get(offset..end)
            .ok_or_else(|| layer_error("truncated layer parameter"))?;
        if tag == *b"name" {
            name = decode_name(value)?;
        }
        offset = end;
    }

    Ok(Sai2Layer {
        id,
        layer_type,
        blend_mode,
        opacity,
        flags,
        name,
        tile_count,
        image: None,
    })
}

fn decode_name(value: &[u8]) -> Result<String, ParseError> {
    if value.len() < 2 {
        return Err(layer_error("truncated layer name"));
    }
    let count = usize::from(u16::from_le_bytes(read(value, 0)?));
    let bytes = count
        .checked_mul(2)
        .and_then(|n| n.checked_add(2))
        .ok_or_else(|| layer_error("layer name length overflow"))?;
    let encoded = value
        .get(2..bytes)
        .ok_or_else(|| layer_error("truncated UTF-16 layer name"))?;
    let units = encoded
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| layer_error("invalid UTF-16 layer name"))
}

fn decode_small_raster_layer(
    body: &[u8],
    tile_count: u32,
    width: u32,
    height: u32,
) -> Result<Option<RgbaImage>, ParseError> {
    if tile_count == 0 {
        let len = image_len(width, height)?;
        return Ok(Some(RgbaImage::from_pixels(width, height, vec![0; len])));
    }
    // The first compatibility slice deliberately handles the fully observed
    // one-block fixture layout. Larger sparse tile grids remain metadata-only.
    if tile_count != 1 || width > BLOCK_SIZE_U32 || height > BLOCK_SIZE_U32 {
        return Ok(None);
    }
    if body.len() < 8 || read::<4>(body, 0)? != *b"dpcm" {
        return Err(layer_error("unsupported lpix encoding"));
    }
    let tile_size = usize::try_from(u32::from_le_bytes(read(body, 4)?))
        .map_err(|_| layer_error("lpix tile is too large"))?;
    let tile_end = 8_usize
        .checked_add(tile_size)
        .ok_or_else(|| layer_error("lpix tile overflow"))?;
    let tile = body
        .get(8..tile_end)
        .ok_or_else(|| layer_error("truncated lpix tile"))?;
    let values = decode_first_block(tile)?.unwrap_or([[0; CHANNELS]; BLOCK_PIXELS]);

    let width_usize = usize::try_from(width).map_err(|_| layer_error("invalid layer width"))?;
    let height_usize = usize::try_from(height).map_err(|_| layer_error("invalid layer height"))?;
    let mut rgba = Vec::with_capacity(image_len(width, height)?);
    for y in 0..height_usize {
        for x in 0..width_usize {
            let pixel = values[y * BLOCK_SIZE + x];
            let alpha = pixel[3];
            let r = unpremultiply(pixel[2], alpha);
            let g = unpremultiply(pixel[1], alpha);
            let b = unpremultiply(pixel[0], alpha);
            rgba.extend_from_slice(&[r, g, b, scale_14_to_8(alpha)]);
        }
    }
    Ok(Some(RgbaImage::from_pixels(width, height, rgba)))
}

fn decode_first_block(tile: &[u8]) -> Result<Option<[[i32; CHANNELS]; BLOCK_PIXELS]>, ParseError> {
    let mut offset = 0;
    while offset + 2 <= tile.len() {
        let marker = u16::from_le_bytes(read(tile, offset)?);
        offset += 2;
        if marker & 0xff != 0xff {
            return Err(layer_error("invalid lpix block marker"));
        }
        let kind = (marker >> 12) as u8;
        let index = usize::from((marker >> 8) & 0x0f);
        match kind {
            0x0 => {
                let _skip = u16::from_le_bytes(read(tile, offset)?);
                offset += 2;
            }
            0x5 => {
                let mut color = [0_i32; CHANNELS];
                for channel in &mut color {
                    *channel = i32::from(u16::from_le_bytes(read(tile, offset)?));
                    offset += 2;
                }
                if index == 0 {
                    return Ok(Some([color; BLOCK_PIXELS]));
                }
            }
            0xa => {
                let size = usize::from(u16::from_le_bytes(read(tile, offset)?));
                offset += 2;
                let end = offset
                    .checked_add(size)
                    .ok_or_else(|| layer_error("lpix block overflow"))?;
                let compressed = tile
                    .get(offset..end)
                    .ok_or_else(|| layer_error("truncated lpix block"))?;
                if index == 0 {
                    return decode_dpcm_block(compressed).map(Some);
                }
                offset = end;
            }
            0xf => return Ok(None),
            _ => return Err(layer_error("unsupported lpix block kind")),
        }
    }
    Err(layer_error("lpix tile has no terminator"))
}

fn decode_dpcm_block(compressed: &[u8]) -> Result<[[i32; CHANNELS]; BLOCK_PIXELS], ParseError> {
    let mut deltas = [0_i16; BLOCK_PIXELS * CHANNELS];
    let consumed = crate::image::decode_delta_row(compressed, &mut deltas, BLOCK_PIXELS, CHANNELS)?;
    if consumed != compressed.len() {
        return Err(layer_error("lpix block has unused compressed bytes"));
    }
    let mut values = [[0_i32; CHANNELS]; BLOCK_PIXELS];
    for y in 0..BLOCK_SIZE {
        let mut left = [0_i32; CHANNELS];
        let mut upper_left = [0_i32; CHANNELS];
        for x in 0..BLOCK_SIZE {
            let index = y * BLOCK_SIZE + x;
            for channel in 0..CHANNELS {
                let above = if y == 0 {
                    0
                } else {
                    values[index - BLOCK_SIZE][channel]
                };
                let predicted = (left[channel] + above - upper_left[channel]).clamp(0, CHANNEL_MAX);
                let value = (predicted + i32::from(deltas[index * CHANNELS + channel]))
                    .clamp(0, CHANNEL_MAX);
                values[index][channel] = value;
                left[channel] = value;
                upper_left[channel] = above;
            }
        }
    }
    Ok(values)
}

fn unpremultiply(value: i32, alpha: i32) -> u8 {
    if alpha <= 0 {
        return 0;
    }
    let straight = ((value * 255 + alpha / 2) / alpha).clamp(0, 255);
    u8::try_from(straight).unwrap_or(255)
}

fn scale_14_to_8(value: i32) -> u8 {
    let scaled =
        ((value.clamp(0, CHANNEL_MAX) * 255 + CHANNEL_MAX / 2) / CHANNEL_MAX).clamp(0, 255);
    u8::try_from(scaled).unwrap_or(255)
}

fn image_len(width: u32, height: u32) -> Result<usize, ParseError> {
    usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| layer_error("layer image is too large"))
}

fn chunk_body<'a>(input: &'a [u8], chunk: &crate::Chunk) -> Result<&'a [u8], ParseError> {
    let start =
        usize::try_from(chunk.offset()).map_err(|_| layer_error("chunk offset is too large"))?;
    let end = start
        .checked_add(chunk.size())
        .ok_or_else(|| layer_error("chunk range overflow"))?;
    input
        .get(start..end)
        .ok_or_else(|| layer_error("truncated chunk body"))
}

fn read<const N: usize>(input: &[u8], offset: usize) -> Result<[u8; N], ParseError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| layer_error("byte range overflow"))?;
    input
        .get(offset..end)
        .ok_or_else(|| layer_error("truncated layer data"))?
        .try_into()
        .map_err(|_| layer_error("truncated layer data"))
}

const fn layer_error(reason: &'static str) -> ParseError {
    ParseError::MalformedLayer { reason }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SAI2_MAGIC;

    #[test]
    fn decodes_a_synthetic_transparent_raster_layer() {
        let bytes = synthetic_layer_document();
        let layers = decode_layers(&bytes, DecodeLimits::default()).expect("layer should decode");
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].id(), 2);
        assert_eq!(layers[0].name(), "Test");
        assert!(layers[0].visible());
        assert!(
            layers[0]
                .image()
                .expect("raster pixels should decode")
                .pixels()
                .iter()
                .all(|byte| *byte == 0)
        );
    }

    #[test]
    fn decodes_owned_two_layer_fixture_when_available() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/private/32x32-redball-greenball-multiple-layer.sai2");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let layers =
            decode_layers(&bytes, DecodeLimits::default()).expect("fixture layers should decode");
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].id(), 3);
        assert_eq!(layers[0].blend_mode().as_bytes(), *b"mult");
        assert_eq!(layers[1].id(), 2);
        assert_eq!(layers[1].blend_mode().as_bytes(), *b"norm");
        assert!(layers.iter().all(|layer| layer.image().is_some()));
        let green_pixels = layers[0].image().unwrap().pixels();
        let red_pixels = layers[1].image().unwrap().pixels();
        assert!(
            green_pixels
                .chunks_exact(4)
                .any(|pixel| pixel[1] > pixel[0] && pixel[3] > 0)
        );
        assert!(
            red_pixels
                .chunks_exact(4)
                .any(|pixel| pixel[0] > pixel[1] && pixel[3] > 0)
        );
    }

    #[test]
    fn keeps_large_layer_metadata_when_owned_fixture_is_available() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/private/300x300-izunaface-white-background.sai2");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let layers =
            decode_layers(&bytes, DecodeLimits::default()).expect("layer metadata should parse");
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].tile_count(), 32);
        assert!(layers[0].image().is_none());
    }

    fn synthetic_layer_document() -> Vec<u8> {
        let mut layr = vec![0_u8; 80];
        layr[0..4].copy_from_slice(b"layr");
        layr[4..8].copy_from_slice(&2_u32.to_le_bytes());
        layr[16..20].copy_from_slice(b"norm");
        layr[40..44].copy_from_slice(&1_u32.to_le_bytes());
        layr[44..48].copy_from_slice(b"norm");
        layr[48..52].copy_from_slice(&100_u32.to_le_bytes());
        layr[52..56].copy_from_slice(&0x0001_0000_u32.to_le_bytes());
        layr[56..60].copy_from_slice(b"name");
        layr[60..64].copy_from_slice(&12_u32.to_le_bytes());
        layr[64..66].copy_from_slice(&4_u16.to_le_bytes());
        for (index, unit) in "Test".encode_utf16().enumerate() {
            let offset = 66 + index * 2;
            layr[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
        }

        let mut compressed = Vec::new();
        for _ in 0..4 {
            for _ in 0..7 {
                compressed.extend_from_slice(&[0x80, 0xff]);
            }
            compressed.extend_from_slice(&[0x80, 0x8f]);
        }
        let mut tile = Vec::new();
        tile.extend_from_slice(&0xa0ff_u16.to_le_bytes());
        tile.extend_from_slice(&u16::try_from(compressed.len()).unwrap().to_le_bytes());
        tile.extend_from_slice(&compressed);
        tile.extend_from_slice(&0xf1ff_u16.to_le_bytes());
        let mut lpix = Vec::new();
        lpix.extend_from_slice(b"dpcm");
        lpix.extend_from_slice(&u32::try_from(tile.len()).unwrap().to_le_bytes());
        lpix.extend_from_slice(&tile);

        let first_offset = 64 + 2 * 16;
        let second_offset = first_offset + layr.len();
        let mut document = vec![0_u8; first_offset];
        document[0..16].copy_from_slice(&SAI2_MAGIC);
        document[16..20].copy_from_slice(&0x0100_u32.to_le_bytes());
        document[20..24].copy_from_slice(&32_u32.to_le_bytes());
        document[24..28].copy_from_slice(&32_u32.to_le_bytes());
        document[32..36].copy_from_slice(&2_u32.to_le_bytes());
        document[60..64].copy_from_slice(b"norm");
        document[64..68].copy_from_slice(b"layr");
        document[68..72].copy_from_slice(&2_u32.to_le_bytes());
        document[72..80].copy_from_slice(&u64::try_from(first_offset).unwrap().to_le_bytes());
        document[80..84].copy_from_slice(b"lpix");
        document[84..88].copy_from_slice(&2_u32.to_le_bytes());
        document[88..96].copy_from_slice(&u64::try_from(second_offset).unwrap().to_le_bytes());
        document.extend_from_slice(&layr);
        document.extend_from_slice(&lpix);
        document
    }
}
