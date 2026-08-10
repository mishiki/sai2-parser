use std::{
    fs,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn converts_the_owned_two_layer_fixture_when_available() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/private/32x32-redball-greenball-multiple-layer.sai2");
    if !fixture.exists() {
        return;
    }
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should follow the Unix epoch")
        .as_nanos();
    let output = std::env::temp_dir().join(format!(
        "sai2topsd-test-{}-{unique}.psd",
        std::process::id()
    ));

    let command = Command::new(env!("CARGO_BIN_EXE_sai2topsd"))
        .arg(&fixture)
        .arg(&output)
        .output()
        .expect("sai2topsd should run");
    let psd = fs::read(&output).expect("output PSD should be readable");
    let _ = fs::remove_file(output);

    assert!(command.status.success());
    assert_eq!(&psd[..4], b"8BPS");
    assert!(psd.windows(4).any(|window| window == b"mul "));
    assert!(psd.windows(8).any(|window| window == b"8BIMluni"));
}
