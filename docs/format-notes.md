# SAI2 format notes

This document deliberately separates published facts, local observations,
implementation assumptions, and open questions. The current implementation is
based on the Photopea author's
[unofficial specification](https://github.com/photopea/SAI2-specification), not
on reverse engineering the PaintTool SAI executable.

## Known from the public specification

- A SAI2 file begins with a fixed 64-byte header.
- Multi-byte integers in the described structures are little-endian.
- Header offsets currently used by `sai2-core` are:

  | Offset | Size | Meaning |
  | ---: | ---: | --- |
  | `0x00` | 16 | ASCII signature `SAI-CANVAS-TYPE0` |
  | `0x10` | 4 | flags (`uint32`) |
  | `0x14` | 4 | canvas width (`uint32`) |
  | `0x18` | 4 | canvas height (`uint32`) |
  | `0x1c` | 4 | unknown (`uint32`) |
  | `0x20` | 4 | number of chunks (`uint32`) |
  | `0x24` | 4 | unknown (`uint32`) |
  | `0x28` | 16 | described as zero bytes |
  | `0x38` | 4 | background/grey color, commonly `0xff808080` |
  | `0x3c` | 4 | ASCII format tag, documented as `norm` or `ver1` |

- The header is followed by `N` 16-byte chunk-list records and then chunk
  bodies. Each published table record contains:

  | Relative offset | Size | Meaning |
  | ---: | ---: | --- |
  | `0x00` | 4 | four-byte chunk type (`hist`, `intg`, `layr`, `lpix`, ...) |
  | `0x04` | 4 | object ID (`uint32`) |
  | `0x08` | 8 | absolute chunk offset (`uint64`) |

  Photopea's specification describes the last eight bytes as a 32-bit offset
  followed by a zero. The public `Wunkolo/libsai` API models the same bytes as
  one 64-bit absolute offset. The parser uses `uint64` so it does not discard
  high bits; all current fixtures have zero high bits and are compatible with
  both descriptions.

## Observed in owned fixtures

Three purpose-built, privately held 32 x 32 SAI2 fixtures were compared: a
blank transparent canvas, one raster layer over a white background, and two
raster layers. The files themselves remain ignored and are not published.

- The fixed header and all table entries match the public specification.
- The chunk table begins at byte 64 and ends at `64 + N * 16`.
- The first chunk starts exactly at the end of the table in all three files.
- Chunk offsets are absolute file offsets and strictly increase in table order.
- The distance to the next offset, or to end-of-file for the final entry,
  exactly bounds every observed chunk body.
- The upper 32 offset bits are zero in all 14 observed entries.
- The blank and single-layer files contain `hist`, `intg`, `layr`, and `lpix`.
  The two-layer file adds a second `layr` and `lpix`, for six entries total.
- Matching `layr` and `lpix` entries share object IDs. The `hist` and `intg`
  entries use object ID zero in these fixtures.
- Observed chunk bodies do not have a uniform prefix: `layr` begins with
  `layr` and its object ID, `intg` and nonempty `lpix` begin with `dpcm`, while
  the blank layer's four-byte `lpix` body is zero. Phase 2 does not interpret
  any of these bodies.

### Integrated image (`intg`)

All three fixtures use this single-tile framing:

1. ASCII `dpcm`;
2. one little-endian `uint32` byte length per 256 x 256 tile;
3. for each tile row, each sized tile blob followed by a two-byte row marker;
4. zero padding to a four-byte chunk boundary.

Each sized tile blob starts with a marker `(tile_x << 8) | 0x00ff`; the marker
is included in the declared tile length. The marker after a complete tile row
is `(tiles_x << 8) | 0x00ff` and is not included in any tile length. For the
one-tile fixtures these are `ff 00` and `ff 01` on disk.

The remaining tile bytes contain one independently byte-aligned stream per
image row. Bits are read least-significant-bit first. Values are grouped by
channel, with unary opcodes selecting literal delta widths or zero runs. The
decoded signed 16-bit deltas are restored with a left/up/up-left row predictor
and reduced to eight-bit BGRA; the public API description compares this stage
to PNG's Up filter. The reader then returns RGBA.

The second byte of the header flags selected four input channels for the
transparent blank fixture and three channels for both opaque fixtures. In the
three-channel mode the output alpha channel is filled with 255.

The tested opaque fixtures use header flags `0x00000100` and display over a
white canvas; the transparent fixture uses `0x00002000`. `sai2topsd` adds a
white bottom layer only for the observed three-channel/opaque mode. This is an
export representation of SAI2's canvas setting, not a source `layr` chunk.

Decoded results:

- the blank transparent fixture produces 1,024 pixels with alpha zero;
- both nonblank fixtures produce 1,024 pixels with alpha 255;
- the red circle and overlapping red/green circles visually match the supplied
  SAI2 thumbnails, including placement and overlap order.
- a privately held 300 x 300 artwork fixture produces four tiles, including
  44-pixel right and bottom edges; its decoded RGBA output differs from the
  supplied reference PNG in zero of 90,000 pixels, including across both
  256-pixel tile boundaries.

### Raster layers (`layr` / `lpix`)

The fixed 56-byte portion of every observed `layr` chunk agrees with the public
field ordering for layer ID, type, four signed integers, an integer, a row
count, blend mode, opacity, and flags. In the tested raster layers, offsets 28
and 32 are the signed X/Y origins of a 32 x 32 block grid, offset 36 is its
width, and offset 40 is its height. The observed `name` parameter begins with a
little-endian UTF-16 code-unit count followed by the UTF-16LE layer name.

The two-layer 32 x 32 fixture has one outer `lpix` tile per layer. Each outer
tile contains one compressed 32 x 32 pixel block:

1. marker `ff a0`;
2. a little-endian `uint16` compressed byte length;
3. one DPCM stream covering 1,024 pixels and four channels;
4. terminal marker `ff f1`.

The same LSB-first delta/RLE vocabulary used by `intg` decodes the block when
the 32 x 32 pixels are treated as a single four-channel stream. The restored
channels are 14-bit premultiplied BGRA with `0x4000` as full intensity. The
reader applies the two-dimensional predictor per 32-pixel row, converts to
straight-alpha RGBA, and crops the block to the canvas.

The 300 x 300 artwork uses a 22 x 32 block grid with origin (-1, -4). Its `lpix`
body begins with `dpcm`, then one little-endian `uint32` stream length for each
block row. Each row stream contains compressed block markers `ff aN`, apparent
uniform-color markers `ff 5N`, transparent-run records `ff 0N`, and one `ff fN`
terminal record. The `N` nibble is the absolute block X coordinate modulo 16.
Transparent-run payloads are a little-endian `uint16` run length minus one.
Compressed and uniform blocks advance X by one; the terminal is reached after
the declared grid width. Blocks are positioned in canvas space using the
signed grid origin and cropped at canvas edges.

The reader decodes this sparse grid and its compressed raster blocks. When the
resulting straight-alpha 8-bit layer is composited over white, 3,522 of 270,000
RGB channel values differ from the saved integrated image, all by exactly one
level. The saved 14-bit premultiplied channels cannot always be represented
losslessly as 8-bit straight alpha. `sai2topsd` therefore also embeds the saved
integrated image as the PSD composite, which matches the supplied reference PNG
pixel-for-pixel. An observed `5N` block stores one little-endian `uint16` per
channel. The values are 14-bit fixed-point intensities whose inclusive maximum
is `0x4000`, not `0x3fff`. A solid `#FFD500` fixture stores premultiplied BGRA
as `[0x0000, 0x3556, 0x4000, 0x4000]`; masking these values with `0x3fff`
would incorrectly turn full red and full alpha into zero.

### PSD source-layer preservation (`s2ly`)

`sai2topsd` writes one private Additional Layer Information block to every PSD
layer. Its four-byte key is `s2ly`; it is not part of the SAI2 format. The
payload uses big-endian integers, like its PSD container:

| Size | Meaning |
| ---: | --- |
| 8 | ASCII `SAI2LYR` followed by zero |
| 4 | preservation format version, currently 1 |
| 4 | number of source chunks |
| repeated | chunk records in original chunk-table order |

Each chunk record contains its four-byte kind, 32-bit object ID, 64-bit source
offset, 64-bit body size, and the exact body bytes. The block therefore retains
unknown layer-specific data without interpreting or duplicating it in the core
document model. Tests compare every embedded body byte with its owned SAI2
fixture, and an independent PSD reader successfully opens the file while
retaining the unknown tagged block. This provides a lossless preservation path
for future linework/control-point and text decoders, but it does not yet expose
those structures as editable PSD vector or text objects. Unknown/private PSD
metadata can also be lost if another application re-saves the PSD.

SAI2 `layr` chunks in the owned two-layer fixture occur from top to bottom. PSD
layer records are composited from bottom to top, so `sai2topsd` reverses the
source order and places the synthetic canvas background first. Independent
`psd-tools` recomposition differs from the saved 32 x 32 composite by at most
one 8-bit level and from the 300 x 300 composite by at most two levels. The
remaining error is consistent with converting SAI2's 14-bit premultiplied
channels into PSD's 8-bit straight-alpha representation and renderer rounding.

### Folder hierarchy, masks, linework, and shapes

An owned 300 x 300 fixture contains six `layr` records in SAI2's top-to-bottom
order: a folder, linework, masked raster, raster, shape, and solid-color raster.
The low byte of the layer flag words is `0, 1, 1, 1, 1, 0`; it exactly
describes folder nesting depth. The folder has type `fold`, blend key `pass`,
and high flag bits `0x40010000`. `sai2topsd` maps the hierarchy to PSD `lsct`
records (type 1 for the folder and type 3 for its bounding divider) and retains
pass-through mode.

The masked layer's `layr` parameter `lmsk` is 24 bytes. In this fixture it
contains mask object ID 12, block origin `(0, 0)`, a 10 x 10 block grid, and
four trailing flag bytes `[1, 1, 1, 0]`. The matching `mpix` chunk is a
one-channel form of the block DPCM family used by `lpix`. It decodes to a
300 x 300 grayscale image with observed values from 0 through 250. PSD export
stores this as channel ID -2 with a full-canvas 20-byte layer-mask record. The
exact `mpix` body is also included in the owning layer's `s2ly` block.

The fixture's `liwk` body contains a `strk` container. Its observed color is
four 14-bit BGRA values, its brush size is a 32-bit float, and its one stroke
contains an origin plus thirteen 64-byte point records. Each point has an ID;
double-precision position, preceding control point, and following control point
pairs; single-precision pressure and width scale; and a 32-bit flag word.
Coordinates are relative to the stroke origin. The typed decoder retains all
fields, including zero and fractional pressure values.

A larger owned document contains five `liwk` chunks and hundreds of strokes.
It confirms that every top-level `strk` record contains one stroke. The
identifier after the parameter terminator is repeated at the start of that
stroke's header; it is not a stroke count. The first small fixture used ID 1,
which made the two interpretations accidentally indistinguishable. Unknown
stroke footers remain bounded by the enclosing record and are preserved in
`s2ly`.

A 5870 x 4175 production fixture contains 343 strokes and 2018 points in one
visible linework layer. It confirms that `scol`, brush size, and the observed
ink-opacity field belong to each `strk` container rather than the whole layer.
The renderer evaluates each cubic segment from the outgoing control of one
point to the incoming control of the next, interpolates pressure and width
scale, and applies the observed soft pen profile. Its nontransparent support
and total alpha closely match the SAI2-exported reference PNG, while the source
Bezier data remains preserved in `s2ly`.

The fixture's `shap` body contains a 14-bit BGRA fill color and one path. The
path stores a double-precision origin and four point records using the same
position/control-point layout, followed by flags. Adding the origin to the
relative positions produces the observed rectangle corners `(18, 20)`,
`(286, 20)`, `(286, 288)`, and `(18, 288)`.

Observed blend keys map to PSD as follows: `pass` remains `pass`, `burn` maps
to Color Burn (`idiv`), and the user-confirmed SAI2 mode 比較（明） stored as
`litn` maps to Lighten (`lite`). These mappings are fixture-backed but are not
a complete blend-mode table.

The PSD writer rasterizes decoded linework to an independent pixel layer. Shape
layers still receive transparent pixel channels while their typed geometry and
exact source chunks are preserved. The PSD composite preview remains
pixel-identical to SAI2's saved `intg`; native PSD vector-shape output remains
open work.

Layer and composite channel pixels are written with PSD PackBits/RLE
compression. A 5870 x 4175 owned document with 32 layers would occupy 2.811 GiB
with full-canvas raw channels, beyond Photoshop's supported 2 GB PSD limit. RLE
reduces the observed output to about 246 MiB without changing decoded pixels.

## Assumed by the current implementation

- The 16-byte signature is required exactly; accepting alternate magic values
  would make file-type detection ambiguous.
- Reserved, unknown, color, and format-tag values are reported but not
  validated. This avoids rejecting future or currently undocumented variants.
- Width and height are exposed as raw unsigned values. Plausibility and
  allocation limits belong to later operations that consume dimensions.
- A numeric file-format version is not inferred from flags or the four-byte
  tag because the public specification does not define such a mapping.
- Chunk sizes are derived from ordered adjacent offsets and end-of-file because
  the table does not store sizes. Inputs with offsets inside the header/table,
  beyond end-of-file, or in decreasing order are rejected. Equal offsets are
  retained as zero-length chunks until real fixtures establish whether they
  should be rejected.
- A chunk is not required to repeat its table type at the start of its body.
  The observed `hist`, `intg`, and `lpix` bodies show that such a requirement
  would reject valid files.

## Unknown

- Meanings and valid bit ranges for the flags field.
- Meanings of the two unknown `uint32` fields.
- Whether the 16 reserved bytes or grey color differ in valid files.
- Exact semantics and compatibility relationship of `norm` and `ver1`.
- Practical maximum values for dimensions and chunk count.
- Whether older or future SAI2 versions use a different signature or header
  length.
- Whether valid files can contain padding between the table and first chunk,
  equal chunk offsets, nonzero high offset bits, or chunks not ordered by
  physical offset.
- Semantics of chunk bodies beyond the limited prefixes listed above.
- Whether other `5N` or `lpix` block-grid variants add flags to the observed
  channel words or use different placement rules.
- Meanings of all canvas-background flag values beyond the two observed modes.
- Folder flag meanings beyond the observed nesting-depth low byte and the
  `0x40000000` bit on one `fold` layer. The observed `0x00000100` flag marks a
  layer clipped to the one below it and maps to PSD's clipping byte.
- Semantics of unobserved `lmsk` flag combinations and mask block variants.
- Linework stroke kinds, point flags, width-scale semantics, brush parameters,
  and rendering details beyond the one observed stroke.
- Shape path and point flags, open paths, stroke parameters, and compound or
  multiple-path fill rules.
- Whether other format tags change the DPCM predictor, channel order, bitstream,
  marker, or padding rules.

These questions should be answered with purpose-built, user-owned fixtures and
black-box comparison before the parser adds stricter validation.
