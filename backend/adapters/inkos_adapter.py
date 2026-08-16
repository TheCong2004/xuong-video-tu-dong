from __future__ import annotations

import os
from typing import Any
from urllib.parse import quote, urlparse

import httpx


class InkOSUnavailableError(RuntimeError):
    pass


class InkOSUpstreamError(RuntimeError):
    def __init__(self, status_code: int, message: str = "InkOS request failed"):
        super().__init__(message)
        self.status_code = status_code


def base_url() -> str:
    value = os.environ.get("INKOS_BASE_URL", "http://127.0.0.1:4569").rstrip("/")
    parsed = urlparse(value)
    if parsed.scheme not in {"http", "https"}:
        raise RuntimeError("INKOS_BASE_URL must use http or https")
    if parsed.scheme == "http" and parsed.hostname not in {"127.0.0.1", "localhost", "::1"}:
        raise RuntimeError("Plain HTTP InkOS must use a loopback host")
    return value


async def _request(path: str, *, method: str = "GET", payload: dict[str, Any] | None = None):
    try:
        async with httpx.AsyncClient(timeout=60.0) as client:
            response = await client.request(method, f"{base_url()}{path}", json=payload)
    except (httpx.HTTPError, RuntimeError) as error:
        raise InkOSUnavailableError("InkOS is unavailable") from error
    if response.status_code >= 400:
        raise InkOSUpstreamError(response.status_code)
    try:
        result = response.json()
    except ValueError as error:
        raise InkOSUpstreamError(502, "InkOS returned invalid JSON") from error
    if not isinstance(result, dict):
        raise InkOSUpstreamError(502, "InkOS returned an invalid response")
    return result


async def health():
    try:
        result = await _request("/api/v1/books")
        return {"status": "ready", "service": "inkos", "books": len(result.get("books", []))}
    except (InkOSUnavailableError, InkOSUpstreamError):
        return {"status": "offline", "message": "InkOS is unavailable"}


async def genres():
    return await _request("/api/v1/genres")


async def projects():
    return await _request("/api/v1/books")


async def project(project_id: str):
    return await _request(f"/api/v1/books/{quote(project_id, safe='')}")


async def create_project(payload: dict[str, Any]):
    return await _request("/api/v1/books/create", method="POST", payload=payload)


async def create_status(project_id: str):
    return await _request(f"/api/v1/books/{quote(project_id, safe='')}/create-status")


async def plan_floword_story(payload: dict[str, Any]):
    return await _request("/api/v1/floword/story-plan", method="POST", payload=payload)
