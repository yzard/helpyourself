"""Isolated, synthetic Compose/Caddy integration check. Never uses production volumes."""

import io
import json
import os
import secrets
import shutil
import socket
import ssl
import struct
import subprocess
import time
import urllib.request
import uuid
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'build' / 'deployment-check'
OUTPUT.mkdir(parents=True, exist_ok=True)
PROJECT = 'helpyourself-check-' + uuid.uuid4().hex[:8]


def free_port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]


http_port, https_port = free_port(), free_port()
environment = dict(
    os.environ,
    HELPYOURSELF_CONFIG=str(ROOT / 'docker/config.toml'),
    HELPYOURSELF_DATA_DIR='helpyourself-data',
    HELPYOURSELF_BIND_ADDRESS='127.0.0.1',
    HELPYOURSELF_DOMAIN='localhost',
    HELPYOURSELF_HTTP_PORT=str(http_port),
    HELPYOURSELF_HTTPS_PORT=str(https_port),
)
compose = ['docker', 'compose', '-p', PROJECT, '-f', str(ROOT / 'docker/docker-compose.yaml')]


def command(args, **kwargs):
    return subprocess.run(
        args, cwd=ROOT, env=environment, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs
    ).stdout


def eventually(action):
    last = None
    for _ in range(100):
        try:
            return action()
        except Exception as error:
            last = error
            time.sleep(0.2)
    raise RuntimeError('Integration check timed out') from last


context = None
token = None
restored = None
snapshot = OUTPUT / (PROJECT + "-snapshot")


def api(path, body=None, *, payload=None, headers=None):
    data = json.dumps(body or {}).encode() if payload is None else payload
    request = urllib.request.Request(
        f'https://localhost:{https_port}/api/v1/{path}',
        data=data,
        headers={
            'Content-Type': 'application/json',
            **({'Authorization': 'Bearer ' + token} if token else {}),
            **(headers or {}),
        },
    )
    with urllib.request.urlopen(request, context=context, timeout=10) as response:
        assert response.headers['Cache-Control'] == 'no-store'
        return json.load(response)


def download(path):
    request = urllib.request.Request(
        f'https://localhost:{https_port}/api/v1/{path}', headers={'Authorization': 'Bearer ' + token}
    )
    with urllib.request.urlopen(request, context=context, timeout=10) as response:
        return response.read()


def png():
    def chunk(kind, data):
        return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))

    return (
        b'\x89PNG\r\n\x1a\n'
        + chunk(b'IHDR', struct.pack('!IIBBBBB', 1, 1, 8, 2, 0, 0, 0))
        + chunk(b'IDAT', zlib.compress(b'\0\xff\xff\xff'))
        + chunk(b'IEND', b'')
    )


try:
    command(compose + ['up', '-d'])
    caddy = command(compose + ['ps', '-q', 'caddy']).decode().strip()
    ca = OUTPUT / 'local-test-ca.crt'
    eventually(lambda: command(['docker', 'cp', f'{caddy}:/data/caddy/pki/authorities/local/root.crt', str(ca)]))
    context = ssl.create_default_context(cafile=ca)
    assert eventually(lambda: api('server/status'))['capabilities']['review']
    password = secrets.token_urlsafe(24)
    command(
        compose
        + [
            'exec',
            '-T',
            '--user',
            '1000:1000',
            'backend',
            '/app/helpyourself',
            '--config',
            '/config/config.toml',
            'create-user',
            '--username',
            'synthetic',
            '--password-stdin',
        ],
        input=(password + '\n').encode(),
    )
    token = api('session/login', {'username': 'synthetic', 'password': password})['token']
    raw = png()
    payload = (
        b'--check\r\nContent-Disposition: form-data; name="file"; filename="synthetic.png"\r\nContent-Type: image/png\r\n\r\n'
        + raw
        + b'\r\n--check--\r\n'
    )
    archive = api(
        'files/upload',
        payload=payload,
        headers={'Content-Type': 'multipart/form-data; boundary=check', 'X-Upload-Id': str(uuid.uuid4())},
    )
    report = archive['file']['file_id']
    raw_path = archive['file']['relative_path']
    assert download(f'files/{report}/download') == raw
    observation = {
        'raw_name': 'Synthetic LDL',
        'raw_result': '120',
        'raw_unit': 'mg/dL',
        'reference_range': '<100',
        'sampled_at': '2026-08-01',
        'metric_id': 'ldl_cholesterol',
        'source': {'page': 1, 'quote': 'Synthetic test only'},
    }
    api(
        'reports/review',
        {
            'report_id': report,
            'expected_revision': 1,
            'observations': [{'status': 'confirmed', 'payload': observation}],
        },
    )
    assert api('trends/get', {'metric_ids': ['ldl_cholesterol']})['points'][0]['value'] == '120'
    export_id = api('exports/create')['export_id']

    def ready():
        row = next(row for row in api('exports/list')['exports'] if row['export_id'] == export_id)
        assert row['status'] == 'ready'
        return row

    eventually(ready)
    with zipfile.ZipFile(io.BytesIO(download(f'exports/{export_id}/download'))) as archive:
        assert archive.read(raw_path) == raw
        assert 'sessions.jsonl' not in archive.namelist()
        assert 'observation_revisions.jsonl' in archive.namelist()
    command(compose + ['restart', 'backend'])
    assert eventually(lambda: api('reports/get', {'report_id': report}))['observations'][0]['status'] == 'confirmed'
    command(compose + ['stop', 'backend'])
    backend = command(compose + ['ps', '-a', '-q', 'backend']).decode().strip()
    snapshot.mkdir()
    command(['docker', 'cp', f'{backend}:/data/.', str(snapshot)])
    command(compose + ['start', 'backend'])
    restored_port = free_port()
    restored = (
        command(
            [
                'docker',
                'run',
                '-d',
                '--name',
                PROJECT + '-restore',
                '-p',
                f'127.0.0.1:{restored_port}:8080',
                '-v',
                f'{snapshot}:/data',
                '-v',
                f'{ROOT / "docker/config.toml"}:/config/config.toml:ro',
                'helpyourself:local',
            ]
        )
        .decode()
        .strip()
    )

    def restored_report():
        request = urllib.request.Request(
            f'http://127.0.0.1:{restored_port}/api/v1/reports/get',
            data=json.dumps({'report_id': report}).encode(),
            headers={'Content-Type': 'application/json', 'Authorization': 'Bearer ' + token},
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.load(response)

    assert eventually(restored_report)['observations'][0]['payload']['raw_result'] == '120'
    command(['docker', 'rm', '-f', restored])
    restored = None
    shutil.rmtree(snapshot)
    eventually(lambda: api('server/status'))
    api('reports/delete', {'report_id': report, 'expected_revision': 2})
    assert not api('reports/list', {'limit': 100})['reports']
    api('user/delete', {'confirmation': 'synthetic'})
    result = {
        'https_with_trusted_test_ca': True,
        'login_upload_review_trend_original_export': True,
        'restart_persistence': True,
        'stopped_copy_restores_complete_archive': True,
        'report_and_account_deletion': True,
        'image': 'helpyourself:local',
        'synthetic_only': True,
    }
    (OUTPUT / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
finally:
    if restored:
        command(['docker', 'rm', '-f', restored])
    if snapshot.exists():
        shutil.rmtree(snapshot)
    command(compose + ['down', '-v'])
