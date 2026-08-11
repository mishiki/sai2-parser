use std::{
    env, fs,
    io::{BufWriter, Write},
    path::PathBuf,
    process::ExitCode,
};

use sai2_core::{DecodeLimits, Sai2Document, decode_integrated_image, decode_layers};
use sai2_psd::write_layered;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("sai2topsd: {error}");
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
    let document = Sai2Document::parse(&input).map_err(|error| error.to_string())?;
    let composite = decode_integrated_image(&input, DecodeLimits::default())
        .map_err(|error| error.to_string())?;
    let layers =
        decode_layers(&input, DecodeLimits::default()).map_err(|error| error.to_string())?;
    if layers.is_empty() {
        return Err("the SAI2 document contains no layers".to_owned());
    }

    let output = fs::File::create(&output_path)
        .map_err(|error| format!("could not create {}: {error}", output_path.display()))?;
    let mut output = BufWriter::new(output);
    write_layered(
        &mut output,
        document.header().width(),
        document.header().height(),
        &layers,
        &composite,
        &input,
        !document.header().integrated_image_has_alpha(),
    )?;
    output
        .flush()
        .map_err(|error| format!("could not finish {}: {error}", output_path.display()))?;
    println!(
        "Converted {} layers from {} to {}",
        layers.len(),
        input_path.display(),
        output_path.display()
    );
    let placeholder_count = layers
        .iter()
        .filter(|layer| !layer.is_folder() && layer.image().is_none() && layer.shape().is_none())
        .count();
    if placeholder_count != 0 {
        println!(
            "Preserved {placeholder_count} structured non-raster layer(s) as transparent PSD placeholders; the saved composite preview remains exact"
        );
    }
    Ok(())
}

fn parse_paths(
    mut arguments: impl Iterator<Item = std::ffi::OsString>,
) -> Result<Option<(PathBuf, PathBuf)>, String> {
    let Some(first) = arguments.next() else {
        return Err("usage: sai2topsd <input.sai2> <output.psd>".to_owned());
    };
    if first == "-h" || first == "--help" {
        println!("Usage: sai2topsd <input.sai2> <output.psd>");
        println!("Convert decoded SAI2 layers to a layered PSD file.");
        println!("Original per-layer chunks are preserved in private s2ly metadata.");
        println!("Unsupported layer rendering is represented by transparent placeholders.");
        return Ok(None);
    }
    let Some(second) = arguments.next() else {
        return Err("usage: sai2topsd <input.sai2> <output.psd>".to_owned());
    };
    if arguments.next().is_some() {
        return Err("usage: sai2topsd <input.sai2> <output.psd>".to_owned());
    }
    Ok(Some((PathBuf::from(first), PathBuf::from(second))))
}
