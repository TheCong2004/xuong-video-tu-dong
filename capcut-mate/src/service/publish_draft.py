from __future__ import annotations

import json
import os
import shutil
import time
import uuid
from pathlib import Path
from typing import Any, Dict, Iterable, Optional

from engines.local.structure_ops import default_draft_roots


class DraftPublishError(RuntimeError):
    def __init__(self, code: str, message: str):
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message


def _canonical(path: Path) -> Path:
    raw = str(path.expanduser().resolve())
    if os.name == "nt":
        if raw.startswith("\\\\?\\UNC\\"):
            raw = "\\\\" + raw[8:]
        elif raw.startswith("\\\\?\\"):
            raw = raw[4:]
        raw = os.path.normpath(raw)
    return Path(raw)


def resolve_capcut_desktop_draft_root() -> Path:
    configured = os.environ.get("CAPCUT_DESKTOP_DRAFT_ROOT", "").strip()
    if configured:
        root = _canonical(Path(os.path.expandvars(os.path.expanduser(configured))))
        if not root.is_dir():
            raise DraftPublishError(
                "CAPCUT_DESKTOP_ROOT_NOT_FOUND",
                f"Configured CapCut Desktop draft root does not exist: {root}",
            )
        return root

    resolved: list[Path] = []
    for candidate in default_draft_roots():
        if candidate.get("label") == "mate-output":
            continue
        path = Path(candidate["path"]).expanduser()
        if path.is_dir():
            canonical = _canonical(path)
            if canonical not in resolved:
                resolved.append(canonical)
    if not resolved:
        raise DraftPublishError(
            "CAPCUT_DESKTOP_ROOT_NOT_FOUND",
            "No installed CapCut Desktop draft root was found",
        )
    if len(resolved) > 1:
        choices = ", ".join(str(path) for path in resolved)
        raise DraftPublishError(
            "CAPCUT_DESKTOP_ROOT_NOT_FOUND",
            f"Multiple CapCut Desktop draft roots are active; configure CAPCUT_DESKTOP_DRAFT_ROOT: {choices}",
        )
    return resolved[0]


def _load_object(path: Path, code: str) -> Dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as exc:
        raise DraftPublishError(code, f"Invalid JSON file {path.name}: {exc}") from exc
    if not isinstance(value, dict):
        raise DraftPublishError(code, f"{path.name} must contain a JSON object")
    return value


def _is_relative_to(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
        return True
    except ValueError:
        return False


def _material_paths(content: Dict[str, Any]) -> Iterable[tuple[str, Path]]:
    materials = content.get("materials")
    if not isinstance(materials, dict):
        return
    for kind in ("videos", "audios"):
        items = materials.get(kind)
        if not isinstance(items, list):
            continue
        for item in items:
            if not isinstance(item, dict):
                continue
            raw = item.get("path")
            if isinstance(raw, str) and raw.strip():
                yield kind, Path(raw)


def _track_segment_count(content: Dict[str, Any], kind: str) -> int:
    total = 0
    tracks = content.get("tracks")
    if not isinstance(tracks, list):
        return 0
    for track in tracks:
        if not isinstance(track, dict) or track.get("type") != kind:
            continue
        segments = track.get("segments")
        if isinstance(segments, list):
            total += len(segments)
    return total


def _validate_draft(
    project: Path,
    *,
    reference_root: Optional[Path] = None,
    error_code: str,
) -> Dict[str, Any]:
    if not project.is_dir():
        raise DraftPublishError(error_code, f"Draft directory does not exist: {project}")
    required = ("draft_content.json", "draft_info.json", "draft_meta_info.json")
    for name in required:
        if not (project / name).is_file():
            raise DraftPublishError(error_code, f"Draft is missing required file: {name}")
    content = _load_object(project / "draft_content.json", error_code)
    info = _load_object(project / "draft_info.json", error_code)
    _load_object(project / "draft_meta_info.json", error_code)
    content_id = str(content.get("id") or "").strip()
    if not content_id or str(info.get("id") or "").strip() != content_id:
        raise DraftPublishError(error_code, "draft_content.json and draft_info.json project IDs do not match")

    ref_root = _canonical(reference_root or project)
    project_root = _canonical(project)
    media = {"video": 0, "audio": 0, "captions": _track_segment_count(content, "text")}
    for kind, raw_path in _material_paths(content):
        candidate = raw_path.expanduser()
        if not candidate.is_absolute():
            candidate = ref_root / candidate
        candidate = _canonical(candidate)
        if not _is_relative_to(candidate, ref_root):
            raise DraftPublishError(
                "CAPCUT_MEDIA_REFERENCE_INVALID",
                f"{kind} media is outside the published draft: {candidate}",
            )
        relative = candidate.relative_to(ref_root)
        actual = project_root / relative
        if not actual.is_file() or actual.stat().st_size == 0:
            raise DraftPublishError(
                "CAPCUT_MEDIA_REFERENCE_INVALID",
                f"{kind} media does not resolve: {actual}",
            )
        media["video" if kind == "videos" else "audio"] += 1

    if media["video"] == 0 or media["audio"] == 0 or media["captions"] == 0:
        raise DraftPublishError(
            "CAPCUT_MEDIA_REFERENCE_INVALID",
            "Draft must contain video, audio, and caption media",
        )
    return {"content": content, "media": media, "project_id": content_id}


def _rewrite_staging_paths(value: Any, staging: Path, final: Path) -> Any:
    if isinstance(value, dict):
        return {key: _rewrite_staging_paths(item, staging, final) for key, item in value.items()}
    if isinstance(value, list):
        return [_rewrite_staging_paths(item, staging, final) for item in value]
    if isinstance(value, str):
        try:
            path = Path(value)
            if path.is_absolute():
                canonical = _canonical(path)
                if _is_relative_to(canonical, staging):
                    return str(final / canonical.relative_to(staging))
        except (OSError, ValueError):
            pass
    return value


def _write_project_metadata(project: Path, desktop_root: Path, final: Path) -> Dict[str, Any]:
    content = _load_object(project / "draft_content.json", "CAPCUT_DRAFT_PUBLISH_FAILED")
    info = _load_object(project / "draft_info.json", "CAPCUT_DRAFT_PUBLISH_FAILED")
    content_id = str(content.get("id") or "").strip()
    name = str(content.get("name") or final.name).strip() or final.name
    duration = int(content.get("duration") or 0)
    now_us = int(time.time() * 1_000_000)
    meta_path = project / "draft_meta_info.json"
    meta = _load_object(meta_path, "CAPCUT_DRAFT_PUBLISH_FAILED")
    meta.update(
        {
            "draft_id": content_id,
            "draft_name": name,
            "draft_fold_path": str(final),
            "draft_json_file": str(final / "draft_content.json"),
            "draft_root_path": str(desktop_root),
            "draft_is_invisible": False,
            "streaming_edit_draft_ready": True,
            "tm_duration": duration,
            "tm_draft_modified": now_us,
        }
    )
    meta.setdefault("tm_draft_create", now_us)
    cover = final / "draft_cover.jpg"
    meta["draft_cover"] = str(cover) if (project / "draft_cover.jpg").is_file() else ""
    meta_path.write_text(json.dumps(meta, ensure_ascii=False, indent=2), encoding="utf-8")
    (project / "draft_content.json").write_text(json.dumps(content, ensure_ascii=False, indent=2), encoding="utf-8")
    (project / "draft_info.json").write_text(json.dumps(info, ensure_ascii=False, indent=2), encoding="utf-8")
    return meta


def _update_root_index(desktop_root: Path, meta: Dict[str, Any]) -> None:
    index_path = desktop_root / "root_meta_info.json"
    if index_path.is_file():
        index = _load_object(index_path, "CAPCUT_DRAFT_PUBLISH_FAILED")
    else:
        index = {"all_draft_store": [], "draft_ids": 0, "root_path": str(desktop_root)}
    entries = index.get("all_draft_store")
    if not isinstance(entries, list):
        entries = []
    draft_id = meta["draft_id"]
    final_path = meta["draft_fold_path"]
    entries = [
        entry
        for entry in entries
        if not isinstance(entry, dict)
        or (entry.get("draft_id") != draft_id and entry.get("draft_fold_path") != final_path)
    ]
    entries.append(meta)
    index["all_draft_store"] = entries
    index["root_path"] = str(desktop_root)
    old_count = index.get("draft_ids") if isinstance(index.get("draft_ids"), int) else 0
    index["draft_ids"] = max(old_count, len(entries))
    temporary = desktop_root / f".root_meta_info.{uuid.uuid4().hex}.tmp"
    temporary.write_text(json.dumps(index, ensure_ascii=False, indent=2), encoding="utf-8")
    os.replace(temporary, index_path)


def _file_count(path: Path) -> int:
    return sum(1 for item in path.rglob("*") if item.is_file())


def publish_draft(staging_path: str, draft_id: str) -> Dict[str, Any]:
    staging = _canonical(Path(staging_path))
    if not draft_id.strip() or staging.name != draft_id:
        raise DraftPublishError("CAPCUT_DRAFT_STAGE_FAILED", "Draft ID does not match the staging folder")
    staged = _validate_draft(staging, error_code="CAPCUT_DRAFT_STAGE_FAILED")
    desktop_root = resolve_capcut_desktop_draft_root()
    final = _canonical(desktop_root / draft_id)
    if final.parent != _canonical(desktop_root):
        raise DraftPublishError("CAPCUT_DRAFT_PUBLISH_FAILED", "Final draft path escapes the Desktop root")

    if final.exists():
        verified = _validate_draft(final, error_code="CAPCUT_DRAFT_VERIFY_FAILED")
        meta = _load_object(final / "draft_meta_info.json", "CAPCUT_DRAFT_VERIFY_FAILED")
        _update_root_index(desktop_root, meta)
        return {
            "draft_id": draft_id,
            "staging_path": str(staging),
            "desktop_root": str(desktop_root),
            "final_path": str(final),
            "media": verified["media"],
        }

    temporary = desktop_root / f".{draft_id}.publishing-{uuid.uuid4().hex}"
    try:
        shutil.copytree(staging, temporary)
        if _file_count(temporary) != _file_count(staging):
            raise DraftPublishError("CAPCUT_DRAFT_PUBLISH_FAILED", "Published file count does not match staging")
        for name in ("draft_content.json", "draft_info.json"):
            path = temporary / name
            document = _load_object(path, "CAPCUT_DRAFT_PUBLISH_FAILED")
            document = _rewrite_staging_paths(document, staging, final)
            path.write_text(json.dumps(document, ensure_ascii=False, indent=2), encoding="utf-8")
        meta = _write_project_metadata(temporary, desktop_root, final)
        _validate_draft(temporary, reference_root=final, error_code="CAPCUT_DRAFT_VERIFY_FAILED")
        os.replace(temporary, final)
        verified = _validate_draft(final, error_code="CAPCUT_DRAFT_VERIFY_FAILED")
        _update_root_index(desktop_root, meta)
    except DraftPublishError:
        if temporary.exists():
            shutil.rmtree(temporary, ignore_errors=True)
        raise
    except OSError as exc:
        if temporary.exists():
            shutil.rmtree(temporary, ignore_errors=True)
        raise DraftPublishError("CAPCUT_DRAFT_PUBLISH_FAILED", str(exc)) from exc

    return {
        "draft_id": draft_id,
        "staging_path": str(staging),
        "desktop_root": str(desktop_root),
        "final_path": str(final),
        "media": verified["media"],
    }
