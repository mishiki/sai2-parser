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

Phase 3 provides experimental compatibility level 1 for the fixtures tested so
far:

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

The decoder has been verified against three purpose-built 32 x 32, single-tile
fixtures. Multi-tile canvases are implemented from the documented framing but
still need independent real-file fixtures. Individual layer decoding is not yet
implemented.

## Usage

```console
cargo run --bin sai2-info -- example.sai2
```

Extract the merged image:

```console
cargo run --bin sai2-extract -- example.sai2 output.png
```

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
- `crates/sai2-cli`: command-line front end;
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
