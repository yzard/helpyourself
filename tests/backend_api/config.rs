use helpyourself::config::{Config, TEMPLATE};

#[test]
fn configuration_is_owned_by_the_service_root() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, crate::configured_template()).unwrap();
    let configuration = Config::load(directory.path()).unwrap();
    assert_eq!(configuration.data_dir, directory.path().to_path_buf());
    assert!(!configuration.ocr.api_key.is_empty());
    assert!(Config::load(&path).is_err());
    assert!(Config::load(std::path::Path::new("relative/data")).is_err());
    assert_ne!(
        configuration.ocr.url,
        configuration.providers.analysis.base_url
    );
}

#[test]
fn malformed_unknown_and_unsafe_configuration_fail() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    assert!(Config::load(directory.path()).is_err());
    let valid = crate::configured_template();
    for content in [
        TEMPLATE.to_owned(),
        crate::configured_template().replace("[ocr]", "[ocr]\nenabled = true"),
        crate::configured_template().replace("[ocr]", "[ocr]\nenabled = false"),
        "not toml".into(),
        format!("data_dir = \"/data\"\n{valid}"),
        valid.replace("session_ttl_seconds = 86400", "session_ttl_seconds = 0"),
        valid.replace("maximum_attempts = 3", "maximum_attempts = 0"),
        valid.replace("maximum_pdf_pages = 100", "maximum_pdf_pages = 0"),
        valid.replace("[server]", "[server]\ndata_dir = \"/data\""),
        valid.replace("http://127.0.0.1:8001", "https://user:secret@example.com"),
        valid.replace("http://127.0.0.1:8001", "file:///tmp/model"),
        valid.replace("timeout_seconds = 120", "timeout_seconds = 0"),
        valid.replace("maximum_attempts = 3", "unexpected = 3"),
    ] {
        std::fs::write(&path, content).unwrap();
        assert!(Config::load(directory.path()).is_err());
    }
}

#[test]
fn embedded_credentials_validate_at_startup_without_secret_files() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let secret = "synthetic-embedded-ocr-key-123456789";
    let valid = TEMPLATE.replacen("api_key = \"\"", &format!("api_key = \"{secret}\""), 1);
    std::fs::write(&path, &valid).unwrap();
    let config = Config::load(directory.path()).unwrap();
    assert_eq!(config.ocr.api_key, secret);
    assert!(!directory.path().join("ocr-key").exists());
    for invalid in [
        valid.replace(secret, ""),
        valid.replace(secret, "short"),
        valid.replace(secret, "synthetic-private-key-with spaces"),
        valid.replace(secret, "synthetic-private-key\\r\\nheader-injection"),
        valid.replace(secret, &"x".repeat(8193)),
        valid.replace("api_key =", "api_key_file ="),
        valid.replace(
            "api_key = \"\"",
            "api_key = \"provider-private-secret-with spaces\"",
        ),
    ] {
        std::fs::write(&path, invalid).unwrap();
        let error = Config::load(directory.path()).err().unwrap().to_string();
        assert!(!error.contains("private"));
        assert!(!error.contains(secret));
    }
}
