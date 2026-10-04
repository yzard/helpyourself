use helpyourself::config::{Config, TEMPLATE};

#[test]
fn template_paths_are_relative_to_configuration_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, TEMPLATE).unwrap();
    let configuration = Config::load(directory.path()).unwrap();
    assert_eq!(configuration.data_dir, directory.path().to_path_buf());
    assert_eq!(
        configuration.ocr.api_key_file,
        directory.path().join("ocr-key")
    );
    assert!(Config::load(&path).is_err());
    assert!(Config::load(std::path::Path::new("relative/data")).is_err());
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
    assert!(Config::load(directory.path()).is_err());
    for content in [
        "not toml".into(),
        format!("data_dir = \"/data\"\n{TEMPLATE}"),
        TEMPLATE.replace("session_ttl_seconds = 86400", "session_ttl_seconds = 0"),
        TEMPLATE.replace("maximum_attempts = 3", "maximum_attempts = 0"),
        TEMPLATE.replace("maximum_pdf_pages = 100", "maximum_pdf_pages = 0"),
        TEMPLATE.replace("[server]", "[server]\ndata_dir = \"/data\""),
        TEMPLATE.replace("http://127.0.0.1:8001", "https://user:secret@example.com"),
        TEMPLATE.replace("http://127.0.0.1:8001", "file:///tmp/model"),
        TEMPLATE.replace("timeout_seconds = 120", "timeout_seconds = 0"),
        TEMPLATE.replace("maximum_attempts = 3", "unexpected = 3"),
    ] {
        std::fs::write(&path, content).unwrap();
        assert!(Config::load(directory.path()).is_err());
    }
}
