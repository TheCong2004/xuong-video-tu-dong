from __future__ import annotations

import json
import secrets
import threading
import time
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import FileResponse

from backend.services.local_access import require_local_request


router = APIRouter(
    prefix="/api/capcut",
    tags=["capcut"],
    dependencies=[Depends(require_local_request)],
)

_ASSET_TTL_SECONDS = 15 * 60
_assets: dict[str, tuple[str, Path, float]] = {}
_assets_lock = threading.Lock()


def register_asset(artifact_id: str, raw_path: str) -> str:
    if not artifact_id.strip():
        raise ValueError("artifactId is required")
    path = Path(raw_path).resolve()
    if not path.is_file() or path.stat().st_size <= 0:
        raise ValueError("artifact path must reference a non-empty file")
    token = secrets.token_urlsafe(32)
    now = time.monotonic()
    with _assets_lock:
        expired = [key for key, (_, _, deadline) in _assets.items() if deadline <= now]
        for key in expired:
            _assets.pop(key, None)
        _assets[token] = (artifact_id, path, now + _ASSET_TTL_SECONDS)
    return token


@router.post("/assets")
async def create_asset_transport(request: Request):
    body = await request.body()
    if len(body) > 64 * 1024:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except (json.JSONDecodeError, UnicodeDecodeError):
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    artifact_id = payload.get("artifactId")
    path = payload.get("path")
    if not isinstance(artifact_id, str) or not isinstance(path, str):
        raise HTTPException(status_code=422, detail="artifactId and path are required")
    try:
        token = register_asset(artifact_id, path)
    except ValueError as error:
        raise HTTPException(status_code=422, detail=str(error)) from error
    base = str(request.base_url).rstrip("/")
    return {"artifactId": artifact_id, "url": f"{base}/api/capcut/assets/{token}"}


@router.get("/assets/{token}")
async def read_asset_transport(token: str):
    with _assets_lock:
        entry = _assets.get(token)
    if entry is None or entry[2] <= time.monotonic():
        raise HTTPException(status_code=404, detail="Artifact transport token is unavailable")
    return FileResponse(entry[1])
