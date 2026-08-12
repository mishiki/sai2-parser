use std::{env, fs, path::PathBuf, process::ExitCode};

use sai2_core::{DecodeLimits, Sai2Document, decode_layers};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sai2-info: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(path) = parse_path(env::args_os().skip(1))? else {
        return Ok(());
    };
    let bytes =
        fs::read(&path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let document = Sai2Document::parse(&bytes).map_err(|error| error.to_string())?;
    let header = document.header();

    println!("SAI2 document");
    println!("Canvas: {} x {}", header.width(), header.height());
    println!("Flags: 0x{:08x}", header.flags());
    println!("Chunk count: {}", header.chunk_count());
    println!("Background color: 0x{:08x}", header.background_color());
    println!("Format tag: {}", header.format_tag());
    println!("Chunks:");
    for chunk in document.chunks() {
        println!(
            "  {} id={} offset={} size={}",
            chunk.kind(),
            chunk.object_id(),
            chunk.offset(),
            chunk.size()
        );
    }

    let layers =
        decode_layers(&bytes, DecodeLimits::default()).map_err(|error| error.to_string())?;
    if !layers.is_empty() {
        println!("Layers:");
        for layer in &layers {
            let indent = "  ".repeat(usize::from(layer.nesting_level()) + 1);
            println!(
                "{indent}{} id={} type={} blend={} opacity={} visible={}",
                layer.name(),
                layer.id(),
                layer.layer_type(),
                layer.blend_mode(),
                layer.opacity(),
                layer.visible()
            );
            if let Some(mask) = layer.mask() {
                let (width, height) = mask.block_dimensions();
                println!(
                    "{indent}  mask id={} blocks={}x{} decoded={}",
                    mask.id(),
                    width,
                    height,
                    mask.image().is_some()
                );
            }
            if let Some(linework) = layer.linework() {
                let point_count = linework
                    .strokes()
                    .iter()
                    .map(|stroke| stroke.points().len())
                    .sum::<usize>();
                println!(
                    "{indent}  linework strokes={} points={} brush_size={:?} color_bgra14={:?}",
                    linework.strokes().len(),
                    point_count,
                    linework.brush_size(),
                    linework.color_bgra14()
                );
            }
            if let Some(shape) = layer.shape() {
                let point_count = shape
                    .paths()
                    .iter()
                    .map(|path| path.points().len())
                    .sum::<usize>();
                println!(
                    "{indent}  shape paths={} points={} fill_bgra14={:?}",
                    shape.paths().len(),
                    point_count,
                    shape.fill_bgra14()
                );
            }
        }
    }

    Ok(())
}

fn parse_path(
    mut arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Option<PathBuf>, String> {
    let Some(first) = arguments.next() else {
        return Err("usage: sai2-info <file.sai2>".to_owned());
    };

    if first == "-h" || first == "--help" {
        println!("Usage: sai2-info <file.sai2>");
        println!("Parse and display SAI2 metadata, chunks, and decoded layer structure.");
        return Ok(None);
    }

    if arguments.next().is_some() {
        return Err("usage: sai2-info <file.sai2>".to_owned());
    }

    Ok(Some(PathBuf::from(first)))
}
