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
pixel-for-pixel. The `5N` uniform-block channel flag bits are only tentatively
understood; more fixtures are needed before calling that variant fully verified.

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
- Exact flag-bit semantics of the observed `5N` uniform-block channel words,
  and whether other `lpix` block-grid variants use the same placement rules.
- Meanings of all canvas-background flag values beyond the two observed modes.
- Whether other format tags change the DPCM predictor, channel order, bitstream,
  marker, or padding rules.

These questions should be answered with purpose-built, user-owned fixtures and
black-box comparison before the parser adds stricter validation.
