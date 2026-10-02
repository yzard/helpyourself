use helpyourself::config::{Config, TEMPLATE};

#[test]
fn template_paths_are_relative_to_configuration_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, TEMPLATE).unwrap();
    let configuration = Config::load(&path).unwrap();
    assert_eq!(configuration.server.data_dir, directory.path().join("data"));
    assert!(!configuration.ocr.enabled);
    assert_ne!(
        configuration.ocr.url,
        configuration.providers.analysis.base_url
    );
}

#[test]
fn malformed_unknown_and_unsafe_configuration_fail() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    assert!(Config::load(&path).is_err());
    for content in [
        "not toml".into(),
        TEMPLATE.replace("session_ttl_seconds = 86400", "session_ttl_seconds = 0"),
        TEMPLATE.replace("maximum_attempts = 3", "maximum_attempts = 0"),
        TEMPLATE.replace("maximum_pdf_pages = 100", "maximum_pdf_pages = 0"),
        TEMPLATE.replace("data_dir = \"data\"", "data_dir = \"\""),
        TEMPLATE.replace("http://127.0.0.1:8001", "https://user:secret@example.com"),
        TEMPLATE.replace("http://127.0.0.1:8001", "file:///tmp/model"),
        TEMPLATE.replace("timeout_seconds = 120", "timeout_seconds = 0"),
        TEMPLATE.replace("maximum_attempts = 3", "unexpected = 3"),
    ] {
        std::fs::write(&path, content).unwrap();
        assert!(Config::load(&path).is_err());
    }
}
