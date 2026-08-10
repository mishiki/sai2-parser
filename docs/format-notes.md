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
  | `0x08` | 4 | chunk offset (`uint32`) |
  | `0x0c` | 4 | described as zero (`uint32`) |

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
- The final table field is zero in all 14 observed entries.
- The blank and single-layer files contain `hist`, `intg`, `layr`, and `lpix`.
  The two-layer file adds a second `layr` and `lpix`, for six entries total.
- Matching `layr` and `lpix` entries share object IDs. The `hist` and `intg`
  entries use object ID zero in these fixtures.
- Observed chunk bodies do not have a uniform prefix: `layr` begins with
  `layr` and its object ID, `intg` and nonempty `lpix` begin with `dpcm`, while
  the blank layer's four-byte `lpix` body is zero. Phase 2 does not interpret
  any of these bodies.

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
  equal chunk offsets, nonzero final table fields, or chunks not ordered by
  physical offset.
- Semantics of chunk bodies beyond the limited prefixes listed above.

These questions should be answered with purpose-built, user-owned fixtures and
black-box comparison before the parser adds stricter validation.
