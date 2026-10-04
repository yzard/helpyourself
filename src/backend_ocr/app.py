import asyncio
import hmac
import logging
import time
from contextlib import asynccontextmanager
from typing import AsyncIterator

import httpx
from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse
from starlette.types import ASGIApp, Receive, Scope, Send

from .config import Config
from .documents import ExtractionRequest, ExtractionResponse, parse_response, prepare_request
from .engine import EngineRuntime
from .errors import OcrError


class RequestGate:
    def __init__(self, app: ASGIApp, authorization: str, maximum_bytes: int) -> None:
        self.app = app
        self.authorization = authorization.encode()
        self.maximum_bytes = maximum_bytes

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope['type'] != 'http':
            await self.app(scope, receive, send)
            return
        headers = dict(scope['headers'])
        if scope['path'] != '/health' and not hmac.compare_digest(
            headers.get(b'authorization', b''), self.authorization
        ):
            await JSONResponse({'error': {'code': 'invalid_service_key'}}, status_code=401)(scope, receive, send)
            return
        if scope['method'] == 'POST':
            chunks: list[bytes] = []
            size = 0
            while True:
                message = await receive()
                if message['type'] == 'http.disconnect':
                    return
                chunk = message.get('body', b'')
                size += len(chunk)
                if size > self.maximum_bytes:
                    await JSONResponse({'error': {'code': 'request_limit'}}, status_code=413)(scope, receive, send)
                    return
                chunks.append(chunk)
                if not message.get('more_body', False):
                    break
            body = b''.join(chunks)
            supplied = False

            async def replay() -> dict:
                nonlocal supplied
                if not supplied:
                    supplied = True
                    return {'type': 'http.request', 'body': body, 'more_body': False}
                return await receive()

            await self.app(scope, replay, send)
        else:
            await self.app(scope, receive, send)


class AccessLog:
    def __init__(self, app: ASGIApp) -> None:
        self.app = app

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope['type'] != 'http':
            await self.app(scope, receive, send)
            return
        started = time.monotonic()
        status = 500

        async def record(message: dict) -> None:
            nonlocal status
            if message['type'] == 'http.response.start':
                status = message['status']
            await send(message)

        try:
            await self.app(scope, receive, record)
        finally:
            route = scope['path'] if scope['path'] in {'/health', '/api/v1/documents/extract'} else 'unmatched'
            method = scope['method'] if scope['method'] in {'GET', 'POST'} else 'OTHER'
            logging.getLogger(__name__).info(
                'method=%s route=%s status=%d duration_ms=%d',
                method,
                route,
                status,
                int((time.monotonic() - started) * 1000),
            )


async def execute(request: ExtractionRequest, config: Config, runtime: EngineRuntime) -> ExtractionResponse:
    try:
        async with asyncio.timeout(config.server.timeout_seconds):
            async with runtime.slot():
                prepared = await asyncio.to_thread(prepare_request, request, config)
                body = await runtime.infer(prepared)
                return await asyncio.to_thread(parse_response, body, config.engine.model, request.page)
    except TimeoutError as error:
        raise OcrError(504, 'ocr_timeout', {}) from error


async def watch_disconnect(request: Request, task: asyncio.Task) -> None:
    while not task.done():
        if await request.is_disconnected():
            task.cancel()
            return
        await asyncio.sleep(0.1)


def create_application(config: Config) -> FastAPI:
    key = config.server.api_key.get_secret_value()

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        async with httpx.AsyncClient(timeout=config.server.timeout_seconds, trust_env=False) as client:
            runtime = EngineRuntime(config, client)
            app.state.runtime = runtime
            monitor = asyncio.create_task(runtime.idle_watch())
            try:
                yield
            finally:
                monitor.cancel()
                await asyncio.gather(monitor, return_exceptions=True)
                async with runtime.lock:
                    await runtime.stop()

    app = FastAPI(lifespan=lifespan, docs_url=None, redoc_url=None, openapi_url=None)
    app.add_middleware(RequestGate, authorization='Bearer ' + key, maximum_bytes=config.server.maximum_request_bytes)
    app.add_middleware(AccessLog)

    @app.exception_handler(RequestValidationError)
    async def invalid_input(_request: Request, _error: RequestValidationError) -> JSONResponse:
        return JSONResponse({'error': {'code': 'invalid_request'}}, status_code=422)

    @app.exception_handler(OcrError)
    async def domain_error(_request: Request, error: OcrError) -> JSONResponse:
        return JSONResponse({**error.archive, 'error': {'code': error.code}}, status_code=error.status)

    @app.exception_handler(httpx.HTTPError)
    @app.exception_handler(OSError)
    @app.exception_handler(UnicodeError)
    async def engine_error(_request: Request, error: Exception) -> JSONResponse:
        logging.getLogger(__name__).warning('OCR engine unavailable error_type=%s', type(error).__name__)
        return JSONResponse({'error': {'code': 'engine_unavailable'}}, status_code=503)

    @app.exception_handler(Exception)
    async def unexpected_error(_request: Request, error: Exception) -> JSONResponse:
        logging.getLogger(__name__).error('OCR request failed error_type=%s', type(error).__name__)
        return JSONResponse({'error': {'code': 'internal_error'}}, status_code=500)

    @app.get('/health')
    async def health(request: Request) -> dict[str, str]:
        runtime: EngineRuntime = request.app.state.runtime
        state = runtime.state
        if runtime.process is not None and runtime.process.returncode is not None:
            state = 'unloaded'
        return {'status': 'ok', 'engine_state': state}

    @app.post('/api/v1/documents/extract', response_model=ExtractionResponse)
    async def extract(body: ExtractionRequest, request: Request) -> ExtractionResponse:
        task = asyncio.create_task(execute(body, config, request.app.state.runtime))
        monitor = asyncio.create_task(watch_disconnect(request, task))
        try:
            return await task
        finally:
            task.cancel()
            monitor.cancel()
            await asyncio.gather(task, monitor, return_exceptions=True)

    return app
