"""Read and audit a version 1 export without a server or third-party packages."""

import argparse
import csv
import hashlib
import io
import json
import re
import stat
import sys
import zipfile
import zlib
from pathlib import Path

TABLES = frozenset(
    'daily_views derived_results deleted_uploads sync_batches reports raw_files extraction_inputs extraction_outputs '
    'observations observation_revisions report_relations extraction_pages health_connections '
    'health_records health_revisions sync_coverage analysis_runs analysis_feedback'.split()
)
MAX_JSON_BYTES = 96 * 1024 * 1024


class InvalidExport(ValueError):
    """The archive does not satisfy the supported export contract."""


def require(condition, message):
    if not condition:
        raise InvalidExport(message)


def object_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'Duplicate JSON field')
        result[key] = value
    return result


def invalid_constant(value):
    raise InvalidExport('Non-finite JSON number')


def parse_json(data):
    return json.loads(data, object_pairs_hook=object_pairs, parse_constant=invalid_constant)


def safe_name(name):
    return (
        isinstance(name, str)
        and '\\' not in name
        and ':' not in name
        and all(part not in ('', '.', '..') for part in name.split('/'))
    )


def read_object(archive, name):
    with archive.open(name) as stream:
        data = stream.read(MAX_JSON_BYTES + 1)
    require(len(data) <= MAX_JSON_BYTES, 'JSON object exceeds the size limit')
    value = parse_json(data)
    require(isinstance(value, dict), 'Expected a JSON object')
    return value


def rows(archive, name):
    with archive.open(name) as stream:
        while True:
            line = stream.readline(MAX_JSON_BYTES + 1)
            if not line:
                return
            require(len(line) <= MAX_JSON_BYTES, 'JSONL row exceeds the size limit')
            value = parse_json(line)
            require(isinstance(value, dict), 'Expected a JSONL object')
            yield value


def audit(path, max_bytes):
    require(max_bytes > 0, 'The archive size limit must be positive')
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        names = {entry.filename for entry in entries}
        require(len(names) == len(entries), 'Duplicate ZIP member')
        require(all(safe_name(name) for name in names), 'Unsafe ZIP member path')
        require(
            all(not stat.S_ISLNK(entry.external_attr >> 16) for entry in entries),
            'Symbolic links are not supported',
        )
        require(
            sum(entry.file_size for entry in entries) <= max_bytes,
            'Archive exceeds the uncompressed size limit',
        )
        require('manifest.json' in names, 'Missing manifest')
        manifest = read_object(archive, 'manifest.json')
        require(manifest.get('format') == 'helpyourself-export', 'Unknown archive format')
        require(type(manifest.get('version')) is int and manifest['version'] == 1,
                'Only export version 1 is supported')
        version = manifest['version']
        require(manifest.get('database_schema_version') == 1, 'Unknown database schema')
        inventory = manifest.get('members')
        require(isinstance(inventory, dict) and set(inventory) == names - {'manifest.json'},
                'Member inventory differs from the archive')
        require(manifest.get('integrity') == dict(algorithm='sha256', scope='all_members_except_manifest', authenticity='unsigned'),
                'Unknown integrity contract')
        for name, expected in inventory.items():
            require(isinstance(expected, dict) and isinstance(expected.get('sha256'), str)
                    and re.fullmatch('[0-9a-f]{64}', expected['sha256'])
                    and type(expected.get('byte_count')) is int, 'Invalid member digest')
            digest, size = hashlib.sha256(), 0
            with archive.open(name) as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(block)
                    size += len(block)
            require(size == expected['byte_count'] and digest.hexdigest() == expected['sha256'],
                    'Member digest or byte count differs')
        require(isinstance(manifest.get('table_counts'), dict)
                and set(manifest['table_counts']) == TABLES, 'Invalid table counts')
        require(isinstance(manifest.get('table_columns'), dict)
                and set(manifest['table_columns']) == TABLES, 'Invalid column dictionary')
        dictionary = manifest.get('field_dictionary')
        require(isinstance(dictionary, dict) and dictionary.get('version') == 1
                and isinstance(dictionary.get('tables'), dict) and set(dictionary['tables']) == TABLES,
                'Invalid semantic field dictionary')
        user = manifest.get('user_id')
        require(isinstance(user, str) and bool(user), 'Missing archive owner')
        tables = manifest.get('tables')
        expected_tables = {f'{table}.jsonl' for table in TABLES}
        require(isinstance(tables, list) and all(isinstance(t, str) for t in tables),
                'Invalid table list')
        require(len(tables) == len(expected_tables) and set(tables) == expected_tables,
                'Table list does not match the contract')
        require(expected_tables <= names, 'Missing declared table')
        require(manifest.get('csv') == 'observations.csv' and 'observations.csv' in names,
                'Missing observations CSV')
        require(manifest.get('raw_directory') == 'raw/', 'Invalid raw directory')
        counts = {}
        # Retain only the indexes needed for cross-record checks.
        files, reports, observations, revisions, health = set(), {}, {}, set(), set()
        raw_references, health_references = {}, []
        for table in sorted(TABLES):
            count = 0
            columns = None
            definitions = manifest['table_columns'][table]
            require(isinstance(definitions, list) and bool(definitions), 'Missing column definitions')
            require(all(isinstance(d, dict) and isinstance(d.get('name'), str)
                        and d.get('sqlite_type') in ('TEXT', 'INTEGER', 'REAL', 'BLOB')
                        and type(d.get('not_null')) is bool
                        and type(d.get('primary_key_position')) is int for d in definitions),
                    'Invalid column definitions')
            columns = {d['name'] for d in definitions}
            require(len(columns) == len(definitions), 'Duplicate column definition')
            definition = dictionary['tables'][table]
            require(isinstance(definition, dict) and isinstance(definition.get('meaning'), str)
                    and isinstance(definition.get('fields'), dict)
                    and set(definition['fields']) == columns
                    and all(isinstance(text, str) and text for text in definition['fields'].values()),
                    'Semantic dictionary differs from the column dictionary')
            for row in rows(archive, f'{table}.jsonl'):
                if columns is not None:
                    require(set(row) == columns, 'Row fields differ from the column dictionary')
                count += 1
                require(row.get('user_id') == user, 'Record owner differs from manifest')
                if table == 'raw_files':
                    key = row.get('file_id')
                    require(isinstance(key, str) and key not in files, 'Invalid file ID')
                    files.add(key)
                    raw_path = row.get('relative_path')
                    require(isinstance(raw_path, str), 'Missing original file path')
                    require(raw_path not in raw_references, 'Duplicate original reference')
                    require(isinstance(row.get('sha256'), str)
                            and re.fullmatch('[0-9a-f]{64}', row['sha256']),
                            'Invalid original digest')
                    require(type(row.get('byte_count')) is int and row['byte_count'] > 0,
                            'Invalid original size')
                    raw_references[raw_path] = (row['sha256'], row['byte_count'])
                elif table == 'reports':
                    key = row.get('report_id')
                    require(isinstance(key, str) and key not in reports, 'Invalid report ID')
                    reports[key] = row.get('file_id')
                elif table == 'observations':
                    key = row.get('observation_id')
                    require(isinstance(key, str) and key not in observations,
                            'Invalid observation ID')
                    require(isinstance(row.get('report_id'), str)
                            and type(row.get('current_revision')) is int,
                            'Invalid observation reference')
                    observations[key] = (row['report_id'], row['current_revision'])
                elif table == 'observation_revisions':
                    key = (row.get('observation_id'), row.get('revision'))
                    require(isinstance(key[0], str) and type(key[1]) is int,
                            'Invalid observation revision')
                    require(key not in revisions, 'Duplicate observation revision')
                    revisions.add(key)
                elif table == 'health_records':
                    key = tuple(row.get(k) for k in ('platform', 'source_id', 'record_id'))
                    require(all(isinstance(v, str) for v in key) and key not in health,
                            'Invalid health record key')
                    health.add(key)
                elif table == 'health_revisions':
                    key = tuple(row.get(k) for k in ('platform', 'source_id', 'record_id'))
                    require(all(isinstance(v, str) for v in key), 'Invalid health revision key')
                    raw_path = row.get('raw_path')
                    require(isinstance(raw_path, str), 'Missing health original path')
                    require(raw_path not in raw_references, 'Duplicate original reference')
                    raw_references[raw_path] = (None, None)
                    health_references.append((key, raw_path, row.get('payload_json')))
            counts[table] = count
        require(all(value in files for value in reports.values()), 'Report refers to a missing file')
        require(all(report in reports and (key, revision) in revisions
                    for key, (report, revision) in observations.items()),
                'Observation refers to a missing report or current revision')
        require(all(key in observations for key, _ in revisions), 'Orphan observation revision')
        for key, raw_path, payload in health_references:
            require(key in health, 'Orphan health revision')
            require(raw_path in names, 'Missing health original')
            require(isinstance(payload, str), 'Invalid health payload')
            require(read_object(archive, raw_path) == parse_json(payload),
                    'Health original differs from its indexed payload')
        require(all(type(manifest['table_counts'][table]) is int
                    and manifest['table_counts'][table] == count for table, count in counts.items()),
                'Table counts differ from the manifest')
        expected = expected_tables | {'manifest.json', 'observations.csv'} | raw_references.keys()
        require(names == expected, 'Missing or undeclared archive members')
        for raw_path, (digest, size) in raw_references.items():
            parts = raw_path.split('/')
            require(len(parts) >= 4 and parts[0] == 'raw'
                    and parts[1] in ('apple_health', 'google_health', 'documents', 'photos', 'manual', 'file_import')
                    and parts[2] == user, 'Original path differs from archive owner')
            actual_digest = hashlib.sha256()
            actual_size = 0
            with archive.open(raw_path) as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    actual_digest.update(block)
                    actual_size += len(block)
            if digest is not None:
                require(isinstance(digest, str) and re.fullmatch('[0-9a-f]{64}', digest),
                        'Invalid original digest')
                require(type(size) is int and size > 0, 'Invalid original size')
                require(actual_digest.hexdigest() == digest and actual_size == size,
                        'Original digest or byte count does not match')
            else:
                require(size is None and raw_path.endswith('.json'), 'Missing original digest')
        with archive.open('observations.csv') as stream:
            reader = csv.DictReader(io.TextIOWrapper(stream, encoding='utf-8', newline=''))
            require(reader.fieldnames is not None
                    and {'observation_id', 'revision'} <= set(reader.fieldnames),
                    'Invalid observations CSV header')
            csv_revisions = set()
            for row in reader:
                require(None not in row and all(v is not None for v in row.values()),
                        'Invalid observations CSV row')
                key = (row['observation_id'], int(row['revision']))
                require(key in revisions and key not in csv_revisions,
                        'CSV revision differs from JSONL')
                csv_revisions.add(key)
            require(csv_revisions == revisions, 'CSV omits observation revisions')
        return {
            'status': 'passed', 'format_version': version, 'tables': counts,
            'member_sha256_checked': len(names) - 1,
            'original_files': len(raw_references),
            'document_sha256_checked': counts['raw_files'],
            'health_payloads_checked': counts['health_revisions'],
            'limits': [
                'This checks archive consistency, not medical validity or authenticity.',
                'The manifest is unsigned and cannot establish authenticity.',
                'This is not a restore tool or a complete relational schema audit.',
                'Processing copies are not included.',
            ],
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--max-bytes', type=int, required=True,
                        help='Maximum total uncompressed archive bytes')
    args = parser.parse_args()
    try:
        result = audit(args.archive, args.max_bytes)
    except (OSError, ValueError, KeyError, TypeError, zipfile.BadZipFile,
            RuntimeError, NotImplementedError, EOFError, csv.Error, zlib.error) as error:
        # Archive contents can contain health data. Do not print parser input or paths.
        reason = str(error) if isinstance(error, InvalidExport) else 'Cannot read archive data'
        print(json.dumps({'status': 'failed', 'reason': reason}), file=sys.stderr)
        return 1
    print(json.dumps(result, indent=2))
    return 0


if __name__ == '__main__':
    sys.exit(main())
