pub const SCHEMA_VERSION: &str = "PRAGMA user_version";
pub const BUMP_REPORT: &str =
    "UPDATE reports SET revision = revision + 1 WHERE user_id = ? AND report_id = ?";
pub const HEALTH_TOMBSTONE: &str = "SELECT COUNT(*) FROM health_records WHERE user_id = ? AND platform = ? AND record_id = ? AND deleted = 1 AND (source_id = ? OR source_id = '*')";
pub const DELETE_HEALTH_ORIGINS: &str = "UPDATE health_records SET deleted = 1, start_at = 0, end_at = 0, payload_json = '{}' WHERE user_id = ? AND platform = ? AND record_id = ?";
pub const PURGE_HEALTH_ORIGINS: &str =
    "DELETE FROM health_revisions WHERE user_id = ? AND platform = ? AND record_id = ?";
pub const INSERT_ANALYSIS: &str = "INSERT INTO analysis_runs (run_id, user_id, status, input_digest, input_json, created_at, model) VALUES (?, ?, 'queued', ?, ?, ?, ?)";
pub const ANALYSIS_BY_DIGEST: &str = "SELECT run_id FROM analysis_runs WHERE user_id = ? AND input_digest = ? AND status != 'stale' ORDER BY created_at DESC LIMIT 1";
pub const ANALYSIS_LIST: &str = "SELECT run_id, status, output_json, error_code, created_at, model FROM analysis_runs WHERE user_id = ? ORDER BY created_at DESC LIMIT 100";
pub const ANALYSIS_GET: &str = "SELECT run_id, status, input_json, output_json, error_code, created_at, model FROM analysis_runs WHERE user_id = ? AND run_id = ?";
pub const CLAIM_ANALYSIS: &str = "UPDATE analysis_runs SET status = 'running' WHERE run_id = (SELECT run_id FROM analysis_runs WHERE status = 'queued' ORDER BY created_at LIMIT 1) RETURNING run_id, user_id, input_json";
pub const FINISH_ANALYSIS: &str = "UPDATE analysis_runs SET status = ?, output_json = ?, error_code = ? WHERE user_id = ? AND run_id = ? AND status = 'running'";
pub const RECOVER_ANALYSIS: &str =
    "UPDATE analysis_runs SET status = 'queued' WHERE status = 'running'";
pub const RETRY_ANALYSIS: &str = "UPDATE analysis_runs SET status = 'queued', error_code = NULL WHERE user_id = ? AND run_id = ? AND status = 'failed'";
pub const INSERT_FEEDBACK: &str = "INSERT INTO analysis_feedback (user_id, run_id, feedback_id, note, created_at) VALUES (?, ?, ?, ?, ?)";
pub const ANALYSIS_FEEDBACK: &str = "SELECT feedback_id, note, created_at FROM analysis_feedback WHERE user_id = ? AND run_id = ? ORDER BY created_at";
pub const EXPORT_TABLES: &[(&str, &str)] = &[
    (
        "deleted_uploads",
        "SELECT * FROM deleted_uploads WHERE user_id = ?",
    ),
    (
        "sync_batches",
        "SELECT * FROM sync_batches WHERE user_id = ?",
    ),
    ("reports", "SELECT * FROM reports WHERE user_id = ?"),
    ("raw_files", "SELECT * FROM raw_files WHERE user_id = ?"),
    (
        "extraction_inputs",
        "SELECT * FROM extraction_inputs WHERE user_id = ?",
    ),
    (
        "extraction_outputs",
        "SELECT * FROM extraction_outputs WHERE user_id = ?",
    ),
    (
        "observations",
        "SELECT * FROM observations WHERE user_id = ?",
    ),
    (
        "observation_revisions",
        "SELECT * FROM observation_revisions WHERE user_id = ?",
    ),
    (
        "report_relations",
        "SELECT * FROM report_relations WHERE user_id = ?",
    ),
    (
        "extraction_pages",
        "SELECT * FROM extraction_pages WHERE user_id = ?",
    ),
    (
        "health_connections",
        "SELECT * FROM health_connections WHERE user_id = ?",
    ),
    (
        "health_records",
        "SELECT * FROM health_records WHERE user_id = ?",
    ),
    (
        "health_revisions",
        "SELECT * FROM health_revisions WHERE user_id = ?",
    ),
    (
        "sync_coverage",
        "SELECT * FROM sync_coverage WHERE user_id = ?",
    ),
    (
        "analysis_runs",
        "SELECT * FROM analysis_runs WHERE user_id = ?",
    ),
    (
        "analysis_feedback",
        "SELECT * FROM analysis_feedback WHERE user_id = ?",
    ),
];
pub const DELETE_EXPORT: &str = "DELETE FROM exports WHERE user_id = ? AND export_id = ?";
pub const USER_DATA_REVISION: &str = "SELECT data_revision FROM users WHERE user_id = ?";
pub const TOUCH_USER_DATA: &str =
    "UPDATE users SET data_revision = data_revision + 1 WHERE user_id = ?";
pub const DELETE_FILE: &str = "DELETE FROM raw_files WHERE user_id = ? AND file_id = ?";
pub const DELETE_USER: &str = "DELETE FROM users WHERE user_id = ?";
pub const PURGE_FEEDBACK: &str = "DELETE FROM analysis_feedback WHERE user_id = ?";
pub const INSERT_CLEANUP: &str = "INSERT INTO cleanup_files (cleanup_id, relative_path, created_at) VALUES (?, ?, ?) ON CONFLICT (relative_path) DO NOTHING";
pub const CLEANUP_LIST: &str =
    "SELECT cleanup_id, relative_path FROM cleanup_files ORDER BY created_at LIMIT 100";
pub const CLEANUP_DONE: &str = "DELETE FROM cleanup_files WHERE cleanup_id = ?";
pub const INSERT_EXPORT: &str =
    "INSERT INTO exports (export_id, user_id, status, created_at) VALUES (?, ?, 'queued', ?)";
pub const EXPORT_LIST: &str = "SELECT export_id, status, error_code, created_at FROM exports WHERE user_id = ? ORDER BY created_at DESC LIMIT 100";
pub const EXPORT_GET: &str = "SELECT status FROM exports WHERE user_id = ? AND export_id = ?";
pub const CLAIM_EXPORT: &str = "UPDATE exports SET status = 'running' WHERE export_id = (SELECT export_id FROM exports WHERE status = 'queued' ORDER BY created_at LIMIT 1) RETURNING export_id, user_id";
pub const FINISH_EXPORT: &str = "UPDATE exports SET status = ?, error_code = ? WHERE user_id = ? AND export_id = ? AND status = 'running'";
pub const RECOVER_EXPORTS: &str = "UPDATE exports SET status = 'queued' WHERE status = 'running'";
pub const ACTIVATE_EXTRACTION: &str = "UPDATE jobs SET status = 'queued', error_code = NULL WHERE kind = 'document_extract' AND status = 'blocked'";
pub const VERIFY_JOB_LEASE: &str = "SELECT COUNT(*) FROM jobs WHERE user_id = ? AND job_id = ? AND lease_token = ? AND status = 'running' AND lease_until > ?";
pub const CREATE_CONNECTION: &str = "INSERT INTO health_connections (connection_id, user_id, platform, installation_id, created_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT (user_id, platform, installation_id) DO NOTHING";
pub const CONNECTION_BY_INSTALLATION: &str = "SELECT connection_id, platform, installation_id FROM health_connections WHERE user_id = ? AND platform = ? AND installation_id = ?";
pub const CONNECTION: &str =
    "SELECT platform FROM health_connections WHERE user_id = ? AND connection_id = ?";
pub const SYNC_BATCH: &str =
    "SELECT digest FROM sync_batches WHERE user_id = ? AND connection_id = ? AND batch_id = ?";
pub const INSERT_SYNC_BATCH: &str = "INSERT INTO sync_batches (user_id, connection_id, batch_id, digest, received_at) VALUES (?, ?, ?, ?, ?)";
pub const HEALTH_RECORD: &str = "SELECT version, deleted, payload_json FROM health_records WHERE user_id = ? AND platform = ? AND source_id = ? AND record_id = ?";
pub const UPSERT_HEALTH: &str = "INSERT INTO health_records (user_id, platform, source_id, record_id, record_type, start_at, end_at, version, deleted, payload_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (user_id, platform, source_id, record_id) DO UPDATE SET record_type = excluded.record_type, start_at = excluded.start_at, end_at = excluded.end_at, version = excluded.version, deleted = excluded.deleted, payload_json = excluded.payload_json";
pub const INSERT_HEALTH_REVISION: &str = "INSERT INTO health_revisions (user_id, platform, source_id, record_id, revision_id, raw_path, payload_json, received_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)";
pub const PURGE_HEALTH_REVISIONS: &str = "DELETE FROM health_revisions WHERE user_id = ? AND platform = ? AND source_id = ? AND record_id = ?";
pub const UPSERT_COVERAGE: &str = "INSERT INTO sync_coverage (user_id, connection_id, record_type, status, last_sync_at, first_success_at, last_success_at) VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (user_id, connection_id, record_type) DO UPDATE SET status = excluded.status, last_sync_at = excluded.last_sync_at, first_success_at = COALESCE(sync_coverage.first_success_at, excluded.first_success_at), last_success_at = COALESCE(excluded.last_success_at, sync_coverage.last_success_at)";
pub const COVERAGE: &str = "SELECT c.connection_id, c.platform, c.installation_id, s.record_type, s.status, s.last_sync_at, s.first_success_at, s.last_success_at, (SELECT MIN(h.start_at) FROM health_records h WHERE h.user_id = c.user_id AND h.platform = c.platform AND h.record_type = s.record_type AND h.deleted = 0) AS visible_start_at, (SELECT MAX(h.end_at) FROM health_records h WHERE h.user_id = c.user_id AND h.platform = c.platform AND h.record_type = s.record_type AND h.deleted = 0) AS visible_end_at FROM health_connections c LEFT JOIN sync_coverage s ON s.user_id = c.user_id AND s.connection_id = c.connection_id WHERE c.user_id = ? ORDER BY c.connection_id, s.record_type";
pub const HEALTH_RANGE: &str = r#"
SELECT platform
     , source_id
     , record_id
     , record_type
     , start_at
     , end_at
     , version
     , json_object('payload', json_object(
           'value', CASE WHEN json_type(payload_json, '$.payload.value') IN ('integer', 'real')
                         THEN json_extract(payload_json, '$.payload.value') ELSE NULL END,
           'unit', CASE WHEN json_extract(payload_json, '$.payload.unit') IN ('count', 'ms', 'count/min')
                        THEN json_extract(payload_json, '$.payload.unit') ELSE NULL END,
           'category', CASE WHEN json_type(payload_json, '$.payload.category') = 'integer'
                            THEN json_extract(payload_json, '$.payload.category') ELSE NULL END
       )) AS payload_json
  FROM health_records
 WHERE user_id = ?
   AND deleted = 0
   AND record_type = ?
   AND end_at >= ?
   AND start_at < ?
 ORDER BY start_at
        , source_id
        , record_id
 LIMIT 50001
"#;
pub const HEALTH_LIST: &str = "SELECT platform, source_id, record_id, record_type, start_at, end_at, version, payload_json FROM health_records WHERE user_id = ? AND deleted = 0 ORDER BY start_at, source_id, record_id LIMIT ? OFFSET ?";
pub const INSERT_REPORT: &str =
    "INSERT INTO reports (report_id, user_id, file_id, created_at) VALUES (?, ?, ?, ?)";
pub const REPORT: &str = "SELECT r.report_id, r.revision, r.context_json, f.original_name, f.page_count, r.created_at FROM reports r JOIN raw_files f ON f.user_id = r.user_id AND f.file_id = r.file_id WHERE r.user_id = ? AND r.report_id = ?";
pub const REPORTS: &str = "SELECT r.report_id, r.revision, r.context_json, f.original_name, f.page_count, r.created_at FROM reports r JOIN raw_files f ON f.user_id = r.user_id AND f.file_id = r.file_id WHERE r.user_id = ? AND (? IS NULL OR r.report_id > ?) ORDER BY r.report_id LIMIT ?";
pub const OBSERVATIONS: &str = "SELECT o.observation_id, o.report_id, v.revision, v.status, v.payload_json FROM observations o JOIN observation_revisions v ON v.user_id = o.user_id AND v.observation_id = o.observation_id AND v.revision = o.current_revision WHERE o.user_id = ? AND o.report_id = ? ORDER BY o.observation_id";
pub const OBSERVATION: &str = "SELECT o.current_revision FROM observations o WHERE o.user_id = ? AND o.report_id = ? AND o.observation_id = ?";
pub const INSERT_OBSERVATION: &str = "INSERT INTO observations (observation_id, user_id, report_id, current_revision, candidate_key) VALUES (?, ?, ?, 1, ?)";
pub const INSERT_OBSERVATION_REVISION: &str = "INSERT INTO observation_revisions (user_id, observation_id, revision, status, payload_json, created_at) VALUES (?, ?, ?, ?, ?, ?)";
pub const UPDATE_OBSERVATION: &str =
    "UPDATE observations SET current_revision = ? WHERE user_id = ? AND observation_id = ?";
pub const UPDATE_REPORT: &str = "UPDATE reports SET revision = revision + 1, context_json = COALESCE(?, context_json) WHERE user_id = ? AND report_id = ? AND revision = ?";
pub const OBSERVATION_HISTORY: &str = "SELECT observation_id, revision, status, payload_json, created_at FROM observation_revisions WHERE user_id = ? AND observation_id = ? ORDER BY revision";
pub const CONFIRMED_OBSERVATIONS: &str = "SELECT o.observation_id, o.report_id, v.revision, v.status, v.payload_json FROM observations o JOIN observation_revisions v ON v.user_id = o.user_id AND v.observation_id = o.observation_id AND v.revision = o.current_revision WHERE o.user_id = ? AND v.status = 'confirmed' AND NOT EXISTS (SELECT 1 FROM report_relations r WHERE r.user_id = o.user_id AND r.report_id = o.report_id) ORDER BY o.observation_id LIMIT 10001";
pub const REPORT_RELATION: &str =
    "SELECT preferred_report_id, kind FROM report_relations WHERE user_id = ? AND report_id = ?";
pub const RELATION_DEPENDENTS: &str =
    "SELECT COUNT(*) FROM report_relations WHERE user_id = ? AND preferred_report_id = ?";
pub const SET_RELATION: &str = "INSERT INTO report_relations (user_id, report_id, preferred_report_id, kind) VALUES (?, ?, ?, ?) ON CONFLICT (user_id, report_id) DO UPDATE SET preferred_report_id = excluded.preferred_report_id, kind = excluded.kind";
pub const CLEAR_RELATION: &str = "DELETE FROM report_relations WHERE user_id = ? AND report_id = ?";
pub const INVALIDATE_ANALYSES: &str = "UPDATE analysis_runs SET status = 'stale', input_json = '{}', output_json = NULL, error_code = 'data_changed' WHERE user_id = ? AND status != 'stale'";
pub const INVALIDATE_EXPORTS: &str = "UPDATE exports SET status = 'stale' WHERE user_id = ?";
pub const CANDIDATE_EXISTS: &str =
    "SELECT COUNT(*) FROM observations WHERE user_id = ? AND report_id = ? AND candidate_key = ?";
pub const INSERT_PAGE: &str = "INSERT INTO extraction_pages (user_id, report_id, run_id, page_number, status, content, model, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)";
pub const EXTRACTION_PAGES: &str = "SELECT run_id, page_number, status, content, model, created_at FROM extraction_pages WHERE user_id = ? AND report_id = ? ORDER BY created_at DESC, run_id, page_number";

pub const SET_SCHEMA_VERSION: &str = "PRAGMA user_version = 7";

pub const CREATE_USER: &str = r#"
INSERT INTO users (user_id
     , username
     , password_hash
     , created_at)
VALUES (?
     , ?
     , ?
     , ?)
"#;

pub const CREDENTIALS: &str = r#"
SELECT user_id
     , username
     , password_hash
     , credential_version
  FROM users
 WHERE username = ?
   AND is_active = 1
"#;

pub const DISABLE_USER: &str = r#"
UPDATE users
   SET is_active = 0
     , credential_version = credential_version + 1
 WHERE username = ?
RETURNING user_id
"#;

pub const RESET_PASSWORD: &str = r#"
UPDATE users
   SET password_hash = ?
     , credential_version = credential_version + 1
 WHERE username = ?
RETURNING user_id
"#;

pub const REVOKE_USER_SESSIONS: &str = r#"
DELETE
  FROM sessions
 WHERE user_id = ?
"#;

pub const EXPIRE_SESSIONS: &str = r#"
DELETE
  FROM sessions
 WHERE expires_at <= ?
"#;

pub const CREATE_SESSION: &str = r#"
INSERT INTO sessions (token_hash
     , user_id
     , expires_at)
SELECT ?
     , user_id
     , ?
  FROM users
 WHERE user_id = ?
   AND credential_version = ?
   AND is_active = 1
"#;

pub const SESSION_USER: &str = r#"
SELECT users.user_id 
     , users.username
  FROM sessions
  JOIN users ON users.user_id = sessions.user_id
 WHERE sessions.token_hash = ?
   AND sessions.expires_at > ?
   AND users.is_active = 1
"#;

pub const REVOKE_SESSION: &str = r#"
DELETE
  FROM sessions
 WHERE token_hash = ?
"#;

pub const EXPIRE_LOGIN_WINDOWS: &str = r#"
DELETE
  FROM login_windows
 WHERE starts_at <= ?
"#;

pub const RESERVE_LOGIN: &str = r#"
INSERT INTO login_windows (username_hash
     , starts_at
     , attempt_count)
VALUES (?
     , ?
     , 1) ON CONFLICT (username_hash) DO UPDATE
   SET attempt_count = attempt_count + 1
RETURNING attempt_count
"#;

pub const INSERT_FILE: &str = r#"
INSERT INTO raw_files (file_id
     , user_id
     , upload_id
     , relative_path
     , processing_path
     , processing_sha256
     , original_name
     , content_type
     , sha256
     , byte_count
     , page_count
     , created_at)
VALUES (?
     , ?
     , ?
     , ?
     , ?
     , ?
     , ?
     , ?
     , ?
     , ?
     , ?
     , ?)
"#;

pub const FILE_BY_UPLOAD: &str = r#"
SELECT file_id
     , upload_id
     , relative_path
     , processing_path
     , processing_sha256
     , original_name
     , content_type
     , sha256
     , byte_count
     , page_count
     , created_at
  FROM raw_files
 WHERE user_id = ?
   AND upload_id = ?
"#;

pub const FILE_BY_ID: &str = r#"
SELECT file_id
     , upload_id
     , relative_path
     , processing_path
     , processing_sha256
     , original_name
     , content_type
     , sha256
     , byte_count
     , page_count
     , created_at
  FROM raw_files
 WHERE user_id = ?
   AND file_id = ?
"#;

pub const LIST_FILES: &str = r#"
SELECT file_id
     , upload_id
     , relative_path
     , processing_path
     , processing_sha256
     , original_name
     , content_type
     , sha256
     , byte_count
     , page_count
     , created_at
  FROM raw_files
 WHERE user_id = ?
   AND (? IS NULL OR file_id > ?)
ORDER BY file_id
 LIMIT ?
"#;

pub const INSERT_JOB: &str = r#"
INSERT INTO jobs (job_id
     , user_id
     , file_id
     , kind
     , status
     , error_code
     , created_at)
VALUES (?
     , ?
     , ?
     , 'document_extract'
     , 'blocked'
     , 'provider_disabled'
     , ?)
"#;

pub const JOB_BY_ID: &str = r#"
SELECT job_id
     , file_id
     , kind
     , status
     , attempt_count
     , error_code
     , created_at
  FROM jobs
 WHERE user_id = ?
   AND job_id = ?
"#;

pub const JOB_BY_FILE: &str = r#"
SELECT job_id
     , file_id
     , kind
     , status
     , attempt_count
     , error_code
     , created_at
  FROM jobs
 WHERE user_id = ?
   AND file_id = ?
"#;

pub const LIST_JOBS: &str = r#"
SELECT job_id
     , file_id
     , kind
     , status
     , attempt_count
     , error_code
     , created_at
  FROM jobs
 WHERE user_id = ?
   AND (? IS NULL OR job_id > ?)
ORDER BY job_id
 LIMIT ?
"#;

pub const RETRY_JOB: &str = r#"
UPDATE jobs
   SET status = 'queued'
     , error_code = NULL
     , attempt_count = 0
 WHERE user_id = ?
   AND job_id = ?
   AND status = 'failed'
"#;

pub const QUEUE_BLOCKED_JOB: &str = r#"
UPDATE jobs
   SET status = 'queued'
     , error_code = NULL
 WHERE user_id = ?
   AND job_id = ?
   AND status = 'blocked'
"#;

pub const EXHAUST_JOBS: &str = r#"
UPDATE jobs
   SET status = 'failed'
     , error_code = 'attempts_exhausted'
     , lease_token = NULL
     , lease_until = NULL
 WHERE status = 'running'
   AND lease_until <= ?
   AND attempt_count >= ?
"#;

pub const CLAIM_JOB: &str = r#"
UPDATE jobs
   SET status = 'running' 
     , attempt_count = attempt_count + 1 
     , lease_token = ? 
     , lease_until = ?
 WHERE job_id = (
SELECT job_id
  FROM jobs
 WHERE (status = 'queued' OR (status = 'running'
   AND lease_until <= ?))
   AND attempt_count < ?
ORDER BY created_at
     , job_id
 LIMIT 1 )
RETURNING job_id
     , user_id
     , file_id
     , lease_token
     , lease_until
"#;

pub const FINISH_JOB: &str = r#"
UPDATE jobs
   SET status = ?
     , error_code = ?
     , lease_token = NULL
     , lease_until = NULL
 WHERE job_id = ?
   AND lease_token = ?
   AND status = 'running'
   AND lease_until > ?
"#;

pub const RENEW_JOB: &str = r#"
UPDATE jobs
   SET lease_until = ?
 WHERE job_id = ?
   AND lease_token = ?
   AND status = 'running'
   AND lease_until > ?
"#;

pub const ALL_EXPORT_IDS: &str = "SELECT export_id FROM exports WHERE user_id = ?";

pub const UPLOAD_DELETED: &str =
    "SELECT COUNT(*) FROM deleted_uploads WHERE user_id = ? AND upload_id = ?";
pub const TOMBSTONE_UPLOAD: &str = "INSERT INTO deleted_uploads (user_id, upload_id) SELECT user_id, upload_id FROM raw_files WHERE user_id = ? AND file_id = ? ON CONFLICT DO NOTHING";
pub const DUPLICATE_FILES: &str = "SELECT file_id, original_name FROM raw_files WHERE user_id = ? AND sha256 = (SELECT sha256 FROM raw_files WHERE user_id = ? AND file_id = ?) AND file_id != ? ORDER BY file_id LIMIT 100";

pub const RAW_PATHS: &str = "SELECT relative_path FROM raw_files UNION SELECT raw_path AS relative_path FROM health_revisions UNION SELECT relative_path FROM cleanup_files";
pub const HEALTH_RAW_TO_DELETE: &str = "SELECT raw_path FROM health_revisions WHERE user_id = ? AND platform = ? AND record_id = ? AND (source_id = ? OR ? = '*')";
pub const HEALTH_RAW_LIST: &str = "SELECT revision_id, platform, source_id, record_id, raw_path, received_at FROM health_revisions WHERE user_id = ? AND (? IS NULL OR revision_id > ?) ORDER BY revision_id LIMIT ?";
pub const HEALTH_RAW_GET: &str =
    "SELECT raw_path FROM health_revisions WHERE user_id = ? AND revision_id = ?";
pub const INSERT_EXTRACTION_OUTPUT: &str = "INSERT INTO extraction_outputs (user_id, report_id, run_id, page_number, stage, response_body, status_code, content, model, adapter, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
pub const EXTRACTION_OUTPUTS: &str = "SELECT run_id, page_number, stage, model, adapter, created_at FROM extraction_outputs WHERE user_id = ? AND report_id = ? ORDER BY created_at DESC, run_id, page_number, stage";

pub const EXTRACTION_OUTPUT: &str = "SELECT response_body, status_code, content FROM extraction_outputs WHERE user_id = ? AND report_id = ? AND run_id = ? AND page_number = ? AND stage = ?";

pub const INSERT_EXTRACTION_INPUT: &str = "INSERT INTO extraction_inputs (user_id, report_id, run_id, page_number, input_json, created_at) VALUES (?, ?, ?, ?, ?, ?)";
pub const EXTRACTION_INPUTS: &str = "SELECT run_id, page_number, created_at, json_extract(input_json, '$.text_layer.status') AS text_status, json_extract(input_json, '$.error_code') AS error_code FROM extraction_inputs WHERE user_id = ? AND report_id = ? ORDER BY created_at DESC, run_id, page_number";

pub const EXTRACTION_INPUT: &str = "SELECT input_json FROM extraction_inputs WHERE user_id = ? AND report_id = ? AND run_id = ? AND page_number = ?";
