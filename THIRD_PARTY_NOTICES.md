# Third-party notices

ArtCraft's CapCut Automation migration uses the following components only when
their licenses and resources are explicitly verified:

- CapCap original source/reference: Apache-2.0 (Copyright 2026 Hacht).
- FFmpeg/FFprobe: the license configuration of the bundled build.
- whisper.cpp: MIT; the bundled ggml-tiny model remains subject to the OpenAI
  Whisper model card terms.
- Argos Translate/CTranslate2 and SentencePiece: their respective open-source
  licenses; the bundled OPUS-MT en→vi model is CC-BY 4.0.
- Tesseract language data: Apache-2.0 and the exact tessdata terms. The OCR
  executable is not bundled until a redistributable Windows build is verified.
- RapidOCR 1.2.3 and its bundled PP-OCRv3 ONNX models: Apache-2.0; ONNX
  Runtime, OpenCV, NumPy, Pillow, Shapely and pyclipper retain their package
  licenses in the ArtCraft-owned Python runtime.
- Faster-Whisper, SenseVoice and Piper voices: not bundled by this migration;
  each optional resource must carry its own license. Sherpa-ONNX 1.13.7 is
  bundled only as the offline speaker-worker runtime under ArtCraft resources;
  its segmentation/embedding model files remain optional and require their
  model-specific terms before activation.

No CapCap model, binary, virtual environment, cache, output, credential, or
user data is bundled by the current migration. Optional models must be placed
under ArtCraft's model manager and pass the ArtCraft resource manifest before
they can be reported as available.
