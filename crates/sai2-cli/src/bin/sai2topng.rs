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
            eprintln!("sai2topng: {error}");
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
        "Converted {} x {} integrated image from {} to {}",
        image.width(),
        image.height(),
        input_path.display(),
        output_path.display()
    );
    Ok(())
}

fn parse_paths(
    mut arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Option<(PathBuf, PathBuf)>, String> {
    let Some(first) = arguments.next() else {
        return Err("usage: sai2topng <input.sai2> [output.png]".to_owned());
    };
    if first == "-h" || first == "--help" {
        println!("Usage: sai2topng <input.sai2> [output.png]");
        println!("Extract the exact saved SAI2 composite as an RGBA PNG.");
        println!("Without output.png, write beside the input using the same file name.");
        return Ok(None);
    }

    let input = PathBuf::from(first);
    let output = arguments
        .next()
        .map_or_else(|| input.with_extension("png"), PathBuf::from);
    if arguments.next().is_some() {
        return Err("usage: sai2topng <input.sai2> [output.png]".to_owned());
    }
    if input == output {
        return Err("input and output paths must be different".to_owned());
    }
    Ok(Some((input, output)))
}
