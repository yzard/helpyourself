use clap::Parser;
use helpyourself::cli::{Cli, Command, write_template};

#[test]
fn config_generation_never_overwrites_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    write_template(directory.path()).unwrap();
    assert!(write_template(directory.path()).is_err());
    assert!(helpyourself::config::Config::load(directory.path()).is_ok());
    let parsed = Cli::try_parse_from([
        "helpyourself",
        "--data-dir",
        directory.path().to_str().unwrap(),
        "reset-password",
        "--username",
        "alice",
        "--password-stdin",
    ])
    .unwrap();
    assert!(matches!(parsed.command, Command::ResetPassword { .. }));
    assert!(
        Cli::try_parse_from([
            "helpyourself",
            "create-user",
            "--username",
            "alice",
            "--password",
            "secret"
        ])
        .is_err()
    );
}
