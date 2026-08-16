from __future__ import annotations

import ipaddress
from urllib.parse import urlparse

from fastapi import HTTPException, Request


def is_loopback(host: str | None) -> bool:
    if not host:
        return False
    if host.lower() == "localhost":
        return True
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


async def require_local_request(request: Request) -> None:
    if request.client is None or not is_loopback(request.client.host):
        raise HTTPException(status_code=403, detail="Gateway action is local-only")

    origin = request.headers.get("origin")
    if not origin:
        return
    parsed = urlparse(origin)
    trusted_tauri_origin = origin in {
        "tauri://localhost",
        "http://tauri.localhost",
        "https://tauri.localhost",
    }
    if not trusted_tauri_origin and not (
        parsed.scheme in {"http", "https"} and is_loopback(parsed.hostname)
    ):
        raise HTTPException(status_code=403, detail="Untrusted gateway origin")
