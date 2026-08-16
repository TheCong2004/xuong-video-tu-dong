from __future__ import annotations

import json

from fastapi import APIRouter, Depends, HTTPException, Request

from backend.adapters import vynaro_adapter
from backend.adapters.vynaro_adapter import VynaroExecutionError, VynaroUnavailableError
from backend.services.local_access import require_local_request


router = APIRouter(
    prefix="/api/video",
    tags=["video"],
    dependencies=[Depends(require_local_request)],
)


@router.get("/health")
async def vynaro_health():
    return await vynaro_adapter.health()


@router.post("/plans")
async def build_video_plans(request: Request):
    body = await request.body()
    if len(body) > 256 * 1024:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    try:
        return await vynaro_adapter.build_video_plans(payload)
    except VynaroUnavailableError as error:
        raise HTTPException(status_code=503, detail=str(error))
    except VynaroExecutionError as error:
        raise HTTPException(status_code=422, detail=str(error))


@router.post("/probe")
async def probe_video(request: Request):
    body = await request.body()
    if len(body) > 16 * 1024:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    path = payload.get("path") if isinstance(payload, dict) else None
    if not isinstance(path, str):
        raise HTTPException(status_code=422, detail="path must be a string")
    try:
        return await vynaro_adapter.probe_video(path)
    except ValueError as error:
        raise HTTPException(status_code=422, detail=str(error))
    except VynaroUnavailableError as error:
        raise HTTPException(status_code=503, detail=str(error))
    except VynaroExecutionError as error:
        raise HTTPException(status_code=422, detail=str(error))
