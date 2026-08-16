from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path
from typing import Any


class VynaroUnavailableError(RuntimeError):
    pass


class VynaroExecutionError(RuntimeError):
    pass


def bridge_binary() -> Path:
    configured = os.environ.get("VYNARO_BRIDGE_BINARY", "").strip()
    if configured:
        return Path(configured)
    suffix = ".exe" if os.name == "nt" else ""
    return Path(__file__).resolve().parents[2] / "vynaro" / "target" / "debug" / f"vynaro-bridge{suffix}"


async def _run(command: str, payload: dict[str, Any] | None = None, *, timeout: float = 30.0):
    binary = bridge_binary()
    if not binary.is_file():
        raise VynaroUnavailableError("Vynaro bridge binary is not available")
    body = json.dumps(payload).encode("utf-8") if payload is not None else None
    process = await asyncio.create_subprocess_exec(
        str(binary),
        command,
        stdin=asyncio.subprocess.PIPE if body is not None else asyncio.subprocess.DEVNULL,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    try:
        stdout, stderr = await asyncio.wait_for(process.communicate(body), timeout=timeout)
    except asyncio.TimeoutError as error:
        process.kill()
        await process.wait()
        raise VynaroExecutionError("Vynaro action timed out") from error
    if process.returncode != 0:
        message = "Vynaro action failed"
        try:
            error_payload = json.loads(stderr.decode("utf-8", errors="replace"))
            if isinstance(error_payload, dict) and isinstance(error_payload.get("error"), str):
                message = error_payload["error"]
        except json.JSONDecodeError:
            pass
        raise VynaroExecutionError(message)
    if len(stdout) > 5 * 1024 * 1024:
        raise VynaroExecutionError("Vynaro response is too large")
    try:
        result = json.loads(stdout)
    except json.JSONDecodeError as error:
        raise VynaroExecutionError("Vynaro returned invalid JSON") from error
    if not isinstance(result, dict):
        raise VynaroExecutionError("Vynaro returned an invalid response")
    return result


async def health():
    try:
        return await _run("health", timeout=5.0)
    except VynaroUnavailableError:
        return {"status": "offline", "message": "Vynaro bridge binary is not available"}
    except VynaroExecutionError:
        return {"status": "error", "message": "Vynaro bridge health check failed"}


async def build_video_plans(payload: dict[str, Any]):
    return await _run("plan", payload, timeout=15.0)


async def probe_video(path: str):
    normalized = path.strip()
    if not normalized or len(normalized) > 4096:
        raise ValueError("path must contain between 1 and 4096 characters")
    return await _run("probe", {"path": normalized}, timeout=30.0)
