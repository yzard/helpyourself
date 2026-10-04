CREATE TABLE users (
    user_id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    credential_version INTEGER NOT NULL DEFAULT 1,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at INTEGER NOT NULL,
    data_revision INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    expires_at INTEGER NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);
CREATE INDEX sessions_expiry ON sessions (expires_at);

CREATE TABLE login_windows (
    username_hash TEXT PRIMARY KEY,
    starts_at INTEGER NOT NULL,
    attempt_count INTEGER NOT NULL
);

CREATE TABLE raw_files (
    file_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    upload_id TEXT NOT NULL,
    original_name TEXT NOT NULL,
    relative_path TEXT NOT NULL UNIQUE,
    processing_path TEXT UNIQUE,
    processing_sha256 TEXT,
    content_type TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    byte_count INTEGER NOT NULL CHECK (byte_count > 0),
    page_count INTEGER NOT NULL CHECK (page_count > 0),
    created_at INTEGER NOT NULL,
    UNIQUE (user_id, file_id),
    UNIQUE (user_id, upload_id),
    CHECK ((processing_path IS NULL) = (processing_sha256 IS NULL))
);
CREATE INDEX raw_files_user ON raw_files (user_id, file_id);
CREATE INDEX raw_files_digest ON raw_files (user_id, sha256);

CREATE TABLE jobs (
    job_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    file_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind = 'document_extract'),
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'canceled')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    lease_token TEXT,
    lease_until INTEGER,
    error_code TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (user_id, file_id) REFERENCES raw_files (user_id, file_id) ON DELETE CASCADE,
    UNIQUE (user_id, file_id, kind)
);
CREATE INDEX jobs_user ON jobs (user_id, job_id);
CREATE INDEX jobs_claim ON jobs (status, lease_until, created_at);

CREATE TABLE reports (
    report_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    file_id TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    context_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    UNIQUE (user_id, report_id),
    FOREIGN KEY (user_id, file_id) REFERENCES raw_files (user_id, file_id) ON DELETE CASCADE
);

CREATE TABLE observations (
    observation_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    report_id TEXT NOT NULL,
    current_revision INTEGER NOT NULL,
    candidate_key TEXT,
    UNIQUE (user_id, observation_id),
    UNIQUE (user_id, report_id, candidate_key),
    FOREIGN KEY (user_id, report_id) REFERENCES reports (user_id, report_id) ON DELETE CASCADE
);
CREATE TABLE observation_revisions (
    user_id TEXT NOT NULL,
    observation_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'confirmed', 'rejected')),
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, observation_id, revision),
    FOREIGN KEY (user_id, observation_id) REFERENCES observations (user_id, observation_id) ON DELETE CASCADE
);
CREATE INDEX observations_report ON observations (user_id, report_id);
CREATE TABLE report_relations (
    user_id TEXT NOT NULL,
    report_id TEXT NOT NULL,
    preferred_report_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('duplicate', 'superseded')),
    PRIMARY KEY (user_id, report_id),
    FOREIGN KEY (user_id, report_id) REFERENCES reports (user_id, report_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id, preferred_report_id) REFERENCES reports (user_id, report_id) ON DELETE CASCADE,
    CHECK (report_id != preferred_report_id)
);
CREATE TABLE extraction_pages (
    user_id TEXT NOT NULL,
    report_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    page_number INTEGER NOT NULL,
    status TEXT NOT NULL,
    content TEXT NOT NULL,
    model TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, report_id, run_id, page_number),
    FOREIGN KEY (user_id, report_id) REFERENCES reports (user_id, report_id) ON DELETE CASCADE
);
CREATE TABLE health_connections (
    connection_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    platform TEXT NOT NULL CHECK (platform IN ('apple_health', 'health_connect')),
    installation_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (user_id, connection_id),
    UNIQUE (user_id, platform, installation_id)
);
CREATE TABLE health_records (
    user_id TEXT NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('apple_health', 'health_connect')),
    source_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    record_type TEXT NOT NULL,
    start_at INTEGER NOT NULL,
    end_at INTEGER NOT NULL,
    version INTEGER NOT NULL,
    deleted INTEGER NOT NULL CHECK (deleted IN (0, 1)),
    payload_json TEXT NOT NULL,
    PRIMARY KEY (user_id, platform, source_id, record_id),
    FOREIGN KEY (user_id) REFERENCES users (user_id) ON DELETE CASCADE
);
CREATE INDEX health_records_range ON health_records (user_id, record_type, start_at, end_at);
CREATE TABLE health_revisions (
    user_id TEXT NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('apple_health', 'health_connect')),
    source_id TEXT NOT NULL,
    record_id TEXT NOT NULL,
    revision_id TEXT NOT NULL,
    raw_path TEXT NOT NULL UNIQUE,
    payload_json TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, revision_id),
    FOREIGN KEY (user_id, platform, source_id, record_id) REFERENCES health_records (user_id, platform, source_id, record_id) ON DELETE CASCADE
);
CREATE INDEX health_revisions_record ON health_revisions (user_id, platform, source_id, record_id);
CREATE TABLE sync_batches (
    user_id TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    batch_id TEXT NOT NULL,
    digest TEXT NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, connection_id, batch_id),
    FOREIGN KEY (user_id, connection_id) REFERENCES health_connections (user_id, connection_id) ON DELETE CASCADE
);
CREATE TABLE sync_coverage (
    user_id TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    record_type TEXT NOT NULL,
    status TEXT NOT NULL,
    last_sync_at INTEGER NOT NULL,
    first_success_at INTEGER,
    last_success_at INTEGER,
    PRIMARY KEY (user_id, connection_id, record_type),
    FOREIGN KEY (user_id, connection_id) REFERENCES health_connections (user_id, connection_id) ON DELETE CASCADE
);
CREATE TABLE analysis_runs (
    run_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    input_digest TEXT NOT NULL,
    input_json TEXT NOT NULL,
    output_json TEXT,
    error_code TEXT,
    created_at INTEGER NOT NULL,
    model TEXT NOT NULL,
    UNIQUE (user_id, run_id)
);
CREATE INDEX analysis_queue ON analysis_runs (status, created_at);
CREATE TABLE analysis_feedback (
    user_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    feedback_id TEXT NOT NULL,
    note TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, feedback_id),
    FOREIGN KEY (user_id, run_id) REFERENCES analysis_runs (user_id, run_id) ON DELETE CASCADE
);
CREATE TABLE cleanup_files (
    cleanup_id TEXT PRIMARY KEY,
    relative_path TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);
CREATE TABLE exports (
    export_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    error_code TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (user_id, export_id)
);

CREATE TABLE deleted_uploads (
    user_id TEXT NOT NULL REFERENCES users (user_id) ON DELETE CASCADE,
    upload_id TEXT NOT NULL,
    PRIMARY KEY (user_id, upload_id)
);

CREATE TABLE extraction_outputs (
    user_id TEXT NOT NULL,
    report_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    page_number INTEGER NOT NULL,
    stage TEXT NOT NULL CHECK (stage = 'ocr'),
    response_body TEXT NOT NULL,
    status_code INTEGER NOT NULL,
    content TEXT,
    model TEXT NOT NULL,
    adapter TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, report_id, run_id, page_number, stage),
    FOREIGN KEY (user_id, report_id) REFERENCES reports (user_id, report_id) ON DELETE CASCADE
);

CREATE TABLE extraction_inputs (
    user_id TEXT NOT NULL,
    report_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    page_number INTEGER NOT NULL CHECK (page_number > 0),
    input_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, report_id, run_id, page_number),
    FOREIGN KEY (user_id, report_id) REFERENCES reports (user_id, report_id) ON DELETE CASCADE
);
