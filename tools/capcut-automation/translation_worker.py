"""ArtCraft-owned Argos/OPUS-MT worker.

The worker is intentionally a line-delimited JSON protocol so the Rust
supervisor can own its process and terminate it.  It reads only a staged
Argos model directory and never contacts a remote provider.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import tempfile
import zipfile
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")
if hasattr(sys.stdin, "reconfigure"):
    # Rust serializes WorkerRequest as UTF-8 bytes.  Windows pipes otherwise
    # inherit the active ANSI code page (often cp1252), corrupting CJK text
    # before SentencePiece sees it.
    sys.stdin.reconfigure(encoding="utf-8", errors="strict")


def _safe_message(exc: BaseException) -> str:
    return " ".join(str(exc).split())[:240]


def _set_stage(diagnostics: dict[str, object], stage: str) -> None:
    diagnostics["operationStage"] = stage


def _record_shape(diagnostics: dict[str, object], encoded: list[list[str]]) -> None:
    first_item = encoded[0] if encoded else None
    first_token = first_item[0] if first_item else None
    diagnostics.update({
        "outerType": type(encoded).__name__,
        "firstItemType": type(first_item).__name__ if first_item is not None else "none",
        "firstTokenType": type(first_token).__name__ if first_token is not None else "none",
        "emptyTokenSequenceCount": sum(1 for tokens in encoded if not tokens),
        "maxTokenCount": max((len(tokens) for tokens in encoded), default=0),
        "batchCount": len(encoded),
    })


def translate(package_root: Path, segments: list[dict], diagnostics: dict[str, object]) -> list[dict]:
    import ctranslate2
    import sentencepiece as spm
    extraction: tempfile.TemporaryDirectory[str] | None = None
    _set_stage(diagnostics, "INPUT_VALIDATION")
    if not isinstance(segments, list) or any(not isinstance(item, dict) for item in segments):
        raise TypeError("segments must be a list of objects")
    if package_root.is_file():
        _set_stage(diagnostics, "MODEL_EXTRACT")
        extraction = tempfile.TemporaryDirectory(prefix="artcraft-argos-")
        with zipfile.ZipFile(package_root) as archive:
            archive.extractall(extraction.name)
        roots = list(Path(extraction.name).glob("*/metadata.json"))
        if not roots:
            raise RuntimeError("Argos package metadata missing")
        package_root = roots[0].parent
    _set_stage(diagnostics, "TOKENIZER_LOAD")
    tokenizer = spm.SentencePieceProcessor(model_file=str(package_root / "sentencepiece.model"))
    _set_stage(diagnostics, "MODEL_LOAD")
    translator = ctranslate2.Translator(str(package_root / "model"), device="cpu")
    # CTranslate2's Python boundary requires a batch of token batches:
    # List[List[str]]. Passing a raw string or List[str] produces a PyBind11
    # cast error and must be rejected before the native call.
    # Materialise *every* token through the builtin ``str`` constructor.  A
    # SentencePiece result can be a proxy/string subclass on some Python
    # builds; CTranslate2's pybind boundary rejects that object even though it
    # prints like a string.  The native API contract is exactly List[List[str]].
    _set_stage(diagnostics, "TOKENIZE")
    encoded: list[list[str]] = [
        [str(token) for token in tokenizer.encode(str(item.get("text", "")), out_type=str)]
        for item in segments
    ]
    if not all(isinstance(tokens, list) and all(isinstance(token, str) for token in tokens) for tokens in encoded):
        raise TypeError("CTranslate2 input must be List[List[str]]")
    _record_shape(diagnostics, encoded)
    diagnostics.update({
        "inputPythonType": type(encoded).__name__,
        "batchType": "examples",
        "batchDepth": 2,
        "itemCount": len(encoded),
        "tokenCount": sum(len(tokens) for tokens in encoded),
        "allTokensAreStrings": all(isinstance(token, str) for tokens in encoded for token in tokens),
    })
    # Keep each native call to one example.  This preserves the required
    # List[List[str]] boundary while avoiding a pybind cast failure observed
    # with mixed/large OCR batches in the production pipeline.
    batches = []
    for index, tokens in enumerate(encoded):
        _set_stage(diagnostics, "TRANSLATE_BATCH")
        call_diagnostics = {**diagnostics, "hopIndex": diagnostics.get("hopIndex", 0), "itemCount": 1, "tokenCount": len(tokens)}
        print("CAPCUT_TRANSLATION_DIAGNOSTICS " + json.dumps(call_diagnostics, ensure_ascii=True, sort_keys=True), file=sys.stderr, flush=True)
        try:
            batches.extend(translator.translate_batch([tokens], batch_type="examples", replace_unknowns=True, beam_size=2, num_hypotheses=1))
        except Exception as exc:
            raise RuntimeError(f"CTranslate2 translate_batch failed at segment {index}/{len(encoded)}: {exc}") from exc
    result: list[dict] = []
    for item, batch in zip(segments, batches):
        _set_stage(diagnostics, "DECODE")
        tokens = batch.hypotheses[0] if batch.hypotheses else []
        diagnostics.update({
            "resultsType": type(batches).__name__,
            "hypothesisType": type(batch.hypotheses).__name__,
            "hypothesisTokenType": type(tokens[0]).__name__ if tokens else "none",
        })
        # Decode explicitly as SentencePiece pieces to avoid the overloaded
        # decode/decode_ids PyBind boundary for List[str] hypotheses.
        text = tokenizer.decode_pieces([str(token) for token in tokens]).replace("▁", " ").strip()
        result.append({"id": item.get("id"), "translatedText": text})
        continue
        text = tokenizer.decode(tokens).replace("▁", " ").strip()
        result.append({"id": item.get("id"), "translatedText": text})
    if extraction is not None:
        extraction.cleanup()
    _set_stage(diagnostics, "RESPONSE_SERIALIZE")
    print("CAPCUT_TRANSLATION_DIAGNOSTICS " + json.dumps(diagnostics, ensure_ascii=True, sort_keys=True), file=sys.stderr, flush=True)
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--model-package", required=True, type=Path)
    parser.add_argument("--model-id", default="argos-en-vi-1-9")
    parser.add_argument("--source-language", default="unknown")
    parser.add_argument("--effective-source-language", default="unknown")
    parser.add_argument("--route-kind", default="unknown")
    parser.add_argument("--hop-index", default=0, type=int)
    args = parser.parse_args()
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            payload = json.loads(line)
            segments = payload.get("segments", [])
            worker_sha = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
            diagnostics = {
                "sourceLanguage": args.source_language,
                "effectiveSourceLanguage": args.effective_source_language,
                "route": args.route_kind,
                "hopIndex": args.hop_index,
                "modelId": args.model_id,
                "modelSha256": hashlib.sha256(args.model_package.read_bytes()).hexdigest() if args.model_package.is_file() else "unknown",
                "workerSha": os.environ.get("CAPCUT_WORKER_SHA256", worker_sha),
            }
            output = {"provider": "argos-translate", "model": args.model_id, "segments": translate(args.model_package, segments, diagnostics)}
            print(json.dumps(output, ensure_ascii=False), flush=True)
        except Exception as exc:  # protocol errors are returned, never faked as success
            diagnostics["exceptionType"] = type(exc).__name__
            diagnostics["sanitizedMessage"] = _safe_message(exc)
            print("CAPCUT_TRANSLATION_DIAGNOSTICS " + json.dumps(diagnostics, ensure_ascii=True, sort_keys=True), file=sys.stderr, flush=True)
            print(json.dumps({"error": "CAPCUT_TRANSLATION_FAILED", "detail": str(exc)}), flush=True)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
