import copy
import hashlib
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'src/archive/check_export.py'
spec = importlib.util.spec_from_file_location('check_export', SCRIPT)
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
USER = '11111111-1111-4111-8111-111111111111'
RAW = f'raw/documents/{USER}/file'
HEALTH = f'raw/apple_health/{USER}/revision.json'


def fixture():
    data = {name: [] for name in checker.TABLES}
    data['raw_files'] = [dict(user_id=USER, file_id='file', relative_path=RAW,
                              sha256=hashlib.sha256(b'original').hexdigest(), byte_count=8)]
    data['reports'] = [dict(user_id=USER, report_id='report', file_id='file')]
    data['observations'] = [dict(user_id=USER, report_id='report', observation_id='obs', current_revision=1)]
    data['observation_revisions'] = [dict(user_id=USER, observation_id='obs', revision=1)]
    health = dict(user_id=USER, platform='apple_health', source_id='watch', record_id='sample')
    data['health_records'] = [health]
    data['health_revisions'] = [dict(health, raw_path=HEALTH, payload_json='{"value": 42}')]
    manifest = dict(format='helpyourself-export', version=1, user_id=USER,
                    tables=[name + '.jsonl' for name in sorted(data)],
                    csv='observations.csv', raw_directory='raw/')
    return data, manifest


def write_archive(path, data, manifest, replacements):
    members = {name + '.jsonl': ''.join(json.dumps(row) + '\n' for row in rows)
               for name, rows in data.items()}
    members.update({'manifest.json': json.dumps(manifest), RAW: b'original',
                    HEALTH: '{"value":42}', 'observations.csv': 'observation_id,revision\nobs,1\n'})
    if manifest.get('version') == 1:
        manifest = copy.deepcopy(manifest)
        manifest['database_schema_version'] = 1
        manifest['table_counts'] = {name: len(rows) for name, rows in data.items()}
        manifest['table_columns'] = {name: [dict(name=field, sqlite_type='TEXT', not_null=False, primary_key_position=0)
                                          for field in sorted(set().union(*(row.keys() for row in rows)) if rows else {'user_id'})]
                                     for name, rows in data.items()}
        manifest['field_dictionary'] = dict(version=1, tables={table: dict(meaning='Synthetic fixture', fields={column['name']: 'Synthetic field' for column in columns}) for table, columns in manifest['table_columns'].items()})
        manifest['integrity'] = dict(algorithm='sha256', scope='all_members_except_manifest', authenticity='unsigned')
        manifest['members'] = {name: dict(sha256=hashlib.sha256(content.encode() if isinstance(content, str) else content).hexdigest(),
                                         byte_count=len(content.encode() if isinstance(content, str) else content))
                               for name, content in members.items() if name != 'manifest.json'}
        members['manifest.json'] = json.dumps(manifest)
    members.update(replacements)
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as archive:
        for name, content in members.items():
            if content is not None:
                archive.writestr(name, content)


class ExportTests(unittest.TestCase):
    def test_reads_valid_archive_without_extracting_files(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'export.zip'
            data, manifest = fixture()
            write_archive(path, data, manifest, {})
            result = checker.audit(path, 1024 * 1024)
            self.assertEqual(result['status'], 'passed')
            self.assertEqual(result['document_sha256_checked'], 1)
            self.assertEqual(result['health_payloads_checked'], 1)
            self.assertEqual(list(Path(directory).iterdir()), [path])

    def test_rejects_missing_tampered_or_undeclared_members(self):
        cases = [{RAW: None}, {RAW: b'tampered'}, {HEALTH: '{"value":43}'},
                 {'reports.jsonl': None}, {'observations.csv': 'observation_id,revision\n'},
                 {'../escape': 'x'}, {'sessions.jsonl': 'secret'},
                 {'observations.csv': 'observation_id,revision\nobs,1\nobs,1\n'},
                 {'reports.jsonl': '{"user_id":"secret", "user_id":"other"}\n'}]
        for replacement in cases:
            with self.subTest(replacement=list(replacement)), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / 'export.zip'
                data, manifest = fixture()
                write_archive(path, data, manifest, replacement)
                with self.assertRaises(checker.InvalidExport):
                    checker.audit(path, 1024 * 1024)

    def test_rejects_wrong_owner_broken_references_and_missing_digest(self):
        for table, field, value in [('reports', 'user_id', 'another-user'),
                                    ('reports', 'file_id', 'missing'),
                                    ('observations', 'current_revision', 2),
                                    ('health_revisions', 'record_id', 'missing'),
                                    ('raw_files', 'sha256', None)]:
            with self.subTest(table=table, field=field), tempfile.TemporaryDirectory() as directory:
                data, manifest = fixture()
                data[table][0][field] = value
                path = Path(directory) / 'export.zip'
                write_archive(path, data, manifest, {})
                with self.assertRaises(checker.InvalidExport):
                    checker.audit(path, 1024 * 1024)

    def test_rejects_version_table_drift_size_limit_and_duplicates(self):
        data, original = fixture()
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'export.zip'
            for changes in [dict(version=5), dict(tables=[]), dict(csv='other.csv')]:
                manifest = copy.deepcopy(original)
                manifest.update(changes)
                write_archive(path, data, manifest, {})
                with self.assertRaises(checker.InvalidExport):
                    checker.audit(path, 1024 * 1024)
            write_archive(path, data, original, {})
            with self.assertRaises(checker.InvalidExport):
                checker.audit(path, 10)
            with warnings.catch_warnings():
                warnings.simplefilter('ignore', UserWarning)
                with zipfile.ZipFile(path, 'a') as archive:
                    archive.writestr('manifest.json', '{}')
            with self.assertRaises(checker.InvalidExport):
                checker.audit(path, 1024 * 1024)

    def test_version_one_checks_every_member_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'export.zip'
            data, manifest = fixture()
            manifest['version'] = 1
            write_archive(path, data, manifest, {})
            result = checker.audit(path, 1024 * 1024)
            self.assertEqual(result['format_version'], 1)
            self.assertEqual(result['member_sha256_checked'], 21)
            for replacement in [{'reports.jsonl': json.dumps(data['reports'][0]) + ' \n'},
                                {HEALTH: '{"value": 42}'}, {'observations.csv': 'observation_id,revision\nobs,1\n\n'}]:
                write_archive(path, data, manifest, replacement)
                with self.assertRaises(checker.InvalidExport):
                    checker.audit(path, 1024 * 1024)

    def test_cli_failure_does_not_print_health_content(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'private-name.zip'
            data, manifest = fixture()
            write_archive(path, data, manifest, {'reports.jsonl': 'private-health-value\n'})
            result = subprocess.run([sys.executable, str(SCRIPT), str(path), '--max-bytes',
                                     '1048576'], text=True, capture_output=True, check=False)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(json.loads(result.stderr)['status'], 'failed')
            self.assertNotIn('private', result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
