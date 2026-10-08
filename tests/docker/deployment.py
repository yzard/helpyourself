"""Isolated, synthetic Compose/Caddy integration check. Never uses production volumes."""

import argparse
import concurrent.futures
import io
import json
import os
import secrets
import shutil
import socket
import ssl
import struct
import subprocess
import threading
import time
import urllib.error
import urllib.request
import uuid
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'build' / 'deployment-check'
OUTPUT.mkdir(parents=True, exist_ok=True)
PROJECT = 'helpyourself-check-' + uuid.uuid4().hex[:8]
parser = argparse.ArgumentParser()
parser.add_argument('--ocr', action='store_true', help='Run actual Qwen3.8/NInfer GPU inference on a synthetic page')
parser.add_argument('--webgui', type=Path, help='Run browser checks with the given Playwright index.mjs module')
parser.add_argument(
    '--load',
    action='store_true',
    help='Measure concurrent synthetic 32/40 MiB Health sync, limits and server responsiveness',
)
options = parser.parse_args()


def free_port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]


http_port, https_port = free_port(), free_port()
environment = dict(
    os.environ,
    HELPYOURSELF_API_DATA_DIR=str(OUTPUT / (PROJECT + '-api')),
    HELPYOURSELF_OCR_DATA_DIR=str(OUTPUT / (PROJECT + '-ocr')),
    PUID=str(os.getuid()),
    PGID=str(os.getgid()),
    HELPYOURSELF_BIND_ADDRESS='127.0.0.1',
    HELPYOURSELF_DOMAIN='localhost',
    HELPYOURSELF_HTTP_PORT=str(http_port),
    HELPYOURSELF_HTTPS_PORT=str(https_port),
)
api_root = Path(environment['HELPYOURSELF_API_DATA_DIR'])
ocr_root = Path(environment['HELPYOURSELF_OCR_DATA_DIR'])
for root in (api_root, ocr_root):
    root.mkdir(mode=0o700)
    os.chmod(root, 0o700)
key = secrets.token_urlsafe(32)
(api_root / 'config.toml').write_text(
    (ROOT / 'docker/config.toml')
    .read_text()
    .replace(
        'api_key = "" # Set the shared OCR key here; playground initialization generates it.',
        'api_key = ' + json.dumps(key),
    )
)
(ocr_root / 'config.toml').write_text(
    (ROOT / 'src/backend_ocr/config.toml')
    .read_text()
    .replace('idle_timeout_seconds = 300', 'idle_timeout_seconds = 2')
    .replace(
        'api_key = "" # Set the shared OCR key here; playground initialization generates it.',
        'api_key = ' + json.dumps(key),
    )
)
for root in (api_root, ocr_root):
    os.chmod(root / 'config.toml', 0o600)
    assert not (root / 'ocr-key').exists()
compose = ['docker', 'compose', '-p', PROJECT, '-f', str(ROOT / 'docker/docker-compose.yaml')]


def command(args, **kwargs):
    return subprocess.run(
        args, cwd=ROOT, env=environment, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, **kwargs
    ).stdout


class TerminalJobError(RuntimeError):
    pass


def eventually(action, timeout_seconds):
    last = None
    deadline = time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        try:
            return action()
        except TerminalJobError:
            raise
        except Exception as error:
            last = error
            time.sleep(0.2)
    raise RuntimeError('Integration check timed out') from last


context = None
token = None
restored = None
pdf_report = None
scan_report = None
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


def completed_report(upload, label):
    job = api('jobs/get', {'job_id': upload['job']['job_id']})
    if job['status'] == 'failed':
        report_id = upload['file']['file_id']
        report = api('reports/get', {'report_id': report_id})
        diagnostic = {'synthetic_only': True, 'job': job, 'report': report, 'original_outputs': []}
        for output in report['extraction_outputs']:
            diagnostic['original_outputs'].append(
                api(
                    'reports/extraction/get',
                    {
                        'report_id': report_id,
                        'run_id': output['run_id'],
                        'page': output['page'],
                        'stage': output['stage'],
                    },
                )
            )
        (OUTPUT / (PROJECT + '-ocr-failure.json')).write_text(
            json.dumps(diagnostic, ensure_ascii=False, indent=2) + '\n'
        )
        raise TerminalJobError(label + ' failed: ' + str(job.get('error_code')))
    assert job['status'] == 'succeeded', job['status']
    return api('reports/get', {'report_id': upload['file']['file_id']})


def ocr_health():
    return json.loads(
        command(
            compose
            + [
                'exec',
                '-T',
                'backend_ocr',
                'python3',
                '-c',
                'import json, urllib.request; print(urllib.request.urlopen("http://127.0.0.1:8000/health").read().decode())',
            ]
        )
    )


def png():
    def chunk(kind, data):
        return struct.pack('!I', len(data)) + kind + data + struct.pack('!I', zlib.crc32(kind + data))

    return (
        b'\x89PNG\r\n\x1a\n'
        + chunk(b'IHDR', struct.pack('!IIBBBBB', 1, 1, 8, 2, 0, 0, 0))
        + chunk(b'IDAT', zlib.compress(b'\0\xff\xff\xff'))
        + chunk(b'IEND', b'')
    )


def text_pdf():
    content = b'BT /F1 32 Tf 60 540 Td (LABORATORY REPORT - SYNTHETIC TEST ONLY) Tj 0 -70 Td (Collection date: 2026-08-01) Tj 0 -120 Td (LDL Cholesterol    2.586    mmol/L    Reference <2.586) Tj ET'
    objects = [
        b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1400 600] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>',
        b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
        b'<< /Length ' + str(len(content)).encode() + b' >>\nstream\n' + content + b'\nendstream',
    ]
    return pdf_from_objects(objects)


def pdf_from_objects(objects):
    output = bytearray(b'%PDF-1.4\n')
    offsets = [0]
    for number, body in enumerate(objects, 1):
        offsets.append(len(output))
        output.extend(f'{number} 0 obj\n'.encode() + body + b'\nendobj\n')
    xref = len(output)
    output.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for offset in offsets[1:]:
        output.extend(f'{offset:010d} 00000 n \n'.encode())
    output.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode())
    return bytes(output)


def scanned_and_mixed_pdf():
    from PIL import Image, ImageDraw, ImageFont

    image = Image.new('RGB', (1400, 600), 'white')
    draw = ImageDraw.Draw(image)
    font = ImageFont.load_default(size=32)
    for position, text in [
        ((60, 40), 'SYNTHETIC LABORATORY TEST ONLY'),
        ((60, 110), 'Collection date: 2026-08-01'),
        ((60, 230), 'Test'),
        ((610, 230), 'Result'),
        ((820, 230), 'Unit'),
        ((1050, 230), 'Reference'),
        ((60, 310), 'Glucose'),
        ((610, 310), '5.551'),
        ((820, 310), 'mmol/L'),
        ((1050, 310), '<5.551'),
        ((60, 390), 'Apolipoprotein B'),
        ((610, 390), '0.9'),
        ((820, 390), 'g/L'),
        ((1050, 390), '<1.0'),
    ]:
        draw.text(position, text, font=font, fill='black')
    pixels = zlib.compress(image.tobytes())
    plain = b'q 1400 0 0 600 0 0 cm /Im1 Do Q'
    mixed = plain + b'\nBT /F1 14 Tf 60 20 Td (Synthetic footer only) Tj ET'
    resources = b' /Resources << /Font << /F1 3 0 R >> /XObject << /Im1 4 0 R >> >> '
    objects = [
        b'<< /Type /Catalog /Pages 2 0 R >>',
        b'<< /Type /Pages /Kids [6 0 R 8 0 R] /Count 2 >>',
        b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
        b'<< /Type /XObject /Subtype /Image /Width 1400 /Height 600 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length '
        + str(len(pixels)).encode()
        + b' >>\nstream\n'
        + pixels
        + b'\nendstream',
        b'<< /Length ' + str(len(plain)).encode() + b' >>\nstream\n' + plain + b'\nendstream',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1400 600]' + resources + b'/Contents 5 0 R >>',
        b'<< /Length ' + str(len(mixed)).encode() + b' >>\nstream\n' + mixed + b'\nendstream',
        b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1400 600]' + resources + b'/Contents 7 0 R >>',
    ]
    return pdf_from_objects(objects)


def health_load():
    connection = api('health/connect', {'platform': 'apple_health', 'installation_id': str(uuid.uuid4())})
    existing_raw_ids = {item['raw_id'] for item in api('health/raw/list', {'limit': 100})['files']}

    def record(identifier, character, payload_bytes):
        payload = {'blob': '', 'synthetic': True, 'printed_unit': 'mg/dL'}
        overhead = len(json.dumps(payload, separators=(',', ':')).encode())
        payload['blob'] = character * (payload_bytes - overhead)
        return {
            'record_id': identifier,
            'source_id': 'synthetic-load',
            'record_type': 'synthetic_raw',
            'start_at': 1,
            'end_at': 2,
            'version': 1,
            'deleted': False,
            'payload': payload,
        }

    def batch(records):
        return {
            'connection_id': connection['connection_id'],
            'batch_id': str(uuid.uuid4()),
            'record_type': 'synthetic_raw',
            'coverage_status': 'observed',
            'records': records,
        }

    def encoded(value):
        return json.dumps(value, separators=(',', ':')).encode()

    first = batch([record('32-mib', 'A', 32 * 1024 * 1024)])
    second = batch([record('40-mib-a', 'B', 20 * 1024 * 1024), record('40-mib-b', 'C', 20 * 1024 * 1024)])
    overflow = len(encoded(second)) - 40 * 1024 * 1024
    second['records'][1]['payload']['blob'] = second['records'][1]['payload']['blob'][:-overflow]
    bodies = [encoded(first), encoded(second)]
    assert len(encoded(first['records'][0]['payload'])) == 32 * 1024 * 1024
    assert len(bodies[1]) == 40 * 1024 * 1024
    initial_rss = rss_bytes()
    rss_samples = [initial_rss]
    latencies = []
    start = threading.Event()

    def sync(body):
        start.wait()
        return api('health/sync', payload=body)

    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
        workers = [executor.submit(sync, body) for body in bodies]
        start.set()
        while any(not worker.done() for worker in workers):
            began = time.monotonic()
            assert api('server/status')['capabilities']['review']
            latencies.append((time.monotonic() - began) * 1000)
            rss_samples.append(rss_bytes())
            time.sleep(0.05)
        for worker in workers:
            assert worker.result()['replayed'] is False
    assert latencies and max(latencies) < 1000, latencies
    # Lost response retry must retain the same digest and create no duplicate originals.
    assert api('health/sync', payload=bodies[0])['replayed'] is True
    all_originals = api('health/raw/list', {'limit': 100})['files']
    originals = [item for item in all_originals if item['raw_id'] not in existing_raw_ids]
    assert {item['raw_id'] for item in all_originals} >= existing_raw_ids
    assert len(originals) == 3
    expected = {row['record_id']: row for input_batch in [first, second] for row in input_batch['records']}
    for original in originals:
        assert json.loads(download(f"health/raw/{original['raw_id']}/download")) == expected[original['record_id']]
    invalid = batch([record('too-large', 'D', 32 * 1024 * 1024 + 1)])
    for body in [encoded(invalid), b' ' * (40 * 1024 * 1024 + 1)]:
        try:
            api('health/sync', payload=body)
            raise AssertionError('Oversize Health request was accepted')
        except urllib.error.HTTPError as error:
            assert error.code == 413
    assert {item['raw_id'] for item in api('health/raw/list', {'limit': 100})['files']} == {item['raw_id'] for item in all_originals}
    summary = {
        'synthetic_only': True,
        'concurrent_requests': 2,
        'request_body_bytes': [len(body) for body in bodies],
        'rss_before_mib': round(initial_rss / 1024 / 1024, 2),
        'rss_peak_during_sync_mib': round(max(rss_samples) / 1024 / 1024, 2),
        'status_samples': len(latencies),
        'status_latency_p95_ms': round(sorted(latencies)[min(len(latencies) - 1, int(len(latencies) * 0.95))], 2),
        'status_latency_max_ms': round(max(latencies), 2),
        'exact_payload_limit_and_batch_limit_accepted': True,
        'oversize_rejected_atomically': True,
        'raw_payload_and_replay_preserved': True,
    }
    (OUTPUT / 'load-result.json').write_text(json.dumps(summary, indent=2) + '\n')
    print(json.dumps({'health_load': summary}), flush=True)
    return summary


def rss_bytes():
    status = command(compose + ['exec', '-T', 'backend_api', 'cat', '/proc/1/status']).decode()
    return int(next(line.split()[1] for line in status.splitlines() if line.startswith('VmRSS:'))) * 1024


def assert_service_identity(service):
    status = command(compose + ['exec', '-T', service, 'cat', '/proc/1/status']).decode()
    for label, expected in [('Uid:', os.getuid()), ('Gid:', os.getgid())]:
        values = next(line.split()[1:] for line in status.splitlines() if line.startswith(label))
        assert values == [str(expected)] * 4


try:
    resolved = json.loads(command(compose + ['config', '--format', 'json']))
    for service, source, read_only in [('backend_api', api_root, False), ('backend_ocr', ocr_root, True)]:
        mounts = resolved['services'][service]['volumes']
        assert len(mounts) == 1
        assert mounts[0]['source'] == str(source) and mounts[0]['target'] == '/data'
        assert bool(mounts[0].get('read_only', False)) == read_only
    command(compose + ['up', '-d'])
    for service in ['backend_api', 'backend_ocr']:
        eventually(lambda: assert_service_identity(service), 30)
    command(compose + ['exec', '-T', 'backend_ocr', 'test', '!', '-e', '/data/database.sqlite'])
    caddy = command(compose + ['ps', '-q', 'caddy']).decode().strip()
    ca = OUTPUT / 'local-test-ca.crt'
    eventually(lambda: command(['docker', 'cp', f'{caddy}:/data/caddy/pki/authorities/local/root.crt', str(ca)]), 60)
    context = ssl.create_default_context(cafile=ca)
    assert eventually(lambda: api('server/status'), 60)['capabilities']['review']
    for path, content_type in [
        ('/', 'text/html'),
        ('/assets/app.mjs', 'text/javascript'),
        ('/assets/app.css', 'text/css'),
    ]:
        with urllib.request.urlopen(f'https://localhost:{https_port}{path}', context=context, timeout=10) as response:
            assert response.headers['Content-Type'].startswith(content_type)
            assert response.headers['Cache-Control'] == 'no-store'
            assert "connect-src 'self'" in response.headers['Content-Security-Policy']
            assert response.read()
    assert eventually(ocr_health, 60)['engine_state'] == 'unloaded'
    password = secrets.token_urlsafe(24)
    command(
        compose
        + [
            'exec',
            '-T',
            '--user',
            f'{os.getuid()}:{os.getgid()}',
            'backend_api',
            '/app/helpyourself',
            '--data-dir',
            '/data',
            'create-user',
            '--username',
            'synthetic',
            '--password-stdin',
        ],
        input=(password + '\n').encode(),
    )
    token = api('session/login', {'username': 'synthetic', 'password': password})['token']
    if options.ocr:
        from PIL import Image, ImageDraw, ImageFont

        image = Image.new('RGB', (1400, 600), 'white')
        draw = ImageDraw.Draw(image)
        font = ImageFont.load_default(size=32)
        for position, text in [
            ((60, 40), 'LABORATORY REPORT - SYNTHETIC TEST ONLY'),
            ((60, 110), 'Collection date: 2026-08-01'),
            ((60, 230), 'Test'),
            ((610, 230), 'Result'),
            ((820, 230), 'Unit'),
            ((1050, 230), 'Reference'),
            ((60, 310), 'LDL Cholesterol'),
            ((610, 310), '120'),
            ((820, 310), 'mg/dL'),
            ((1050, 310), '<100'),
        ]:
            draw.text(position, text, font=font, fill='black')
        output = io.BytesIO()
        image.save(output, format='PNG')
        raw = output.getvalue()
    else:
        # OCR is mandatory. Lightweight checks exercise a real outage without loading the GPU.
        command(compose + ['stop', 'backend_ocr'])
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
    assert (api_root / 'database.sqlite').stat().st_uid == os.getuid()
    assert (api_root / raw_path).stat().st_uid == os.getuid()
    current = api('reports/get', {'report_id': report})
    recognized = None
    if not options.ocr:

        def failed_during_outage():
            job = api('jobs/get', {'job_id': archive['job']['job_id']})
            assert job['status'] == 'failed', job['status']
            return job

        eventually(failed_during_outage, 60)
        current = api('reports/get', {'report_id': report})
    if options.ocr:

        def extracted():
            return completed_report(archive, 'Synthetic image inference')

        current = eventually(extracted, 660)
        assert len(current['observations']) == 1, current
        recognized = current['observations'][0]
        assert recognized['status'] == 'pending'
        assert recognized['payload']['raw_result'] == '120'
        assert recognized['payload']['raw_unit'] == 'mg/dL'
        assert recognized['payload']['metric_id'] is None
        metadata = current['extraction_outputs'][0]
        complete = api(
            'reports/extraction/get',
            {'report_id': report, 'run_id': metadata['run_id'], 'page': metadata['page'], 'stage': metadata['stage']},
        )
        original = json.loads(complete['response_body'])
        assert original['prompt_version'] == 'laboratory-page-v2'
        assert original['engine'] == 'ninfer'
        assert original['model'] == 'qwen3.8-27b-ninfer-nvfp4'
        assert json.loads(original['raw_response_body'])['choices'][0]['finish_reason'] == 'stop'

        def unloaded():
            health = ocr_health()
            assert health['engine_state'] == 'unloaded', health
            return health

        eventually(unloaded, 30)
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
            'expected_revision': current['report']['revision'],
            'observations': [
                {
                    'status': 'confirmed',
                    'payload': observation,
                    **(
                        {'observation_id': recognized['observation_id'], 'expected_revision': recognized['revision']}
                        if recognized
                        else {}
                    ),
                }
            ],
        },
    )
    assert api('trends/get', {'metric_ids': ['ldl_cholesterol']})['points'][0]['value'] == '120'
    if options.ocr:
        pdf = text_pdf()
        pdf_payload = (
            b'--check\r\nContent-Disposition: form-data; name="file"; filename="text.pdf"\r\nContent-Type: application/pdf\r\n\r\n'
            + pdf
            + b'\r\n--check--\r\n'
        )
        pdf_upload = api(
            'files/upload',
            payload=pdf_payload,
            headers={'Content-Type': 'multipart/form-data; boundary=check', 'X-Upload-Id': str(uuid.uuid4())},
        )
        pdf_report = pdf_upload['file']['file_id']

        def pdf_ready():
            return completed_report(pdf_upload, 'Synthetic text PDF inference')

        pdf_document = eventually(pdf_ready, 660)
        assert len(pdf_document['observations']) == 1
        pdf_observation = pdf_document['observations'][0]
        assert pdf_observation['payload']['raw_result'] == '2.586'
        assert pdf_observation['payload']['raw_unit'] == 'mmol/L'
        assert pdf_observation['status'] == 'pending'
        evidence_index = pdf_document['extraction_inputs'][0]
        assert evidence_index['text_status'] == 'available'
        evidence = api('reports/input/get', {'report_id': pdf_report, 'run_id': evidence_index['run_id'], 'page': 1})
        assert '2.586' in evidence['text_layer']['text']
        assert evidence['text_layer']['words']
        assert 'xMin' in evidence['raw_bbox_xml']
        pdf_observation['payload']['metric_id'] = 'ldl_cholesterol'
        api(
            'reports/review',
            {
                'report_id': pdf_report,
                'expected_revision': pdf_document['report']['revision'],
                'observations': [
                    {
                        'observation_id': pdf_observation['observation_id'],
                        'expected_revision': pdf_observation['revision'],
                        'status': 'confirmed',
                        'payload': pdf_observation['payload'],
                    }
                ],
            },
        )
        point = next(
            point
            for point in api('trends/get', {'metric_ids': ['ldl_cholesterol']})['points']
            if point['report_id'] == pdf_report
        )
        assert point['value'] == '100' and point['unit'] == 'mg/dL'
        assert point['original']['raw_result'] == '2.586'
        assert point['reference']['upper'] == '100'
        assert point['reference']['unit'] == 'mg/dL'

    if options.ocr:
        scanned = scanned_and_mixed_pdf()
        payload = (
            b'--check\r\nContent-Disposition: form-data; name="file"; filename="scanned-mixed.pdf"\r\nContent-Type: application/pdf\r\n\r\n'
            + scanned
            + b'\r\n--check--\r\n'
        )
        scan_upload = api(
            'files/upload',
            payload=payload,
            headers={'Content-Type': 'multipart/form-data; boundary=check', 'X-Upload-Id': str(uuid.uuid4())},
        )
        scan_report = scan_upload['file']['file_id']

        def scan_ready():
            return completed_report(scan_upload, 'Synthetic scanned/mixed inference')

        scanned_document = eventually(scan_ready, 660)
        assert len(scanned_document['observations']) == 4, scanned_document['observations']
        assert {row['page']: row['text_status'] for row in scanned_document['extraction_inputs']} == {
            1: 'empty',
            2: 'available',
        }
        expected = {'Glucose': ('5.551', 'mmol/L', 'glucose', '100'), 'Apolipoprotein B': ('0.9', 'g/L', 'apob', '90')}
        review = []
        for page in [1, 2]:
            rows = [row for row in scanned_document['observations'] if row['payload']['source']['page'] == page]
            assert {row['payload']['raw_name'] for row in rows} == set(expected)
            for row in rows:
                result, unit, metric, _ = expected[row['payload']['raw_name']]
                assert row['status'] == 'pending' and row['payload']['metric_id'] is None
                assert row['payload']['raw_result'] == result and row['payload']['raw_unit'] == unit
                row['payload']['metric_id'] = metric
                review.append(
                    {
                        'observation_id': row['observation_id'],
                        'expected_revision': row['revision'],
                        'status': 'confirmed',
                        'payload': row['payload'],
                    }
                )
        api(
            'reports/review',
            {
                'report_id': scan_report,
                'expected_revision': scanned_document['report']['revision'],
                'observations': review,
            },
        )
        points = [
            point
            for point in api('trends/get', {'metric_ids': ['glucose', 'apob']})['points']
            if point['report_id'] == scan_report
        ]
        assert len(points) == 4
        for point in points:
            assert point['value'] == expected[point['original']['raw_name']][3]
            assert point['unit'] == 'mg/dL' and point['reference']['upper'] == '100'
        assert download(f'files/{scan_report}/download') == scanned

    if options.webgui:
        connection = api('health/connect', {'platform': 'apple_health', 'installation_id': str(uuid.uuid4())})
        timestamp = int(time.time())
        api(
            'health/sync',
            {
                'connection_id': connection['connection_id'],
                'batch_id': str(uuid.uuid4()),
                'record_type': 'resting_heart_rate',
                'coverage_status': 'observed',
                'records': [
                    {
                        'record_id': 'browser-sample',
                        'source_id': 'synthetic-watch',
                        'record_type': 'resting_heart_rate',
                        'start_at': timestamp,
                        'end_at': timestamp,
                        'version': 1,
                        'deleted': False,
                        'payload': {'value': 58, 'unit': 'count/min', 'metadata': {'synthetic': True}},
                    }
                ],
            },
        )
        for kind, records in [
            ('sleep', [dict(record_id='browser-sleep', source_id='synthetic-watch', record_type='sleep',
                            start_at=timestamp - 7200, end_at=timestamp - 3600, version=1, deleted=False,
                            payload=dict(category=1, metadata=dict(synthetic=True)))]),
            ('heart_rate', [dict(record_id=f'browser-heart-{index}', source_id='synthetic-watch', record_type='heart_rate',
                                 start_at=timestamp - 120 + index * 10, end_at=timestamp - 120 + index * 10,
                                 version=1, deleted=False, payload=dict(value=100 + index * 10, unit='count/min'))
                            for index in range(7)]),
        ]:
            api('health/sync', dict(connection_id=connection['connection_id'], batch_id=str(uuid.uuid4()),
                                    record_type=kind, coverage_status='observed', records=records))
        if not pdf_report:
            pdf_payload = (
                b'--check\r\nContent-Disposition: form-data; name="file"; filename="text.pdf"\r\nContent-Type: application/pdf\r\n\r\n'
                + text_pdf()
                + b'\r\n--check--\r\n'
            )
            pdf_report = api(
                'files/upload',
                payload=pdf_payload,
                headers={'Content-Type': 'multipart/form-data; boundary=check', 'X-Upload-Id': str(uuid.uuid4())},
            )['file']['file_id']
        browser_env = dict(
            environment,
            WEBGUI_TEST_ORIGIN=f'https://localhost:{https_port}',
            WEBGUI_TEST_USERNAME='synthetic',
            WEBGUI_TEST_PASSWORD=password,
            WEBGUI_TEST_REPORT=report,
            WEBGUI_TEST_PDF=pdf_report,
        )
        subprocess.run(
            ['node', str(ROOT / 'tests/frontend/e2e/browser.mjs'), str(options.webgui.resolve())],
            cwd=ROOT,
            env=browser_env,
            check=True,
        )

    load_result = health_load() if options.load else None
    export_id = api('exports/create')['export_id']

    def ready():
        row = next(row for row in api('exports/list')['exports'] if row['export_id'] == export_id)
        assert row['status'] == 'ready'
        return row

    eventually(ready, 60)
    with zipfile.ZipFile(io.BytesIO(download(f'exports/{export_id}/download'))) as archive:
        assert archive.read(raw_path) == raw
        assert 'sessions.jsonl' not in archive.namelist()
        assert 'observation_revisions.jsonl' in archive.namelist()
        assert json.loads(archive.read('manifest.json'))['version'] == 1
        if options.load:
            assert sum(json.loads(line)['platform'] == 'apple_health' for line in archive.read('health_revisions.jsonl').splitlines()) == 3 + 9 * bool(options.webgui)
            assert len([name for name in archive.namelist() if name.startswith('raw/apple_health/')]) == 3 + 9 * bool(
                options.webgui
            )
        if options.ocr:
            assert archive.read(pdf_upload['file']['relative_path']) == pdf
            assert b'2.586' in archive.read('extraction_inputs.jsonl')
            assert b'lab-units-v3' in archive.read('observations.csv')
            assert archive.read(scan_upload['file']['relative_path']) == scanned
    command(compose + ['restart', 'backend_api'])
    assert eventually(lambda: api('reports/get', {'report_id': report}), 60)['observations'][0]['status'] == 'confirmed'
    command(compose + ['stop', 'backend_api'])
    backend = command(compose + ['ps', '-a', '-q', 'backend_api']).decode().strip()
    snapshot.mkdir()
    command(['docker', 'cp', f'{backend}:/data/.', str(snapshot)])
    command(compose + ['start', 'backend_api'])
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
                'helpyourself-backend-api:local',
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

    assert eventually(restored_report, 60)['observations'][0]['payload']['raw_result'] == '120'
    command(['docker', 'rm', '-f', restored])
    restored = None
    shutil.rmtree(snapshot)
    eventually(lambda: api('server/status'), 60)
    api(
        'reports/delete',
        {'report_id': report, 'expected_revision': api('reports/get', {'report_id': report})['report']['revision']},
    )
    for extra_report in [pdf_report, scan_report]:
        if extra_report:
            api(
                'reports/delete',
                {
                    'report_id': extra_report,
                    'expected_revision': api('reports/get', {'report_id': extra_report})['report']['revision'],
                },
            )
    assert not api('reports/list', {'limit': 100})['reports']
    command(compose + ['stop', 'backend_ocr'])
    assert api('server/status')['capabilities']['review']
    assert api('server/status')['capabilities']['document_extraction']
    api('user/delete', {'confirmation': 'synthetic'})
    result = {
        'https_with_trusted_test_ca': True,
        'separate_data_roots_fixed_config_and_nonroot_identity': True,
        'embedded_webgui_assets': True,
        'webgui_browser_checks': bool(options.webgui),
        'login_upload_review_trend_original_export': True,
        'restart_persistence': True,
        'stopped_copy_restores_complete_archive': True,
        'report_and_account_deletion': True,
        'image': 'helpyourself-backend-api:local',
        'synthetic_only': True,
        'health_load_and_limit_checks': load_result,
        'real_qwen3_8_ninfer_inference': options.ocr,
        'real_pdf_text_and_unit_reference_conversion': options.ocr,
        'real_scanned_mixed_pdf_multi_unit_extraction': options.ocr,
        'ocr_lazy_load_idle_unload_api_survives_ocr_stop': True,
    }
    (OUTPUT / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
finally:
    if restored:
        command(['docker', 'rm', '-f', restored])
    if snapshot.exists():
        shutil.rmtree(snapshot)
    command(compose + ['down', '-v'])
    for root in (api_root, ocr_root):
        shutil.rmtree(root)
