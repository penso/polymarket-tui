use std::process::Command;

#[test]
fn cli_prints_greeting() {
    // Cargo sets this env var for integration tests to reference the built binary.
    let bin_path = env!("CARGO_BIN_EXE_polymarket-cli");
    let output = Command::new(bin_path)
        .output()
        .expect("failed to run polymarket-cli");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hello from polymarket-bot library"));
}
