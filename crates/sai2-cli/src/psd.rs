use std::io::Write;

use sai2_core::{RgbaImage, Sai2Layer};

const CHANNEL_IDS: [i16; 4] = [0, 1, 2, -1];

#[allow(clippy::too_many_lines)]
pub fn write_layered(
    output: &mut impl Write,
    width: u32,
    height: u32,
    layers: &[Sai2Layer],
    composite: &RgbaImage,
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
    let records_len = layers.iter().try_fold(0_usize, |sum, layer| {
        sum.checked_add(layer_record_len(layer))
            .ok_or_else(|| "PSD layer records are too large".to_owned())
    })?;
    let image_data_len = layers
        .len()
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
        i16::try_from(layers.len()).map_err(|_| "PSD has too many layers")?,
    )?;

    for layer in layers {
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
        let extra_len = layer_extra_len(layer);
        write_u32(output, checked_u32(extra_len)?)?;
        write_u32(output, 0)?; // layer mask
        write_u32(output, 0)?; // blending ranges
        write_pascal_name(output, layer)?;
        write_unicode_name(output, layer.name())?;
    }

    for layer in layers {
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

fn layer_record_len(layer: &Sai2Layer) -> usize {
    16 + 2 + 4 * 6 + 12 + 4 + layer_extra_len(layer)
}

fn layer_extra_len(layer: &Sai2Layer) -> usize {
    4 + 4 + pascal_name_len(layer) + unicode_name_block_len(layer.name())
}

fn pascal_name_len(layer: &Sai2Layer) -> usize {
    let fallback = format!("Layer {}", layer.id());
    round_up(1 + fallback.len().min(255), 4)
}

fn unicode_name_block_len(name: &str) -> usize {
    let data_len = 4 + name.encode_utf16().count() * 2;
    12 + round_up(data_len, 4)
}

fn write_pascal_name(output: &mut impl Write, layer: &Sai2Layer) -> Result<(), String> {
    let fallback = format!("Layer {}", layer.id());
    let bytes = fallback.as_bytes();
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
    use sai2_core::{DecodeLimits, decode_integrated_image, decode_layers};

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
        write_layered(&mut psd, 32, 32, &layers, &composite).unwrap();

        assert_eq!(&psd[..4], b"8BPS");
        assert_eq!(u16::from_be_bytes(psd[12..14].try_into().unwrap()), 4);
        assert!(psd.windows(4).any(|window| window == b"mul "));
        assert!(psd.windows(8).any(|window| window == b"8BIMluni"));
    }
}
