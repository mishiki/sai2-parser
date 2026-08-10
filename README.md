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

Phase 1 implements compatibility level 0:

- validates the 16-byte SAI2 signature;
- parses the fixed 64-byte header;
- reports canvas dimensions, flags, chunk count, background color, and the
  four-byte format tag;
- returns structured errors for invalid and truncated input;
- provides the `sai2-info` command-line tool.

Chunk tables, integrated-image extraction, and layer decoding are not yet
implemented.

## Usage

```console
cargo run --bin sai2-info -- example.sai2
```

Example output:

```text
SAI2 document
Canvas: 4096 x 4096
Flags: 0x00000100
Chunk count: 12
Background color: 0xff808080
Format tag: norm
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

## License

MIT. See [LICENSE](LICENSE).
