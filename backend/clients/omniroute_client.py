from __future__ import annotations

import httpx
import os
from typing import Any
from urllib.parse import urlparse


class OmniRouteUpstreamError(Exception):
    def __init__(self, status_code: int, payload: Any | None = None) -> None:
        super().__init__(f"OmniRoute returned HTTP {status_code}")
        self.status_code = status_code
        self.payload = payload


def base_url() -> str:
    url = os.environ.get(
        "OMNIROUTE_BASE_URL",
        os.environ.get("LLM_BASE_URL", "http://127.0.0.1:20128"),
    ).rstrip("/")
    parsed = urlparse(url)
    if parsed.scheme == "http" and parsed.hostname not in {"127.0.0.1", "localhost", "::1"}:
        raise RuntimeError("Plain HTTP OmniRoute URLs must use a loopback host")
    return url


def request_headers() -> dict[str, str]:
    # OmniRoute runs as a local sub-app on loopback and is not authenticated.
    return {
        "Content-Type": "application/json",
    }


def _response_payload(response: httpx.Response) -> Any | None:
    try:
        return response.json()
    except ValueError:
        return None


async def request_json(
    method: str,
    path: str,
    *,
    payload: dict[str, Any] | None = None,
    timeout: float = 30.0,
) -> Any:
    request_args: dict[str, Any] = {
        "headers": request_headers(),
    }
    if payload is not None:
        request_args["json"] = payload

    async with httpx.AsyncClient(timeout=timeout) as client:
        response = await client.request(
            method,
            f"{base_url()}{path}",
            **request_args,
        )

    body = _response_payload(response)
    if not response.is_success:
        raise OmniRouteUpstreamError(response.status_code, body)
    return body


async def get_models():
    return await request_json("GET", "/v1/models")


async def chat_completion(payload: dict):
    return await request_json(
        "POST", "/v1/chat/completions", payload=payload, timeout=120.0
    )


async def list_providers():
    return await request_json("GET", "/api/providers")


async def create_provider(payload: dict[str, Any]):
    return await request_json("POST", "/api/providers", payload=payload)


async def get_provider(provider_id: str):
    return await request_json("GET", f"/api/providers/{provider_id}")


async def update_provider(provider_id: str, payload: dict[str, Any]):
    return await request_json(
        "PUT", f"/api/providers/{provider_id}", payload=payload
    )


async def delete_provider(provider_id: str):
    return await request_json("DELETE", f"/api/providers/{provider_id}")


async def test_provider(provider_id: str, payload: dict[str, Any] | None = None):
    return await request_json(
        "POST",
        f"/api/providers/{provider_id}/test",
        payload=payload or {},
        timeout=60.0,
    )


async def health():
    try:
        async with httpx.AsyncClient(timeout=5.0) as client:
            response = await client.get(
                f"{base_url()}/api/health/ping",
                headers=request_headers(),
            )

        status = "ready" if response.is_success else "error"

        return {
            "status": status,
            "status_code": response.status_code,
        }

    except (httpx.HTTPError, OSError, RuntimeError):
        return {
            "status": "offline",
            "error": {
                "code": "OMNIROUTE_UNAVAILABLE",
                "message": "OmniRoute is unavailable",
                "service": "omniroute",
            },
        }
