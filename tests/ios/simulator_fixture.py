"""Synthetic API fixture for iOS UI tests; never uses real patient data."""
import base64
import io
import json
import zipfile
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

exports = []
reports = []
upload_ids = []
fail_upload = False
observations = []
context = {}
analysis_enabled = False
runs = []
relation = {}
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_POST(self):
        global fail_upload, context, analysis_enabled, relation
        length = int(self.headers.get('Content-Length', 0))
        raw = self.rfile.read(length)
        body = json.loads(raw) if self.headers.get('Content-Type') == 'application/json' else {}
        route = self.path.removeprefix('/api/v1/')
        status = 200
        if route == 'health/sync' and self.headers.get('Authorization') == 'Bearer reject-sync':
            status = 503
        if route == 'test/reset':
            exports.clear(); reports.clear(); upload_ids.clear(); observations.clear(); context = {}; runs.clear(); relation = {}; analysis_enabled = False; fail_upload = False
        if route == 'test/seed-report':
            reports[:] = [{'report_id': 'fixture-report', 'original_name': 'fixture.png', 'page_count': 1, 'revision': 1, 'created_at': 1790985600}]
            observations[:] = [{'observation_id': 'fixture-glucose', 'report_id': 'fixture-report', 'revision': 1, 'status': 'confirmed',
                'payload': {'metric_id': 'glucose', 'raw_name': 'Glucose', 'raw_result': '5.5', 'raw_unit': 'mmol/L', 'sampled_at': '2026-10-01', 'source': {'page': 1, 'quote': 'Glucose 5.5 mmol/L'}}}]
        if route == 'reports/review':
            if body.get('context') is not None: context = body['context']
            for change in body.get('observations', []):
                old = next((item for item in observations if item['observation_id'] == change.get('observation_id')), None)
                revised = {'observation_id': change.get('observation_id') or 'fixture-added', 'report_id': 'fixture-report', 'revision': (old or {}).get('revision', 0) + 1, 'status': change['status'], 'payload': change['payload']}
                if old: observations.remove(old)
                observations.append(revised)
            reports[0]['revision'] += 1
        if route == 'test/enable-workflows':
            analysis_enabled = True
            reports.append({'report_id': 'other-report', 'original_name': 'other.png', 'page_count': 1, 'revision': 1, 'created_at': 1790985601})
        if route == 'reports/relate':
            relation = {'kind': body.get('kind'), 'preferred_report_id': body.get('preferred_report_id')}
        if route == 'exports/delete':
            exports.clear()
        if route == 'analysis/create':
            runs[:] = [{'run_id': 'fixture-analysis', 'status': 'ready', 'created_at': 1790985600,
                'output': {'review': {'summary': 'Synthetic lipid review', 'findings': []}, 'evidence': []},
                'input': {'observations': observations.copy()}, 'feedback': []}]
        if route == 'analysis/feedback':
            runs[0]['feedback'].append({'feedback_id': 'fixture-feedback', 'note': body['note']})
        if route == 'test/fail-upload':
            fail_upload = True
        if route == 'test/delete-report':
            reports.clear()
        if route == 'files/upload':
            upload_ids.append(self.headers.get('X-Upload-Id'))
            if fail_upload:
                fail_upload = False
                status = 503
            elif not reports:
                reports.append({'report_id': 'fixture-report', 'original_name': 'fixture.png', 'page_count': 1, 'revision': 1, 'created_at': 1790985600})
        if route == 'reports/get' and not reports:
            status = 404
        response = {
            'session/login': {'token': 'synthetic-token', 'user': {'user_id': 'simulator-only', 'username': 'simulator'}},
            'server/status': {'capabilities': {'analysis': analysis_enabled}},
            'reports/list': {'reports': reports}, 'jobs/list': {'jobs': []},
            'metrics/list': {'metrics': [{'metric_id': 'ldl_cholesterol', 'name': 'LDL'}, {'metric_id': 'hdl_cholesterol', 'name': 'HDL'}, {'metric_id': 'glucose', 'name': 'Glucose'}]},
            'trends/get': {'points': [{'observation_id': 'trend-' + str(index), 'report_id': 'fixture-report', 'sampled_at': date, 'timestamp': 1790812800 + index * 86400, 'value': 100 + index * 10, 'unit': 'mg/dL', 'revision': 1, 'original': {'raw_name': 'Synthetic LDL', 'raw_result': str(100 + index * 10), 'raw_unit': 'mg/dL', 'source': {'page': 1}}} for index, date in enumerate(['2026-10-01', '2026-10-02'])] if analysis_enabled else [], 'incomparable_count': 0},
            'analysis/list': {'runs': runs}, 'health/coverage': {'coverage': []},
            'wellness/sleep/regularity': {'sources': [], 'notes': []},
            'wellness/import/list': {'imports': [], 'next_after_id': None},
            'wellness/timeline': {'events': [], 'next_cursor': None},
            'wellness/review': {'days': [], 'measurements': [], 'strength_bests': [], 'notes': []},
            'wellness/series': {'sources': [], 'notes': []},
            'wellness/preferences/get': {'version': 0, 'preferences': {'source_priority': {}, 'favorite_metrics': [], 'sleep_target_minutes': None}},
            'wellness/day': {'date': body.get('date'), 'timezone': body.get('timezone'), 'metrics': [], 'computed_at': 1790985600},
            'wellness/sources': {'sources': []}, 'wellness/sleep': {'sources': [], 'algorithm_version': 'synthetic'}, 'wellness/entries/list': {'entries': []},
            'health/aggregate': {'days': []}, 'health/connect': {'connection_id': 'synthetic-connection'},
            'exports/list': {'exports': exports},
            'reports/get': {'report': reports[0] if reports else {}, 'observations': observations, 'context': context, 'relation': relation, 'pages': [{'page': 1, 'status': 'ready', 'content': 'Glucose 5.5 mmol/L'}], 'extraction_inputs': [{'page': 1, 'text_status': 'available', 'run_id': 'fixture-ocr'}], 'extraction_outputs': [{'page': 1, 'stage': 'ocr', 'run_id': 'fixture-ocr', 'model': 'synthetic'}]},
            'reports/review': {'report': reports[0] if reports else {}, 'observations': observations, 'context': context, 'relation': relation, 'pages': [{'page': 1, 'status': 'ready', 'content': 'Glucose 5.5 mmol/L'}], 'extraction_inputs': [{'page': 1, 'text_status': 'available', 'run_id': 'fixture-ocr'}], 'extraction_outputs': [{'page': 1, 'stage': 'ocr', 'run_id': 'fixture-ocr', 'model': 'synthetic'}]},
            'observations/history': {'history': observations},
            'analysis/get': runs[0] if runs else {},
            'reports/input/get': {'text_layer': {'status': 'available', 'text': 'Synthetic page text', 'words': []}},
            'reports/extraction/get': {'content': 'Synthetic OCR output', 'response_body': '{"synthetic": true}'},
            'test/state': {'upload_ids': upload_ids},
        }.get(route, {})
        if route == 'exports/create':
            exports.append({'export_id': 'synthetic-export', 'status': 'ready', 'created_at': 1790985600})
        data = json.dumps(response).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)
    def do_GET(self):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, 'w') as archive:
            archive.writestr('fixture.txt', 'Synthetic simulator test export')
        data = buffer.getvalue()
        if '/files/' in self.path:
            data = base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aX1sAAAAASUVORK5CYII=')
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

if __name__ == '__main__':
    ThreadingHTTPServer(('127.0.0.1', 18765), Handler).serve_forever()
