import hashlib
import json
import sqlite3
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'src/archive'))
import check_export
import restore_export

USER = '11111111-1111-4111-8111-111111111111'


def archive_fixture(path, broken=False):
    connection = sqlite3.connect(':memory:')
    connection.executescript(restore_export.SCHEMA.read_text())
    connection.execute("INSERT INTO users (user_id, username, password_hash, created_at) VALUES (?, 'test', '', 0)", (USER,))
    columns = {}
    members = {}
    counts = {}
    for table in sorted(check_export.TABLES):
        columns[table] = [dict(name=name, sqlite_type=kind, not_null=bool(required), primary_key_position=key)
                          for name, kind, required, key in restore_export.queries.columns(connection, table)]
        rows = []
        if broken and table == 'sync_batches':
            rows = [dict(user_id=USER, connection_id='missing', batch_id='batch', digest='x', received_at=0)]
        members[table + '.jsonl'] = ''.join(json.dumps(row) + '\n' for row in rows).encode()
        counts[table] = len(rows)
    connection.close()
    members['observations.csv'] = b'observation_id,revision\n'
    manifest = dict(format='helpyourself-export', version=1, database_schema_version=1,
                    user_id=USER, created_at=0, data_revision=42,
                    tables=[table + '.jsonl' for table in sorted(check_export.TABLES)],
                    field_dictionary=json.loads((ROOT / "src/archive/field_dictionary.json").read_text()),
                    table_columns=columns, table_counts=counts, raw_directory='raw/', csv='observations.csv',
                    integrity=dict(algorithm='sha256', scope='all_members_except_manifest', authenticity='unsigned'),
                    members={name: dict(sha256=hashlib.sha256(content).hexdigest(), byte_count=len(content))
                             for name, content in members.items()})
    members['manifest.json'] = json.dumps(manifest).encode()
    with zipfile.ZipFile(path, 'w') as archive:
        for name, content in members.items():
            archive.writestr(name, content)


class RestoreTests(unittest.TestCase):
    def test_restores_initial_schema_and_disables_account(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive_fixture(root / 'export.zip')
            result = restore_export.restore(root / 'export.zip', root / 'restored', 'alice', 1048576)
            self.assertEqual(result['account_status'], 'disabled')
            connection = sqlite3.connect(root / 'restored/database.sqlite')
            self.assertEqual(connection.execute('PRAGMA user_version').fetchone(), (1,))
            self.assertEqual(connection.execute('SELECT user_id, is_active, data_revision, password_hash FROM users').fetchone(),
                             (USER, 0, 42, ''))
            connection.close()

    def test_existing_destination_is_never_changed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive_fixture(root / 'export.zip')
            destination = root / 'existing'
            destination.mkdir()
            (destination / 'keep').write_text('existing')
            with self.assertRaises(FileExistsError):
                restore_export.restore(root / 'export.zip', destination, 'alice', 1048576)
            self.assertEqual((destination / 'keep').read_text(), 'existing')
            self.assertEqual(len(list(destination.iterdir())), 1)

    def test_failed_relational_restore_removes_only_its_new_directory(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive_fixture(root / 'export.zip', broken=True)
            check_export.audit(root / 'export.zip', 1048576)
            with self.assertRaises(check_export.InvalidExport):
                restore_export.restore(root / 'export.zip', root / 'failed', 'alice', 1048576)
            self.assertFalse((root / 'failed').exists())
            self.assertTrue((root / 'export.zip').exists())


if __name__ == '__main__':
    unittest.main()
