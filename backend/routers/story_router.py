from __future__ import annotations

import json

from fastapi import APIRouter, Depends, HTTPException, Request

from backend.adapters import inkos_adapter
from backend.adapters.inkos_adapter import InkOSUnavailableError, InkOSUpstreamError
from backend.services.local_access import require_local_request


router = APIRouter(
    prefix="/api/create/story",
    tags=["create"],
    dependencies=[Depends(require_local_request)],
)


async def _call(awaitable):
    try:
        return await awaitable
    except InkOSUnavailableError as error:
        raise HTTPException(status_code=503, detail=str(error))
    except InkOSUpstreamError as error:
        status = error.status_code if 400 <= error.status_code < 500 else 502
        raise HTTPException(status_code=status, detail=str(error))


@router.get("/health")
async def story_health():
    return await inkos_adapter.health()


@router.get("/genres")
async def story_genres():
    return await _call(inkos_adapter.genres())


@router.post("/plan")
async def story_plan(request: Request):
    body = await request.body()
    if len(body) > 2 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    return await _call(inkos_adapter.plan_floword_story(payload))


@router.get("/projects")
async def story_projects():
    return await _call(inkos_adapter.projects())


@router.get("/projects/{project_id}")
async def story_project(project_id: str):
    return await _call(inkos_adapter.project(project_id))


@router.get("/projects/{project_id}/create-status")
async def story_create_status(project_id: str):
    return await _call(inkos_adapter.create_status(project_id))


@router.post("/projects")
async def story_create_project(request: Request):
    body = await request.body()
    if len(body) > 64 * 1024:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except json.JSONDecodeError:
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    title = payload.get("title")
    genre = payload.get("genre")
    if not isinstance(title, str) or not title.strip():
        raise HTTPException(status_code=422, detail="title is required")
    if not isinstance(genre, str) or not genre.strip():
        raise HTTPException(status_code=422, detail="genre is required")
    allowed = {"title", "genre", "language", "platform", "chapterWordCount", "targetChapters", "blurb"}
    normalized = {key: value for key, value in payload.items() if key in allowed}
    return await _call(inkos_adapter.create_project(normalized))
