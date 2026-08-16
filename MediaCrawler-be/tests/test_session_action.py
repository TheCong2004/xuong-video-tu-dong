from pathlib import Path

import pytest
from pydantic import ValidationError

from api.schemas import CrawlerStartRequest
from api.services.crawler_manager import CrawlerManager


def test_session_action_schema_accepts_only_login_or_verify():
    assert CrawlerStartRequest(platform="xhs", session_action="login").session_action == "login"
    assert CrawlerStartRequest(platform="xhs", session_action="verify").session_action == "verify"
    with pytest.raises(ValidationError):
        CrawlerStartRequest(platform="xhs", session_action="crawl-and-pretend")


def test_session_action_is_forwarded_to_canonical_worker():
    manager = CrawlerManager()
    command = manager._build_command(
        CrawlerStartRequest(
            platform="xhs",
            login_type="qrcode",
            session_action="verify",
            headless=True,
        )
    )
    index = command.index("--session_action")
    assert command[index + 1] == "verify"
    assert Path(command[0]).name.lower() in {"python", "python.exe", "uv"}
