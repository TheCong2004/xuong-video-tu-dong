"""Local ArtCraft voice-profile and zero-shot TTS service.

This service deliberately exposes only loopback profile/TTS endpoints.  It is
new ArtCraft orchestration around the Apache-2.0 OmniVoice library and does not
reuse VoiceStudio's AGPL application, database, API routers, or UI.
"""

from __future__ import annotations

import asyncio
import json
import os
import sys
import tempfile
import uuid
from datetime import UTC, datetime
from pathlib import Path
from typing import Any

import numpy as np
import soundfile as sound_file
from fastapi import BackgroundTasks, FastAPI, File, Form, HTTPException, UploadFile
from fastapi.responses import FileResponse
from pydantic import BaseModel, Field

RUNTIME_ROOT = Path(__file__).resolve().parents[1]
VENDOR_ROOT = RUNTIME_ROOT / "vendor"
if str(VENDOR_ROOT) not in sys.path:
  sys.path.insert(0, str(VENDOR_ROOT))

DEFAULT_DATA_DIR = Path(os.environ.get("ARTCRAFT_SPEECH_DATA_DIR", Path.home() / "AppData" / "Roaming" / "ArtCraft" / "speech"))
DEFAULT_MODEL = os.environ.get("ARTCRAFT_SPEECH_MODEL", "k2-fsa/OmniVoice")
MAX_REFERENCE_BYTES = 100 * 1024 * 1024


class SpeechRequest(BaseModel):
  model: str = DEFAULT_MODEL
  input: str = Field(min_length=1, max_length=16_000)
  voice: str = ""
  response_format: str = "wav"
  speed: float = Field(default=1.0, ge=0.5, le=2.0)
  language: str | None = None
  instruct: str | None = Field(default=None, max_length=1_000)
  duration: float | None = Field(default=None, gt=0.0, le=120.0)


class SpeechRuntime:
  def __init__(self, data_dir: Path) -> None:
    self.data_dir = data_dir
    self.profiles_dir = data_dir / "profiles"
    self.profile_index = data_dir / "profiles.json"
    self.terms_path = data_dir / "model_terms.json"
    self.model: Any | None = None
    self.model_id: str | None = None
    self.lock = asyncio.Lock()
    self.data_dir.mkdir(parents=True, exist_ok=True)
    self.profiles_dir.mkdir(parents=True, exist_ok=True)

  def model_terms_accepted(self) -> bool:
    if os.environ.get("ARTCRAFT_SPEECH_MODEL_TERMS_ACCEPTED") == "1":
      return True
    try:
      document = json.loads(self.terms_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
      return False
    return document.get("accepted") is True

  def accept_model_terms(self) -> None:
    temporary = self.terms_path.with_suffix(".json.partial")
    temporary.write_text(json.dumps({"accepted": True, "acceptedAt": datetime.now(UTC).isoformat()}), encoding="utf-8")
    temporary.replace(self.terms_path)

  def profiles(self) -> list[dict[str, Any]]:
    if not self.profile_index.is_file():
      return []
    try:
      raw = json.loads(self.profile_index.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
      raise HTTPException(status_code=500, detail="ARTCRAFT_SPEECH_PROFILE_STORE_INVALID") from error
    return raw if isinstance(raw, list) else []

  def write_profiles(self, profiles: list[dict[str, Any]]) -> None:
    temp_path = self.profile_index.with_suffix(".json.partial")
    temp_path.write_text(json.dumps(profiles, ensure_ascii=False, indent=2), encoding="utf-8")
    temp_path.replace(self.profile_index)

  def profile(self, profile_id: str) -> dict[str, Any]:
    for profile in self.profiles():
      if profile.get("id") == profile_id:
        return profile
    raise HTTPException(status_code=404, detail="ARTCRAFT_SPEECH_PROFILE_NOT_FOUND")

  async def ensure_model(self, model_id: str) -> Any:
    if not self.model_terms_accepted():
      raise HTTPException(status_code=412, detail="ARTCRAFT_SPEECH_MODEL_TERMS_REQUIRED")
    async with self.lock:
      if self.model is not None and self.model_id == model_id:
        return self.model
      try:
        import torch
        from omnivoice import OmniVoice
      except ImportError as error:
        raise HTTPException(status_code=503, detail="ARTCRAFT_SPEECH_RUNTIME_NOT_INSTALLED") from error
      device = "cuda" if torch.cuda.is_available() else "cpu"
      dtype = torch.float16 if device == "cuda" else torch.float32
      try:
        self.model = await asyncio.to_thread(OmniVoice.from_pretrained, model_id, device_map=device, dtype=dtype)
      except Exception as error:  # Model downloads/errors must remain actionable to the caller.
        raise HTTPException(status_code=503, detail=f"ARTCRAFT_SPEECH_MODEL_LOAD_FAILED:{type(error).__name__}") from error
      self.model_id = model_id
      return self.model


runtime = SpeechRuntime(DEFAULT_DATA_DIR)
app = FastAPI(title="ArtCraft Speech Runtime", version="0.1.0")


@app.get("/health")
def health() -> dict[str, Any]:
  return {
    "service": "artcraft-speech",
    "status": "ready" if runtime.model is not None else "ready_without_model",
    "model": runtime.model_id,
    "dataDir": str(runtime.data_dir),
  }


@app.get("/.well-known/voicestudio-speech")
def capabilities() -> dict[str, Any]:
  return {
    "service": "artcraft-speech",
    "profiles": True,
    "zeroShotVoiceClone": True,
    "localOnly": True,
    "modelTermsRequired": True,
  }


@app.post("/settings/model-terms")
def accept_model_terms() -> dict[str, bool]:
  runtime.accept_model_terms()
  return {"accepted": True}


@app.get("/profiles")
def list_profiles() -> list[dict[str, Any]]:
  return runtime.profiles()


@app.post("/profiles")
async def create_profile(
  audio: UploadFile = File(...),
  name: str = Form(..., min_length=1, max_length=120),
  consent: bool = Form(...),
  reference_text: str | None = Form(default=None, max_length=4_000),
) -> dict[str, Any]:
  if not consent:
    raise HTTPException(status_code=422, detail="ARTCRAFT_SPEECH_CONSENT_REQUIRED")
  suffix = Path(audio.filename or "reference.wav").suffix.lower()
  if suffix not in {".wav", ".mp3", ".m4a", ".flac", ".ogg"}:
    raise HTTPException(status_code=415, detail="ARTCRAFT_SPEECH_REFERENCE_FORMAT_UNSUPPORTED")
  profile_id = str(uuid.uuid4())
  profile_path = runtime.profiles_dir / f"{profile_id}{suffix}"
  total = 0
  try:
    with profile_path.open("wb") as target:
      while chunk := await audio.read(1024 * 1024):
        total += len(chunk)
        if total > MAX_REFERENCE_BYTES:
          raise HTTPException(status_code=413, detail="ARTCRAFT_SPEECH_REFERENCE_TOO_LARGE")
        target.write(chunk)
  except Exception:
    profile_path.unlink(missing_ok=True)
    raise
  if total == 0:
    profile_path.unlink(missing_ok=True)
    raise HTTPException(status_code=422, detail="ARTCRAFT_SPEECH_REFERENCE_EMPTY")
  profile = {
    "id": profile_id,
    "name": name.strip(),
    "referenceAudio": str(profile_path),
    "referenceText": reference_text.strip() if reference_text and reference_text.strip() else None,
    "consent": True,
    "createdAt": datetime.now(UTC).isoformat(),
  }
  profiles = runtime.profiles()
  profiles.append(profile)
  runtime.write_profiles(profiles)
  return profile


@app.post("/v1/audio/speech")
async def synthesize(request: SpeechRequest, background_tasks: BackgroundTasks) -> FileResponse:
  profile = runtime.profile(request.voice) if request.voice.strip() else None
  model = await runtime.ensure_model(request.model)
  output_fd, output_name = tempfile.mkstemp(prefix="artcraft-speech-", suffix=".wav")
  os.close(output_fd)
  output = Path(output_name)
  try:
    audio = await asyncio.to_thread(
      model.generate,
      text=request.input,
      language=request.language,
      ref_audio=profile["referenceAudio"] if profile else None,
      ref_text=profile["referenceText"] if profile else None,
      instruct=request.instruct,
      duration=request.duration,
      speed=request.speed,
    )
    samples = np.asarray(audio[0], dtype=np.float32)
    await asyncio.to_thread(sound_file.write, output, samples, model.sampling_rate)
  except HTTPException:
    output.unlink(missing_ok=True)
    raise
  except Exception as error:
    output.unlink(missing_ok=True)
    raise HTTPException(status_code=500, detail=f"ARTCRAFT_SPEECH_SYNTHESIS_FAILED:{type(error).__name__}") from error
  background_tasks.add_task(output.unlink, missing_ok=True)
  return FileResponse(output, media_type="audio/wav", filename="speech.wav", background=background_tasks)


if __name__ == "__main__":
  import uvicorn

  uvicorn.run(app, host="127.0.0.1", port=int(os.environ.get("ARTCRAFT_SPEECH_PORT", "3900")), log_level="info")
