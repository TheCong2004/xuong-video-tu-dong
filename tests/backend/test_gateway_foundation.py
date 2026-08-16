from __future__ import annotations

import unittest
import os
import json
import tempfile
from pathlib import Path
from unittest.mock import AsyncMock, MagicMock, patch

from backend.adapters import inkos_adapter
from backend.adapters import mediacrawler_adapter
from backend.adapters import omniroute_adapter
from backend.adapters import openmontage_adapter
from backend.adapters import vynaro_adapter
from backend.adapters import youwee_adapter
from backend.routers.capcut_router import register_asset
from backend.clients import omniroute_client
from backend.services.local_access import is_loopback
from backend.services.service_registry import (
    SERVICE_RUNTIME_STATUS,
    build_service_registry,
    set_service_runtime_status,
)


class OmniRouteClientTests(unittest.IsolatedAsyncioTestCase):
    def test_endpoint_key_never_falls_back_to_legacy_llm_example_key(self):
        with patch.dict(
            os.environ,
            {"OMNIROUTE_ENDPOINT_KEY": "", "OMNIROUTE_API_KEY": "", "LLM_API_KEY": "known-example"},
        ):
            self.assertEqual(omniroute_client.endpoint_key(), "")

    def test_plain_http_omniroute_must_be_loopback(self):
        with patch.dict(os.environ, {"OMNIROUTE_BASE_URL": "http://192.0.2.10:20128"}):
            with self.assertRaises(RuntimeError):
                omniroute_client.base_url()

    def test_ai_gateway_loopback_filter(self):
        self.assertTrue(is_loopback("127.0.0.1"))
        self.assertTrue(is_loopback("::1"))
        self.assertFalse(is_loopback("192.0.2.10"))

    async def test_health_returns_structured_offline_status_without_raw_exception(self):
        with patch(
            "backend.clients.omniroute_client.httpx.AsyncClient.get",
            new=AsyncMock(side_effect=RuntimeError("secret upstream detail")),
        ):
            result = await omniroute_client.health()

        self.assertEqual(result["status"], "offline")
        self.assertEqual(result["error"]["code"], "OMNIROUTE_UNAVAILABLE")
        self.assertNotIn("secret upstream detail", str(result))

    async def test_provider_id_is_encoded_before_upstream_request(self):
        with patch(
            "backend.adapters.omniroute_adapter.omniroute_client.get_provider",
            new=AsyncMock(return_value={"connection": {"id": "a/b"}}),
        ) as get_provider:
            result = await omniroute_adapter.provider("a/b")

        get_provider.assert_awaited_once_with("a%2Fb")
        self.assertEqual(result["connection"]["id"], "a/b")

    async def test_chat_forces_json_response_and_rejects_streaming(self):
        with self.assertRaisesRegex(ValueError, "Streaming chat"):
            await omniroute_adapter.chat(
                {"model": "test", "stream": True, "messages": []}
            )

        with patch(
            "backend.adapters.omniroute_adapter.omniroute_client.chat_completion",
            new=AsyncMock(return_value={"id": "chat-1"}),
        ) as chat:
            await omniroute_adapter.chat({"model": "test", "messages": []})
        self.assertFalse(chat.await_args.args[0]["stream"])


class CapCutAssetTransportTests(unittest.TestCase):
    def test_registers_only_real_nonempty_artifact_files(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "voice.wav"
            path.write_bytes(b"real-audio")
            token = register_asset("voice-artifact", str(path))
            self.assertGreater(len(token), 20)

            empty = Path(directory) / "empty.wav"
            empty.touch()
            with self.assertRaisesRegex(ValueError, "non-empty"):
                register_asset("empty-artifact", str(empty))


class ServiceRegistryTests(unittest.IsolatedAsyncioTestCase):
    def setUp(self):
        self.original_status = {key: value.copy() for key, value in SERVICE_RUNTIME_STATUS.items()}

    def tearDown(self):
        SERVICE_RUNTIME_STATUS.clear()
        SERVICE_RUNTIME_STATUS.update(self.original_status)

    async def test_registry_uses_real_probe_status_and_marks_capcut_ui_separate(self):
        set_service_runtime_status("capcut", "ready")
        set_service_runtime_status("mediacrawler", "offline", "not mounted")

        with patch(
            "backend.services.service_registry.omniroute_health",
            new=AsyncMock(return_value={"status": "ready", "status_code": 200}),
        ), patch(
            "backend.services.service_registry.youwee_health",
            new=AsyncMock(return_value={"status": "ready", "service": "youwee"}),
        ), patch("backend.services.service_registry.shutil.which", return_value=None):
            services = await build_service_registry()

        by_id = {service["id"]: service for service in services}
        self.assertEqual(by_id["omniroute"]["status"], "ready")
        self.assertEqual(by_id["capcut"]["status"], "ready")
        self.assertEqual(by_id["capcut"]["uiMode"], "separate")
        self.assertEqual(by_id["ffmpeg"]["status"], "offline")
        self.assertEqual(by_id["tts"]["status"], "not_configured")
        self.assertEqual(by_id["mediacrawler"]["status"], "offline")
        self.assertEqual(by_id["youwee"]["status"], "ready")


class MediaCrawlerAdapterTests(unittest.IsolatedAsyncioTestCase):
    async def test_research_without_login_is_auth_required_and_does_not_start(self):
        with patch.dict(os.environ, {"MEDIACRAWLER_COOKIES": ""}):
            with self.assertRaises(mediacrawler_adapter.MediaCrawlerAuthRequiredError):
                await mediacrawler_adapter.research_operation(
                    {"query": "video trend", "input_artifact_ids": ["art-runtime"]}
                )

    def test_research_result_json_is_parseable(self):
        with tempfile.TemporaryDirectory() as directory:
            result = Path(directory) / "research.json"
            result.write_text(json.dumps({"records": [{"title": "trend"}]}), encoding="utf-8")
            self.assertEqual(mediacrawler_adapter._parse_records(result), [{"title": "trend"}])

    async def test_start_uses_backend_schema_and_singleton_manager(self):
        request_type = type(
            "RequestType",
            (),
            {"model_validate": staticmethod(lambda payload: payload)},
        )
        manager = MagicMock()
        manager.start = AsyncMock(return_value=True)
        manager.get_status.return_value = {"status": "running", "platform": "bili"}

        with patch(
            "backend.adapters.mediacrawler_adapter._runtime",
            return_value=(request_type, manager),
        ):
            result = await mediacrawler_adapter.start({"platform": "bili"})

        manager.start.assert_awaited_once_with({"platform": "bili"})
        self.assertEqual(result["status"], "running")


class YouweeAdapterTests(unittest.IsolatedAsyncioTestCase):
    async def test_health_reports_offline_when_bridge_is_missing(self):
        with patch(
            "backend.adapters.youwee_adapter._run",
            new=AsyncMock(side_effect=youwee_adapter.YouweeUnavailableError("missing")),
        ):
            result = await youwee_adapter.health()

        self.assertEqual(result["status"], "offline")
        self.assertNotIn("missing", str(result))

    async def test_search_passes_validated_argv_to_bridge(self):
        with patch(
            "backend.adapters.youwee_adapter._run",
            new=AsyncMock(return_value={"videos": []}),
        ) as run:
            result = await youwee_adapter.search(" OpenAI ", 3)

        run.assert_awaited_once_with("search", "OpenAI", "3", timeout=90.0)
        self.assertEqual(result, {"videos": []})


class VynaroAdapterTests(unittest.IsolatedAsyncioTestCase):
    async def test_plan_payload_is_sent_to_real_bridge_command(self):
        payload = {"sources": ["a.mp4", "b.mp4"], "strategy": "concat"}
        with patch(
            "backend.adapters.vynaro_adapter._run",
            new=AsyncMock(return_value={"plans": [{"name": "output-concat"}]}),
        ) as run:
            result = await vynaro_adapter.build_video_plans(payload)

        run.assert_awaited_once_with("plan", payload, timeout=15.0)
        self.assertEqual(result["plans"][0]["name"], "output-concat")

    async def test_probe_passes_validated_path_to_bridge(self):
        with patch(
            "backend.adapters.vynaro_adapter._run",
            new=AsyncMock(return_value={"probe": {"durationSeconds": 1.0}}),
        ) as run:
            result = await vynaro_adapter.probe_video(" C:/media/test.mp4 ")

        run.assert_awaited_once_with(
            "probe",
            {"path": "C:/media/test.mp4"},
            timeout=30.0,
        )
        self.assertEqual(result["probe"]["durationSeconds"], 1.0)


class InkOSAdapterTests(unittest.IsolatedAsyncioTestCase):
    async def test_floword_plan_uses_story_studio_runtime_endpoint(self):
        payload = {"prompt": "demo", "sourceMetadata": {}, "scenes": {"scenes": []}}
        with patch(
            "backend.adapters.inkos_adapter._request",
            new=AsyncMock(return_value={"story": {}, "scriptRequest": {}}),
        ) as request:
            await inkos_adapter.plan_floword_story(payload)
        request.assert_awaited_once_with(
            "/api/v1/floword/story-plan", method="POST", payload=payload
        )

    async def test_project_id_is_encoded_before_upstream_request(self):
        with patch(
            "backend.adapters.inkos_adapter._request",
            new=AsyncMock(return_value={"book": {"id": "a/b"}}),
        ) as request:
            result = await inkos_adapter.project("a/b")

        request.assert_awaited_once_with("/api/v1/books/a%2Fb")
        self.assertEqual(result["book"]["id"], "a/b")

    async def test_health_hides_upstream_failure_details(self):
        with patch(
            "backend.adapters.inkos_adapter._request",
            new=AsyncMock(side_effect=inkos_adapter.InkOSUnavailableError("secret")),
        ):
            result = await inkos_adapter.health()

        self.assertEqual(result["status"], "offline")
        self.assertNotIn("secret", str(result))

    async def test_create_uses_existing_inkos_backend_action(self):
        payload = {"title": "Demo", "genre": "cozy"}
        with patch(
            "backend.adapters.inkos_adapter._request",
            new=AsyncMock(return_value={"status": "creating", "bookId": "demo"}),
        ) as request:
            result = await inkos_adapter.create_project(payload)

        request.assert_awaited_once_with("/api/v1/books/create", method="POST", payload=payload)
        self.assertEqual(result["bookId"], "demo")


class OpenMontageAdapterTests(unittest.IsolatedAsyncioTestCase):
    async def test_timeline_delegates_to_openmontage_core(self):
        build = MagicMock(return_value={"timelinePath": "timeline.json", "captionsPath": "captions.json"})
        with patch(
            "backend.adapters.openmontage_adapter._timeline_runtime",
            return_value=(ValueError, build),
        ):
            result = await openmontage_adapter.compose_floword_timeline({"sourceVideo": {}})

        build.assert_called_once_with({"sourceVideo": {}})
        self.assertEqual(result["timelinePath"], "timeline.json")

    async def test_timeline_maps_runtime_write_failure_without_swallowing_cause(self):
        build = MagicMock(side_effect=PermissionError("denied runtime output"))
        with patch(
            "backend.adapters.openmontage_adapter._timeline_runtime",
            return_value=(ValueError, build),
        ):
            with self.assertRaises(openmontage_adapter.OpenMontageComposeError) as raised:
                await openmontage_adapter.compose_floword_timeline({"sourceVideo": {}})

        self.assertEqual(raised.exception.code, "OPENMONTAGE_COMPOSE_FAILED")

    async def test_projects_use_real_summary_runtime(self):
        summaries = MagicMock(return_value=[{"project_id": "demo"}])
        with patch(
            "backend.adapters.openmontage_adapter._runtime",
            return_value=(MagicMock(), summaries, MagicMock(), MagicMock()),
        ):
            result = await openmontage_adapter.projects()

        summaries.assert_called_once_with()
        self.assertEqual(result["projects"][0]["project_id"], "demo")

    async def test_create_uses_existing_openmontage_project_action(self):
        root = MagicMock()
        projects_dir = MagicMock()
        project_dir = MagicMock()
        project_dir.exists.return_value = False
        projects_dir.__truediv__.return_value = project_dir
        pattern = MagicMock()
        pattern.fullmatch.return_value = True
        manifest = MagicMock()
        manifest.is_file.return_value = True
        root.__truediv__.return_value.__truediv__.return_value = manifest
        init_project = MagicMock()
        summarize = MagicMock(return_value={"project_id": "runtime-demo"})
        with patch(
            "backend.adapters.openmontage_adapter._create_runtime",
            return_value=(root, projects_dir, pattern, init_project, summarize),
        ):
            result = await openmontage_adapter.create_project(
                "runtime-demo", "Runtime Demo", "framework-smoke"
            )

        init_project.assert_called_once_with(
            "runtime-demo",
            title="Runtime Demo",
            pipeline_type="framework-smoke",
            pipeline_dir=projects_dir,
        )
        summarize.assert_called_once_with(project_dir)
        self.assertEqual(result["project_id"], "runtime-demo")


if __name__ == "__main__":
    unittest.main()
