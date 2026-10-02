import asyncio
import json
import os
import socket
import tempfile
import time
import unittest
from pathlib import Path

import httpx
from src.backend_ocr.app import execute
from src.backend_ocr.documents import ExtractionRequest
from src.backend_ocr.engine import EngineRuntime
from src.backend_ocr.errors import OcrError
from tests.backend_ocr.documents import ROOT, fixture, image_url


def available_port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        return listener.getsockname()[1]


class RuntimeTests(unittest.IsolatedAsyncioTestCase):
    async def test_request_deadline_includes_loading_and_reaps_timed_out_generation(self):
        with tempfile.TemporaryDirectory() as directory:
            config = fixture(directory)
            config.server.timeout_seconds = 1
            config.engine.binary = str(ROOT / 'tests/backend_ocr/fixtures/fake_engine.py')
            config.engine.port = available_port()
            config.engine.path = Path(directory) / 'model.json'
            config.engine.path.write_text(json.dumps({'delay': 30, 'response': {}}))
            async with httpx.AsyncClient(timeout=40, trust_env=False) as client:
                runtime = EngineRuntime(config, client)
                try:
                    with self.assertRaises(OcrError) as caught:
                        await execute(
                            ExtractionRequest(page=1, image_url=image_url(), text_layer=None), config, runtime
                        )
                    self.assertEqual(caught.exception.status, 504)
                    self.assertIsNone(runtime.process)
                    self.assertEqual(runtime.pending, 0)
                    self.assertEqual(runtime.state, 'unloaded')
                finally:
                    await runtime.stop()

    async def test_native_process_unloads_after_idle_and_health_poll_does_not_touch_deadline(self):
        with tempfile.TemporaryDirectory() as directory:
            config = fixture(directory)
            config.engine.binary = str(ROOT / 'tests/backend_ocr/fixtures/fake_engine.py')
            config.engine.port = available_port()
            config.engine.path = Path(directory) / 'model.json'
            config.engine.path.write_text(json.dumps({'delay': 0, 'response': {'synthetic': True}}))
            config.server.idle_timeout_seconds = 1
            async with httpx.AsyncClient(timeout=3, trust_env=False) as client:
                runtime = EngineRuntime(config, client)
                async with runtime.slot():
                    self.assertEqual(json.loads(await runtime.infer({})), {'synthetic': True})
                self.assertEqual(runtime.state, 'ready')
                pid = runtime.process.pid
                runtime.last_finished = time.monotonic() - 5
                monitor = asyncio.create_task(runtime.idle_watch())
                try:
                    await asyncio.sleep(1.2)
                    self.assertEqual(runtime.state, 'unloaded')
                    self.assertIsNone(runtime.process)
                    with self.assertRaises(ProcessLookupError):
                        os.kill(pid, 0)
                finally:
                    monitor.cancel()
                    await asyncio.gather(monitor, return_exceptions=True)
                    await runtime.stop()

    async def test_cancellation_reaps_native_generation_before_next_request(self):
        with tempfile.TemporaryDirectory() as directory:
            config = fixture(directory)
            config.engine.binary = str(ROOT / 'tests/backend_ocr/fixtures/fake_engine.py')
            config.engine.port = available_port()
            config.engine.path = Path(directory) / 'model.json'
            config.engine.path.write_text(json.dumps({'delay': 30, 'response': {}}))
            async with httpx.AsyncClient(timeout=40, trust_env=False) as client:
                runtime = EngineRuntime(config, client)

                async def run():
                    async with runtime.slot():
                        return await runtime.infer({})

                task = asyncio.create_task(run())
                try:
                    async with asyncio.timeout(5):
                        while runtime.state != 'busy':
                            await asyncio.sleep(0.02)
                    pid = runtime.process.pid
                    task.cancel()
                    with self.assertRaises(asyncio.CancelledError):
                        await task
                    self.assertEqual(runtime.state, 'unloaded')
                    self.assertEqual(runtime.pending, 0)
                    with self.assertRaises(ProcessLookupError):
                        os.kill(pid, 0)
                finally:
                    task.cancel()
                    await asyncio.gather(task, return_exceptions=True)
                    await runtime.stop()

    async def test_queue_is_bounded_before_decoding_images(self):
        with tempfile.TemporaryDirectory() as directory:
            config = fixture(directory)
            config.server.maximum_pending_requests = 1
            async with httpx.AsyncClient() as client:
                runtime = EngineRuntime(config, client)
                async with runtime.slot():
                    with self.assertRaises(OcrError) as caught:
                        async with runtime.slot():
                            self.fail('An extra request was admitted')
                    self.assertEqual(caught.exception.status, 429)
                self.assertEqual(runtime.pending, 0)
