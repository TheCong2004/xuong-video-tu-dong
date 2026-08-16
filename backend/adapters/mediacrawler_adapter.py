from __future__ import annotations

import asyncio
import json
import os
import shutil
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


class MediaCrawlerUnavailableError(RuntimeError):
    def __init__(self, message: str, code: str = "MEDIACRAWLER_INTERNAL_ERROR"):
        super().__init__(message)
        self.code = code


class MediaCrawlerConflictError(RuntimeError):
    pass


class MediaCrawlerAuthRequiredError(RuntimeError):
    def __init__(self, message: str, code: str = "MEDIACRAWLER_AUTH_REQUIRED"):
        super().__init__(message)
        self.code = code


class MediaCrawlerContractError(ValueError):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def _runtime():
    try:
        from api.schemas import CrawlerStartRequest
        from api.services import crawler_manager

        return CrawlerStartRequest, crawler_manager
    except (ImportError, ModuleNotFoundError) as error:
        raise MediaCrawlerUnavailableError("MediaCrawler backend is unavailable") from error


async def health() -> dict[str, Any]:
    try:
        _, manager = _runtime()
        return {"status": "ready", "crawler": manager.get_status()}
    except MediaCrawlerUnavailableError:
        return {
            "status": "offline",
            "error": {
                "code": "MEDIACRAWLER_UNAVAILABLE",
                "message": "MediaCrawler backend is unavailable",
                "service": "mediacrawler",
            },
        }


async def platforms():
    try:
        from api.main import get_platforms
    except (ImportError, ModuleNotFoundError) as error:
        raise MediaCrawlerUnavailableError("MediaCrawler backend is unavailable") from error
    return await get_platforms()


async def start(payload: dict[str, Any]):
    request_type, manager = _runtime()
    request = request_type.model_validate(payload)
    if not await manager.start(request):
        if manager.process and manager.process.poll() is None:
            raise MediaCrawlerConflictError("Crawler is already running")
        raise MediaCrawlerUnavailableError("MediaCrawler failed to start")
    return manager.get_status()


async def status():
    _, manager = _runtime()
    return manager.get_status()


async def logs(limit: int = 100):
    _, manager = _runtime()
    bounded_limit = max(1, min(limit, 500))
    return {
        "logs": [entry.model_dump() for entry in manager.logs[-bounded_limit:]]
    }


async def stop():
    _, manager = _runtime()
    if not await manager.stop():
        raise MediaCrawlerConflictError("No crawler is running")
    return manager.get_status()


async def results(platform: str | None = None, file_type: str | None = None):
    try:
        from api.routers.data import list_data_files
    except (ImportError, ModuleNotFoundError) as error:
        raise MediaCrawlerUnavailableError("MediaCrawler backend is unavailable") from error
    return await list_data_files(platform=platform, file_type=file_type)


async def result(file_path: str, limit: int = 100):
    try:
        from api.routers.data import get_file_content
    except (ImportError, ModuleNotFoundError) as error:
        raise MediaCrawlerUnavailableError("MediaCrawler backend is unavailable") from error
    return await get_file_content(file_path=file_path, preview=True, limit=limit)


RESEARCH_PLATFORMS = {"xhs", "dy", "ks", "bili", "wb", "tieba", "zhihu"}
RESEARCH_MODES = {"search", "detail", "creator"}
SESSION_AUTH_METHODS = {"browser", "qrcode", "cookie"}


def _session_state_path(manager) -> Path:
    root = Path(getattr(manager, "_runtime_root", Path.cwd())).resolve()
    path = root / "browser_data" / "floword_sessions.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    return path


def _read_session_states(manager) -> dict[str, dict[str, Any]]:
    path = _session_state_path(manager)
    if not path.is_file():
        return {}
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
        return value if isinstance(value, dict) else {}
    except (OSError, UnicodeError, json.JSONDecodeError):
        return {}


def _write_session_states(manager, states: dict[str, dict[str, Any]]) -> None:
    path = _session_state_path(manager)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(json.dumps(states, ensure_ascii=False, indent=2), encoding="utf-8")
    temporary.replace(path)


def _normalize_variant(platform: str, variant: str | None) -> str:
    if platform == "xhs":
        if variant:
            v = str(variant).strip().lower()
            if v in ("international", "rednote", "global"):
                return "international"
            return "mainland"
        env_val = os.getenv("MEDIACRAWLER_XHS_INTERNATIONAL", "false").strip().lower()
        return "international" if env_val in {"1", "true", "yes", "on"} else "mainland"
    return ""


def _session_key(platform: str, variant: str | None = None) -> str:
    norm = _normalize_variant(platform, variant)
    if platform == "xhs" and norm == "international":
        return f"{platform}:{norm}"
    return platform


def _session_record(platform: str, auth_method: str, status: str, variant: str | None = None, **extra) -> dict[str, Any]:
    norm = _normalize_variant(platform, variant)
    profile_suffix = f":{norm}" if platform == "xhs" and norm == "international" else ""
    return {
        "platform": platform,
        "variant": norm if platform == "xhs" else None,
        "auth_method": auth_method,
        "profile_id": f"mediacrawler:{platform}{profile_suffix}",
        "status": status,
        "last_verified_at": extra.get("last_verified_at"),
        "error": extra.get("error"),
    }


def _save_session_record(manager, record: dict[str, Any], variant: str | None = None) -> dict[str, Any]:
    states = _read_session_states(manager)
    key = _session_key(record["platform"], variant or record.get("variant"))
    states[key] = record
    if record["platform"] == "xhs" and (variant == "mainland" or record.get("variant") == "mainland"):
        states["xhs"] = record
    _write_session_states(manager, states)
    return record


def _session_logs(manager) -> str:
    return "\n".join(entry.message for entry in manager.logs[-60:])


def _session_error_code(messages: str, default: str = "MEDIACRAWLER_VERIFY_FAILED") -> str:
    lowered = messages.lower()
    if "no available browser found" in lowered:
        return "BROWSER_NOT_FOUND"
    if "browser failed to start" in lowered or "browser launch failed" in lowered:
        return "BROWSER_LAUNCH_FAILED"
    if "cdp connection failed" in lowered or "cannot connect to existing browser" in lowered:
        return "CDP_CONNECT_FAILED"
    if "captcha" in lowered or "验证码" in messages or "滑块" in messages:
        return "MEDIACRAWLER_CAPTCHA_REQUIRED"
    if "mediacrawler_session_invalid" in lowered:
        return "MEDIACRAWLER_SESSION_INVALID"
    if "mediacrawler_login_failed" in lowered:
        return "MEDIACRAWLER_LOGIN_FAILED"
    if "mediacrawler_login_timeout" in lowered:
        return "MEDIACRAWLER_LOGIN_TIMEOUT"
    return default


def _session_request(request_type, platform: str, auth_method: str, action: str, cookies: str = "", variant: str | None = None):
    login_type = "qrcode" if auth_method == "browser" else auth_method
    norm_variant = _normalize_variant(platform, variant)
    return request_type.model_validate(
        {
            "platform": platform,
            "login_type": login_type,
            "crawler_type": "search",
            "save_option": "json",
            "cookies": cookies,
            "headless": action == "verify" or auth_method == "cookie",
            "enable_comments": False,
            "enable_sub_comments": False,
            "max_notes_count": 1,
            "max_comments_count": 1,
            "session_action": action,
            "xhs_variant": norm_variant,
        }
    )


def _waiting_input_response(platform: str, variant: str, auth_method: str = "browser") -> dict[str, Any]:
    return {
        "status": "waiting_input",
        "code": "RESEARCH_AUTH_REQUIRED",
        "message": f"Waiting for interactive {platform} authentication",
        "platform": platform,
        "xhs_variant": variant,
        "auth_method": auth_method,
    }


async def _wait_for_session_process(manager, timeout_seconds: int = 150) -> bool:
    for _ in range(timeout_seconds * 4):
        process = manager.process
        if process is not None and process.poll() is not None:
            return process.returncode == 0 and "FLOWORD_SESSION_VALID" in _session_logs(manager)
        await asyncio.sleep(0.25)
    if manager.process and manager.process.poll() is None:
        await manager.stop()
    return False


async def session_status(platform: str, variant: str | None = None) -> dict[str, Any]:
    platform = platform.strip().lower()
    if platform not in RESEARCH_PLATFORMS:
        raise MediaCrawlerContractError("MEDIACRAWLER_PLATFORM_UNSUPPORTED", f"Unsupported research platform: {platform or '(empty)'}")
    norm_variant = _normalize_variant(platform, variant)
    key = _session_key(platform, norm_variant)
    _, manager = _runtime()
    states = _read_session_states(manager)
    stored = states.get(key)
    if not stored and platform == "xhs" and norm_variant == "mainland":
        stored = states.get("xhs")
    config = manager.current_config
    if config and config.platform.value == platform and config.session_action:
        config_variant = getattr(config, "xhs_variant", None) or ""
        norm_config_variant = _normalize_variant(platform, config_variant)
        if norm_config_variant == norm_variant:
            if manager.process and manager.process.poll() is None:
                status = "AWAITING_LOGIN" if config.session_action == "login" else "CONNECTING"
                auth_method = (stored or {}).get("auth_method") or ("browser" if config.login_type.value == "qrcode" else config.login_type.value)
                return _session_record(platform, auth_method, status, variant=norm_variant)
            valid = manager.process is not None and manager.process.returncode == 0 and "FLOWORD_SESSION_VALID" in _session_logs(manager)
            auth_method = (stored or {}).get("auth_method") or ("browser" if config.login_type.value == "qrcode" else config.login_type.value)
            if valid:
                return _save_session_record(manager, _session_record(platform, auth_method, "CONNECTED", variant=norm_variant, last_verified_at=datetime.now(timezone.utc).isoformat()), variant=norm_variant)
            code = _session_error_code(_session_logs(manager))
            return _save_session_record(manager, _session_record(platform, auth_method, "INVALID", variant=norm_variant, error={"code": code, "message": "MediaCrawler session is not valid"}), variant=norm_variant)
    if stored and stored.get("status") in {"AWAITING_LOGIN", "CONNECTING"}:
        return _save_session_record(
            manager,
            _session_record(platform, stored.get("auth_method") or "browser", "DISCONNECTED", variant=norm_variant),
            variant=norm_variant,
        )
    return stored or _session_record(platform, "browser", "DISCONNECTED", variant=norm_variant)


async def session_login(payload: dict[str, Any]) -> dict[str, Any]:
    platform = str(payload.get("platform", "")).strip().lower()
    variant = payload.get("variant") or payload.get("xhs_variant")
    norm_variant = _normalize_variant(platform, variant)
    auth_method = str(payload.get("auth_method", "browser")).strip().lower()
    if platform not in RESEARCH_PLATFORMS:
        raise MediaCrawlerContractError("MEDIACRAWLER_PLATFORM_UNSUPPORTED", f"Unsupported research platform: {platform or '(empty)'}")
    if auth_method == "phone":
        raise MediaCrawlerContractError("MEDIACRAWLER_PHONE_LOGIN_BLOCKED", "Phone login requires the MediaCrawler Redis SMS-code integration and is not available through Floword")
    if auth_method not in SESSION_AUTH_METHODS:
        raise MediaCrawlerContractError("MEDIACRAWLER_LOGIN_FAILED", f"Unsupported authentication method: {auth_method}")
    cookies = os.environ.get("MEDIACRAWLER_COOKIES", "").strip() if auth_method == "cookie" else ""
    if auth_method == "cookie" and not cookies:
        raise MediaCrawlerContractError("MEDIACRAWLER_SESSION_NOT_FOUND", "Cookie compatibility fallback is not configured")
    request_type, manager = _runtime()
    request = _session_request(request_type, platform, auth_method, "login", cookies, variant=norm_variant)
    if not await manager.start(request):
        raise MediaCrawlerConflictError("MediaCrawler is already running")
    record = _session_record(platform, auth_method, "CONNECTING" if auth_method == "cookie" else "AWAITING_LOGIN", variant=norm_variant)
    _save_session_record(manager, record, variant=norm_variant)
    return record


async def session_verify(platform: str, auth_method: str | None = None, variant: str | None = None) -> dict[str, Any]:
    platform = platform.strip().lower()
    norm_variant = _normalize_variant(platform, variant)
    current = await session_status(platform, variant=norm_variant)
    selected_auth = (auth_method or current.get("auth_method") or "browser").strip().lower()
    cookies = os.environ.get("MEDIACRAWLER_COOKIES", "").strip() if selected_auth == "cookie" else ""
    request_type, manager = _runtime()
    request = _session_request(request_type, platform, selected_auth, "verify", cookies, variant=norm_variant)
    if not await manager.start(request):
        raise MediaCrawlerConflictError("MediaCrawler is already running")
    valid = await _wait_for_session_process(manager, 75)
    if valid:
        return _save_session_record(manager, _session_record(platform, selected_auth, "CONNECTED", variant=norm_variant, last_verified_at=datetime.now(timezone.utc).isoformat()), variant=norm_variant)
    code = _session_error_code(_session_logs(manager))
    if current.get("status") == "DISCONNECTED" and code == "MEDIACRAWLER_SESSION_INVALID":
        code = "MEDIACRAWLER_SESSION_NOT_FOUND"
    record = _save_session_record(manager, _session_record(platform, selected_auth, "INVALID", variant=norm_variant, error={"code": code, "message": "MediaCrawler session verification failed"}), variant=norm_variant)
    return record


async def session_reconnect(platform: str, variant: str | None = None) -> dict[str, Any]:
    verified = await session_verify(platform, variant=variant)
    if verified.get("status") == "CONNECTED":
        return verified
    return await session_login({"platform": platform, "auth_method": verified.get("auth_method") or "browser", "variant": variant})


async def session_clear(platform: str, variant: str | None = None) -> dict[str, Any]:
    platform = platform.strip().lower()
    if platform not in RESEARCH_PLATFORMS:
        raise MediaCrawlerContractError("MEDIACRAWLER_PLATFORM_UNSUPPORTED", f"Unsupported research platform: {platform or '(empty)'}")
    norm_variant = _normalize_variant(platform, variant)
    key = _session_key(platform, norm_variant)
    _, manager = _runtime()
    states = _read_session_states(manager)
    states.pop(key, None)
    if platform == "xhs" and norm_variant == "mainland":
        states.pop("xhs", None)
    _write_session_states(manager, states)
    if manager.process and manager.process.poll() is None:
        await manager.stop()
    runtime_root = Path(getattr(manager, "_runtime_root", Path.cwd())).resolve()
    browser_root = (runtime_root / "browser_data").resolve()
    if platform == "xhs":
        profile_name = "xhs_user_data_dir" if norm_variant == "international" else "cdp_xhs_user_data_dir"
        targets = [browser_root / profile_name]
    else:
        targets = [browser_root / f"cdp_{platform}_user_data_dir", browser_root / f"{platform}_user_data_dir"]
    for target in targets:
        resolved = target.resolve()
        if resolved.parent != browser_root:
            raise MediaCrawlerUnavailableError("Refusing to clear an unexpected browser profile path")
        if resolved.is_dir():
            shutil.rmtree(resolved)
    states = _read_session_states(manager)
    states.pop(platform, None)
    _write_session_states(manager, states)
    manager.current_config = None
    manager.process = None
    return _session_record(platform, "browser", "DISCONNECTED", variant=norm_variant)


def _data_files(manager) -> dict[Path, int]:
    root = Path(getattr(manager, "_runtime_root", Path.cwd())) / "data"
    if not root.is_dir():
        return {}
    return {
        path.resolve(): path.stat().st_mtime_ns
        for path in root.rglob("*")
        if path.is_file() and path.suffix.lower() in {".json", ".jsonl"}
    }


def _parse_records(path: Path) -> list[Any]:
    text = path.read_text(encoding="utf-8-sig")
    # MediaCrawler creates the comments JSON file before the first comment is
    # flushed. A successful content crawl can therefore leave a legitimate
    # zero-byte comments placeholder; it must not invalidate content records
    # written by the same operation.
    if not text.strip():
        return []
    if path.suffix.lower() == ".jsonl":
        return [json.loads(line) for line in text.splitlines() if line.strip()]
    value = json.loads(text)
    if isinstance(value, list):
        return value
    if isinstance(value, dict):
        for key in ("records", "data", "items"):
            if isinstance(value.get(key), list):
                return value[key]
        return [value]
    raise ValueError(f"unsupported JSON root in {path.name}")


def _operation_result(
    manager,
    before: dict[Path, int],
    *,
    platform: str,
    query: str,
    mode: str,
    artifact_ids: list[str],
    variant: str | None = None,
    enrichment_status: str = "completed",
    enrichment_reason: str | None = None,
) -> dict[str, Any] | None:
    after = _data_files(manager)
    changed = [path for path, mtime in after.items() if path not in before or before[path] != mtime]
    records: list[Any] = []
    files: list[str] = []
    for path in sorted(changed):
        parsed = _parse_records(path)
        if parsed:
            records.extend(parsed)
            files.append(str(path))
    if not records:
        return None
    enrichment: dict[str, Any] = {"status": enrichment_status, "comments_enabled": True}
    if enrichment_reason:
        enrichment["reason"] = enrichment_reason
    norm_variant = _normalize_variant(platform, variant)
    res: dict[str, Any] = {
        "status": "completed",
        "service": "mediacrawler",
        "platform": platform,
        "query": query,
        "mode": mode,
        "input_artifact_ids": artifact_ids,
        "files": files,
        "record_count": len(records),
        "records": records,
        "enrichment": enrichment,
    }
    if platform == "xhs" and norm_variant:
        res["xhs_variant"] = norm_variant
    return res


async def research_operation(payload: dict[str, Any]) -> dict[str, Any]:
    """Run one real MediaCrawler search and return only newly written records."""
    query = str(payload.get("query", "")).strip()
    platform = str(payload.get("platform", "")).strip().lower()
    mode = str(payload.get("mode", "search")).strip().lower()
    artifact_ids = payload.get("input_artifact_ids", [])
    variant = payload.get("xhs_variant") or payload.get("variant")
    norm_variant = _normalize_variant(platform, variant)
    if not query:
        raise MediaCrawlerContractError("MEDIACRAWLER_QUERY_INVALID", "Research query is required")
    if platform not in RESEARCH_PLATFORMS:
        raise MediaCrawlerContractError(
            "MEDIACRAWLER_PLATFORM_UNSUPPORTED",
            f"Unsupported research platform: {platform or '(empty)'}",
        )
    if mode not in RESEARCH_MODES:
        raise MediaCrawlerContractError(
            "MEDIACRAWLER_QUERY_INVALID", f"Unsupported research mode: {mode}"
        )
    if not isinstance(artifact_ids, list) or not all(isinstance(value, str) for value in artifact_ids):
        raise MediaCrawlerContractError(
            "MEDIACRAWLER_QUERY_INVALID", "input_artifact_ids must be a string array"
        )

    cookies = os.environ.get("MEDIACRAWLER_COOKIES", "").strip()

    # Real preflight: use the platform client's authenticated `pong()` through
    # the same persisted CDP profile and stop before any crawl operation.
    if cookies:
        await session_login({"platform": platform, "auth_method": "cookie", "variant": norm_variant})
        _, preflight_manager = _runtime()
        valid = await _wait_for_session_process(preflight_manager, 75)
        preflight = await session_status(platform, variant=norm_variant)
        if not valid or preflight.get("status") != "CONNECTED":
            raise MediaCrawlerAuthRequiredError("MediaCrawler cookie session could not be verified", "RESEARCH_AUTH_REQUIRED")
    else:
        current = await session_status(platform, variant=norm_variant)
        if current.get("status") == "AWAITING_LOGIN":
            return _waiting_input_response(platform, norm_variant, current.get("auth_method") or "browser")
        # A successful interactive login has already verified the canonical
        # persisted profile. Re-verifying it headlessly here can invalidate a
        # freshly connected RedNote session before the resumed crawl starts.
        if current.get("status") != "CONNECTED":
            preflight = await session_verify(platform, variant=norm_variant)
            if preflight.get("status") != "CONNECTED":
                login = await session_login(
                    {
                        "platform": platform,
                        "auth_method": "browser",
                        "variant": norm_variant,
                    }
                )
                return _waiting_input_response(platform, norm_variant, login.get("auth_method", "browser"))

    request_type, manager = _runtime()
    before = _data_files(manager)
    request = request_type.model_validate(
        {
            "platform": platform,
            # Prefer an explicitly provisioned cookie, otherwise reuse the
            # canonical persistent CDP browser profile. MediaCrawler checks the
            # existing session before presenting QR login.
            "login_type": "cookie" if cookies else "qrcode",
            "crawler_type": mode,
            "keywords": query if mode == "search" else "",
            "specified_ids": query if mode == "detail" else "",
            "creator_ids": query if mode == "creator" else "",
            "save_option": "json",
            "cookies": cookies,
            # RedNote's browser-auth session is verified in a visible,
            # persistent profile. Reopening that profile headlessly can make
            # pong() reject the otherwise valid login and hide a second QR
            # prompt from the user. Cookie auth remains safe to run headless.
            "headless": bool(cookies),
            "enable_comments": True,
            "enable_sub_comments": False,
            "max_notes_count": int(os.environ.get("MEDIACRAWLER_MAX_NOTES", "20")),
            "max_comments_count": int(os.environ.get("MEDIACRAWLER_MAX_COMMENTS", "50")),
            "xhs_variant": norm_variant,
        }
    )
    if not await manager.start(request):
        if manager.process and manager.process.poll() is None:
            raise MediaCrawlerConflictError("Crawler is already running")
        raise MediaCrawlerUnavailableError("MediaCrawler failed to start")

    timeout = max(10, int(os.environ.get("RESEARCH_TIMEOUT_SECONDS", "180")))
    try:
        for _ in range(timeout * 4):
            process = manager.process
            if process is not None and process.poll() is not None:
                break
            await asyncio.sleep(0.25)
        else:
            messages = "\n".join(entry.message for entry in manager.logs[-30:])
            await manager.stop()
            try:
                preliminary = _operation_result(
                    manager,
                    before,
                    platform=platform,
                    query=query,
                    mode=mode,
                    artifact_ids=artifact_ids,
                    variant=norm_variant,
                    enrichment_status="partial",
                    enrichment_reason="comments_timeout",
                )
            except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
                raise MediaCrawlerUnavailableError(f"MediaCrawler result is not parseable: {error}") from error
            if preliminary is not None:
                return preliminary
            if any(token in messages.lower() for token in ("login", "qrcode", "扫码", "登录")):
                raise MediaCrawlerAuthRequiredError("MediaCrawler browser session is missing or expired")
            raise MediaCrawlerUnavailableError(
                "MediaCrawler operation timed out", "MEDIACRAWLER_TIMEOUT"
            )
    except asyncio.CancelledError:
        if manager.process and manager.process.poll() is None:
            await manager.stop()
        raise

    return_code = manager.process.returncode if manager.process else None
    if return_code != 0:
        messages = "\n".join(entry.message for entry in manager.logs[-30:])
        lowered = messages.lower()
        if any(token in lowered for token in ("captcha", "验证码", "滑块")):
            raise MediaCrawlerContractError("MEDIACRAWLER_CAPTCHA_REQUIRED", "MediaCrawler requires captcha verification")
        if any(token in lowered for token in ("rate limit", "too many requests", "频繁")):
            raise MediaCrawlerContractError("MEDIACRAWLER_RATE_LIMITED", "MediaCrawler platform rate limit reached")
        if any(token in lowered for token in ("login", "cookie", "qrcode", "扫码", "登录")):
            raise MediaCrawlerAuthRequiredError("MediaCrawler authentication expired or was rejected")
        raise MediaCrawlerUnavailableError(f"MediaCrawler exited with code {return_code}")

    try:
        result = _operation_result(
            manager,
            before,
            platform=request.platform.value,
            query=query,
            mode=request.crawler_type.value,
            artifact_ids=artifact_ids,
            variant=norm_variant,
        )
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        raise MediaCrawlerUnavailableError(f"MediaCrawler result is not parseable: {error}") from error
    if result is None:
        raise MediaCrawlerUnavailableError("MediaCrawler produced no parseable research records")
    return result
