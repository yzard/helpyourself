"""Restore a version 1 export into a new directory with a disabled account."""

import argparse
import csv
import json
import os
import re
import shutil
import sqlite3
import sys
import zipfile
import zlib
from pathlib import Path

import check_export
import restore_queries as queries

SCHEMA = Path(__file__).resolve().parents[1] / 'backend_api/database/schema.sql'


def restore(archive_path, destination, username, max_bytes):
    check_export.require(destination.is_absolute(), 'Destination must be absolute')
    check_export.require(re.fullmatch('[a-z0-9_.@-]{3,64}', username) is not None,
                         'Invalid restoration username')
    # Keep the same open file description through audit and restoration.
    with archive_path.open('rb') as source:
        audit = check_export.audit(source, max_bytes)
        source.seek(0)
        with zipfile.ZipFile(source) as archive:
            manifest = check_export.read_object(archive, 'manifest.json')
            revision = manifest.get('data_revision')
            created = manifest.get('created_at')
            check_export.require(type(revision) is int and 0 <= revision < 2**63
                                 and type(created) is int and 0 <= created < 2**63,
                                 'Invalid archive metadata')
            # mkdir is exclusive. Never reuse or remove an existing destination.
            destination.mkdir(mode=0o700)
            try:
                database = destination / 'database.sqlite'
                with database.open('xb') as stream:
                    os.chmod(stream.fileno(), 0o600)
                connection = sqlite3.connect(database)
                try:
                    connection.execute(queries.FOREIGN_KEYS)
                    connection.executescript(SCHEMA.read_text())
                    connection.execute('BEGIN IMMEDIATE')
                    connection.execute(queries.DEFER_KEYS)
                    connection.execute(queries.CREATE_OWNER, (manifest['user_id'], username, created, revision))
                    for table in sorted(check_export.TABLES):
                        definitions = queries.columns(connection, table)
                        expected = [dict(name=name, sqlite_type=kind, not_null=bool(required),
                                         primary_key_position=key)
                                    for name, kind, required, key in definitions]
                        check_export.require(manifest['table_columns'][table] == expected,
                                             'Archive columns differ from the initial schema')
                        fields = [definition[0] for definition in definitions]
                        statement = queries.insert(table, fields)
                        for row in check_export.rows(archive, table + '.jsonl'):
                            connection.execute(statement, [row[field] for field in fields])
                    interrupted = connection.execute(queries.CANCEL_ANALYSIS).rowcount
                    connection.execute(queries.CLEAR_PROCESSING)
                    connection.execute(queries.REQUEUE_DAILY_VIEWS)
                    check_export.require(not connection.execute(queries.CHECK_KEYS).fetchall(),
                                         'Restored foreign keys are invalid')
                    check_export.require(connection.execute(queries.CHECK_DATABASE).fetchall() == [('ok',)],
                                         'Restored database is invalid')
                    for member in archive.infolist():
                        if not member.filename.startswith('raw/'):
                            continue
                        output = destination / member.filename
                        output.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                        with archive.open(member) as incoming, output.open('xb') as outgoing:
                            os.chmod(outgoing.fileno(), 0o600)
                            shutil.copyfileobj(incoming, outgoing, length=1024 * 1024)
                            outgoing.flush()
                            os.fsync(outgoing.fileno())
                    connection.execute(queries.SET_VERSION)
                    connection.commit()
                finally:
                    connection.close()
                result = dict(status='restored', database_schema_version=1,
                              tables=audit['tables'], original_files=audit['original_files'],
                              account_status='disabled', interrupted_analyses=interrupted,
                              notes=['Set a new password and enable the account before use.',
                                     'Sessions, worker jobs, processing copies, exports and provider secrets are not restored.',
                                     'Source originals, record revisions and completed analyses retain their contents.'])
                receipt = destination / 'restore.json'
                with receipt.open('x') as stream:
                    os.chmod(stream.fileno(), 0o600)
                    json.dump(result, stream, indent=2)
                return result
            except BaseException:
                # The exclusive mkdir above proves that this invocation owns this directory.
                shutil.rmtree(destination)
                raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('archive', type=Path)
    parser.add_argument('--destination', required=True, type=Path)
    parser.add_argument('--username', required=True)
    parser.add_argument('--max-bytes', required=True, type=int)
    args = parser.parse_args()
    try:
        result = restore(args.archive, args.destination, args.username, args.max_bytes)
    except (OSError, ValueError, KeyError, TypeError, EOFError, csv.Error, sqlite3.Error,
            zipfile.BadZipFile, zlib.error, RuntimeError, NotImplementedError):
        print(json.dumps(dict(status='failed', error='restore_failed')), file=sys.stderr)
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
