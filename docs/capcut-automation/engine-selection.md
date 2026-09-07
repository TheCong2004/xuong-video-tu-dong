# Local engine selection

ArtCraft owns the runtime boundary.  Every executable and model is installed
under the ArtCraft app-data `capcut-automation` directory and is verified by
the resource resolver before use.  This document is a selection record, not a
claim that the resources are already installed.

| Capability | Selected engine | ArtCraft path | Status |
| --- | --- | --- | --- |
| Transcription | whisper.cpp CLI + ggml model | `bin/whisper-cli.exe`, `models/whisper/ggml-tiny.bin` | Adapter ready; resource install pending |
| Translation | Argos Translate package | `models/argos/` | Package install pending; package license must be recorded |
| OCR | Tesseract + language data | `bin/tesseract.exe`, `models/tesseract/eng.traineddata` | Adapter boundary pending; resource install pending |
| TTS | Piper | `bin/piper.exe`, `voices/piper/` | License review required |
| Speaker turns | Sherpa-ONNX | `models/diarization/` | Model license review required |

The transcription adapter executes only the verified `whisper-cli.exe` and
reads its JSON output.  It has no Python, CapCap, cloud, or payment fallback.
Until the binary and model are downloaded from an approved upstream release,
the engine remains `NOT_INSTALLED` and the UI must report that state.

Approved upstream references for the initial review:

- whisper.cpp: <https://github.com/ggml-org/whisper.cpp> (MIT source; model terms are separate).
- Argos Translate: <https://github.com/argosopentech/argos-translate> (offline package runtime; each package carries its own metadata).
- Tesseract: <https://tesseract-ocr.github.io/> (Apache-2.0 source; language data is separately licensed).

No resource is copied from `D:\capcutpolot\CapCap`, and no CapCap process or
environment variable is used by the ArtCraft implementation.
