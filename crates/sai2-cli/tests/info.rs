use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn reports_metadata_from_a_synthetic_header() {
    let mut bytes = [0_u8; 88];
    bytes[0..16].copy_from_slice(b"SAI-CANVAS-TYPE0");
    bytes[16..20].copy_from_slice(&0x0100_u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&800_u32.to_le_bytes());
    bytes[24..28].copy_from_slice(&600_u32.to_le_bytes());
    bytes[32..36].copy_from_slice(&1_u32.to_le_bytes());
    bytes[56..60].copy_from_slice(&0xff80_8080_u32.to_le_bytes());
    bytes[60..64].copy_from_slice(b"norm");
    bytes[64..68].copy_from_slice(b"intg");
    bytes[72..76].copy_from_slice(&80_u32.to_le_bytes());
    bytes[80..88].copy_from_slice(b"12345678");

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should follow the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "sai2-info-test-{}-{unique}.sai2",
        std::process::id()
    ));
    fs::write(&path, bytes).expect("synthetic header should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_sai2-info"))
        .arg(&path)
        .output()
        .expect("sai2-info should run");
    let _ = fs::remove_file(path);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("CLI output should be UTF-8");
    assert!(stdout.contains("SAI2 document"));
    assert!(stdout.contains("Canvas: 800 x 600"));
    assert!(stdout.contains("Flags: 0x00000100"));
    assert!(stdout.contains("Chunk count: 1"));
    assert!(stdout.contains("Background color: 0xff808080"));
    assert!(stdout.contains("Format tag: norm"));
    assert!(stdout.contains("Chunks:"));
    assert!(stdout.contains("intg id=0 offset=80 size=8"));
}
