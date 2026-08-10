use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn extracts_a_synthetic_red_pixel_to_png() {
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

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should follow the Unix epoch")
        .as_nanos();
    let input = std::env::temp_dir().join(format!(
        "sai2-extract-test-{}-{unique}.sai2",
        std::process::id()
    ));
    let output = input.with_extension("png");
    fs::write(&input, document).expect("synthetic SAI2 should be writable");

    let command = Command::new(env!("CARGO_BIN_EXE_sai2-extract"))
        .arg(&input)
        .arg(&output)
        .output()
        .expect("sai2-extract should run");
    let png = fs::read(&output).expect("output PNG should be readable");
    let _ = fs::remove_file(input);
    let _ = fs::remove_file(output);

    assert!(command.status.success());
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 1);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 1);
}
