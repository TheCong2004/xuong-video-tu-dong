# CapCap → ArtCraft migration manifest

This document records the read-only inventory of `D:\\capcutpolot\\CapCap` and the
destination owned by ArtCraft. The CapCap directory is a source reference only;
ArtCraft production code must not resolve, import, spawn, or depend on it.

## License boundary

- CapCap original source: Apache-2.0 (`CapCap/LICENSE`, copyright Hacht 2026).
- Third-party runtime components retain their own licenses. The inventory in
  `CapCap/THIRD_PARTY_LICENSES.md` names PySide6/Qt, RapidOCR, Sherpa-ONNX,
  Faster-Whisper, CTranslate2, Piper, Edge-TTS, FFmpeg, mpv, CUDA/cuDNN, and
  model/voice assets.
- No model, binary, font, credential, cookie, cache, output, or virtualenv has
  been copied from CapCap. Models are absent from the checked-in CapCap tree
  (`models/faster_whisper/README.txt` only); ArtCraft owns separately verified
  resources under its own `resources/capcut-automation` tree.

## Source mapping

| CapCap source | Capability | ArtCraft destination | Resource | License disposition | Status |
| --- | --- | --- | --- | --- | --- |
| `app/video_processor.py` | FFmpeg render, subtitle/overlay filters | `crates/desktop/artcraft/src/services/pipeline/capcut_automation.rs` | ArtCraft FFmpeg resource | Reimplemented; bundled FFmpeg notice required | PORTED |
| `app/video_filter_chain.py` | color/LUT filter planning | native preset/filter graph | ArtCraft-owned preset/runtime | CapCap Apache source, reimplementation | REIMPLEMENTED |
| `app/subtitle_builder.py`, `app/layers/subtitle.py` | SRT/ASS and subtitle layers | native localization/ASS writer | none | Reimplemented | PORTED |
| `app/layers/blur.py`, `mask.py`, `sticker.py` | timed blur/mask/sticker overlays | native foreign-text treatment/stickers | user-selected files | User assets stay outside package | PORTED |
| `app/whisper_processor.py` | Faster-Whisper transcription | `services/pipeline/capcut_automation_transcription.rs` | ArtCraft engine manager + whisper.cpp adapter | whisper.cpp MIT; model card terms | REAL_FIXTURE_PASS |
| `app/sensevoice_processor.py` | SenseVoice transcription | same transcription abstraction | SenseVoice model | Model license/resource missing | BLOCKED_BY_LICENSE |
| `app/ocr_processor.py` | OCR frame scan and normalized regions | `services/pipeline/capcut_automation_ocr.rs` | RapidOCR ONNX worker and model bundle | RapidOCR wheel/model bundle Apache-2.0 | REAL_FIXTURE_PASS |
| `app/translator.py`, `app/translation/**` | subtitle translation | `services/pipeline/capcut_automation_translation.rs` | Argos/OPUS-MT local model + worker | Model and ArtCraft-owned portable Python runtime verified; one-file native bundle rejected after crash | REAL_FIXTURE_PASS_PORTABLE_RUNTIME |
| `app/tts_processor.py`, `app/engines/tts_adapter.py` | Piper speech | `services/pipeline/voice.rs` + ArtCraft engine manager | Piper Windows release + Vietnamese VIVOS voice | Piper MIT; voice model terms recorded in resource manifest | REAL_FIXTURE_PASS |
| `app/services/speaker_diarization_service.py` | Sherpa-ONNX speaker turns | `services/pipeline/capcut_automation_speaker.rs` + ArtCraft worker | diarization + embedding models | Sherpa-ONNX Apache-2.0; model-specific terms | REAL_FIXTURE_PASS |
| `app/audio_mixer.py`, `app/engines/audio_mix_adapter.py` | mix/duck/replace audio | native FFmpeg audio policy | ArtCraft FFmpeg | Reimplemented | REIMPLEMENTED |
| `app/workflows/**` | workflow planning | native job manager/workflow planner | ArtCraft AppData job cache | Reimplementation required | NOT_STARTED |
| `app/layers/timeline.py`, `ui/views/editor/timeline.py` | timeline/layer editing | `NativeAutomationWorkspace` + timeline components | ArtCraft project data | UI/data parity required | NOT_STARTED |
| `ui/controllers/**`, `ui/views/**` | PySide UI/controllers | React/Tauri pages and components | none | UI reimplementation; no PySide port | NOT_NEEDED |

## Current ArtCraft gates

The existing native renderer, queue, progress/cancel/retry, preview cache,
manual subtitle/ASS, hook, blur/cover/sticker, FFprobe, atomic output, and
SHA256/MD5 receipt paths remain enabled. The following are deliberately *not*
claimed as implemented until a real engine and fixture pass exists:

`WHISPER_SENSEVOICE` and `FULL_TIMELINE_EDITOR` remain outside the selected
production path. Speaker-to-voice assignment is implemented and covered by
targeted overlap tests; the selected Piper Vietnamese voice is verified by a
real offline fixture.

The ArtCraft-owned engine control plane is now present at
`capcut_automation_engine_manager.rs` with Tauri commands for listing,
downloading, cancelling, verifying, and removing model artifacts. The real
transcription adapter executes only a verified Whisper.cpp CLI and parses its
timestamped JSON output. Whisper.cpp v1.9.1 was built from the official MIT
source for Windows x64; the `ggml-tiny` model was verified and a local JFK
fixture produced one non-empty timestamped segment. The Argos adapter executes
an ArtCraft-owned portable Python runtime and worker with the verified OPUS-MT
model; an English fixture translated offline while preserving segment IDs. A
PyInstaller one-file bundle was rejected after a reproducible `0xC0000005`
  crash. The OCR adapter now executes an ArtCraft-owned RapidOCR ONNX worker
  with bundled detector/recognizer/classifier models and parses normalized
  regions; a synthetic 800x240 fixture produced one real region offline. The
  Tesseract TSV parser remains for compatibility evidence, but Tesseract is not
  required by the selected OCR path. OCR translation is verified through the
  local RapidOCR → Argos fixture. Speaker detection, voice assignment, and TTS
  resolve only through the ArtCraft-owned engine manager. Piper speech is now
  verified offline with a Vietnamese voice;
  speaker-to-voice assignment passes targeted overlap tests without a cloud or
  legacy fallback.

## Resource ownership target

Future resources must resolve only through ArtCraft's packaged resource,
development runtime, or AppData model directories. No absolute CapCap path,
symlink, junction, `PYTHONPATH`, port `8765`, or CapCap subprocess is allowed.
