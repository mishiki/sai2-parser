use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn synthetic_red_pixel() -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(b"dpcm");
    body.extend_from_slice(&5_u32.to_le_bytes());
    body.extend_from_slice(&0x00ff_u16.to_le_bytes());
    body.extend_from_slice(&[0x05, 0x01, 0x00]);
    body.extend_from_slice(&0x01ff_u16.to_le_bytes());
    body.push(0);

    let mut document = vec![0_u8; 80];
    document[0..16].copy_from_slice(b"SAI-CANVAS-TYPE0");
    document[16..20].copy_from_slice(&0x0100_u32.to_le_bytes());
    document[20..24].copy_from_slice(&1_u32.to_le_bytes());
    document[24..28].copy_from_slice(&1_u32.to_le_bytes());
    document[32..36].copy_from_slice(&1_u32.to_le_bytes());
    document[60..64].copy_from_slice(b"norm");
    document[64..68].copy_from_slice(b"intg");
    document[72..80].copy_from_slice(&80_u64.to_le_bytes());
    document.extend_from_slice(&body);
    document
}

fn unique_input(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should follow the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sai2topng-{label}-{}-{unique}.sai2",
        std::process::id()
    ))
}

fn assert_png(path: &std::path::Path) {
    let png = fs::read(path).expect("output PNG should be readable");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 1);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 1);
}

#[test]
fn writes_beside_the_input_when_output_is_omitted() {
    let input = unique_input("implicit");
    let output = input.with_extension("png");
    fs::write(&input, synthetic_red_pixel()).expect("synthetic SAI2 should be writable");

    let command = Command::new(env!("CARGO_BIN_EXE_sai2topng"))
        .arg(&input)
        .output()
        .expect("sai2topng should run");
    assert!(command.status.success());
    assert_png(&output);

    let _ = fs::remove_file(input);
    let _ = fs::remove_file(output);
}

#[test]
fn accepts_an_explicit_output_path() {
    let input = unique_input("explicit");
    let output = input.with_file_name(format!(
        "{}-converted.png",
        input.file_stem().unwrap().to_string_lossy()
    ));
    fs::write(&input, synthetic_red_pixel()).expect("synthetic SAI2 should be writable");

    let command = Command::new(env!("CARGO_BIN_EXE_sai2topng"))
        .arg(&input)
        .arg(&output)
        .output()
        .expect("sai2topng should run");
    assert!(command.status.success());
    assert_png(&output);

    let _ = fs::remove_file(input);
    let _ = fs::remove_file(output);
}
