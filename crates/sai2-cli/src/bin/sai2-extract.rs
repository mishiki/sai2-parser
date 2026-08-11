use std::{
    env, fs,
    io::{BufWriter, Write},
    path::PathBuf,
    process::ExitCode,
};

use sai2_core::{DecodeLimits, decode_integrated_image};
use sai2_png::write_rgba;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sai2-extract: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some((input_path, output_path)) = parse_paths(env::args_os().skip(1))? else {
        return Ok(());
    };
    let input = fs::read(&input_path)
        .map_err(|error| format!("could not read {}: {error}", input_path.display()))?;
    let image = decode_integrated_image(&input, DecodeLimits::default())
        .map_err(|error| error.to_string())?;
    let output = fs::File::create(&output_path)
        .map_err(|error| format!("could not create {}: {error}", output_path.display()))?;
    let mut output = BufWriter::new(output);
    write_rgba(&mut output, image.width(), image.height(), image.pixels())?;
    output
        .flush()
        .map_err(|error| format!("could not finish {}: {error}", output_path.display()))?;
    println!(
        "Extracted {} x {} integrated image to {}",
        image.width(),
        image.height(),
        output_path.display()
    );
    Ok(())
}

fn parse_paths(
    mut arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Option<(PathBuf, PathBuf)>, String> {
    let Some(first) = arguments.next() else {
        return Err("usage: sai2-extract <input.sai2> <output.png>".to_owned());
    };
    if first == "-h" || first == "--help" {
        println!("Usage: sai2-extract <input.sai2> <output.png>");
        println!("Decode the merged SAI2 image and write an RGBA PNG.");
        return Ok(None);
    }
    let Some(second) = arguments.next() else {
        return Err("usage: sai2-extract <input.sai2> <output.png>".to_owned());
    };
    if arguments.next().is_some() {
        return Err("usage: sai2-extract <input.sai2> <output.png>".to_owned());
    }
    Ok(Some((PathBuf::from(first), PathBuf::from(second))))
}
