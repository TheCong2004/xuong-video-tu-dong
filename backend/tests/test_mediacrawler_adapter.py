import asyncio
import unittest
from unittest import mock
from pathlib import Path
from types import SimpleNamespace
import tempfile

from backend.adapters.mediacrawler_adapter import (
    _operation_result,
    MediaCrawlerContractError,
    MediaCrawlerAuthRequiredError,
    research_operation,
    session_clear,
    session_status,
    session_verify,
    session_reconnect,
)


class ResearchContractTests(unittest.TestCase):
    def test_empty_comments_json_does_not_discard_valid_content_records(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / "data" / "xhs" / "json"
            output.mkdir(parents=True)
            (output / "search_comments.json").write_text("", encoding="utf-8")
            (output / "search_contents.json").write_text(
                '[{"note_id":"real-note","title":"AI video"}]',
                encoding="utf-8",
            )
            manager = SimpleNamespace(_runtime_root=root)

            result = _operation_result(
                manager,
                {},
                platform="xhs",
                query="AI video editing",
                mode="search",
                artifact_ids=[],
            )

            self.assertIsNotNone(result)
            self.assertEqual(result["record_count"], 1)
            self.assertEqual(result["records"][0]["note_id"], "real-note")

    def test_research_accepts_empty_source_artifact_list(self):
        with mock.patch.dict("os.environ", {}, clear=True):
            with mock.patch(
                "backend.adapters.mediacrawler_adapter.session_status",
                new=mock.AsyncMock(return_value={"status": "DISCONNECTED"}),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter.session_verify",
                new=mock.AsyncMock(return_value={"status": "CONNECTED"}),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter.session_login",
                new=mock.AsyncMock(),
            ) as login, mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                side_effect=RuntimeError("runtime reached"),
            ):
                with self.assertRaisesRegex(RuntimeError, "runtime reached"):
                    asyncio.run(
                        research_operation(
                            {
                                "platform": "xhs",
                                "query": "AI video editing",
                                "mode": "search",
                                "input_artifact_ids": [],
                            }
                        )
                    )
                login.assert_not_awaited()

    def test_connected_interactive_session_resumes_crawl_without_reverify(self):
        with mock.patch.dict("os.environ", {}, clear=True), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_status",
            new=mock.AsyncMock(return_value={"status": "CONNECTED", "auth_method": "browser"}),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_verify",
            new=mock.AsyncMock(),
        ) as verify, mock.patch(
            "backend.adapters.mediacrawler_adapter.session_login",
            new=mock.AsyncMock(),
        ) as login, mock.patch(
            "backend.adapters.mediacrawler_adapter._runtime",
            side_effect=RuntimeError("crawl runtime reached"),
        ):
            with self.assertRaisesRegex(RuntimeError, "crawl runtime reached"):
                asyncio.run(
                    research_operation(
                        {"platform": "xhs", "variant": "international", "query": "trend", "mode": "search"}
                    )
                )
        verify.assert_not_awaited()
        login.assert_not_awaited()
    def test_tiktok_is_not_aliased_to_douyin(self):
        with self.assertRaises(MediaCrawlerContractError) as raised:
            asyncio.run(
                research_operation(
                    {"platform": "tiktok", "query": "trend", "mode": "search"}
                )
            )
        self.assertEqual(raised.exception.code, "MEDIACRAWLER_PLATFORM_UNSUPPORTED")

    def test_mode_must_be_a_real_mediacrawler_mode(self):
        with self.assertRaises(MediaCrawlerContractError) as raised:
            asyncio.run(
                research_operation(
                    {"platform": "xhs", "query": "trend", "mode": "viral"}
                )
            )
        self.assertEqual(raised.exception.code, "MEDIACRAWLER_QUERY_INVALID")

    def test_missing_international_session_starts_login_and_returns_waiting(self):
        with mock.patch(
            "backend.adapters.mediacrawler_adapter.session_verify",
            new=mock.AsyncMock(
                return_value={"status": "INVALID", "error": {"code": "MEDIACRAWLER_SESSION_INVALID"}}
            ),
        ) as verify, mock.patch(
            "backend.adapters.mediacrawler_adapter.session_status",
            new=mock.AsyncMock(return_value={"status": "DISCONNECTED", "auth_method": "browser"}),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_login",
            new=mock.AsyncMock(return_value={"status": "AWAITING_LOGIN"}),
        ) as login:
            result = asyncio.run(
                research_operation(
                    {
                        "platform": "xhs",
                        "xhs_variant": "international",
                        "query": "trend",
                        "mode": "search",
                    }
                )
            )
        login.assert_awaited_once_with(
            {"platform": "xhs", "auth_method": "browser", "variant": "international"}
        )
        self.assertEqual(verify.await_count, 1)
        self.assertEqual(result["status"], "waiting_input")

    def test_interactive_login_returns_waiting_without_cleanup(self):
        manager = SimpleNamespace(logs=[], process=SimpleNamespace(poll=lambda: None))
        manager.stop = mock.AsyncMock()
        with mock.patch(
            "backend.adapters.mediacrawler_adapter.session_status",
            new=mock.AsyncMock(return_value={"status": "DISCONNECTED", "auth_method": "browser"}),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_verify",
            new=mock.AsyncMock(
                return_value={"status": "INVALID", "error": {"code": "MEDIACRAWLER_SESSION_INVALID"}}
            ),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_login",
            new=mock.AsyncMock(return_value={"status": "AWAITING_LOGIN"}),
        ) as login, mock.patch(
            "backend.adapters.mediacrawler_adapter._runtime",
            return_value=(object(), manager),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter._wait_for_session_process",
            new=mock.AsyncMock(return_value=False),
        ) as wait:
            result = asyncio.run(
                research_operation(
                    {"platform": "xhs", "variant": "international", "query": "trend", "mode": "search"}
                )
            )
        login.assert_awaited_once()
        wait.assert_not_awaited()
        manager.stop.assert_not_awaited()
        self.assertEqual(result["status"], "waiting_input")
        self.assertEqual(result["code"], "RESEARCH_AUTH_REQUIRED")

    def test_active_interactive_login_is_reused_without_duplicate_process(self):
        with mock.patch(
            "backend.adapters.mediacrawler_adapter.session_status",
            new=mock.AsyncMock(return_value={"status": "AWAITING_LOGIN", "auth_method": "browser"}),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_verify",
            new=mock.AsyncMock(),
        ) as verify, mock.patch(
            "backend.adapters.mediacrawler_adapter.session_login",
            new=mock.AsyncMock(),
        ) as login:
            result = asyncio.run(
                research_operation(
                    {"platform": "xhs", "variant": "international", "query": "trend", "mode": "search"}
                )
            )
        verify.assert_not_awaited()
        login.assert_not_awaited()
        self.assertEqual(result["status"], "waiting_input")

    def test_mainland_auto_login_preserves_mainland_variant(self):
        with mock.patch(
            "backend.adapters.mediacrawler_adapter.session_status",
            new=mock.AsyncMock(return_value={"status": "DISCONNECTED", "auth_method": "browser"}),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_verify",
            new=mock.AsyncMock(return_value={"status": "INVALID"}),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_login",
            new=mock.AsyncMock(return_value={"status": "AWAITING_LOGIN"}),
        ) as login:
            result = asyncio.run(
                research_operation(
                    {"platform": "xhs", "variant": "mainland", "query": "trend", "mode": "search"}
                )
            )
        login.assert_awaited_once_with(
            {"platform": "xhs", "auth_method": "browser", "variant": "mainland"}
        )
        self.assertEqual(result["status"], "waiting_input")

    def test_comment_timeout_returns_parseable_preliminary_records(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            process = SimpleNamespace(poll=lambda: None)
            manager = SimpleNamespace(
                _runtime_root=root,
                current_config=None,
                process=process,
                logs=[],
                stop=mock.AsyncMock(return_value=True),
            )

            async def start(_request):
                output = root / "data" / "xhs" / "json" / "search_contents.json"
                output.parent.mkdir(parents=True)
                output.write_text('[{"note_id":"real-note"}]', encoding="utf-8")
                return True

            manager.start = mock.AsyncMock(side_effect=start)
            def validate(payload):
                values = dict(payload)
                values["platform"] = SimpleNamespace(value=payload["platform"])
                values["crawler_type"] = SimpleNamespace(value=payload["crawler_type"])
                return SimpleNamespace(**values)

            request_type = SimpleNamespace(model_validate=validate)
            with mock.patch.dict("os.environ", {"RESEARCH_TIMEOUT_SECONDS": "10"}, clear=True), mock.patch(
                "backend.adapters.mediacrawler_adapter.session_status",
                new=mock.AsyncMock(return_value={"status": "DISCONNECTED"}),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter.session_verify",
                new=mock.AsyncMock(return_value={"status": "CONNECTED"}),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(request_type, manager),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter.asyncio.sleep",
                new=mock.AsyncMock(),
            ):
                result = asyncio.run(
                    research_operation(
                        {"platform": "xhs", "query": "AI video editing", "mode": "search"}
                    )
                )

            self.assertEqual(result["status"], "completed")
            self.assertEqual(result["enrichment"]["status"], "partial")
            self.assertEqual(result["enrichment"]["reason"], "comments_timeout")
            self.assertEqual(len(result["records"]), 1)
            self.assertFalse(manager.start.await_args.args[0].headless)
            manager.stop.assert_awaited_once()


class SessionLifecycleTests(unittest.TestCase):
    @staticmethod
    def manager(root: Path):
        return SimpleNamespace(
            _runtime_root=root,
            current_config=None,
            process=None,
            logs=[],
            start=mock.AsyncMock(return_value=True),
            stop=mock.AsyncMock(return_value=True),
        )

    def test_verify_persists_only_safe_connected_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(Path(directory))
            request_type = SimpleNamespace(
                model_validate=lambda payload: SimpleNamespace(**payload)
            )
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(request_type, manager),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter._wait_for_session_process",
                new=mock.AsyncMock(return_value=True),
            ):
                result = asyncio.run(session_verify("xhs"))
                restored = asyncio.run(session_status("xhs"))
            self.assertEqual(result["status"], "CONNECTED")
            self.assertEqual(restored["profile_id"], "mediacrawler:xhs")
            state_text = (Path(directory) / "browser_data" / "floword_sessions.json").read_text(encoding="utf-8")
            self.assertNotIn("cookie", state_text.lower())
            self.assertNotIn("token", state_text.lower())

    def test_running_login_reports_awaiting_until_process_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(Path(directory))
            manager.current_config = SimpleNamespace(
                platform=SimpleNamespace(value="xhs"),
                session_action="login",
                xhs_variant="international",
                login_type=SimpleNamespace(value="qrcode"),
            )
            manager.process = SimpleNamespace(poll=lambda: None)
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(object(), manager),
            ):
                result = asyncio.run(session_status("xhs", variant="international"))
            self.assertEqual(result["status"], "AWAITING_LOGIN")
            manager.stop.assert_not_awaited()

    def test_stale_waiting_state_after_restart_becomes_disconnected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            state_dir = root / "browser_data"
            state_dir.mkdir(parents=True)
            (state_dir / "floword_sessions.json").write_text(
                '{"xhs:international":{"platform":"xhs","variant":"international","auth_method":"browser","profile_id":"mediacrawler:xhs:international","status":"AWAITING_LOGIN"}}',
                encoding="utf-8",
            )
            manager = self.manager(root)
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(object(), manager),
            ):
                result = asyncio.run(session_status("xhs", variant="international"))
            self.assertEqual(result["status"], "DISCONNECTED")

    def test_login_failure_is_classified_only_after_process_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(Path(directory))
            manager.current_config = SimpleNamespace(
                platform=SimpleNamespace(value="xhs"),
                session_action="login",
                xhs_variant="international",
                login_type=SimpleNamespace(value="qrcode"),
            )
            manager.process = SimpleNamespace(poll=lambda: 1, returncode=1)
            manager.logs = [SimpleNamespace(message="MEDIACRAWLER_LOGIN_FAILED")]
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(object(), manager),
            ):
                result = asyncio.run(session_status("xhs", variant="international"))
            self.assertEqual(result["status"], "INVALID")
            self.assertEqual(result["error"]["code"], "MEDIACRAWLER_LOGIN_FAILED")

    def test_clear_removes_exact_profile_and_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = root / "browser_data" / "cdp_xhs_user_data_dir"
            profile.mkdir(parents=True)
            (profile / "state").write_text("private", encoding="utf-8")
            manager = self.manager(root)
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(object(), manager),
            ):
                result = asyncio.run(session_clear("xhs"))
                restored = asyncio.run(session_status("xhs"))
            self.assertEqual(result["status"], "DISCONNECTED")
            self.assertEqual(restored["status"], "DISCONNECTED")
            self.assertFalse(profile.exists())

    def test_clear_international_preserves_mainland_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            international = root / "browser_data" / "xhs_user_data_dir"
            mainland = root / "browser_data" / "cdp_xhs_user_data_dir"
            international.mkdir(parents=True)
            mainland.mkdir(parents=True)
            manager = self.manager(root)
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(object(), manager),
            ):
                result = asyncio.run(session_clear("xhs", variant="international"))
            self.assertEqual(result["variant"], "international")
            self.assertFalse(international.exists())
            self.assertTrue(mainland.exists())

    def test_reconnect_reuses_valid_profile_without_starting_login(self):
        with mock.patch(
            "backend.adapters.mediacrawler_adapter.session_verify",
            new=mock.AsyncMock(
                return_value={
                    "platform": "xhs",
                    "profile_id": "mediacrawler:xhs",
                    "auth_method": "browser",
                    "status": "CONNECTED",
                }
            ),
        ), mock.patch(
            "backend.adapters.mediacrawler_adapter.session_login",
            new=mock.AsyncMock(),
        ) as login:
            result = asyncio.run(session_reconnect("xhs"))
        self.assertEqual(result["profile_id"], "mediacrawler:xhs")
        login.assert_not_awaited()

    def test_xhs_mainland_is_default_and_uses_mainland_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(Path(directory))
            captured_request = None

            async def mock_start(req):
                nonlocal captured_request
                captured_request = req
                return True

            manager.start = mock.AsyncMock(side_effect=mock_start)
            request_type = SimpleNamespace(
                model_validate=lambda payload: SimpleNamespace(**payload)
            )
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(request_type, manager),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter._wait_for_session_process",
                new=mock.AsyncMock(return_value=True),
            ):
                result = asyncio.run(session_verify("xhs", variant="mainland"))
            self.assertEqual(result["status"], "CONNECTED")
            self.assertEqual(result["variant"], "mainland")
            self.assertEqual(result["profile_id"], "mediacrawler:xhs")
            self.assertEqual(captured_request.xhs_variant, "mainland")

    def test_xhs_international_variant_applies_distinct_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(Path(directory))
            captured_request = None

            async def mock_start(req):
                nonlocal captured_request
                captured_request = req
                return True

            manager.start = mock.AsyncMock(side_effect=mock_start)
            request_type = SimpleNamespace(
                model_validate=lambda payload: SimpleNamespace(**payload)
            )
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(request_type, manager),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter._wait_for_session_process",
                new=mock.AsyncMock(return_value=True),
            ):
                result = asyncio.run(session_verify("xhs", variant="international"))
            self.assertEqual(result["status"], "CONNECTED")
            self.assertEqual(result["variant"], "international")
            self.assertEqual(result["profile_id"], "mediacrawler:xhs:international")
            self.assertEqual(captured_request.xhs_variant, "international")

    def test_xhs_variant_sessions_are_isolated(self):
        with tempfile.TemporaryDirectory() as directory:
            manager = self.manager(Path(directory))
            request_type = SimpleNamespace(
                model_validate=lambda payload: SimpleNamespace(**payload)
            )
            with mock.patch(
                "backend.adapters.mediacrawler_adapter._runtime",
                return_value=(request_type, manager),
            ), mock.patch(
                "backend.adapters.mediacrawler_adapter._wait_for_session_process",
                new=mock.AsyncMock(return_value=True),
            ):
                # Verify international
                asyncio.run(session_verify("xhs", variant="international"))
                # Verify mainland
                asyncio.run(session_verify("xhs", variant="mainland"))

                status_intl = asyncio.run(session_status("xhs", variant="international"))
                status_main = asyncio.run(session_status("xhs", variant="mainland"))

            self.assertEqual(status_intl["profile_id"], "mediacrawler:xhs:international")
            self.assertEqual(status_main["profile_id"], "mediacrawler:xhs")


if __name__ == "__main__":
    unittest.main()
