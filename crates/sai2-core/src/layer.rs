use crate::{Chunk, DecodeLimits, FourCc, ParseError, RgbaImage, Sai2Document};

const LAYER_HEADER_LEN: usize = 56;
const BLOCK_SIZE: usize = 32;
const BLOCK_SIZE_I64: i64 = 32;
const BLOCK_PIXELS: usize = BLOCK_SIZE * BLOCK_SIZE;
const CHANNELS: usize = 4;
const CHANNEL_MAX: i32 = 0x4000;

/// An eight-bit grayscale image, used for decoded SAI2 layer masks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrayImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl GrayImage {
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

/// Metadata and, when supported, decoded pixels for one SAI2 layer mask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sai2Mask {
    id: u32,
    block_origin_x: i32,
    block_origin_y: i32,
    block_width: u32,
    block_height: u32,
    flags: [u8; 4],
    source_chunks: Vec<Chunk>,
    image: Option<GrayImage>,
}

/// One editable point from an observed SAI2 linework stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct Sai2StrokePoint {
    id: u32,
    position: [f64; 2],
    control_before: [f64; 2],
    control_after: [f64; 2],
    pressure: f32,
    width_scale: f32,
    flags: u32,
}

impl Sai2StrokePoint {
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }
    #[must_use]
    pub const fn position(&self) -> [f64; 2] {
        self.position
    }
    #[must_use]
    pub const fn control_before(&self) -> [f64; 2] {
        self.control_before
    }
    #[must_use]
    pub const fn control_after(&self) -> [f64; 2] {
        self.control_after
    }
    #[must_use]
    pub const fn pressure(&self) -> f32 {
        self.pressure
    }
    #[must_use]
    pub const fn width_scale(&self) -> f32 {
        self.width_scale
    }
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
}

/// One observed SAI2 linework stroke and its editable Bezier points.
#[derive(Debug, Clone, PartialEq)]
pub struct Sai2Stroke {
    id: u32,
    origin: [f64; 2],
    kind: u32,
    points: Vec<Sai2StrokePoint>,
}

impl Sai2Stroke {
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }
    #[must_use]
    pub const fn origin(&self) -> [f64; 2] {
        self.origin
    }
    #[must_use]
    pub const fn kind(&self) -> u32 {
        self.kind
    }
    #[must_use]
    pub fn points(&self) -> &[Sai2StrokePoint] {
        &self.points
    }
}

/// Structured content decoded from an observed `liwk` chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct Sai2Linework {
    color_bgra14: Option<[u16; 4]>,
    brush_size: Option<f32>,
    strokes: Vec<Sai2Stroke>,
}

impl Sai2Linework {
    #[must_use]
    pub const fn color_bgra14(&self) -> Option<[u16; 4]> {
        self.color_bgra14
    }
    #[must_use]
    pub const fn brush_size(&self) -> Option<f32> {
        self.brush_size
    }
    #[must_use]
    pub fn strokes(&self) -> &[Sai2Stroke] {
        &self.strokes
    }
}

/// One editable path point from an observed SAI2 shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Sai2ShapePoint {
    id: u32,
    position: [f64; 2],
    control_before: [f64; 2],
    control_after: [f64; 2],
    flags: u32,
}

impl Sai2ShapePoint {
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }
    #[must_use]
    pub const fn position(&self) -> [f64; 2] {
        self.position
    }
    #[must_use]
    pub const fn control_before(&self) -> [f64; 2] {
        self.control_before
    }
    #[must_use]
    pub const fn control_after(&self) -> [f64; 2] {
        self.control_after
    }
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
}

/// One closed or open path from an observed SAI2 shape layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Sai2ShapePath {
    id: u32,
    origin: [f64; 2],
    flags: u32,
    points: Vec<Sai2ShapePoint>,
}

impl Sai2ShapePath {
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }
    #[must_use]
    pub const fn origin(&self) -> [f64; 2] {
        self.origin
    }
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }
    #[must_use]
    pub fn points(&self) -> &[Sai2ShapePoint] {
        &self.points
    }
}

/// Structured content decoded from an observed `shap` chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct Sai2Shape {
    fill_bgra14: Option<[u16; 4]>,
    paths: Vec<Sai2ShapePath>,
}

impl Sai2Shape {
    #[must_use]
    pub const fn fill_bgra14(&self) -> Option<[u16; 4]> {
        self.fill_bgra14
    }
    #[must_use]
    pub fn paths(&self) -> &[Sai2ShapePath] {
        &self.paths
    }
}

impl Sai2Mask {
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    #[must_use]
    pub const fn block_origin(&self) -> (i32, i32) {
        (self.block_origin_x, self.block_origin_y)
    }

    #[must_use]
    pub const fn block_dimensions(&self) -> (u32, u32) {
        (self.block_width, self.block_height)
    }

    #[must_use]
    pub const fn flags(&self) -> [u8; 4] {
        self.flags
    }

    #[must_use]
    pub fn source_chunks(&self) -> &[Chunk] {
        &self.source_chunks
    }

    #[must_use]
    pub const fn image(&self) -> Option<&GrayImage> {
        self.image.as_ref()
    }
}

/// Metadata and, when supported, decoded pixels for one SAI2 layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Sai2Layer {
    id: u32,
    layer_type: FourCc,
    blend_mode: FourCc,
    opacity: u8,
    flags: u32,
    name: String,
    block_origin_x: i32,
    block_origin_y: i32,
    block_width: u32,
    tile_count: u32,
    mask: Option<Sai2Mask>,
    linework: Option<Sai2Linework>,
    shape: Option<Sai2Shape>,
    source_chunks: Vec<Chunk>,
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
    pub const fn block_origin(&self) -> (i32, i32) {
        (self.block_origin_x, self.block_origin_y)
    }
    #[must_use]
    pub const fn block_dimensions(&self) -> (u32, u32) {
        (self.block_width, self.tile_count)
    }
    #[must_use]
    pub const fn image(&self) -> Option<&RgbaImage> {
        self.image.as_ref()
    }
    #[must_use]
    pub const fn mask(&self) -> Option<&Sai2Mask> {
        self.mask.as_ref()
    }
    #[must_use]
    pub const fn linework(&self) -> Option<&Sai2Linework> {
        self.linework.as_ref()
    }
    #[must_use]
    pub const fn shape(&self) -> Option<&Sai2Shape> {
        self.shape.as_ref()
    }
    /// Returns the original SAI2 chunks associated with this layer.
    ///
    /// The records retain offsets into the input rather than copying chunk
    /// bodies, so callers can preserve still-unknown layer data without a
    /// second allocation proportional to the source document.
    #[must_use]
    pub fn source_chunks(&self) -> &[Chunk] {
        &self.source_chunks
    }
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.flags & 0x0001_0000 != 0
    }

    /// Returns the observed folder nesting level stored in the low flag word.
    #[must_use]
    pub const fn nesting_level(&self) -> u16 {
        let bytes = self.flags.to_le_bytes();
        u16::from_le_bytes([bytes[0], bytes[1]])
    }

    #[must_use]
    pub fn is_folder(&self) -> bool {
        self.layer_type.as_bytes() == *b"fold"
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
        let mask_id = layer.mask.as_ref().map(Sai2Mask::id);
        layer.source_chunks = document
            .chunks()
            .iter()
            .filter(|candidate| {
                candidate.object_id() == layer.id || Some(candidate.object_id()) == mask_id
            })
            .cloned()
            .collect();
        if let Some(mask) = &mut layer.mask {
            mask.source_chunks = document
                .chunks()
                .iter()
                .filter(|candidate| candidate.object_id() == mask.id)
                .cloned()
                .collect();
            if let Some(pixel_chunk) = mask
                .source_chunks
                .iter()
                .find(|candidate| candidate.kind() == FourCc::from_bytes(*b"mpix"))
            {
                mask.image = Some(decode_mask_image(
                    chunk_body(input, pixel_chunk)?,
                    mask.block_origin_x,
                    mask.block_origin_y,
                    mask.block_width,
                    mask.block_height,
                    header.width(),
                    header.height(),
                )?);
            }
        }
        if layer.layer_type == FourCc::from_bytes(*b"liwk")
            && let Some(data_chunk) = layer
                .source_chunks
                .iter()
                .find(|candidate| candidate.kind() == FourCc::from_bytes(*b"liwk"))
        {
            layer.linework = Some(decode_linework(chunk_body(input, data_chunk)?)?);
        }
        if layer.layer_type == FourCc::from_bytes(*b"shap")
            && let Some(data_chunk) = layer
                .source_chunks
                .iter()
                .find(|candidate| candidate.kind() == FourCc::from_bytes(*b"shap"))
        {
            layer.shape = Some(decode_shape(chunk_body(input, data_chunk)?)?);
        }
        if layer.layer_type == FourCc::from_bytes(*b"norm") {
            if let Some(pixel_chunk) = document.chunks().iter().find(|candidate| {
                candidate.kind() == FourCc::from_bytes(*b"lpix")
                    && candidate.object_id() == layer.id
            }) {
                let pixel_body = chunk_body(input, pixel_chunk)?;
                layer.image = Some(decode_raster_layer(
                    pixel_body,
                    layer.block_origin_x,
                    layer.block_origin_y,
                    layer.block_width,
                    layer.tile_count,
                    header.width(),
                    header.height(),
                )?);
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
    let block_origin_x = i32::from_le_bytes(read(body, 28)?);
    let block_origin_y = i32::from_le_bytes(read(body, 32)?);
    let block_width = u32::from_le_bytes(read(body, 36)?);
    let tile_count = u32::from_le_bytes(read(body, 40)?);
    let blend_mode = FourCc::from_bytes(read(body, 44)?);
    let opacity_raw = u32::from_le_bytes(read(body, 48)?);
    let opacity = u8::try_from(opacity_raw.min(100)).map_err(|_| layer_error("invalid opacity"))?;
    let flags = u32::from_le_bytes(read(body, 52)?);

    let mut name = format!("Layer {id}");
    let mut mask = None;
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
        } else if tag == *b"lmsk" {
            mask = Some(decode_mask_descriptor(value)?);
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
        block_origin_x,
        block_origin_y,
        block_width,
        tile_count,
        mask,
        linework: None,
        shape: None,
        source_chunks: Vec::new(),
        image: None,
    })
}

fn decode_mask_descriptor(value: &[u8]) -> Result<Sai2Mask, ParseError> {
    if value.len() != 24 {
        return Err(layer_error("unsupported layer-mask descriptor length"));
    }
    Ok(Sai2Mask {
        id: u32::from_le_bytes(read(value, 0)?),
        block_origin_x: i32::from_le_bytes(read(value, 4)?),
        block_origin_y: i32::from_le_bytes(read(value, 8)?),
        block_width: u32::from_le_bytes(read(value, 12)?),
        block_height: u32::from_le_bytes(read(value, 16)?),
        flags: read(value, 20)?,
        source_chunks: Vec::new(),
        image: None,
    })
}

fn decode_linework(body: &[u8]) -> Result<Sai2Linework, ParseError> {
    let mut result = Sai2Linework {
        color_bgra14: None,
        brush_size: None,
        strokes: Vec::new(),
    };
    let mut offset = 0_usize;
    while offset + 4 <= body.len() {
        let tag = read::<4>(body, offset)?;
        if tag == [0; 4] {
            if body[offset..].iter().any(|byte| *byte != 0) {
                return Err(layer_error("invalid liwk padding"));
            }
            return Ok(result);
        }
        let length = usize::try_from(u32::from_le_bytes(read(body, offset + 4)?))
            .map_err(|_| layer_error("linework record is too large"))?;
        let start = offset + 8;
        let end = start
            .checked_add(length)
            .ok_or_else(|| layer_error("linework record overflow"))?;
        let value = body
            .get(start..end)
            .ok_or_else(|| layer_error("truncated linework record"))?;
        if tag == *b"strk" {
            decode_stroke_container(value, &mut result)?;
        }
        offset = end;
    }
    Err(layer_error("linework chunk has no terminator"))
}

fn decode_stroke_container(value: &[u8], result: &mut Sai2Linework) -> Result<(), ParseError> {
    if value.len() < 16 {
        return Err(layer_error("truncated linework stroke container"));
    }
    let mut offset = 8_usize;
    loop {
        let tag = read::<4>(value, offset)?;
        if tag == [0; 4] {
            offset += 4;
            break;
        }
        let length = usize::try_from(u32::from_le_bytes(read(value, offset + 4)?))
            .map_err(|_| layer_error("linework parameter is too large"))?;
        let start = offset + 8;
        let end = start
            .checked_add(length)
            .ok_or_else(|| layer_error("linework parameter overflow"))?;
        let parameter = value
            .get(start..end)
            .ok_or_else(|| layer_error("truncated linework parameter"))?;
        match tag {
            value if value == *b"scol" => result.color_bgra14 = Some(decode_color14(parameter)?),
            value if value == *b"inkd" && parameter.len() >= 4 => {
                let size = f32::from_le_bytes(read(parameter, 0)?);
                if !size.is_finite() {
                    return Err(layer_error("non-finite linework brush size"));
                }
                result.brush_size = Some(size);
            }
            _ => {}
        }
        offset = end;
    }

    let stroke_count = usize::try_from(u32::from_le_bytes(read(value, offset)?))
        .map_err(|_| layer_error("too many linework strokes"))?;
    offset += 4;
    for _ in 0..stroke_count {
        let header_end = offset
            .checked_add(32)
            .ok_or_else(|| layer_error("linework stroke header overflow"))?;
        if header_end > value.len() {
            return Err(layer_error("truncated linework stroke header"));
        }
        let id = u32::from_le_bytes(read(value, offset)?);
        let origin = [read_f64(value, offset + 12)?, read_f64(value, offset + 20)?];
        let kind = u32::from_le_bytes(read(value, offset + 28)?);
        offset = header_end;
        let mut points = Vec::new();
        loop {
            let point_id = u32::from_le_bytes(read(value, offset)?);
            if point_id == 0 {
                offset += 4;
                break;
            }
            let end = offset
                .checked_add(64)
                .ok_or_else(|| layer_error("linework point overflow"))?;
            if end > value.len() {
                return Err(layer_error("truncated linework point"));
            }
            let point = Sai2StrokePoint {
                id: point_id,
                position: [read_f64(value, offset + 4)?, read_f64(value, offset + 12)?],
                control_before: [read_f64(value, offset + 20)?, read_f64(value, offset + 28)?],
                control_after: [read_f64(value, offset + 36)?, read_f64(value, offset + 44)?],
                pressure: read_f32(value, offset + 52)?,
                width_scale: read_f32(value, offset + 56)?,
                flags: u32::from_le_bytes(read(value, offset + 60)?),
            };
            points.push(point);
            offset = end;
        }
        result.strokes.push(Sai2Stroke {
            id,
            origin,
            kind,
            points,
        });
    }
    Ok(())
}

fn decode_shape(body: &[u8]) -> Result<Sai2Shape, ParseError> {
    if body.len() < 8 || read::<4>(body, 0)? != *b"shap" {
        return Err(layer_error("invalid shape chunk"));
    }
    let length = usize::try_from(u32::from_le_bytes(read(body, 4)?))
        .map_err(|_| layer_error("shape data is too large"))?;
    let end = 8_usize
        .checked_add(length)
        .ok_or_else(|| layer_error("shape data overflow"))?;
    let value = body
        .get(8..end)
        .ok_or_else(|| layer_error("truncated shape data"))?;
    if body[end..].iter().any(|byte| *byte != 0) {
        return Err(layer_error("invalid shape padding"));
    }
    decode_shape_container(value)
}

fn decode_shape_container(value: &[u8]) -> Result<Sai2Shape, ParseError> {
    if value.len() < 16 {
        return Err(layer_error("truncated shape container"));
    }
    let mut result = Sai2Shape {
        fill_bgra14: None,
        paths: Vec::new(),
    };
    let mut offset = 8_usize;
    loop {
        let tag = read::<4>(value, offset)?;
        if tag == [0; 4] {
            offset += 4;
            break;
        }
        let length = usize::try_from(u32::from_le_bytes(read(value, offset + 4)?))
            .map_err(|_| layer_error("shape parameter is too large"))?;
        let start = offset + 8;
        let end = start
            .checked_add(length)
            .ok_or_else(|| layer_error("shape parameter overflow"))?;
        let parameter = value
            .get(start..end)
            .ok_or_else(|| layer_error("truncated shape parameter"))?;
        if tag == *b"fcol" {
            result.fill_bgra14 = Some(decode_color14(parameter)?);
        }
        offset = end;
    }

    let path_count = usize::try_from(u32::from_le_bytes(read(value, offset)?))
        .map_err(|_| layer_error("too many shape paths"))?;
    offset += 4;
    for _ in 0..path_count {
        let header_end = offset
            .checked_add(32)
            .ok_or_else(|| layer_error("shape path header overflow"))?;
        if header_end > value.len() {
            return Err(layer_error("truncated shape path header"));
        }
        let id = u32::from_le_bytes(read(value, offset)?);
        let origin = [read_f64(value, offset + 12)?, read_f64(value, offset + 20)?];
        let flags = u32::from_le_bytes(read(value, offset + 28)?);
        offset = header_end;
        let mut points = Vec::new();
        loop {
            let point_id = u32::from_le_bytes(read(value, offset)?);
            if point_id == 0 {
                offset += 4;
                break;
            }
            let end = offset
                .checked_add(64)
                .ok_or_else(|| layer_error("shape point overflow"))?;
            if end > value.len() {
                return Err(layer_error("truncated shape point"));
            }
            points.push(Sai2ShapePoint {
                id: point_id,
                position: [read_f64(value, offset + 4)?, read_f64(value, offset + 12)?],
                control_before: [read_f64(value, offset + 20)?, read_f64(value, offset + 28)?],
                control_after: [read_f64(value, offset + 36)?, read_f64(value, offset + 44)?],
                flags: u32::from_le_bytes(read(value, offset + 60)?),
            });
            offset = end;
        }
        result.paths.push(Sai2ShapePath {
            id,
            origin,
            flags,
            points,
        });
    }
    Ok(result)
}

fn decode_color14(value: &[u8]) -> Result<[u16; 4], ParseError> {
    if value.len() != 8 {
        return Err(layer_error("unsupported 14-bit color length"));
    }
    Ok([
        u16::from_le_bytes(read(value, 0)?),
        u16::from_le_bytes(read(value, 2)?),
        u16::from_le_bytes(read(value, 4)?),
        u16::from_le_bytes(read(value, 6)?),
    ])
}

fn read_f64(input: &[u8], offset: usize) -> Result<f64, ParseError> {
    let value = f64::from_le_bytes(read(input, offset)?);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(layer_error("non-finite vector coordinate"))
    }
}

fn read_f32(input: &[u8], offset: usize) -> Result<f32, ParseError> {
    let value = f32::from_le_bytes(read(input, offset)?);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(layer_error("non-finite vector value"))
    }
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

fn decode_raster_layer(
    body: &[u8],
    block_origin_x: i32,
    block_origin_y: i32,
    block_width: u32,
    block_height: u32,
    width: u32,
    height: u32,
) -> Result<RgbaImage, ParseError> {
    let mut rgba = vec![0; image_len(width, height)?];
    decode_block_grid::<CHANNELS, _>(
        body,
        block_origin_x,
        block_origin_y,
        block_width,
        block_height,
        width,
        height,
        |values, block_x, block_y| blit_block(values, block_x, block_y, width, height, &mut rgba),
    )?;
    Ok(RgbaImage::from_pixels(width, height, rgba))
}

fn decode_mask_image(
    body: &[u8],
    block_origin_x: i32,
    block_origin_y: i32,
    block_width: u32,
    block_height: u32,
    width: u32,
    height: u32,
) -> Result<GrayImage, ParseError> {
    let mut pixels = vec![0; gray_image_len(width, height)?];
    decode_block_grid::<1, _>(
        body,
        block_origin_x,
        block_origin_y,
        block_width,
        block_height,
        width,
        height,
        |values, block_x, block_y| {
            blit_mask_block(values, block_x, block_y, width, height, &mut pixels)
        },
    )?;
    Ok(GrayImage {
        width,
        height,
        pixels,
    })
}

#[allow(clippy::too_many_arguments)]
fn decode_block_grid<const N: usize, F>(
    body: &[u8],
    block_origin_x: i32,
    block_origin_y: i32,
    block_width: u32,
    block_height: u32,
    width: u32,
    height: u32,
    mut blit: F,
) -> Result<(), ParseError>
where
    F: FnMut(&[[i32; N]; BLOCK_PIXELS], i64, i64) -> Result<(), ParseError>,
{
    if block_width == 0 && block_height == 0 {
        return Ok(());
    }
    if block_width == 0 || block_height == 0 {
        return Err(layer_error("inconsistent lpix block dimensions"));
    }
    if body.len() < 4 || read::<4>(body, 0)? != *b"dpcm" {
        return Err(layer_error("unsupported lpix encoding"));
    }
    let rows = usize::try_from(block_height).map_err(|_| layer_error("too many lpix rows"))?;
    let table_bytes = rows
        .checked_mul(4)
        .and_then(|length| length.checked_add(4))
        .ok_or_else(|| layer_error("lpix row-size table overflow"))?;
    if body.len() < table_bytes {
        return Err(layer_error("truncated lpix row-size table"));
    }

    let mut row_sizes = Vec::with_capacity(rows);
    for row in 0..rows {
        let offset = 4 + row * 4;
        row_sizes.push(
            usize::try_from(u32::from_le_bytes(read(body, offset)?))
                .map_err(|_| layer_error("lpix row is too large"))?,
        );
    }
    let mut offset = table_bytes;
    for (row, size) in row_sizes.into_iter().enumerate() {
        let end = offset
            .checked_add(size)
            .ok_or_else(|| layer_error("lpix row overflow"))?;
        let stream = body
            .get(offset..end)
            .ok_or_else(|| layer_error("truncated lpix row"))?;
        decode_block_row::<N, _>(
            stream,
            block_origin_x,
            block_origin_y,
            block_width,
            row,
            width,
            height,
            &mut blit,
        )?;
        offset = end;
    }
    let padding = body
        .get(offset..)
        .ok_or_else(|| layer_error("invalid lpix padding"))?;
    if padding.len() > 3 || padding.iter().any(|byte| *byte != 0) {
        return Err(layer_error("invalid lpix padding"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn decode_block_row<const N: usize, F>(
    stream: &[u8],
    block_origin_x: i32,
    block_origin_y: i32,
    block_width: u32,
    row: usize,
    canvas_width: u32,
    canvas_height: u32,
    blit: &mut F,
) -> Result<(), ParseError>
where
    F: FnMut(&[[i32; N]; BLOCK_PIXELS], i64, i64) -> Result<(), ParseError>,
{
    let blocks = usize::try_from(block_width).map_err(|_| layer_error("lpix row is too wide"))?;
    let row_i32 = i32::try_from(row).map_err(|_| layer_error("lpix row index is too large"))?;
    let block_y = block_origin_y
        .checked_add(row_i32)
        .ok_or_else(|| layer_error("lpix block Y overflow"))?;
    let mut block_x = 0_usize;
    let mut offset = 0_usize;
    while offset + 2 <= stream.len() {
        let marker = u16::from_le_bytes(read(stream, offset)?);
        offset += 2;
        if marker & 0xff != 0xff {
            return Err(layer_error("invalid lpix block marker"));
        }
        let kind = (marker >> 12) as u8;
        let marker_x = u8::try_from((marker >> 8) & 0x0f)
            .map_err(|_| layer_error("invalid lpix block X marker"))?;
        let absolute_x = i64::from(block_origin_x)
            .checked_add(i64::try_from(block_x).map_err(|_| layer_error("lpix block X overflow"))?)
            .ok_or_else(|| layer_error("lpix block X overflow"))?;
        let expected_x = u8::try_from(absolute_x.rem_euclid(16))
            .map_err(|_| layer_error("invalid lpix block X checksum"))?;
        if marker_x != expected_x {
            return Err(layer_error("unexpected lpix block X checksum"));
        }

        match kind {
            0x0 => {
                let skip = usize::from(u16::from_le_bytes(read(stream, offset)?)) + 1;
                offset += 2;
                block_x = block_x
                    .checked_add(skip)
                    .ok_or_else(|| layer_error("lpix transparent run overflow"))?;
                if block_x > blocks {
                    return Err(layer_error("lpix transparent run exceeds row width"));
                }
            }
            0x5 => {
                if block_x >= blocks {
                    return Err(layer_error("lpix solid block exceeds row width"));
                }
                let mut color = [0_i32; N];
                for channel in &mut color {
                    let value = i32::from(u16::from_le_bytes(read(stream, offset)?));
                    offset += 2;
                    if value > CHANNEL_MAX {
                        return Err(layer_error("lpix solid channel exceeds 14-bit maximum"));
                    }
                    *channel = value;
                }
                if block_intersects_canvas(
                    absolute_x,
                    i64::from(block_y),
                    canvas_width,
                    canvas_height,
                ) {
                    blit(&[color; BLOCK_PIXELS], absolute_x, i64::from(block_y))?;
                }
                block_x += 1;
            }
            0xa => {
                if block_x >= blocks {
                    return Err(layer_error("lpix DPCM block exceeds row width"));
                }
                let size = usize::from(u16::from_le_bytes(read(stream, offset)?));
                offset += 2;
                let end = offset
                    .checked_add(size)
                    .ok_or_else(|| layer_error("lpix block overflow"))?;
                let compressed = stream
                    .get(offset..end)
                    .ok_or_else(|| layer_error("truncated lpix block"))?;
                if block_intersects_canvas(
                    absolute_x,
                    i64::from(block_y),
                    canvas_width,
                    canvas_height,
                ) {
                    let values = decode_dpcm_block::<N>(compressed)?;
                    blit(&values, absolute_x, i64::from(block_y))?;
                }
                offset = end;
                block_x += 1;
            }
            0xf => {
                if block_x != blocks || offset != stream.len() {
                    return Err(layer_error("lpix row ended at the wrong block"));
                }
                return Ok(());
            }
            _ => return Err(layer_error("unsupported lpix block kind")),
        }
    }
    Err(layer_error("lpix row has no terminator"))
}

fn block_intersects_canvas(block_x: i64, block_y: i64, width: u32, height: u32) -> bool {
    let left = block_x * BLOCK_SIZE_I64;
    let top = block_y * BLOCK_SIZE_I64;
    let right = left + BLOCK_SIZE_I64;
    let bottom = top + BLOCK_SIZE_I64;
    right > 0 && bottom > 0 && left < i64::from(width) && top < i64::from(height)
}

fn blit_block(
    values: &[[i32; CHANNELS]; BLOCK_PIXELS],
    block_x: i64,
    block_y: i64,
    canvas_width: u32,
    canvas_height: u32,
    rgba: &mut [u8],
) -> Result<(), ParseError> {
    let width = usize::try_from(canvas_width).map_err(|_| layer_error("invalid layer width"))?;
    let left = block_x * BLOCK_SIZE_I64;
    let top = block_y * BLOCK_SIZE_I64;
    for y in 0..BLOCK_SIZE {
        let canvas_y = top + i64::try_from(y).map_err(|_| layer_error("lpix pixel Y overflow"))?;
        if canvas_y < 0 || canvas_y >= i64::from(canvas_height) {
            continue;
        }
        for x in 0..BLOCK_SIZE {
            let canvas_x =
                left + i64::try_from(x).map_err(|_| layer_error("lpix pixel X overflow"))?;
            if canvas_x < 0 || canvas_x >= i64::from(canvas_width) {
                continue;
            }
            let pixel = values[y * BLOCK_SIZE + x];
            let alpha = pixel[3];
            let destination =
                (usize::try_from(canvas_y).map_err(|_| layer_error("invalid pixel Y"))? * width
                    + usize::try_from(canvas_x).map_err(|_| layer_error("invalid pixel X"))?)
                    * CHANNELS;
            rgba[destination..destination + CHANNELS].copy_from_slice(&[
                unpremultiply(pixel[2], alpha),
                unpremultiply(pixel[1], alpha),
                unpremultiply(pixel[0], alpha),
                scale_14_to_8(alpha),
            ]);
        }
    }
    Ok(())
}

fn blit_mask_block(
    values: &[[i32; 1]; BLOCK_PIXELS],
    block_x: i64,
    block_y: i64,
    canvas_width: u32,
    canvas_height: u32,
    pixels: &mut [u8],
) -> Result<(), ParseError> {
    let width = usize::try_from(canvas_width).map_err(|_| layer_error("invalid mask width"))?;
    let left = block_x * BLOCK_SIZE_I64;
    let top = block_y * BLOCK_SIZE_I64;
    for y in 0..BLOCK_SIZE {
        let canvas_y = top + i64::try_from(y).map_err(|_| layer_error("mask pixel Y overflow"))?;
        if canvas_y < 0 || canvas_y >= i64::from(canvas_height) {
            continue;
        }
        for x in 0..BLOCK_SIZE {
            let canvas_x =
                left + i64::try_from(x).map_err(|_| layer_error("mask pixel X overflow"))?;
            if canvas_x < 0 || canvas_x >= i64::from(canvas_width) {
                continue;
            }
            let destination =
                usize::try_from(canvas_y).map_err(|_| layer_error("invalid mask pixel Y"))? * width
                    + usize::try_from(canvas_x).map_err(|_| layer_error("invalid mask pixel X"))?;
            pixels[destination] = scale_14_to_8(values[y * BLOCK_SIZE + x][0]);
        }
    }
    Ok(())
}

fn decode_dpcm_block<const N: usize>(
    compressed: &[u8],
) -> Result<[[i32; N]; BLOCK_PIXELS], ParseError> {
    let mut deltas = vec![0_i16; BLOCK_PIXELS * N];
    let consumed = crate::image::decode_delta_row(compressed, &mut deltas, BLOCK_PIXELS, N, N)?;
    if consumed != compressed.len() {
        return Err(layer_error("lpix block has unused compressed bytes"));
    }
    let mut values = [[0_i32; N]; BLOCK_PIXELS];
    for y in 0..BLOCK_SIZE {
        let mut left = [0_i32; N];
        let mut upper_left = [0_i32; N];
        for x in 0..BLOCK_SIZE {
            let index = y * BLOCK_SIZE + x;
            for channel in 0..N {
                let above = if y == 0 {
                    0
                } else {
                    values[index - BLOCK_SIZE][channel]
                };
                let predicted = (left[channel] + above - upper_left[channel]).clamp(0, CHANNEL_MAX);
                let value =
                    (predicted + i32::from(deltas[index * N + channel])).clamp(0, CHANNEL_MAX);
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

fn gray_image_len(width: u32, height: u32) -> Result<usize, ParseError> {
    usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| layer_error("mask image is too large"))
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
    fn decodes_large_offset_layer_when_owned_fixture_is_available() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/private/300x300-izunaface-white-background.sai2");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let layers =
            decode_layers(&bytes, DecodeLimits::default()).expect("layer metadata should parse");
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].tile_count(), 32);
        assert_eq!(layers[0].block_origin(), (-1, -4));
        assert_eq!(layers[0].block_dimensions(), (22, 32));
        let integrated = crate::decode_integrated_image(&bytes, DecodeLimits::default())
            .expect("integrated image should decode");
        let layer = layers[0].image().expect("large raster layer should decode");
        let mut different = 0_usize;
        let mut maximum_delta = 0_u8;
        for (source, expected) in layer
            .pixels()
            .chunks_exact(4)
            .zip(integrated.pixels().chunks_exact(4))
        {
            let alpha = u16::from(source[3]);
            for channel in 0..3 {
                let actual = u8::try_from(
                    (u16::from(source[channel]) * alpha + 255 * (255 - alpha) + 127) / 255,
                )
                .unwrap();
                let delta = actual.abs_diff(expected[channel]);
                maximum_delta = maximum_delta.max(delta);
                different += usize::from(delta != 0);
            }
        }
        // The 14-bit premultiplied source must be quantized to 8-bit straight
        // alpha for PSD. Re-compositing may therefore differ by one level.
        assert!(
            different < 4_000,
            "too many differing channels: {different}"
        );
        assert!(maximum_delta <= 1, "maximum channel delta: {maximum_delta}");
    }

    #[test]
    fn decodes_owned_folder_and_mask_fixture_when_available() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../fixtures/private/izunaface-multipleLayersInFolder-maskWithBitmapLayer-singleLineVector-shapeLayer.sai2",
        );
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };

        let layers = decode_layers(&bytes, DecodeLimits::default()).unwrap();

        assert_eq!(layers.len(), 6);
        assert_eq!(
            layers
                .iter()
                .map(|layer| layer.layer_type().as_bytes())
                .collect::<Vec<_>>(),
            [*b"fold", *b"liwk", *b"norm", *b"norm", *b"shap", *b"norm"]
        );
        assert_eq!(
            layers
                .iter()
                .map(|layer| layer.flags() & 0xffff)
                .collect::<Vec<_>>(),
            [0, 1, 1, 1, 1, 0]
        );
        assert!(layers[0].image().is_none());
        assert!(layers[1].image().is_none());
        assert!(layers[2].image().is_some());
        assert!(layers[3].image().is_some());
        assert!(layers[4].image().is_none());
        assert!(layers[5].image().is_some());

        let solid = layers[5]
            .image()
            .expect("solid-color raster pixels should decode");
        assert!(
            solid
                .pixels()
                .chunks_exact(CHANNELS)
                .all(|pixel| pixel == [255, 213, 0, 255]),
            "the #FFD500 layer should fill the complete canvas"
        );

        let mask = layers[2].mask().expect("bitmap layer should have a mask");
        assert_eq!(mask.id(), 12);
        assert_eq!(mask.block_origin(), (0, 0));
        assert_eq!(mask.block_dimensions(), (10, 10));
        assert_eq!(mask.flags(), [1, 1, 1, 0]);
        assert_eq!(mask.source_chunks().len(), 1);
        assert_eq!(mask.source_chunks()[0].kind().as_bytes(), *b"mpix");
        let image = mask.image().expect("mask pixels should decode");
        assert_eq!((image.width(), image.height()), (300, 300));
        assert_eq!(image.pixels().iter().copied().min(), Some(0));
        assert_eq!(image.pixels().iter().copied().max(), Some(250));
        assert_eq!(layers[2].source_chunks().len(), 3);

        let linework = layers[1]
            .linework()
            .expect("linework structure should decode");
        assert_eq!(
            linework.color_bgra14(),
            Some([0x2f6f, 0x23e3, 0x12d2, 0x4000])
        );
        assert_eq!(linework.brush_size(), Some(50.0));
        assert_eq!(linework.strokes().len(), 1);
        let stroke = &linework.strokes()[0];
        assert_eq!(stroke.id(), 1);
        assert_eq!(stroke.kind(), 2);
        assert_eq!(stroke.points().len(), 13);
        assert_eq!(
            stroke
                .points()
                .iter()
                .map(Sai2StrokePoint::id)
                .collect::<Vec<_>>(),
            [1, 2, 13, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
        );
        assert_eq!(stroke.points()[0].pressure().to_bits(), 1.0_f32.to_bits());
        assert_eq!(stroke.points()[4].pressure().to_bits(), 0.0_f32.to_bits());
        assert!((stroke.points()[7].pressure() - 0.497_263_13).abs() < 0.000_001);
        assert!((stroke.points()[10].pressure() - 0.249_684_21).abs() < 0.000_001);

        let shape = layers[4].shape().expect("shape structure should decode");
        assert_eq!(shape.fill_bgra14(), Some([0x2f6f, 0x23e3, 0x12d2, 0x4000]));
        assert_eq!(shape.paths().len(), 1);
        let path = &shape.paths()[0];
        assert_eq!(
            path.origin().map(f64::to_bits),
            [152.0_f64.to_bits(), 154.0_f64.to_bits()]
        );
        assert_eq!(path.points().len(), 4);
        let absolute = path
            .points()
            .iter()
            .map(|point| {
                [
                    path.origin()[0] + point.position()[0],
                    path.origin()[1] + point.position()[1],
                ]
            })
            .collect::<Vec<_>>();
        assert_eq!(
            absolute,
            [[18.0, 20.0], [286.0, 20.0], [286.0, 288.0], [18.0, 288.0]]
        );
    }

    fn synthetic_layer_document() -> Vec<u8> {
        let mut layr = vec![0_u8; 80];
        layr[0..4].copy_from_slice(b"layr");
        layr[4..8].copy_from_slice(&2_u32.to_le_bytes());
        layr[16..20].copy_from_slice(b"norm");
        layr[36..40].copy_from_slice(&1_u32.to_le_bytes());
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
