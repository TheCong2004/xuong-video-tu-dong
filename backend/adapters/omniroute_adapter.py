from __future__ import annotations

from typing import Any
from urllib.parse import quote

from backend.clients import omniroute_client


def _provider_id(value: str) -> str:
    normalized = value.strip()
    if not normalized:
        raise ValueError("Provider id is required")
    return quote(normalized, safe="")


async def models():
    return await omniroute_client.get_models()


async def chat(payload: dict[str, Any]):
    if payload.get("stream") is True:
        raise ValueError("Streaming chat is not supported by this gateway endpoint")
    return await omniroute_client.chat_completion({**payload, "stream": False})


async def providers():
    return await omniroute_client.list_providers()


async def create_provider(payload: dict[str, Any]):
    return await omniroute_client.create_provider(payload)


async def provider(provider_id: str):
    return await omniroute_client.get_provider(_provider_id(provider_id))


async def update_provider(provider_id: str, payload: dict[str, Any]):
    return await omniroute_client.update_provider(_provider_id(provider_id), payload)


async def delete_provider(provider_id: str):
    return await omniroute_client.delete_provider(_provider_id(provider_id))


async def test_provider(provider_id: str, payload: dict[str, Any]):
    return await omniroute_client.test_provider(_provider_id(provider_id), payload)
