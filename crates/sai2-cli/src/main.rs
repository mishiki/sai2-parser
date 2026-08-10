use std::{env, fs, path::PathBuf, process::ExitCode};

use sai2_core::Sai2Document;

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
        println!("Parse and display the SAI2 document header and chunk table.");
        return Ok(None);
    }

    if arguments.next().is_some() {
        return Err("usage: sai2-info <file.sai2>".to_owned());
    }

    Ok(Some(PathBuf::from(first)))
}
