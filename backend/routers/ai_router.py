from __future__ import annotations

import json
from typing import Any, Awaitable, Callable

import httpx
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import JSONResponse

from backend.adapters import omniroute_adapter
from backend.clients.omniroute_client import OmniRouteUpstreamError, health
from backend.services.local_access import require_local_request


MAX_AI_BODY_BYTES = 1024 * 1024


router = APIRouter(
    prefix="/api/ai",
    tags=["ai"],
    dependencies=[Depends(require_local_request)],
)


async def read_json_body(request: Request) -> dict[str, Any]:
    content_length = request.headers.get("content-length")
    if content_length:
        try:
            if int(content_length) > MAX_AI_BODY_BYTES:
                raise HTTPException(status_code=413, detail="Request body is too large")
        except ValueError:
            raise HTTPException(status_code=400, detail="Invalid Content-Length")

    body = await request.body()
    if len(body) > MAX_AI_BODY_BYTES:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    return payload


def omniroute_error_response() -> JSONResponse:
    return JSONResponse(
        status_code=502,
        content={
            "error": {
                "code": "OMNIROUTE_UNAVAILABLE",
                "message": "OmniRoute is unavailable",
                "service": "omniroute",
            }
        },
    )


def upstream_error_response(error: OmniRouteUpstreamError) -> JSONResponse:
    status_code = error.status_code if 400 <= error.status_code < 500 else 502
    content = error.payload if isinstance(error.payload, dict) else {
        "error": {
            "code": "OMNIROUTE_UPSTREAM_ERROR",
            "message": "OmniRoute rejected the request",
            "service": "omniroute",
        }
    }
    return JSONResponse(status_code=status_code, content=content)


async def run_adapter(call: Callable[[], Awaitable[Any]]):
    try:
        return await call()
    except ValueError as error:
        return JSONResponse(
            status_code=400,
            content={
                "error": {
                    "code": "INVALID_REQUEST",
                    "message": str(error),
                    "service": "omniroute",
                }
            },
        )
    except OmniRouteUpstreamError as error:
        return upstream_error_response(error)
    except (httpx.HTTPError, OSError, RuntimeError):
        return omniroute_error_response()


@router.get("/health")
async def ai_health():
    return await health()


@router.get("/models")
async def ai_models():
    return await run_adapter(omniroute_adapter.models)


@router.post("/chat")
async def ai_chat(request: Request):
    payload = await read_json_body(request)
    return await run_adapter(lambda: omniroute_adapter.chat(payload))


@router.get("/providers")
async def ai_providers():
    return await run_adapter(omniroute_adapter.providers)


@router.post("/providers")
async def ai_create_provider(request: Request):
    payload = await read_json_body(request)
    return await run_adapter(lambda: omniroute_adapter.create_provider(payload))


@router.get("/providers/{provider_id}")
async def ai_provider(provider_id: str):
    return await run_adapter(lambda: omniroute_adapter.provider(provider_id))


@router.put("/providers/{provider_id}")
async def ai_update_provider(provider_id: str, request: Request):
    payload = await read_json_body(request)
    return await run_adapter(
        lambda: omniroute_adapter.update_provider(provider_id, payload)
    )


@router.delete("/providers/{provider_id}")
async def ai_delete_provider(provider_id: str):
    return await run_adapter(lambda: omniroute_adapter.delete_provider(provider_id))


@router.post("/providers/{provider_id}/test")
async def ai_test_provider(provider_id: str, request: Request):
    payload = await read_json_body(request)
    return await run_adapter(
        lambda: omniroute_adapter.test_provider(provider_id, payload)
    )
