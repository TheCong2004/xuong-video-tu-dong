from __future__ import annotations

import asyncio
import json
import os
from pathlib import Path
from typing import Any


class YouweeUnavailableError(RuntimeError):
    pass


class YouweeExecutionError(RuntimeError):
    pass


def bridge_binary() -> Path:
    configured = os.environ.get("YOUWEE_BRIDGE_BINARY", "").strip()
    if configured:
        return Path(configured)
    suffix = ".exe" if os.name == "nt" else ""
    return Path(__file__).resolve().parents[2] / "target" / "debug" / f"youwee-bridge{suffix}"


async def _run(*args: str, timeout: float = 60.0) -> dict[str, Any]:
    binary = bridge_binary()
    if not binary.is_file():
        raise YouweeUnavailableError("Youwee bridge binary is not available")

    process = await asyncio.create_subprocess_exec(
        str(binary),
        *args,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    try:
        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=timeout)
    except asyncio.TimeoutError as error:
        process.kill()
        await process.wait()
        raise YouweeExecutionError("Youwee action timed out") from error

    if process.returncode != 0:
        message = "Youwee action failed"
        try:
            error_payload = json.loads(stderr.decode("utf-8", errors="replace"))
            if isinstance(error_payload, dict) and isinstance(error_payload.get("error"), str):
                message = error_payload["error"]
        except json.JSONDecodeError:
            pass
        raise YouweeExecutionError(message)

    if len(stdout) > 5 * 1024 * 1024:
        raise YouweeExecutionError("Youwee response is too large")
    try:
        payload = json.loads(stdout)
    except json.JSONDecodeError as error:
        raise YouweeExecutionError("Youwee returned invalid JSON") from error
    if not isinstance(payload, dict):
        raise YouweeExecutionError("Youwee returned an invalid response")
    return payload


async def health():
    try:
        return await _run("health", timeout=5.0)
    except YouweeUnavailableError:
        return {"status": "offline", "message": "Youwee bridge binary is not available"}
    except YouweeExecutionError:
        return {"status": "error", "message": "Youwee bridge health check failed"}


async def search(query: str, limit: int = 10):
    normalized = query.strip()
    if not normalized or len(normalized) > MAX_QUERY_LENGTH:
        raise ValueError("query must contain between 1 and 500 characters")
    if not 1 <= limit <= 50:
        raise ValueError("limit must be between 1 and 50")
    return await _run("search", normalized, str(limit), timeout=90.0)


MAX_QUERY_LENGTH = 500
