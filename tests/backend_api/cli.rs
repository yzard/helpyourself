use clap::Parser;
use helpyourself::cli::{Cli, Command, write_template};

#[test]
fn config_generation_never_overwrites_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    write_template(directory.path()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = std::fs::metadata(directory.path().join("config.toml"))
            .unwrap()
            .permissions();
        assert_eq!(permissions.mode() & 0o777, 0o600);
    }
    assert!(write_template(directory.path()).is_err());
    assert!(helpyourself::config::Config::load(directory.path()).is_err());
    std::fs::write(
        directory.path().join("config.toml"),
        crate::configured_template(),
    )
    .unwrap();
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
