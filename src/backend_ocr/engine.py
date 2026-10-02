import asyncio
import os
import signal
import time
from contextlib import asynccontextmanager
from typing import Any, AsyncIterator

import httpx

from .config import Config
from .errors import OcrError


def engine_command(config: Config) -> list[str]:
    return [
        config.engine.binary,
        str(config.engine.path),
        '--model-id',
        config.engine.model,
        '--host',
        '127.0.0.1',
        '--port',
        str(config.engine.port),
        '--max-context',
        str(config.engine.context_length),
        '--kv-capacity',
        str(config.engine.context_length),
        '--max-concurrency',
        '1',
        '--kv-dtype',
        'fp8',
        '--device-state-slots',
        '0',
        '--host-state-slots',
        '1',
        '--host-kv-mib',
        '512',
        '--spec',
        'mtp',
        '--draft-tokens',
        '3',
        '--lm-head-draft',
        '--vision',
        '--media-preprocess-threads',
        '4',
        '--max-request-mib',
        '64',
    ]


class EngineRuntime:
    """Own serial admission, lazy native loading and process-group cleanup."""

    def __init__(self, config: Config, client: httpx.AsyncClient) -> None:
        self.config = config
        self.client = client
        self.process: asyncio.subprocess.Process | None = None
        self.state = 'unloaded'
        self.pending = 0
        self.last_finished = time.monotonic()
        self.lock = asyncio.Lock()

    @asynccontextmanager
    async def slot(self) -> AsyncIterator[None]:
        if self.pending >= self.config.server.maximum_pending_requests:
            raise OcrError(429, 'ocr_queue_full', {})
        self.pending += 1
        try:
            async with self.lock:
                yield
        finally:
            self.pending -= 1
            self.last_finished = time.monotonic()

    async def infer(self, request: dict[str, Any]) -> str:
        try:
            await self._ensure_ready()
            self.state = 'busy'
            async with self.client.stream(
                'POST', f'http://127.0.0.1:{self.config.engine.port}/v1/chat/completions', json=request
            ) as response:
                data = bytearray()
                async for chunk in response.aiter_bytes():
                    if len(data) + len(chunk) > 3 * 1024 * 1024:
                        raise OcrError(413, 'native_response_limit', {})
                    data.extend(chunk)
                body = data.decode('utf-8')
                if response.status_code != 200:
                    raise OcrError(
                        502,
                        'native_inference_failed',
                        {'model': self.config.engine.model, 'engine': 'ninfer', 'raw_response_body': body},
                    )
                return body
        except asyncio.CancelledError:
            await self.stop()
            raise
        except (httpx.HTTPError, OSError, UnicodeError, OcrError):
            await self.stop()
            raise
        finally:
            if self.process is not None:
                self.state = 'ready' if self.process.returncode is None else 'unloaded'

    async def stop(self) -> None:
        if self.process is not None:
            process = self.process
            self.state = 'stopping'
            if process.returncode is None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                try:
                    await asyncio.wait_for(process.wait(), 10)
                except TimeoutError:
                    try:
                        os.killpg(process.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    await process.wait()
            else:
                await process.wait()
            self.process = None
        self.state = 'unloaded'

    async def idle_watch(self) -> None:
        while True:
            await asyncio.sleep(min(1, self.config.server.idle_timeout_seconds))
            async with self.lock:
                if (
                    self.pending == 0
                    and time.monotonic() - self.last_finished >= self.config.server.idle_timeout_seconds
                ):
                    await self.stop()

    async def _ensure_ready(self) -> None:
        if self.process is not None and self.process.returncode is None:
            return
        await self.stop()
        self.state = 'loading'
        self.process = await asyncio.create_subprocess_exec(
            *engine_command(self.config),
            start_new_session=True,
            stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.DEVNULL,
        )
        while self.process.returncode is None:
            try:
                reply = await self.client.get(f'http://127.0.0.1:{self.config.engine.port}/health', timeout=2)
                if reply.status_code == 200:
                    self.state = 'ready'
                    return
            except httpx.TransportError:
                pass
            await asyncio.sleep(0.25)
        await self.stop()
        raise OcrError(503, 'native_engine_start_failed', {})
