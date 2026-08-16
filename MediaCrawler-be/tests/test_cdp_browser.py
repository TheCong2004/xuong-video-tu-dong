# -*- coding: utf-8 -*-
from unittest.mock import AsyncMock, MagicMock

import pytest

import config
from tools.cdp_browser import CDPBrowserManager


@pytest.mark.asyncio
async def test_existing_browser_connects_directly_to_devtools_browser(monkeypatch):
    monkeypatch.setattr(config, "CDP_CONNECT_EXISTING", True)
    monkeypatch.setattr(config, "BROWSER_LAUNCH_TIMEOUT", 60)

    manager = CDPBrowserManager()
    manager.debug_port = 9222
    manager._using_existing_browser = True
    manager._get_browser_websocket_url = AsyncMock(  # type: ignore[method-assign]
        side_effect=AssertionError("existing browser mode must not call /json/version")
    )

    browser = MagicMock()
    browser.is_connected.return_value = True
    browser.contexts = []

    playwright = MagicMock()
    playwright.chromium.connect_over_cdp = AsyncMock(return_value=browser)

    await manager._connect_via_cdp(playwright)

    playwright.chromium.connect_over_cdp.assert_awaited_once_with(
        "ws://localhost:9222/devtools/browser",
        timeout=60000,
    )


@pytest.mark.asyncio
async def test_existing_browser_falls_back_to_discovered_websocket_url(monkeypatch):
    monkeypatch.setattr(config, "CDP_CONNECT_EXISTING", True)
    monkeypatch.setattr(config, "BROWSER_LAUNCH_TIMEOUT", 60)

    manager = CDPBrowserManager()
    manager.debug_port = 9222
    manager._using_existing_browser = True
    manager._get_browser_websocket_url = AsyncMock(  # type: ignore[method-assign]
        return_value="ws://localhost:9222/devtools/browser/generated-id"
    )

    browser = MagicMock()
    browser.is_connected.return_value = True
    browser.contexts = []

    playwright = MagicMock()
    playwright.chromium.connect_over_cdp = AsyncMock(
        side_effect=[RuntimeError("direct websocket failed"), browser]
    )

    await manager._connect_via_cdp(playwright)

    manager._get_browser_websocket_url.assert_awaited_once_with(9222)
    assert playwright.chromium.connect_over_cdp.await_args_list[0].args == (
        "ws://localhost:9222/devtools/browser",
    )
    assert playwright.chromium.connect_over_cdp.await_args_list[0].kwargs == {
        "timeout": 60000,
    }
    assert playwright.chromium.connect_over_cdp.await_args_list[1].args == (
        "ws://localhost:9222/devtools/browser/generated-id",
    )
    assert playwright.chromium.connect_over_cdp.await_args_list[1].kwargs == {
        "timeout": 60000,
    }


@pytest.mark.asyncio
async def test_launched_browser_uses_discovered_websocket_url(monkeypatch):
    monkeypatch.setattr(config, "CDP_CONNECT_EXISTING", False)

    manager = CDPBrowserManager()
    manager.debug_port = 9223
    manager._get_browser_websocket_url = AsyncMock(  # type: ignore[method-assign]
        return_value="ws://localhost:9223/devtools/browser/generated-id"
    )

    browser = MagicMock()
    browser.is_connected.return_value = True
    browser.contexts = []

    playwright = MagicMock()
    playwright.chromium.connect_over_cdp = AsyncMock(return_value=browser)

    await manager._connect_via_cdp(playwright)

    manager._get_browser_websocket_url.assert_awaited_once_with(9223)
    playwright.chromium.connect_over_cdp.assert_awaited_once_with(
        "ws://localhost:9223/devtools/browser/generated-id"
    )


@pytest.mark.asyncio
async def test_interactive_login_launches_visible_browser_when_cdp_port_is_missing(monkeypatch):
    monkeypatch.setattr(config, "CDP_CONNECT_EXISTING", True)
    monkeypatch.setattr(config, "CDP_DEBUG_PORT", 9222)
    monkeypatch.setattr(config, "SESSION_ACTION", "login")
    monkeypatch.setattr(config, "BROWSER_LAUNCH_TIMEOUT", 1)

    manager = CDPBrowserManager()
    manager._test_cdp_connection = AsyncMock(return_value=False)  # type: ignore[method-assign]
    manager._get_browser_path = AsyncMock(return_value="chrome.exe")  # type: ignore[method-assign]
    manager._launch_browser = AsyncMock()  # type: ignore[method-assign]
    manager._register_cleanup_handlers = MagicMock()  # type: ignore[method-assign]
    manager._connect_via_cdp = AsyncMock()  # type: ignore[method-assign]
    context = MagicMock()
    manager._create_browser_context = AsyncMock(return_value=context)  # type: ignore[method-assign]

    result = await manager.launch_and_connect(MagicMock(), headless=True)

    assert result is context
    manager._launch_browser.assert_awaited_once_with("chrome.exe", False)
    assert manager._using_existing_browser is False


@pytest.mark.asyncio
async def test_interactive_login_reuses_reachable_existing_browser(monkeypatch):
    monkeypatch.setattr(config, "CDP_CONNECT_EXISTING", True)
    monkeypatch.setattr(config, "CDP_DEBUG_PORT", 9222)
    monkeypatch.setattr(config, "SESSION_ACTION", "login")

    manager = CDPBrowserManager()
    manager._test_cdp_connection = AsyncMock(return_value=True)  # type: ignore[method-assign]
    context = MagicMock()
    manager._connect_existing_browser = AsyncMock(return_value=context)  # type: ignore[method-assign]
    manager._launch_browser = AsyncMock()  # type: ignore[method-assign]

    result = await manager.launch_and_connect(MagicMock(), headless=False)

    assert result is context
    manager._launch_browser.assert_not_awaited()
    assert manager._using_existing_browser is True


@pytest.mark.asyncio
async def test_verify_does_not_auto_launch_visible_browser(monkeypatch):
    monkeypatch.setattr(config, "CDP_CONNECT_EXISTING", True)
    monkeypatch.setattr(config, "CDP_DEBUG_PORT", 9222)
    monkeypatch.setattr(config, "SESSION_ACTION", "verify")

    manager = CDPBrowserManager()
    manager._test_cdp_connection = AsyncMock(return_value=False)  # type: ignore[method-assign]
    manager._connect_existing_browser = AsyncMock()  # type: ignore[method-assign]
    manager._launch_browser = AsyncMock()  # type: ignore[method-assign]

    with pytest.raises(RuntimeError, match="CDP port 9222 is unavailable for session verify"):
        await manager.launch_and_connect(MagicMock(), headless=True)

    manager._connect_existing_browser.assert_not_awaited()
    manager._launch_browser.assert_not_awaited()


@pytest.mark.asyncio
async def test_crawl_without_cdp_port_fails_fast_to_headless_fallback(monkeypatch):
    monkeypatch.setattr(config, "CDP_CONNECT_EXISTING", True)
    monkeypatch.setattr(config, "CDP_DEBUG_PORT", 9222)
    monkeypatch.setattr(config, "SESSION_ACTION", "")

    manager = CDPBrowserManager()
    manager._test_cdp_connection = AsyncMock(return_value=False)  # type: ignore[method-assign]
    manager._connect_existing_browser = AsyncMock()  # type: ignore[method-assign]
    manager._launch_browser = AsyncMock()  # type: ignore[method-assign]

    with pytest.raises(RuntimeError, match="CDP port 9222 is unavailable for crawl"):
        await manager.launch_and_connect(MagicMock(), headless=True)

    manager._connect_existing_browser.assert_not_awaited()
    manager._launch_browser.assert_not_awaited()


@pytest.mark.asyncio
async def test_cleanup_respects_actual_browser_ownership(monkeypatch):
    monkeypatch.setattr(config, "CDP_CONNECT_EXISTING", True)
    monkeypatch.setattr(config, "AUTO_CLOSE_BROWSER", True)
    manager = CDPBrowserManager()
    manager.launcher.cleanup = MagicMock()

    manager._using_existing_browser = True
    await manager.cleanup()
    manager.launcher.cleanup.assert_not_called()

    manager._using_existing_browser = False
    manager.launcher.browser_process = MagicMock()
    await manager.cleanup()
    manager.launcher.cleanup.assert_called_once()


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("international", "expected_name"),
    [(False, "cdp_xhs_user_data_dir"), (True, "xhs_user_data_dir")],
)
async def test_xhs_variants_use_separate_compatible_profiles(
    monkeypatch, tmp_path, international, expected_name
):
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(config, "SAVE_LOGIN_STATE", True)
    monkeypatch.setattr(config, "PLATFORM", "xhs")
    monkeypatch.setattr(config, "XHS_INTERNATIONAL", international)

    manager = CDPBrowserManager()
    manager.debug_port = 9222
    manager.launcher.launch_browser = MagicMock(return_value=MagicMock())
    manager.launcher.wait_for_browser_ready = MagicMock(return_value=True)
    manager._test_cdp_connection = AsyncMock(return_value=True)  # type: ignore[method-assign]

    await manager._launch_browser("chrome.exe", False)

    user_data_dir = manager.launcher.launch_browser.call_args.kwargs["user_data_dir"]
    assert user_data_dir.endswith(expected_name)
