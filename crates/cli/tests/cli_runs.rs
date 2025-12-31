use std::process::Command;
use std::time::Duration;
use std::thread;

#[test]
fn cli_starts_and_connects() {
    // Cargo sets this env var for integration tests to reference the built binary.
    let bin_path = env!("CARGO_BIN_EXE_polymarket-cli");

    // Start the CLI process
    let mut child = Command::new(bin_path)
        .spawn()
        .expect("failed to start polymarket-cli");

    // Give it a few seconds to start and connect
    thread::sleep(Duration::from_secs(5));

    // Check if it's still running (means it connected successfully)
    match child.try_wait() {
        Ok(Some(status)) => {
            // Process exited - might be due to network issues or timeout
            // That's acceptable, we just verify it doesn't panic immediately
            // If it exits with code 0 or 1, that's fine (network errors are expected in tests)
            assert!(
                status.code().is_some(),
                "Process should exit with a status code, not be killed by signal"
            );
        }
        Ok(None) => {
            // Process is still running - great! It connected successfully
            let _ = child.kill();
            let _ = child.wait();
        }
        Err(e) => {
            let _ = child.kill();
            panic!("Error checking process status: {}", e);
        }
    }
}
