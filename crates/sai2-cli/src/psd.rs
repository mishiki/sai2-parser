use std::io::Write;

use sai2_core::{RgbaImage, Sai2Layer};

const CHANNEL_IDS: [i16; 4] = [0, 1, 2, -1];
const SAI2_LAYER_DATA_KEY: &[u8; 4] = b"s2ly";
const SAI2_LAYER_DATA_MAGIC: &[u8; 8] = b"SAI2LYR\0";
const BACKGROUND_NAME: &str = "SAI2 Canvas Background";

#[allow(clippy::too_many_lines)]
pub fn write_layered(
    output: &mut impl Write,
    width: u32,
    height: u32,
    layers: &[Sai2Layer],
    composite: &RgbaImage,
    source: &[u8],
    white_background: bool,
) -> Result<(), String> {
    if width == 0 || height == 0 || width > 30_000 || height > 30_000 {
        return Err(format!(
            "PSD 1.0 dimensions must be within 1..=30000, found {width} x {height}"
        ));
    }
    if composite.width() != width || composite.height() != height {
        return Err("composite dimensions do not match the SAI2 canvas".to_owned());
    }
    for layer in layers {
        let Some(image) = layer.image() else {
            return Err(format!(
                "layer {} ({}) uses a pixel layout that sai2topsd does not decode yet",
                layer.id(),
                layer.name()
            ));
        };
        if image.width() != width || image.height() != height {
            return Err(format!(
                "layer {} dimensions do not match the canvas",
                layer.id()
            ));
        }
    }

    let pixel_count = checked_pixel_count(width, height)?;
    let channel_data_len = checked_u32(
        pixel_count
            .checked_add(2)
            .ok_or("PSD channel length overflow")?,
    )?;
    let extra_lens = layers
        .iter()
        .map(layer_extra_len)
        .collect::<Result<Vec<_>, _>>()?;
    let mut records_len = extra_lens.iter().try_fold(0_usize, |sum, extra_len| {
        sum.checked_add(layer_record_len(*extra_len))
            .ok_or_else(|| "PSD layer records are too large".to_owned())
    })?;
    let background_extra_len = basic_layer_extra_len(BACKGROUND_NAME, BACKGROUND_NAME)?;
    if white_background {
        records_len = records_len
            .checked_add(layer_record_len(background_extra_len))
            .ok_or_else(|| "PSD layer records are too large".to_owned())?;
    }
    let output_layer_count = layers
        .len()
        .checked_add(usize::from(white_background))
        .ok_or_else(|| "PSD has too many layers".to_owned())?;
    let image_data_len = output_layer_count
        .checked_mul(4)
        .and_then(|count| count.checked_mul(pixel_count + 2))
        .ok_or_else(|| "PSD layer image data is too large".to_owned())?;
    let mut layer_info_len = 2_usize
        .checked_add(records_len)
        .and_then(|length| length.checked_add(image_data_len))
        .ok_or_else(|| "PSD layer information is too large".to_owned())?;
    if layer_info_len % 2 != 0 {
        layer_info_len += 1;
    }
    let layer_and_mask_len = 4_usize
        .checked_add(layer_info_len)
        .and_then(|length| length.checked_add(4))
        .ok_or_else(|| "PSD layer and mask section is too large".to_owned())?;

    output.write_all(b"8BPS").map_err(io_error)?;
    write_u16(output, 1)?;
    output.write_all(&[0; 6]).map_err(io_error)?;
    write_u16(output, 4)?;
    write_u32(output, height)?;
    write_u32(output, width)?;
    write_u16(output, 8)?;
    write_u16(output, 3)?;
    write_u32(output, 0)?; // color mode data
    write_u32(output, 0)?; // image resources
    write_u32(output, checked_u32(layer_and_mask_len)?)?;
    write_u32(output, checked_u32(layer_info_len)?)?;
    write_i16(
        output,
        i16::try_from(output_layer_count).map_err(|_| "PSD has too many layers")?,
    )?;

    if white_background {
        write_common_layer_record(output, width, height, channel_data_len)?;
        output.write_all(b"8BIMnorm").map_err(io_error)?;
        output.write_all(&[255, 0, 0, 0]).map_err(io_error)?;
        write_u32(output, checked_u32(background_extra_len)?)?;
        write_u32(output, 0)?; // layer mask
        write_u32(output, 0)?; // blending ranges
        write_pascal_name(output, BACKGROUND_NAME)?;
        write_unicode_name(output, BACKGROUND_NAME)?;
    }
    // SAI2 lists layers from top to bottom, while PSD layer records are
    // composited from the first (bottom) record to the last (top) record.
    for (layer, extra_len) in layers.iter().zip(&extra_lens).rev() {
        write_i32(output, 0)?;
        write_i32(output, 0)?;
        write_i32(
            output,
            i32::try_from(height).map_err(|_| "PSD height is too large")?,
        )?;
        write_i32(
            output,
            i32::try_from(width).map_err(|_| "PSD width is too large")?,
        )?;
        write_u16(output, 4)?;
        for id in CHANNEL_IDS {
            write_i16(output, id)?;
            write_u32(output, channel_data_len)?;
        }
        output.write_all(b"8BIM").map_err(io_error)?;
        output.write_all(&psd_blend_key(layer)).map_err(io_error)?;
        let opacity = u8::try_from((u16::from(layer.opacity()) * 255 + 50) / 100)
            .map_err(|_| "PSD opacity overflow")?;
        let clipping = u8::from(layer.flags() & 0x0100_0000 != 0);
        let flags = if layer.visible() { 0 } else { 2 };
        output
            .write_all(&[opacity, clipping, flags, 0])
            .map_err(io_error)?;
        write_u32(output, checked_u32(*extra_len)?)?;
        write_u32(output, 0)?; // layer mask
        write_u32(output, 0)?; // blending ranges
        write_pascal_name(output, &format!("Layer {}", layer.id()))?;
        write_unicode_name(output, layer.name())?;
        write_sai2_layer_data(output, layer, source)?;
    }

    if white_background {
        for _ in 0..4 {
            write_u16(output, 0)?; // raw channel data
            write_solid_plane(output, 255, pixel_count)?;
        }
    }
    for layer in layers.iter().rev() {
        let pixels = layer.image().expect("layers were validated").pixels();
        for channel in 0..4 {
            write_u16(output, 0)?; // raw channel data
            write_plane(output, pixels, channel)?;
        }
    }
    if (2 + records_len + image_data_len) % 2 != 0 {
        output.write_all(&[0]).map_err(io_error)?;
    }
    write_u32(output, 0)?; // global layer mask

    write_u16(output, 0)?; // raw composite data
    for channel in 0..4 {
        write_plane(output, composite.pixels(), channel)?;
    }
    Ok(())
}

fn write_common_layer_record(
    output: &mut impl Write,
    width: u32,
    height: u32,
    channel_data_len: u32,
) -> Result<(), String> {
    write_i32(output, 0)?;
    write_i32(output, 0)?;
    write_i32(
        output,
        i32::try_from(height).map_err(|_| "PSD height is too large")?,
    )?;
    write_i32(
        output,
        i32::try_from(width).map_err(|_| "PSD width is too large")?,
    )?;
    write_u16(output, 4)?;
    for id in CHANNEL_IDS {
        write_i16(output, id)?;
        write_u32(output, channel_data_len)?;
    }
    Ok(())
}

fn layer_record_len(extra_len: usize) -> usize {
    16 + 2 + 4 * 6 + 12 + 4 + extra_len
}

fn layer_extra_len(layer: &Sai2Layer) -> Result<usize, String> {
    let fallback = format!("Layer {}", layer.id());
    let base = basic_layer_extra_len(&fallback, layer.name())?;
    base.checked_add(sai2_layer_block_len(layer)?)
        .ok_or_else(|| "PSD layer extra data is too large".to_owned())
}

fn sai2_layer_payload_len(layer: &Sai2Layer) -> Result<usize, String> {
    layer
        .source_chunks()
        .iter()
        .try_fold(16_usize, |length, chunk| {
            length
                .checked_add(24)
                .and_then(|value| value.checked_add(chunk.size()))
                .ok_or_else(|| "embedded SAI2 layer data is too large".to_owned())
        })
}

fn sai2_layer_block_len(layer: &Sai2Layer) -> Result<usize, String> {
    let payload_len = sai2_layer_payload_len(layer)?;
    checked_u32(payload_len)?;
    12_usize
        .checked_add(round_up(payload_len, 4))
        .ok_or_else(|| "embedded SAI2 layer block is too large".to_owned())
}

fn basic_layer_extra_len(pascal_name: &str, unicode_name: &str) -> Result<usize, String> {
    4_usize
        .checked_add(4)
        .and_then(|length| length.checked_add(pascal_name_len(pascal_name)))
        .and_then(|length| length.checked_add(unicode_name_block_len(unicode_name)))
        .ok_or_else(|| "PSD layer extra data is too large".to_owned())
}

fn pascal_name_len(name: &str) -> usize {
    round_up(1 + name.len().min(255), 4)
}

fn unicode_name_block_len(name: &str) -> usize {
    let data_len = 4 + name.encode_utf16().count() * 2;
    12 + round_up(data_len, 4)
}

fn write_pascal_name(output: &mut impl Write, name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    let length = bytes.len().min(255);
    output
        .write_all(&[u8::try_from(length).map_err(|_| "PSD layer name is too long")?])
        .map_err(io_error)?;
    output.write_all(&bytes[..length]).map_err(io_error)?;
    for _ in 1 + length..round_up(1 + length, 4) {
        output.write_all(&[0]).map_err(io_error)?;
    }
    Ok(())
}

fn write_unicode_name(output: &mut impl Write, name: &str) -> Result<(), String> {
    let units = name.encode_utf16().collect::<Vec<_>>();
    let data_len = 4_usize
        .checked_add(
            units
                .len()
                .checked_mul(2)
                .ok_or("PSD Unicode name is too long")?,
        )
        .ok_or("PSD Unicode name is too long")?;
    output.write_all(b"8BIMluni").map_err(io_error)?;
    write_u32(output, checked_u32(data_len)?)?;
    write_u32(output, checked_u32(units.len())?)?;
    for unit in units {
        write_u16(output, unit)?;
    }
    for _ in data_len..round_up(data_len, 4) {
        output.write_all(&[0]).map_err(io_error)?;
    }
    Ok(())
}

fn write_sai2_layer_data(
    output: &mut impl Write,
    layer: &Sai2Layer,
    source: &[u8],
) -> Result<(), String> {
    let payload_len = sai2_layer_payload_len(layer)?;
    output.write_all(b"8BIM").map_err(io_error)?;
    output.write_all(SAI2_LAYER_DATA_KEY).map_err(io_error)?;
    write_u32(output, checked_u32(payload_len)?)?;
    output.write_all(SAI2_LAYER_DATA_MAGIC).map_err(io_error)?;
    write_u32(output, 1)?; // preservation format version
    write_u32(
        output,
        u32::try_from(layer.source_chunks().len())
            .map_err(|_| "too many SAI2 chunks belong to one layer")?,
    )?;
    for chunk in layer.source_chunks() {
        output
            .write_all(&chunk.kind().as_bytes())
            .map_err(io_error)?;
        write_u32(output, chunk.object_id())?;
        write_u64(output, chunk.offset())?;
        write_u64(
            output,
            u64::try_from(chunk.size()).map_err(|_| "SAI2 chunk is too large")?,
        )?;
        let offset = usize::try_from(chunk.offset())
            .map_err(|_| "SAI2 chunk offset does not fit this platform")?;
        let end = offset
            .checked_add(chunk.size())
            .ok_or("SAI2 chunk range overflow")?;
        let body = source
            .get(offset..end)
            .ok_or("SAI2 source does not contain a preserved layer chunk")?;
        output.write_all(body).map_err(io_error)?;
    }
    for _ in payload_len..round_up(payload_len, 4) {
        output.write_all(&[0]).map_err(io_error)?;
    }
    Ok(())
}

fn psd_blend_key(layer: &Sai2Layer) -> [u8; 4] {
    match layer.blend_mode().as_bytes() {
        value if value == *b"mult" => *b"mul ",
        value if value == *b"scrn" => *b"scrn",
        value if value == *b"over" => *b"over",
        value if value == *b"dark" => *b"dark",
        value if value == *b"lite" => *b"lite",
        _ => *b"norm",
    }
}

fn write_plane(output: &mut impl Write, rgba: &[u8], channel: usize) -> Result<(), String> {
    let plane = rgba
        .chunks_exact(4)
        .map(|pixel| pixel[channel])
        .collect::<Vec<_>>();
    output.write_all(&plane).map_err(io_error)
}

fn write_solid_plane(output: &mut impl Write, value: u8, length: usize) -> Result<(), String> {
    const BUFFER_LEN: usize = 8 * 1024;
    let buffer = [value; BUFFER_LEN];
    let mut remaining = length;
    while remaining != 0 {
        let count = remaining.min(BUFFER_LEN);
        output.write_all(&buffer[..count]).map_err(io_error)?;
        remaining -= count;
    }
    Ok(())
}

fn checked_pixel_count(width: u32, height: u32) -> Result<usize, String> {
    usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| "PSD canvas is too large".to_owned())
}

fn checked_u32(value: usize) -> Result<u32, String> {
    u32::try_from(value).map_err(|_| "PSD section exceeds the 4 GiB format limit".to_owned())
}

const fn round_up(value: usize, multiple: usize) -> usize {
    value.div_ceil(multiple) * multiple
}

fn write_u16(output: &mut impl Write, value: u16) -> Result<(), String> {
    output.write_all(&value.to_be_bytes()).map_err(io_error)
}
fn write_i16(output: &mut impl Write, value: i16) -> Result<(), String> {
    output.write_all(&value.to_be_bytes()).map_err(io_error)
}
fn write_u32(output: &mut impl Write, value: u32) -> Result<(), String> {
    output.write_all(&value.to_be_bytes()).map_err(io_error)
}
fn write_u64(output: &mut impl Write, value: u64) -> Result<(), String> {
    output.write_all(&value.to_be_bytes()).map_err(io_error)
}
fn write_i32(output: &mut impl Write, value: i32) -> Result<(), String> {
    output.write_all(&value.to_be_bytes()).map_err(io_error)
}
#[allow(clippy::needless_pass_by_value)]
fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sai2_core::{DecodeLimits, FourCc, Sai2Document, decode_integrated_image, decode_layers};

    #[test]
    fn writes_a_layered_psd_when_the_owned_fixture_is_available() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/private/32x32-redball-greenball-multiple-layer.sai2");
        let Ok(input) = std::fs::read(path) else {
            return;
        };
        let composite = decode_integrated_image(&input, DecodeLimits::default()).unwrap();
        let layers = decode_layers(&input, DecodeLimits::default()).unwrap();
        let mut psd = Vec::new();
        write_layered(&mut psd, 32, 32, &layers, &composite, &input, true).unwrap();

        assert_eq!(&psd[..4], b"8BPS");
        assert_eq!(u16::from_be_bytes(psd[12..14].try_into().unwrap()), 4);
        assert!(psd.windows(4).any(|window| window == b"mul "));
        assert!(psd.windows(8).any(|window| window == b"8BIMluni"));
        assert_eq!(
            psd.windows(8)
                .filter(|window| *window == b"8BIMs2ly")
                .count(),
            2
        );
        assert_eq!(
            psd.windows(SAI2_LAYER_DATA_MAGIC.len())
                .filter(|window| *window == SAI2_LAYER_DATA_MAGIC)
                .count(),
            2
        );

        let blocks = psd
            .windows(8)
            .enumerate()
            .filter_map(|(offset, window)| (window == b"8BIMs2ly").then_some(offset))
            .collect::<Vec<_>>();
        for (layer, block_offset) in layers.iter().rev().zip(blocks) {
            assert_preserved_layer_chunks(&psd, block_offset, layer, &input);
        }
    }

    #[test]
    fn maps_an_invisible_sai2_layer_to_the_psd_hidden_flag() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/private/32x32-redball-greenball-multiple-layer.sai2");
        let Ok(mut input) = std::fs::read(path) else {
            return;
        };
        let document = Sai2Document::parse(&input).unwrap();
        let layer_offset = usize::try_from(
            document
                .chunks()
                .iter()
                .find(|chunk| chunk.kind() == FourCc::from_bytes(*b"layr"))
                .unwrap()
                .offset(),
        )
        .unwrap();
        let flags_offset = layer_offset + 52;
        let flags = u32::from_le_bytes(input[flags_offset..flags_offset + 4].try_into().unwrap())
            & !0x0001_0000;
        input[flags_offset..flags_offset + 4].copy_from_slice(&flags.to_le_bytes());

        let composite = decode_integrated_image(&input, DecodeLimits::default()).unwrap();
        let layers = decode_layers(&input, DecodeLimits::default()).unwrap();
        assert!(!layers[0].visible());
        let mut psd = Vec::new();
        write_layered(&mut psd, 32, 32, &layers, &composite, &input, true).unwrap();

        let record = psd
            .windows(8)
            .position(|window| window == b"8BIMmul ")
            .unwrap();
        assert_eq!(psd[record + 10] & 2, 2);
    }

    #[test]
    fn omits_the_synthetic_background_for_a_transparent_canvas() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/private/32x32-blank-transparent-background.sai2");
        let Ok(input) = std::fs::read(path) else {
            return;
        };
        let document = Sai2Document::parse(&input).unwrap();
        assert!(document.header().integrated_image_has_alpha());
        let composite = decode_integrated_image(&input, DecodeLimits::default()).unwrap();
        let layers = decode_layers(&input, DecodeLimits::default()).unwrap();
        let mut psd = Vec::new();
        write_layered(&mut psd, 32, 32, &layers, &composite, &input, false).unwrap();

        assert!(
            !psd.windows(BACKGROUND_NAME.len())
                .any(|window| window == BACKGROUND_NAME.as_bytes())
        );
        assert_eq!(i16::from_be_bytes(psd[42..44].try_into().unwrap()), 1);
    }

    fn assert_preserved_layer_chunks(
        psd: &[u8],
        block_offset: usize,
        layer: &Sai2Layer,
        source: &[u8],
    ) {
        let payload_len = usize::try_from(u32::from_be_bytes(
            psd[block_offset + 8..block_offset + 12].try_into().unwrap(),
        ))
        .unwrap();
        let payload = &psd[block_offset + 12..block_offset + 12 + payload_len];
        assert_eq!(&payload[..8], SAI2_LAYER_DATA_MAGIC);
        assert_eq!(u32::from_be_bytes(payload[8..12].try_into().unwrap()), 1);
        assert_eq!(
            usize::try_from(u32::from_be_bytes(payload[12..16].try_into().unwrap())).unwrap(),
            layer.source_chunks().len()
        );

        let mut cursor = 16;
        for chunk in layer.source_chunks() {
            assert_eq!(&payload[cursor..cursor + 4], &chunk.kind().as_bytes());
            assert_eq!(
                u32::from_be_bytes(payload[cursor + 4..cursor + 8].try_into().unwrap()),
                chunk.object_id()
            );
            let source_offset = usize::try_from(u64::from_be_bytes(
                payload[cursor + 8..cursor + 16].try_into().unwrap(),
            ))
            .unwrap();
            let source_len = usize::try_from(u64::from_be_bytes(
                payload[cursor + 16..cursor + 24].try_into().unwrap(),
            ))
            .unwrap();
            assert_eq!(source_offset, usize::try_from(chunk.offset()).unwrap());
            assert_eq!(source_len, chunk.size());
            cursor += 24;
            assert_eq!(
                &payload[cursor..cursor + source_len],
                &source[source_offset..source_offset + source_len]
            );
            cursor += source_len;
        }
        assert_eq!(cursor, payload.len());
    }
}
