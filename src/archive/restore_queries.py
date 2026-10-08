"""SQLite statements for initial-schema archive restoration."""

CREATE_OWNER = """
INSERT INTO users (user_id, username, password_hash, is_active, created_at, data_revision)
VALUES (?, ?, '', 0, ?, ?)
"""
SET_VERSION = 'PRAGMA user_version = 1'
DEFER_KEYS = 'PRAGMA defer_foreign_keys = ON'
FOREIGN_KEYS = 'PRAGMA foreign_keys = ON'
CHECK_KEYS = 'PRAGMA foreign_key_check'
CHECK_DATABASE = 'PRAGMA integrity_check'
CANCEL_ANALYSIS = """
UPDATE analysis_runs
   SET status = 'failed', error_code = 'restore_interrupted'
 WHERE status IN ('queued', 'running')
"""
CLEAR_PROCESSING = 'UPDATE raw_files SET processing_path = NULL, processing_sha256 = NULL'


def columns(connection, table):
    # The caller supplies only names in the canonical export table set.
    return connection.execute('SELECT name, type, "notnull", pk FROM pragma_table_info(?)',
                              (table,)).fetchall()


def insert(table, fields):
    # Both identifiers come from the local canonical schema, never archive text.
    names = ', '.join('"' + field + '"' for field in fields)
    placeholders = ', '.join('?' for _ in fields)
    return f'INSERT INTO "{table}" ({names}) VALUES ({placeholders})'

REQUEUE_DAILY_VIEWS = "UPDATE daily_views SET status='queued', result_json=NULL, claim_token=NULL, lease_until=NULL, attempts=0, error_code=NULL"
