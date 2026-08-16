from __future__ import annotations

import json

from fastapi import APIRouter, Depends, HTTPException, Request

from backend.adapters import youwee_adapter
from backend.adapters.youwee_adapter import YouweeExecutionError, YouweeUnavailableError
from backend.services.local_access import require_local_request


router = APIRouter(
    prefix="/api/media/youtube",
    tags=["media"],
    dependencies=[Depends(require_local_request)],
)


async def _run(call):
    try:
        return await call()
    except ValueError as error:
        raise HTTPException(status_code=422, detail=str(error))
    except YouweeUnavailableError as error:
        raise HTTPException(status_code=503, detail=str(error))
    except YouweeExecutionError as error:
        raise HTTPException(status_code=502, detail=str(error))


@router.get("/health")
async def youwee_health():
    return await youwee_adapter.health()


@router.post("/search")
async def youwee_search(request: Request):
    body = await request.body()
    if len(body) > 64 * 1024:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    query = payload.get("query")
    limit = payload.get("limit", 10)
    if not isinstance(query, str) or not isinstance(limit, int):
        raise HTTPException(status_code=422, detail="query must be a string and limit an integer")
    return await _run(lambda: youwee_adapter.search(query, limit))
