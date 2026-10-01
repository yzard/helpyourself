#[test]
fn executable_requires_explicit_configuration() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_helpyourself"))
        .arg("check-config")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--config PATH is required"));
}

#[test]
fn administrator_commands_create_reset_and_disable_without_password_arguments() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("config.toml");
    helpyourself::cli::write_template(&config).unwrap();
    for operation in ["create-user", "reset-password"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_helpyourself"))
            .arg("--config")
            .arg(&config)
            .args([operation, "--username", "alice", "--password-stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"synthetic-password-456\n")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-password"));
    }
    assert!(
        Command::new(env!("CARGO_BIN_EXE_helpyourself"))
            .arg("--config")
            .arg(&config)
            .args(["disable-user", "--username", "alice"])
            .output()
            .unwrap()
            .status
            .success()
    );
}
