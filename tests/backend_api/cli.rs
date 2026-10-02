use clap::Parser;
use helpyourself::cli::{Cli, Command, write_template};

#[test]
fn config_generation_never_overwrites_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_template(&path).unwrap();
    assert!(write_template(&path).is_err());
    assert!(helpyourself::config::Config::load(&path).is_ok());
    let parsed = Cli::try_parse_from([
        "helpyourself",
        "--config",
        "config.toml",
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
