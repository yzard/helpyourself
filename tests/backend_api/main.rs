#[test]
fn executable_requires_explicit_configuration() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_helpyourself"))
        .arg("check-config")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--data-dir"));
}

#[test]
fn administrator_commands_create_reset_and_disable_without_password_arguments() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let directory = tempfile::tempdir().unwrap();
    helpyourself::cli::write_template(directory.path()).unwrap();
    for operation in ["create-user", "reset-password"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_helpyourself"))
            .arg("--data-dir")
            .arg(directory.path())
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
            .arg("--data-dir")
            .arg(directory.path())
            .args(["disable-user", "--username", "alice"])
            .output()
            .unwrap()
            .status
            .success()
    );
}
