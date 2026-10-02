#!/usr/bin/env python3
"""Synthetic subprocess for lifecycle tests; never used by runtime images."""

import argparse
import json
import time
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('model')
parser.add_argument('--port', type=int)
arguments, _ = parser.parse_known_args()
settings = json.loads(Path(arguments.model).read_text())


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'{}')

    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        time.sleep(settings['delay'])
        self.send_response(200)
        self.end_headers()
        self.wfile.write(json.dumps(settings['response']).encode())

    def log_message(self, *arguments):
        pass


HTTPServer(('127.0.0.1', arguments.port), Handler).serve_forever()
