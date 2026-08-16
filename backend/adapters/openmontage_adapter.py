from __future__ import annotations

import asyncio
import logging
from typing import Any


logger = logging.getLogger(__name__)


class OpenMontageUnavailableError(RuntimeError):
    pass


class OpenMontageValidationError(ValueError):
    pass


class OpenMontageConflictError(RuntimeError):
    pass


class OpenMontageComposeError(RuntimeError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def _runtime():
    try:
        from backlot.server import REPO_ROOT, _cached_summaries, _safe_project_dir
        from backlot.state import load_board_state
    except ImportError as error:
        raise OpenMontageUnavailableError("OpenMontage is unavailable") from error
    return REPO_ROOT, _cached_summaries, _safe_project_dir, load_board_state


def _create_runtime():
    try:
        from backlot.server import PROJECTS_DIR, PROJECT_ID_PATTERN, REPO_ROOT
        from backlot.state import summarize_project
        from lib.checkpoint import init_project
    except ImportError as error:
        raise OpenMontageUnavailableError("OpenMontage is unavailable") from error
    return REPO_ROOT, PROJECTS_DIR, PROJECT_ID_PATTERN, init_project, summarize_project


def _timeline_runtime():
    try:
        from lib.floword_timeline import FlowordTimelineError, build_floword_timeline
    except ImportError as error:
        raise OpenMontageUnavailableError("OpenMontage timeline core is unavailable") from error
    return FlowordTimelineError, build_floword_timeline


async def health() -> dict[str, Any]:
    try:
        _, summaries, _, _ = _runtime()
        projects = await asyncio.to_thread(summaries)
        return {"status": "ready", "service": "openmontage", "projects": len(projects)}
    except OpenMontageUnavailableError:
        return {"status": "offline", "message": "OpenMontage is unavailable"}


async def pipelines():
    root, _, _, _ = _runtime()
    pipeline_dir = root / "pipeline_defs"
    return {
        "pipelines": [
            {"value": path.stem, "label": path.stem.replace("-", " ").title()}
            for path in sorted(pipeline_dir.glob("*.yaml"))
        ]
    }


async def projects():
    _, summaries, _, _ = _runtime()
    return {"projects": await asyncio.to_thread(summaries)}


async def project_state(project_id: str):
    _, _, safe_project_dir, load_board_state = _runtime()
    project_dir = safe_project_dir(project_id)
    return await asyncio.to_thread(load_board_state, project_dir)


async def create_project(project_id: str, title: str, pipeline_type: str):
    root, projects_dir, project_id_pattern, init_project, summarize_project = _create_runtime()
    normalized_id = project_id.strip().lower()
    normalized_title = title.strip()
    normalized_pipeline = pipeline_type.strip()
    if not project_id_pattern.fullmatch(normalized_id):
        raise OpenMontageValidationError("project_id must use lowercase kebab-case")
    if not normalized_title:
        raise OpenMontageValidationError("title is required")
    manifest = root / "pipeline_defs" / f"{normalized_pipeline}.yaml"
    if not manifest.is_file():
        raise OpenMontageValidationError("unknown pipeline_type")
    project_dir = projects_dir / normalized_id
    if project_dir.exists():
        raise OpenMontageConflictError("project already exists")
    await asyncio.to_thread(
        init_project,
        normalized_id,
        title=normalized_title,
        pipeline_type=normalized_pipeline,
        pipeline_dir=projects_dir,
    )
    return await asyncio.to_thread(summarize_project, project_dir)


async def compose_floword_timeline(payload: dict[str, Any]):
    timeline_error, build_timeline = _timeline_runtime()
    try:
        return await asyncio.to_thread(build_timeline, payload)
    except timeline_error as error:
        raise OpenMontageValidationError(str(error)) from error
    except PermissionError as error:
        logger.exception("OpenMontage timeline output is not writable")
        raise OpenMontageComposeError(
            "OPENMONTAGE_COMPOSE_FAILED",
            "OpenMontage could not write timeline outputs",
        ) from error
    except OSError as error:
        logger.exception("OpenMontage timeline filesystem operation failed")
        raise OpenMontageComposeError(
            "OPENMONTAGE_COMPOSE_FAILED",
            "OpenMontage timeline filesystem operation failed",
        ) from error
    except Exception as error:
        logger.exception("OpenMontage timeline compose failed unexpectedly")
        raise OpenMontageComposeError(
            "OPENMONTAGE_INTERNAL_ERROR",
            "OpenMontage timeline compose failed",
        ) from error
