import json
import os
from pathlib import Path

import pytest

from src.service.publish_draft import DraftPublishError, publish_draft


def _write_staging(root: Path, draft_id: str = "floword-draft") -> Path:
    staging = root / "staging" / draft_id
    video = staging / "assets" / "videos" / "source.mp4"
    audio = staging / "assets" / "audios" / "voice.mp3"
    video.parent.mkdir(parents=True)
    audio.parent.mkdir(parents=True)
    video.write_bytes(b"video")
    audio.write_bytes(b"audio")
    content = {
        "id": "content-project-id",
        "name": "Floword project",
        "duration": 2_000_000,
        "materials": {
            "videos": [{"id": "video-1", "path": str(video)}],
            "audios": [{"id": "audio-1", "path": str(audio)}],
            "texts": [{"id": "caption-1", "content": "hello"}],
        },
        "tracks": [
            {"type": "video", "segments": [{"material_id": "video-1"}]},
            {"type": "audio", "segments": [{"material_id": "audio-1"}]},
            {"type": "text", "segments": [{"material_id": "caption-1"}]},
        ],
    }
    for name in ("draft_content.json", "draft_info.json"):
        (staging / name).write_text(json.dumps(content), encoding="utf-8")
    (staging / "draft_meta_info.json").write_text(
        json.dumps(
            {
                "draft_id": "stale-template-id",
                "draft_fold_path": "C:/stale/template/draft",
                "draft_root_path": "C:/stale/template",
            }
        ),
        encoding="utf-8",
    )
    return staging


def test_publish_is_atomic_self_contained_and_indexed(tmp_path: Path, monkeypatch):
    staging = _write_staging(tmp_path)
    desktop_root = tmp_path / "CapCut" / "User Data" / "Projects" / "com.lveditor.draft"
    desktop_root.mkdir(parents=True)
    (desktop_root / "root_meta_info.json").write_text(
        json.dumps({"all_draft_store": [], "draft_ids": 0, "root_path": str(desktop_root)}),
        encoding="utf-8",
    )
    monkeypatch.setenv("CAPCUT_DESKTOP_DRAFT_ROOT", str(desktop_root))

    result = publish_draft(str(staging), "floword-draft")

    final = desktop_root / "floword-draft"
    assert result["final_path"] == str(final.resolve())
    assert final.is_dir()
    assert not list(desktop_root.glob(".floword-draft.publishing-*"))
    content = json.loads((final / "draft_content.json").read_text(encoding="utf-8"))
    assert content["materials"]["videos"][0]["path"] == str((final / "assets/videos/source.mp4").resolve())
    assert content["materials"]["audios"][0]["path"] == str((final / "assets/audios/voice.mp3").resolve())
    meta = json.loads((final / "draft_meta_info.json").read_text(encoding="utf-8"))
    assert meta["draft_id"] == "content-project-id"
    assert meta["draft_fold_path"] == str(final.resolve())
    assert meta["draft_root_path"] == str(desktop_root.resolve())
    index = json.loads((desktop_root / "root_meta_info.json").read_text(encoding="utf-8"))
    assert any(item["draft_fold_path"] == str(final.resolve()) for item in index["all_draft_store"])
    assert result["media"]["video"] == 1
    assert result["media"]["audio"] == 1
    assert result["media"]["captions"] == 1


def test_publish_rejects_missing_media_reference(tmp_path: Path, monkeypatch):
    staging = _write_staging(tmp_path)
    (staging / "assets/videos/source.mp4").unlink()
    desktop_root = tmp_path / "desktop-root"
    desktop_root.mkdir()
    monkeypatch.setenv("CAPCUT_DESKTOP_DRAFT_ROOT", str(desktop_root))

    with pytest.raises(DraftPublishError) as raised:
        publish_draft(str(staging), "floword-draft")

    assert raised.value.code == "CAPCUT_MEDIA_REFERENCE_INVALID"
    assert not (desktop_root / "floword-draft").exists()


def test_publish_requires_resolvable_desktop_root(tmp_path: Path, monkeypatch):
    staging = _write_staging(tmp_path)
    monkeypatch.setenv("CAPCUT_DESKTOP_DRAFT_ROOT", str(tmp_path / "missing"))

    with pytest.raises(DraftPublishError) as raised:
        publish_draft(str(staging), "floword-draft")

    assert raised.value.code == "CAPCUT_DESKTOP_ROOT_NOT_FOUND"


@pytest.mark.skipif(os.name != "nt", reason="Windows extended paths only")
def test_publish_accepts_windows_extended_staging_path(tmp_path: Path, monkeypatch):
    staging = _write_staging(tmp_path)
    desktop_root = tmp_path / "desktop-root"
    desktop_root.mkdir()
    monkeypatch.setenv("CAPCUT_DESKTOP_DRAFT_ROOT", str(desktop_root))

    result = publish_draft("\\\\?\\" + str(staging), "floword-draft")

    assert Path(result["final_path"]).is_dir()
