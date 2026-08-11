# sai2-parser

An experimental, unofficial reader for SYSTEMAX PaintTool SAI Ver.2 (`.sai2`)
files, written in Rust.

The project exists to support interoperability and long-term preservation of
files that users own. It is not affiliated with or endorsed by SYSTEMAX, and it
is not an official PaintTool SAI project. The file format is not fully
documented, so compatibility is expected to grow incrementally from small,
testable parsing steps.

The project starts as **reader-only** software. It does not currently write
`.sai2` files, and it deliberately avoids offering an experimental writer that
could corrupt original artwork.

## Current compatibility

The current implementation provides experimental compatibility level 1 plus
the first tested slice of layered export:

- validates the 16-byte SAI2 signature;
- parses the fixed 64-byte header;
- reports canvas dimensions, flags, chunk count, background color, and the
  four-byte format tag;
- returns structured errors for invalid and truncated input;
- provides the `sai2-info` command-line tool;
- parses all 16-byte chunk-table entries without requiring known chunk types;
- reports each chunk's type, object ID, absolute offset, and safely derived
  body size;
- decodes the merged `intg` image from bounded 16-bit delta/RLE tile streams;
- supports the observed opaque three-channel and transparent four-channel
  canvas modes;
- writes standards-compliant RGBA PNG files with the `sai2-extract` CLI.
- parses raster-layer IDs, names, types, blend modes, opacity, flags, and tile
  counts from `layr` chunks;
- decodes observed sparse, offset `lpix` block grids into straight-alpha RGBA;
- writes those decoded layers, Unicode names, visibility, opacity, and the
  complete current SAI2 blend-mode menu to a PSD 1.0 file with the
  `sai2topsd` CLI;
- uses PSD PackBits/RLE channel compression so large, sparse layered documents
  do not expand unnecessarily toward PSD's 2 GB file-size limit;
- decodes observed folder depth and pass-through mode and writes native PSD
  section-divider records;
- maps observed SAI2 clipping groups and transparent-pixel protection to PSD's
  clipping byte and transparency-protected flag;
- decodes observed grayscale `mpix` layer masks and writes native PSD user-mask
  channels;
- exposes observed linework Bezier controls, pressure, width scale, brush size,
  and color as typed Rust data;
- rasterizes decoded pressure-sensitive linework into independent RGBA PSD
  layers while retaining the original editable vector payload in `s2ly`;
- exposes observed shape paths, Bezier controls, and fill color as typed Rust
  data;
- writes the observed single-path SAI2 circle, triangle, square, and their
  freely transformed variants as native PSD solid-color shape layers with
  editable vector masks;
- reverses SAI2's top-to-bottom layer records into PSD compositing order and,
  for the observed opaque canvas mode, adds an editable white canvas-background
  layer so recompositing the PSD layers reproduces SAI2's saved appearance;
- embeds every original chunk associated with each layer in a private PSD
  `s2ly` tagged block, retaining still-unknown vector, text, mask, and other
  layer payloads bit-for-bit for future decoders.

The decoder has been verified against three purpose-built 32 x 32 fixtures,
two 300 x 300 artwork fixtures, and a 5870 x 4175 production linework file.
The newer complex fixture contains a
pass-through folder, three raster layers, a grayscale layer mask, a
pressure-sensitive linework stroke, a shape path, and several blend modes. Its
saved composite matches the supplied reference pixel-for-pixel. Folder
structure, the mask, and blend modes are represented natively in the PSD.
Linework and shape geometry are decoded and preserved. Linework is additionally
rasterized into its own PSD pixel layer. The fixture-backed SAI2 circle,
triangle, square, and their freely transformed variants are written as native
PSD solid-color shape layers. Text layers are not decoded yet.

## Usage

```console
cargo run --bin sai2-info -- example.sai2
```

Extract the merged image:

```console
cargo run --bin sai2-extract -- example.sai2 output.png
```

Convert supported raster layers to PSD:

```console
cargo run --bin sai2topsd -- example.sai2 output.psd
```

The generated PSD carries the saved `intg` image as its composite preview, so
applications that only read the flattened PSD view still see the exact saved
canvas. Applications with PSD layer support can edit the independently decoded
raster layers, folder structure, masks, and mapped blend modes. Each PSD layer
also carries a private `s2ly` tagged block with
the exact source `layr`, `lpix`, and any other chunks that share its object ID.
PSD readers are required to skip unknown tagged blocks; this has been tested
with `psd-tools`. Applications may discard private metadata when re-saving a
PSD, so the original `.sai2` remains the archival source of truth.

The two-layer 32 x 32 fixture recomposites from its PSD layers within one 8-bit
level of the saved integrated image. The 300 x 300 artwork recomposites within
two levels. Its original integrated image remains embedded pixel-for-pixel as
the PSD composite preview. Transparent SAI2 canvases do not receive the
synthetic white background layer.

For unsupported rendered layer types, `sai2topsd` prints an explicit
placeholder notice. The PSD's saved composite preview is still the exact SAI2
`intg` image, while those individual PSD layers are transparent until a
renderer is implemented. The fixture-backed SAI2 shape primitives are
supported and do not use this placeholder path.

The PNG writer uses streaming, uncompressed DEFLATE blocks. This keeps memory
usage bounded beyond the decoded RGBA image, at the cost of larger PNG files.

Example output:

```text
SAI2 document
Canvas: 4096 x 4096
Flags: 0x00000100
Chunk count: 12
Background color: 0xff808080
Format tag: norm
Chunks:
  hist id=0 offset=128 size=88
  intg id=0 offset=216 size=688
  layr id=2 offset=904 size=80
  lpix id=2 offset=984 size=1308
```

## Workspace

- `crates/sai2-core`: byte-oriented parsing library with no filesystem or
  operating-system dependency;
- `crates/sai2-cli`: `sai2-info`, `sai2-extract`, and `sai2topsd` command-line
  front ends;
- `docs/format-notes.md`: known, observed, assumed, and unknown format facts;
- `fixtures/README.md`: policy for private and publishable test fixtures.

## Development

```console
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```

The primary public format reference used so far is Photopea's
[unofficial SAI2 specification](https://github.com/photopea/SAI2-specification).
Published descriptions are treated as hypotheses until they can be checked
against independently owned fixture files.

The DPCM work used the public implementation in
[`Wunkolo/libsai`](https://github.com/Wunkolo/libsai) as a behavioral reference.
The Rust decoder was written independently with explicit bounds and resource
checks; no reference code was copied, and PaintTool SAI itself was not
disassembled.

## License

MIT. See [LICENSE](LICENSE).
