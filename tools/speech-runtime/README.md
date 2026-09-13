# ArtCraft Speech Runtime

This is ArtCraft's local zero-shot voice-cloning runtime.  It vendors the
upstream Apache-2.0 `omnivoice` Python package at commit
`08be0b4ccbac3e13e374e86fbfead4b4cac343e2`, not the AGPL VoiceStudio
application.

The runtime keeps profiles under `ARTCRAFT_SPEECH_DATA_DIR`; reference audio
never needs to be uploaded to a cloud service.  A profile stores the reference
clip and can omit its reference transcript: OmniVoice will transcribe the
reference locally when first used.

The model itself is intentionally not bundled. Its selected artifact and
tokenizer have their own license terms. The ArtCraft UI records explicit model
terms acceptance locally before synthesis; sufficient local disk space is also
required. `ARTCRAFT_SPEECH_MODEL_TERMS_ACCEPTED=1` remains available only for
headless development.

Development launch, after installing this project's Python 3.11 dependencies:

```powershell
$env:ARTCRAFT_SPEECH_MODEL_TERMS_ACCEPTED = '1'
$env:ARTCRAFT_SPEECH_DATA_DIR = "$env:APPDATA\\ArtCraft\\speech"
py -3.11 .\\tools\\speech-runtime\\src\\artcraft_speech_runtime.py
```

The service listens only on `127.0.0.1:3900` by default and implements the
small profile/TTS contract consumed by ArtCraft.  It is not an HTTP exposure
for use outside the local machine.
