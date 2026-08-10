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
  bodies. Chunk parsing is Phase 2 and is intentionally not part of the current
  parser.

## Observed in owned fixtures

No privately owned `.sai2` fixture has been supplied to the repository yet.
Consequently, no field value has been independently confirmed against a real
file in this phase.

## Assumed by the current implementation

- The 16-byte signature is required exactly; accepting alternate magic values
  would make file-type detection ambiguous.
- Reserved, unknown, color, and format-tag values are reported but not
  validated. This avoids rejecting future or currently undocumented variants.
- Width and height are exposed as raw unsigned values. Plausibility and
  allocation limits belong to later operations that consume dimensions.
- A numeric file-format version is not inferred from flags or the four-byte
  tag because the public specification does not define such a mapping.

## Unknown

- Meanings and valid bit ranges for the flags field.
- Meanings of the two unknown `uint32` fields.
- Whether the 16 reserved bytes or grey color differ in valid files.
- Exact semantics and compatibility relationship of `norm` and `ver1`.
- Practical maximum values for dimensions and chunk count.
- Whether older or future SAI2 versions use a different signature or header
  length.

These questions should be answered with purpose-built, user-owned fixtures and
black-box comparison before the parser adds stricter validation.
