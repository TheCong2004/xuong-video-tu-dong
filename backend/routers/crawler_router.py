from __future__ import annotations

import json

from fastapi import APIRouter, HTTPException, Query, Request
from pydantic import ValidationError

from backend.adapters import mediacrawler_adapter
from backend.adapters.mediacrawler_adapter import (
    MediaCrawlerAuthRequiredError,
    MediaCrawlerContractError,
    MediaCrawlerConflictError,
    MediaCrawlerUnavailableError,
)


router = APIRouter(prefix="/api/research", tags=["research"])
MAX_CRAWLER_BODY_BYTES = 256 * 1024


async def _payload(request: Request) -> dict:
    body = await request.body()
    if len(body) > MAX_CRAWLER_BODY_BYTES:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        value = json.loads(body) if body else {}
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(value, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    return value


async def _run(call):
    try:
        return await call()
    except ValidationError as error:
        raise HTTPException(status_code=422, detail=error.errors())
    except MediaCrawlerContractError as error:
        raise HTTPException(
            status_code=422,
            detail={"code": error.code, "message": str(error)},
        )
    except ValueError as error:
        raise HTTPException(status_code=422, detail=str(error))
    except MediaCrawlerConflictError as error:
        raise HTTPException(status_code=409, detail=str(error))
    except MediaCrawlerUnavailableError as error:
        raise HTTPException(
            status_code=503,
            detail={"code": error.code, "message": str(error)},
        )
    except MediaCrawlerAuthRequiredError as error:
        raise HTTPException(
            status_code=401,
            detail={"code": error.code, "message": str(error)},
        )


@router.get("/health")
async def crawler_health():
    return await mediacrawler_adapter.health()


@router.get("/platforms")
async def crawler_platforms():
    return await _run(mediacrawler_adapter.platforms)


@router.get("/session/status")
async def research_session_status(
    platform: str = Query(..., min_length=1, max_length=20),
    variant: str | None = Query(None, max_length=30),
):
    return await _run(lambda: mediacrawler_adapter.session_status(platform, variant))


@router.post("/session/login")
async def research_session_login(request: Request):
    payload = await _payload(request)
    return await _run(lambda: mediacrawler_adapter.session_login(payload))


@router.post("/session/verify")
async def research_session_verify(request: Request):
    payload = await _payload(request)
    platform = str(payload.get("platform", ""))
    auth_method = payload.get("auth_method")
    variant = payload.get("variant") or payload.get("xhs_variant")
    return await _run(lambda: mediacrawler_adapter.session_verify(
        platform,
        str(auth_method) if auth_method else None,
        str(variant) if variant else None,
    ))


@router.post("/session/reconnect")
async def research_session_reconnect(request: Request):
    payload = await _payload(request)
    platform = str(payload.get("platform", ""))
    variant = payload.get("variant") or payload.get("xhs_variant")
    return await _run(lambda: mediacrawler_adapter.session_reconnect(
        platform,
        str(variant) if variant else None,
    ))


@router.post("/session/clear")
async def research_session_clear(request: Request):
    payload = await _payload(request)
    platform = str(payload.get("platform", ""))
    variant = payload.get("variant") or payload.get("xhs_variant")
    return await _run(lambda: mediacrawler_adapter.session_clear(
        platform,
        str(variant) if variant else None,
    ))


@router.post("/operation")
async def research_operation(request: Request):
    payload = await _payload(request)
    return await _run(lambda: mediacrawler_adapter.research_operation(payload))


@router.post("/crawler/start")
async def crawler_start(request: Request):
    payload = await _payload(request)
    return await _run(lambda: mediacrawler_adapter.start(payload))


@router.get("/crawler/status")
async def crawler_status():
    return await _run(mediacrawler_adapter.status)


@router.get("/crawler/logs")
async def crawler_logs(limit: int = Query(default=100, ge=1, le=500)):
    return await _run(lambda: mediacrawler_adapter.logs(limit))


@router.post("/crawler/stop")
async def crawler_stop():
    return await _run(mediacrawler_adapter.stop)


@router.get("/results")
async def crawler_results(platform: str | None = None, file_type: str | None = None):
    return await _run(lambda: mediacrawler_adapter.results(platform, file_type))


@router.get("/results/{file_path:path}")
async def crawler_result(file_path: str, limit: int = Query(default=100, ge=1, le=1000)):
    return await _run(lambda: mediacrawler_adapter.result(file_path, limit))
