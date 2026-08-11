use std::io::Write;

use sai2_core::{RgbaImage, Sai2Layer, Sai2Shape};

const CHANNEL_IDS: [i16; 4] = [0, 1, 2, -1];
const SAI2_LAYER_DATA_KEY: &[u8; 4] = b"s2ly";
const SAI2_LAYER_DATA_MAGIC: &[u8; 8] = b"SAI2LYR\0";
const SOLID_COLOR_KEY: [u8; 4] = *b"SoCo";
const VECTOR_MASK_KEY: [u8; 4] = *b"vmsk";
const BACKGROUND_NAME: &str = "SAI2 Canvas Background";
const DIVIDER_NAME: &str = "</Layer group>";

#[derive(Clone, Copy)]
enum PsdRecord<'a> {
    Source(&'a Sai2Layer),
    Divider,
}

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
        if layer.layer_type().as_bytes() == *b"norm" && layer.image().is_none() {
            return Err(format!(
                "raster layer {} ({}) has no decoded pixels",
                layer.id(),
                layer.name()
            ));
        }
        if let Some(image) = layer.image()
            && (image.width() != width || image.height() != height)
        {
            return Err(format!(
                "layer {} dimensions do not match the canvas",
                layer.id()
            ));
        }
        if let Some(mask) = layer.mask()
            && let Some(image) = mask.image()
            && (image.width() != width || image.height() != height)
        {
            return Err(format!(
                "layer {} mask dimensions do not match the canvas",
                layer.id()
            ));
        }
        if layer.layer_type().as_bytes() == *b"shap" {
            validate_native_shape(layer)?;
        }
    }

    let records = build_psd_records(layers)?;
    let record_channel_lengths = records
        .iter()
        .map(|record| record_channel_lengths(*record, width, height))
        .collect::<Result<Vec<_>, _>>()?;
    let background_channel_lengths = if white_background {
        Some(vec![rle_solid_plane_len(width, height)?; 4])
    } else {
        None
    };
    let extra_lens = records
        .iter()
        .map(|record| record_extra_len(*record))
        .collect::<Result<Vec<_>, _>>()?;
    let mut records_len =
        records
            .iter()
            .zip(&extra_lens)
            .try_fold(0_usize, |sum, (record, extra_len)| {
                sum.checked_add(layer_record_len(*extra_len, record_channel_count(*record)))
                    .ok_or_else(|| "PSD layer records are too large".to_owned())
            })?;
    let background_extra_len = basic_layer_extra_len(BACKGROUND_NAME, BACKGROUND_NAME)?;
    if white_background {
        records_len = records_len
            .checked_add(layer_record_len(background_extra_len, 4))
            .ok_or_else(|| "PSD layer records are too large".to_owned())?;
    }
    let output_layer_count = records
        .len()
        .checked_add(usize::from(white_background))
        .ok_or_else(|| "PSD has too many layers".to_owned())?;
    let image_data_len =
        record_channel_lengths
            .iter()
            .flatten()
            .chain(background_channel_lengths.iter().flatten())
            .try_fold(0_usize, |sum, length| {
                sum.checked_add(usize::try_from(*length).map_err(|_| {
                    "PSD layer channel length does not fit this platform".to_owned()
                })?)
                .ok_or_else(|| "PSD layer image data is too large".to_owned())
            })?;
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
        write_common_layer_record(
            output,
            width,
            height,
            background_channel_lengths
                .as_deref()
                .ok_or("missing PSD background channel lengths")?,
        )?;
        output.write_all(b"8BIMnorm").map_err(io_error)?;
        output.write_all(&[255, 0, 0, 0]).map_err(io_error)?;
        write_u32(output, checked_u32(background_extra_len)?)?;
        write_u32(output, 0)?; // layer mask
        write_u32(output, 0)?; // blending ranges
        write_pascal_name(output, BACKGROUND_NAME)?;
        write_unicode_name(output, BACKGROUND_NAME)?;
    }
    for ((record, extra_len), channel_lengths) in
        records.iter().zip(&extra_lens).zip(&record_channel_lengths)
    {
        write_record(
            output,
            *record,
            *extra_len,
            width,
            height,
            channel_lengths,
            source,
        )?;
    }

    if white_background {
        for _ in 0..4 {
            write_rle_solid_plane(output, 255, width, height)?;
        }
    }
    for record in &records {
        write_record_pixels(output, *record, width, height)?;
    }
    if (2 + records_len + image_data_len) % 2 != 0 {
        output.write_all(&[0]).map_err(io_error)?;
    }
    write_u32(output, 0)?; // global layer mask

    write_rle_composite(output, composite.pixels(), width, height)?;
    Ok(())
}

fn build_psd_records(layers: &[Sai2Layer]) -> Result<Vec<PsdRecord<'_>>, String> {
    let mut records = Vec::with_capacity(layers.len() * 2);
    let mut depth = 0_u8;
    for layer in layers.iter().rev() {
        let layer_depth = layer.nesting_level();
        if layer_depth > depth {
            // Reversing SAI2's top-to-bottom list can enter several nested
            // folders at once because their opening records occur later in
            // PSD's bottom-to-top order.
            while depth < layer_depth {
                records.push(PsdRecord::Divider);
                depth += 1;
            }
        } else if layer_depth < depth {
            if layer_depth + 1 != depth || !layer.is_folder() {
                return Err(format!(
                    "layer {} does not close folder depth {depth}",
                    layer.id()
                ));
            }
            depth = layer_depth;
        } else if layer.is_folder() {
            // An empty folder still needs a bounding section divider.
            records.push(PsdRecord::Divider);
        }
        records.push(PsdRecord::Source(layer));
    }
    if depth != 0 {
        return Err(format!("unclosed SAI2 folder depth {depth}"));
    }
    Ok(records)
}

#[allow(clippy::too_many_arguments)]
fn write_record(
    output: &mut impl Write,
    record: PsdRecord<'_>,
    extra_len: usize,
    width: u32,
    height: u32,
    channel_lengths: &[u32],
    source: &[u8],
) -> Result<(), String> {
    let channels = record_channel_count(record);
    if channel_lengths.len() != channels {
        return Err("PSD layer channel-length count mismatch".to_owned());
    }
    if channels == 0 {
        for _ in 0..4 {
            write_i32(output, 0)?;
        }
    } else {
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
    }
    write_u16(
        output,
        u16::try_from(channels).map_err(|_| "too many PSD layer channels")?,
    )?;
    for (id, length) in CHANNEL_IDS
        .into_iter()
        .take(channels.min(4))
        .zip(channel_lengths)
    {
        write_i16(output, id)?;
        write_u32(output, *length)?;
    }
    if channels == 5 {
        write_i16(output, -2)?;
        write_u32(output, channel_lengths[4])?;
    }

    output.write_all(b"8BIM").map_err(io_error)?;
    match record {
        PsdRecord::Source(layer) => {
            output.write_all(&psd_blend_key(layer)).map_err(io_error)?;
            let opacity = u8::try_from((u16::from(layer.opacity()) * 255 + 50) / 100)
                .map_err(|_| "PSD opacity overflow")?;
            let clipping = u8::from(layer.clipped_to_below());
            let mut flags = u8::from(layer.alpha_locked()) | if layer.visible() { 0 } else { 2 };
            if layer.shape().is_some() {
                // Bits 3 and 4 declare that the latter is meaningful and that
                // pixel channels do not define this live shape's appearance.
                flags |= 0x18;
            }
            output
                .write_all(&[opacity, clipping, flags, 0])
                .map_err(io_error)?;
            write_u32(output, checked_u32(extra_len)?)?;
            write_layer_mask_data(output, layer, width, height)?;
            write_u32(output, 0)?; // blending ranges
            write_pascal_name(output, &format!("Layer {}", layer.id()))?;
            write_unicode_name(output, layer.name())?;
            write_sai2_layer_data(output, layer, source)?;
            write_native_shape_data(output, layer, width, height)?;
            if layer.is_folder() {
                write_section_divider(output, 1, Some(psd_blend_key(layer)))?;
            }
        }
        PsdRecord::Divider => {
            output.write_all(b"norm").map_err(io_error)?;
            output.write_all(&[255, 0, 2, 0]).map_err(io_error)?;
            write_u32(output, checked_u32(extra_len)?)?;
            write_u32(output, 0)?; // layer mask
            write_u32(output, 0)?; // blending ranges
            write_pascal_name(output, DIVIDER_NAME)?;
            write_unicode_name(output, DIVIDER_NAME)?;
            write_section_divider(output, 3, None)?;
        }
    }
    Ok(())
}

fn write_record_pixels(
    output: &mut impl Write,
    record: PsdRecord<'_>,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let PsdRecord::Source(layer) = record else {
        return Ok(());
    };
    if layer.is_folder() {
        return Ok(());
    }
    if let Some(image) = layer.image() {
        for channel in 0..4 {
            write_rle_rgba_plane(output, image.pixels(), channel, width, height)?;
        }
    } else {
        for _ in 0..4 {
            write_rle_solid_plane(output, 0, width, height)?;
        }
    }
    if let Some(mask) = layer.mask() {
        let image = mask.image().ok_or_else(|| {
            format!(
                "layer {} ({}) has an undecoded mask",
                layer.id(),
                layer.name()
            )
        })?;
        write_rle_gray_plane(output, image.pixels(), width, height)?;
    }
    Ok(())
}

fn write_common_layer_record(
    output: &mut impl Write,
    width: u32,
    height: u32,
    channel_lengths: &[u32],
) -> Result<(), String> {
    if channel_lengths.len() != 4 {
        return Err("PSD background channel-length count mismatch".to_owned());
    }
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
    for (id, length) in CHANNEL_IDS.into_iter().zip(channel_lengths) {
        write_i16(output, id)?;
        write_u32(output, *length)?;
    }
    Ok(())
}

fn layer_record_len(extra_len: usize, channels: usize) -> usize {
    16 + 2 + channels * 6 + 12 + 4 + extra_len
}

fn record_channel_count(record: PsdRecord<'_>) -> usize {
    match record {
        PsdRecord::Divider => 0,
        PsdRecord::Source(layer) if layer.is_folder() => 0,
        PsdRecord::Source(layer) => 4 + usize::from(layer.mask().is_some()),
    }
}

fn record_channel_lengths(
    record: PsdRecord<'_>,
    width: u32,
    height: u32,
) -> Result<Vec<u32>, String> {
    let PsdRecord::Source(layer) = record else {
        return Ok(Vec::new());
    };
    if layer.is_folder() {
        return Ok(Vec::new());
    }
    let mut lengths = if let Some(image) = layer.image() {
        (0..4)
            .map(|channel| rle_rgba_plane_len(image.pixels(), channel, width, height))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![rle_solid_plane_len(width, height)?; 4]
    };
    if let Some(mask) = layer.mask() {
        let image = mask.image().ok_or_else(|| {
            format!(
                "layer {} ({}) has an undecoded mask",
                layer.id(),
                layer.name()
            )
        })?;
        lengths.push(rle_gray_plane_len(image.pixels(), width, height)?);
    }
    Ok(lengths)
}

fn record_extra_len(record: PsdRecord<'_>) -> Result<usize, String> {
    match record {
        PsdRecord::Divider => basic_layer_extra_len(DIVIDER_NAME, DIVIDER_NAME)?
            .checked_add(section_divider_block_len(false))
            .ok_or_else(|| "PSD divider extra data is too large".to_owned()),
        PsdRecord::Source(layer) => layer_extra_len(layer),
    }
}

fn layer_extra_len(layer: &Sai2Layer) -> Result<usize, String> {
    let fallback = format!("Layer {}", layer.id());
    let mut length = basic_layer_extra_len(&fallback, layer.name())?;
    if layer.mask().is_some() {
        length = length
            .checked_add(20)
            .ok_or_else(|| "PSD layer mask data is too large".to_owned())?;
    }
    length = length
        .checked_add(sai2_layer_block_len(layer)?)
        .ok_or_else(|| "PSD layer extra data is too large".to_owned())?;
    length = length
        .checked_add(native_shape_blocks_len(layer)?)
        .ok_or_else(|| "PSD shape layer data is too large".to_owned())?;
    if layer.is_folder() {
        length = length
            .checked_add(section_divider_block_len(true))
            .ok_or_else(|| "PSD folder extra data is too large".to_owned())?;
    }
    Ok(length)
}

const fn section_divider_block_len(has_blend_mode: bool) -> usize {
    if has_blend_mode { 24 } else { 16 }
}

fn write_layer_mask_data(
    output: &mut impl Write,
    layer: &Sai2Layer,
    width: u32,
    height: u32,
) -> Result<(), String> {
    if layer.mask().is_none() {
        return write_u32(output, 0);
    }
    write_u32(output, 20)?;
    write_i32(output, 0)?;
    write_i32(output, 0)?;
    write_i32(
        output,
        i32::try_from(height).map_err(|_| "PSD mask height is too large")?,
    )?;
    write_i32(
        output,
        i32::try_from(width).map_err(|_| "PSD mask width is too large")?,
    )?;
    output.write_all(&[0, 0]).map_err(io_error)?;
    write_u16(output, 0)?;
    Ok(())
}

fn write_section_divider(
    output: &mut impl Write,
    divider_type: u32,
    blend_mode: Option<[u8; 4]>,
) -> Result<(), String> {
    let data_len = if blend_mode.is_some() { 12 } else { 4 };
    output.write_all(b"8BIMlsct").map_err(io_error)?;
    write_u32(output, data_len)?;
    write_u32(output, divider_type)?;
    if let Some(blend_mode) = blend_mode {
        output.write_all(b"8BIM").map_err(io_error)?;
        output.write_all(&blend_mode).map_err(io_error)?;
    }
    Ok(())
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

fn native_shape_blocks_len(layer: &Sai2Layer) -> Result<usize, String> {
    let Some(shape) = layer.shape() else {
        return Ok(0);
    };
    let solid_color = solid_color_payload(shape.fill_bgra14().ok_or_else(|| {
        format!(
            "shape layer {} ({}) has no fill color",
            layer.id(),
            layer.name()
        )
    })?)?;
    let vector_mask_len = vector_mask_payload_len(shape)?;
    additional_block_len(solid_color.len())?
        .checked_add(additional_block_len(vector_mask_len)?)
        .ok_or_else(|| "PSD shape blocks are too large".to_owned())
}

fn additional_block_len(payload_len: usize) -> Result<usize, String> {
    checked_u32(payload_len)?;
    12_usize
        .checked_add(round_up(payload_len, 4))
        .ok_or_else(|| "PSD additional layer block is too large".to_owned())
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
    12 + data_len
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

fn validate_native_shape(layer: &Sai2Layer) -> Result<(), String> {
    let shape = layer.shape().ok_or_else(|| {
        format!(
            "shape layer {} ({}) has no decoded geometry",
            layer.id(),
            layer.name()
        )
    })?;
    let color = shape.fill_bgra14().ok_or_else(|| {
        format!(
            "shape layer {} ({}) has no fill color",
            layer.id(),
            layer.name()
        )
    })?;
    if color[3] != 0x4000 {
        return Err(format!(
            "shape layer {} ({}) has unsupported translucent fill",
            layer.id(),
            layer.name()
        ));
    }
    if shape.paths().len() != 1 {
        return Err(format!(
            "shape layer {} ({}) must contain one SAI2 primitive path",
            layer.id(),
            layer.name()
        ));
    }
    let points = shape.paths()[0].points().len();
    if !matches!(points, 3 | 4) {
        return Err(format!(
            "shape layer {} ({}) has {points} points; expected a SAI2 triangle, quadrilateral, or ellipse",
            layer.id(),
            layer.name()
        ));
    }
    Ok(())
}

fn write_native_shape_data(
    output: &mut impl Write,
    layer: &Sai2Layer,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let Some(shape) = layer.shape() else {
        return Ok(());
    };
    let color = shape
        .fill_bgra14()
        .ok_or_else(|| "decoded shape has no fill color".to_owned())?;
    let solid_color = solid_color_payload(color)?;
    write_additional_block(output, SOLID_COLOR_KEY, &solid_color)?;
    let vector_mask = vector_mask_payload(shape, width, height)?;
    write_additional_block(output, VECTOR_MASK_KEY, &vector_mask)
}

fn write_additional_block(
    output: &mut impl Write,
    key: [u8; 4],
    payload: &[u8],
) -> Result<(), String> {
    output.write_all(b"8BIM").map_err(io_error)?;
    output.write_all(&key).map_err(io_error)?;
    write_u32(output, checked_u32(payload.len())?)?;
    output.write_all(payload).map_err(io_error)?;
    for _ in payload.len()..round_up(payload.len(), 4) {
        output.write_all(&[0]).map_err(io_error)?;
    }
    Ok(())
}

fn solid_color_payload(color_bgra14: [u16; 4]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    write_u32(&mut output, 16)?; // Descriptor block version.
    write_u32(&mut output, 0)?; // Empty descriptor name.
    write_descriptor_id(&mut output, b"solidColorLayer")?;
    write_u32(&mut output, 1)?;
    write_descriptor_id(&mut output, b"Clr ")?;
    output.write_all(b"Objc").map_err(io_error)?;
    write_u32(&mut output, 0)?; // Empty nested descriptor name.
    write_descriptor_id(&mut output, b"RGBC")?;
    write_u32(&mut output, 3)?;
    for (key, channel) in [
        (b"Rd  ".as_slice(), color_bgra14[2]),
        (b"Grn ".as_slice(), color_bgra14[1]),
        (b"Bl  ".as_slice(), color_bgra14[0]),
    ] {
        write_descriptor_id(&mut output, key)?;
        output.write_all(b"doub").map_err(io_error)?;
        let value = f64::from(channel.min(0x4000)) * 255.0 / 16384.0;
        output.write_all(&value.to_be_bytes()).map_err(io_error)?;
    }
    while output.len() % 4 != 0 {
        output.push(0);
    }
    Ok(output)
}

fn write_descriptor_id(output: &mut impl Write, id: &[u8]) -> Result<(), String> {
    if id.len() == 4 {
        write_u32(output, 0)?;
    } else {
        write_u32(output, checked_u32(id.len())?)?;
    }
    output.write_all(id).map_err(io_error)
}

fn vector_mask_payload_len(shape: &Sai2Shape) -> Result<usize, String> {
    shape
        .paths()
        .iter()
        .try_fold(8_usize + 26 + 26, |length, path| {
            path.points()
                .len()
                .checked_add(1)
                .and_then(|records| records.checked_mul(26))
                .and_then(|path_len| length.checked_add(path_len))
                .ok_or_else(|| "PSD vector mask is too large".to_owned())
        })
}

fn vector_mask_payload(shape: &Sai2Shape, width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(vector_mask_payload_len(shape)?);
    write_u32(&mut output, 3)?;
    write_u32(&mut output, 0)?;

    write_u16(&mut output, 6)?; // Path fill rule.
    output.extend_from_slice(&[0; 24]);
    write_u16(&mut output, 8)?; // Initial fill rule: empty background.
    write_u16(&mut output, 0)?;
    output.extend_from_slice(&[0; 22]);

    for path in shape.paths() {
        write_u16(&mut output, 0)?; // Closed subpath length record.
        write_u16(
            &mut output,
            u16::try_from(path.points().len()).map_err(|_| "too many PSD shape points")?,
        )?;
        write_i16(&mut output, 1)?; // Union/combine operation.
        write_u16(&mut output, 1)?;
        write_u32(&mut output, 0)?;
        write_u32(&mut output, 0)?;
        output.extend_from_slice(&[0; 10]);

        for point in path.points() {
            let linked = points_differ(point.control_before(), point.position())
                || points_differ(point.control_after(), point.position());
            write_u16(&mut output, if linked { 1 } else { 2 })?;
            for coordinate in [
                point.control_before(),
                point.position(),
                point.control_after(),
            ] {
                write_i32(
                    &mut output,
                    shape_fixed_point(path.origin()[1] + coordinate[1], height)?,
                )?;
                write_i32(
                    &mut output,
                    shape_fixed_point(path.origin()[0] + coordinate[0], width)?,
                )?;
            }
        }
    }
    debug_assert_eq!(output.len(), vector_mask_payload_len(shape)?);
    Ok(output)
}

fn points_differ(first: [f64; 2], second: [f64; 2]) -> bool {
    first
        .into_iter()
        .zip(second)
        .any(|(left, right)| (left - right).abs() > f64::EPSILON)
}

#[allow(clippy::cast_possible_truncation)]
fn shape_fixed_point(coordinate: f64, dimension: u32) -> Result<i32, String> {
    let scaled = coordinate / f64::from(dimension) * 16_777_216.0;
    if scaled < f64::from(i32::MIN) || scaled > f64::from(i32::MAX) {
        return Err("PSD shape coordinate exceeds 8.24 fixed-point range".to_owned());
    }
    Ok(scaled as i32)
}

fn psd_blend_key(layer: &Sai2Layer) -> [u8; 4] {
    psd_blend_key_for(layer.blend_mode().as_bytes())
}

fn psd_blend_key_for(mode: [u8; 4]) -> [u8; 4] {
    match mode {
        value if value == *b"pass" => *b"pass",
        value if value == *b"norm" => *b"norm",
        value if value == *b"mult" => *b"mul ",
        value if value == *b"scrn" => *b"scrn",
        value if value == *b"over" => *b"over",
        value if value == *b"sbad" => *b"lbrn", // Shade -> Linear Burn.
        value if value == *b"burn" => *b"lddg", // Shine -> Linear Dodge (Add).
        value if value == *b"ddge" => *b"lLit", // Shade/Shine -> Linear Light.
        value if value == *b"bndg" => *b"idiv", // Burn -> Color Burn.
        value if value == *b"ilit" => *b"div ", // Dodge -> Color Dodge.
        value if value == *b"cdif" => *b"vLit", // Burn/Dodge -> Vivid Light.
        value if value == *b"slit" => *b"sLit",
        value if value == *b"hlit" => *b"hLit",
        value if value == *b"plit" => *b"pLit", // SAI2 Vivid -> Pin Light (provisional).
        value if value == *b"hmix" => *b"hMix",
        value if value == *b"dark" => *b"dark",
        value if value == *b"litn" || value == *b"lite" => *b"lite",
        value if value == *b"drkc" => *b"dkCl",
        value if value == *b"litc" => *b"lgCl",
        value if value == *b"diff" => *b"diff",
        value if value == *b"excl" => *b"smud",
        value if value == *b"fsub" || value == *b"sub " => *b"fsub",
        value if value == *b"fdiv" => *b"fdiv",
        value if value == *b"hue " => *b"hue ",
        value if value == *b"sat " => *b"sat ",
        value if value == *b"col " => *b"colr",
        value if value == *b"lum " => *b"lum ",
        // These compatibility keys are accepted by SAI2 even though its
        // current layer-mode menu uses the aliases above.
        value if value == *b"add " || value == *b"lddg" => *b"lddg",
        value if value == *b"lbrn" => *b"lbrn",
        value if value == *b"llit" => *b"lLit",
        value if value == *b"cbrn" => *b"idiv",
        value if value == *b"cddg" => *b"div ",
        value if value == *b"vlit" => *b"vLit",
        _ => *b"norm",
    }
}

fn rle_rgba_plane_len(rgba: &[u8], channel: usize, width: u32, height: u32) -> Result<u32, String> {
    let row_lengths = rgba_row_lengths(rgba, channel, width, height)?;
    rle_channel_len(&row_lengths)
}

fn rle_gray_plane_len(gray: &[u8], width: u32, height: u32) -> Result<u32, String> {
    let row_lengths = gray_row_lengths(gray, width, height)?;
    rle_channel_len(&row_lengths)
}

fn rle_solid_plane_len(width: u32, height: u32) -> Result<u32, String> {
    let width = usize::try_from(width).map_err(|_| "PSD row is too wide")?;
    let height = usize::try_from(height).map_err(|_| "PSD has too many rows")?;
    let row_len = packbits_row_len(&vec![0; width]);
    let row_len = u16::try_from(row_len).map_err(|_| "PSD RLE row exceeds 65535 bytes")?;
    rle_channel_len(&vec![row_len; height])
}

fn rle_channel_len(row_lengths: &[u16]) -> Result<u32, String> {
    let encoded = row_lengths.iter().try_fold(0_usize, |sum, length| {
        sum.checked_add(usize::from(*length))
            .ok_or_else(|| "PSD RLE channel is too large".to_owned())
    })?;
    let length = 2_usize
        .checked_add(
            row_lengths
                .len()
                .checked_mul(2)
                .ok_or("PSD RLE row table is too large")?,
        )
        .and_then(|value| value.checked_add(encoded))
        .ok_or("PSD RLE channel is too large")?;
    checked_u32(length)
}

fn rgba_row_lengths(
    rgba: &[u8],
    channel: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u16>, String> {
    if channel >= 4 {
        return Err("invalid RGBA channel index".to_owned());
    }
    let width = usize::try_from(width).map_err(|_| "PSD row is too wide")?;
    let height = usize::try_from(height).map_err(|_| "PSD has too many rows")?;
    let stride = width.checked_mul(4).ok_or("PSD row is too wide")?;
    if rgba.len() != stride.checked_mul(height).ok_or("PSD image is too large")? {
        return Err("RGBA pixel length does not match PSD dimensions".to_owned());
    }
    let mut row = vec![0; width];
    let mut lengths = Vec::with_capacity(height);
    for source in rgba.chunks_exact(stride) {
        for (destination, pixel) in row.iter_mut().zip(source.chunks_exact(4)) {
            *destination = pixel[channel];
        }
        lengths.push(
            u16::try_from(packbits_row_len(&row)).map_err(|_| "PSD RLE row exceeds 65535 bytes")?,
        );
    }
    Ok(lengths)
}

fn gray_row_lengths(gray: &[u8], width: u32, height: u32) -> Result<Vec<u16>, String> {
    let width = usize::try_from(width).map_err(|_| "PSD row is too wide")?;
    let height = usize::try_from(height).map_err(|_| "PSD has too many rows")?;
    if gray.len() != width.checked_mul(height).ok_or("PSD image is too large")? {
        return Err("grayscale pixel length does not match PSD dimensions".to_owned());
    }
    gray.chunks_exact(width)
        .map(|row| {
            u16::try_from(packbits_row_len(row))
                .map_err(|_| "PSD RLE row exceeds 65535 bytes".to_owned())
        })
        .collect()
}

fn write_rle_rgba_plane(
    output: &mut impl Write,
    rgba: &[u8],
    channel: usize,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let lengths = rgba_row_lengths(rgba, channel, width, height)?;
    write_u16(output, 1)?;
    write_row_lengths(output, &lengths)?;
    write_rgba_rows(output, rgba, channel, width)
}

fn write_rle_gray_plane(
    output: &mut impl Write,
    gray: &[u8],
    width: u32,
    height: u32,
) -> Result<(), String> {
    let lengths = gray_row_lengths(gray, width, height)?;
    write_u16(output, 1)?;
    write_row_lengths(output, &lengths)?;
    let width = usize::try_from(width).map_err(|_| "PSD row is too wide")?;
    let mut inverted = vec![0_u8; width];
    for row in gray.chunks_exact(width) {
        for (destination, source) in inverted.iter_mut().zip(row) {
            *destination = 255 - source;
        }
        write_packbits_row(output, &inverted)?;
    }
    Ok(())
}

fn write_rle_solid_plane(
    output: &mut impl Write,
    value: u8,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let width = usize::try_from(width).map_err(|_| "PSD row is too wide")?;
    let height = usize::try_from(height).map_err(|_| "PSD has too many rows")?;
    let row = vec![value; width];
    let encoded_len =
        u16::try_from(packbits_row_len(&row)).map_err(|_| "PSD RLE row exceeds 65535 bytes")?;
    write_u16(output, 1)?;
    write_row_lengths(output, &vec![encoded_len; height])?;
    for _ in 0..height {
        write_packbits_row(output, &row)?;
    }
    Ok(())
}

fn write_rle_composite(
    output: &mut impl Write,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<(), String> {
    let lengths = (0..4)
        .map(|channel| rgba_row_lengths(rgba, channel, width, height))
        .collect::<Result<Vec<_>, _>>()?;
    write_u16(output, 1)?;
    for channel_lengths in &lengths {
        write_row_lengths(output, channel_lengths)?;
    }
    for channel in 0..4 {
        write_rgba_rows(output, rgba, channel, width)?;
    }
    Ok(())
}

fn write_row_lengths(output: &mut impl Write, lengths: &[u16]) -> Result<(), String> {
    for length in lengths {
        write_u16(output, *length)?;
    }
    Ok(())
}

fn write_rgba_rows(
    output: &mut impl Write,
    rgba: &[u8],
    channel: usize,
    width: u32,
) -> Result<(), String> {
    let width = usize::try_from(width).map_err(|_| "PSD row is too wide")?;
    let stride = width.checked_mul(4).ok_or("PSD row is too wide")?;
    let mut row = vec![0; width];
    for source in rgba.chunks_exact(stride) {
        for (destination, pixel) in row.iter_mut().zip(source.chunks_exact(4)) {
            *destination = pixel[channel];
        }
        write_packbits_row(output, &row)?;
    }
    Ok(())
}

fn packbits_row_len(row: &[u8]) -> usize {
    let mut encoded = 0_usize;
    let mut offset = 0_usize;
    while offset < row.len() {
        let run = repeated_run_len(row, offset);
        if run >= 3 {
            encoded += 2;
            offset += run;
            continue;
        }
        let start = offset;
        offset += run;
        while offset < row.len() && offset - start < 128 {
            let next_run = repeated_run_len(row, offset);
            if next_run >= 3 {
                break;
            }
            offset += next_run.min(128 - (offset - start));
        }
        encoded += 1 + (offset - start);
    }
    encoded
}

fn write_packbits_row(output: &mut impl Write, row: &[u8]) -> Result<(), String> {
    let mut offset = 0_usize;
    while offset < row.len() {
        let run = repeated_run_len(row, offset);
        if run >= 3 {
            let header = u8::try_from(257 - run).map_err(|_| "invalid PackBits repeat run")?;
            output.write_all(&[header, row[offset]]).map_err(io_error)?;
            offset += run;
            continue;
        }
        let start = offset;
        offset += run;
        while offset < row.len() && offset - start < 128 {
            let next_run = repeated_run_len(row, offset);
            if next_run >= 3 {
                break;
            }
            offset += next_run.min(128 - (offset - start));
        }
        let length = offset - start;
        output
            .write_all(&[u8::try_from(length - 1).map_err(|_| "invalid PackBits literal run")?])
            .map_err(io_error)?;
        output.write_all(&row[start..offset]).map_err(io_error)?;
    }
    Ok(())
}

fn repeated_run_len(row: &[u8], offset: usize) -> usize {
    let value = row[offset];
    let mut length = 1_usize;
    while length < 128 && offset + length < row.len() && row[offset + length] == value {
        length += 1;
    }
    length
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
    fn packbits_length_matches_encoded_rows() {
        let rows = [
            vec![7; 300],
            (0_u8..=255).collect::<Vec<_>>(),
            vec![1, 1, 2, 2, 3, 3, 3, 4, 5, 5, 6],
        ];
        for row in rows {
            let mut encoded = Vec::new();
            write_packbits_row(&mut encoded, &row).unwrap();
            assert_eq!(encoded.len(), packbits_row_len(&row));
        }
    }

    #[test]
    fn maps_all_known_sai2_blend_keys_to_photoshop() {
        let mappings = [
            (*b"norm", *b"norm"),
            (*b"pass", *b"pass"),
            (*b"mult", *b"mul "),
            (*b"scrn", *b"scrn"),
            (*b"over", *b"over"),
            (*b"sbad", *b"lbrn"),
            (*b"burn", *b"lddg"),
            (*b"ddge", *b"lLit"),
            (*b"bndg", *b"idiv"),
            (*b"ilit", *b"div "),
            (*b"cdif", *b"vLit"),
            (*b"slit", *b"sLit"),
            (*b"hlit", *b"hLit"),
            (*b"plit", *b"pLit"),
            (*b"hmix", *b"hMix"),
            (*b"dark", *b"dark"),
            (*b"litn", *b"lite"),
            (*b"drkc", *b"dkCl"),
            (*b"litc", *b"lgCl"),
            (*b"diff", *b"diff"),
            (*b"excl", *b"smud"),
            (*b"fsub", *b"fsub"),
            (*b"fdiv", *b"fdiv"),
            (*b"hue ", *b"hue "),
            (*b"sat ", *b"sat "),
            (*b"col ", *b"colr"),
            (*b"lum ", *b"lum "),
            (*b"sub ", *b"fsub"),
            (*b"add ", *b"lddg"),
            (*b"lbrn", *b"lbrn"),
            (*b"lddg", *b"lddg"),
            (*b"llit", *b"lLit"),
            (*b"cbrn", *b"idiv"),
            (*b"cddg", *b"div "),
            (*b"vlit", *b"vLit"),
        ];

        for (sai2, photoshop) in mappings {
            assert_eq!(psd_blend_key_for(sai2), photoshop, "SAI2 {sai2:?}");
        }
        assert_eq!(psd_blend_key_for(*b"????"), *b"norm");
    }

    #[test]
    fn inverts_sai2_mask_values_when_writing_a_psd_mask_channel() {
        let mut channel = Vec::new();
        write_rle_gray_plane(&mut channel, &[0, 1, 127, 254, 255], 5, 1).unwrap();

        assert_eq!(u16::from_be_bytes(channel[0..2].try_into().unwrap()), 1);
        let encoded_len = usize::from(u16::from_be_bytes(channel[2..4].try_into().unwrap()));
        assert_eq!(encoded_len, channel.len() - 4);
        assert_eq!(
            decode_packbits_test_row(&channel[4..]),
            [255, 254, 128, 1, 0]
        );
    }

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
    fn maps_sai2_clipping_and_alpha_lock_to_distinct_psd_fields() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/private/32x32-redball-greenball-multiple-layer.sai2");
        let Ok(input) = std::fs::read(path) else {
            return;
        };

        let alpha_locked = psd_layer_record_flags(&input, 0x0000_0100);
        assert_eq!(alpha_locked.0, 0, "alpha lock must not enable clipping");
        assert_eq!(alpha_locked.1 & 1, 1, "PSD transparency must be protected");

        let clipped = psd_layer_record_flags(&input, 0x0100_0000);
        assert_eq!(clipped.0, 1, "PSD clipping byte must be enabled");
        assert_eq!(clipped.1 & 1, 0, "clipping must not protect transparency");
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

    fn psd_layer_record_flags(input: &[u8], extra_flags: u32) -> (u8, u8) {
        let mut input = input.to_vec();
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
        let flags = u32::from_le_bytes(input[flags_offset..flags_offset + 4].try_into().unwrap());
        input[flags_offset..flags_offset + 4].copy_from_slice(&(flags | extra_flags).to_le_bytes());

        let composite = decode_integrated_image(&input, DecodeLimits::default()).unwrap();
        let layers = decode_layers(&input, DecodeLimits::default()).unwrap();
        let mut psd = Vec::new();
        write_layered(&mut psd, 32, 32, &layers, &composite, &input, true).unwrap();
        let record = psd
            .windows(8)
            .position(|window| window == b"8BIMmul ")
            .unwrap();
        (psd[record + 9], psd[record + 10])
    }

    fn decode_packbits_test_row(encoded: &[u8]) -> Vec<u8> {
        let mut decoded = Vec::new();
        let mut offset = 0;
        while offset < encoded.len() {
            let header = i8::from_ne_bytes([encoded[offset]]);
            offset += 1;
            if header >= 0 {
                let length = usize::from(u8::try_from(header).unwrap()) + 1;
                decoded.extend_from_slice(&encoded[offset..offset + length]);
                offset += length;
            } else if header != -128 {
                let length = usize::from(u8::try_from(1_i16 - i16::from(header)).unwrap());
                decoded.extend(std::iter::repeat_n(encoded[offset], length));
                offset += 1;
            }
        }
        decoded
    }
}
