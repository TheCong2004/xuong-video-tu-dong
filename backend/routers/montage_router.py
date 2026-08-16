from __future__ import annotations

import json

from fastapi import APIRouter, Depends, HTTPException, Request

from backend.adapters import openmontage_adapter
from backend.adapters.openmontage_adapter import (
    OpenMontageConflictError,
    OpenMontageComposeError,
    OpenMontageUnavailableError,
    OpenMontageValidationError,
)
from backend.services.local_access import require_local_request


router = APIRouter(
    prefix="/api/montage",
    tags=["media"],
    dependencies=[Depends(require_local_request)],
)


async def _call(awaitable):
    try:
        return await awaitable
    except OpenMontageUnavailableError as error:
        raise HTTPException(status_code=503, detail=str(error))
    except OpenMontageValidationError as error:
        raise HTTPException(status_code=422, detail=str(error))
    except OpenMontageConflictError as error:
        raise HTTPException(status_code=409, detail=str(error))
    except OpenMontageComposeError as error:
        raise HTTPException(
            status_code=500,
            detail={"code": error.code, "message": str(error)},
        )
    except HTTPException:
        raise


@router.get("/health")
async def montage_health():
    return await openmontage_adapter.health()


@router.get("/pipelines")
async def montage_pipelines():
    return await _call(openmontage_adapter.pipelines())


@router.get("/projects")
async def montage_projects():
    return await _call(openmontage_adapter.projects())


@router.post("/projects", status_code=201)
async def montage_create_project(request: Request):
    body = await request.body()
    if len(body) > 64 * 1024:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except (json.JSONDecodeError, UnicodeDecodeError):
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    project_id = payload.get("projectId")
    title = payload.get("title")
    pipeline_type = payload.get("pipelineType")
    if not all(isinstance(value, str) for value in (project_id, title, pipeline_type)):
        raise HTTPException(
            status_code=422,
            detail="projectId, title, and pipelineType are required",
        )
    return await _call(openmontage_adapter.create_project(project_id, title, pipeline_type))


@router.get("/projects/{project_id}")
async def montage_project_state(project_id: str):
    return await _call(openmontage_adapter.project_state(project_id))


@router.post("/timeline")
async def montage_timeline(request: Request):
    body = await request.body()
    if len(body) > 2 * 1024 * 1024:
        raise HTTPException(status_code=413, detail="Request body is too large")
    try:
        payload = json.loads(body) if body else {}
    except (json.JSONDecodeError, UnicodeDecodeError):
        raise HTTPException(status_code=400, detail="Invalid JSON body")
    if not isinstance(payload, dict):
        raise HTTPException(status_code=400, detail="JSON body must be an object")
    return await _call(openmontage_adapter.compose_floword_timeline(payload))
