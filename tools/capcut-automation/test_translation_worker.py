"""Protocol-level regression tests for the offline translation worker.

The tests mock the optional native packages so they are deterministic and do
not download or require a staged model.  They exercise the exact boundary that
previously raised a PyBind11 cast error in production.
"""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import types
import unittest
from pathlib import Path


WORKER_PATH = Path(__file__).with_name("translation_worker.py")


def load_worker() -> types.ModuleType:
    spec = importlib.util.spec_from_file_location("artcraft_translation_worker_test", WORKER_PATH)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class WorkerBoundaryTests(unittest.TestCase):
    def test_production_batch_uses_nested_string_lists_and_piece_decode(self) -> None:
        calls: dict[str, object] = {}

        class FakeTokenizer:
            def __init__(self, model_file: str) -> None:
                calls["model_file"] = model_file

            def encode_as_pieces(self, text: str) -> list[str]:
                return ["▁" + token for token in text.split()]

            def decode_pieces(self, pieces: list[str]) -> str:
                calls["decode_pieces"] = pieces
                return " ".join(piece.removeprefix("▁") for piece in pieces)

        class FakeBatch:
            def __init__(self, pieces: list[str]) -> None:
                self.hypotheses = [pieces]

        class FakeTranslator:
            def __init__(self, model: str, device: str) -> None:
                calls["model"] = model
                calls["device"] = device

            def translate_batch(self, encoded: list[list[str]], **kwargs: object) -> list[FakeBatch]:
                calls["encoded"] = encoded
                calls["kwargs"] = kwargs
                return [FakeBatch(tokens) for tokens in encoded]

        fake_spm = types.SimpleNamespace(SentencePieceProcessor=FakeTokenizer)
        fake_ct2 = types.SimpleNamespace(Translator=FakeTranslator)
        old_spm = sys.modules.get("sentencepiece")
        old_ct2 = sys.modules.get("ctranslate2")
        sys.modules["sentencepiece"] = fake_spm  # type: ignore[assignment]
        sys.modules["ctranslate2"] = fake_ct2  # type: ignore[assignment]
        try:
            worker = load_worker()
            with tempfile.TemporaryDirectory() as directory:
                package = Path(directory)
                (package / "sentencepiece.model").write_bytes(b"fixture")
                (package / "model").mkdir()
                diagnostics: dict[str, object] = {}
                result = worker.translate(
                    package,
                    [{"id": "s1", "text": "Xin chào"}, {"id": "s2", "text": "Bạn khỏe không"}],
                    diagnostics,
                )

            self.assertEqual(calls["encoded"], [["▁Xin", "▁chào"], ["▁Bạn", "▁khỏe", "▁không"]])
            self.assertEqual(calls["kwargs"]["batch_type"], "examples")  # type: ignore[index]
            self.assertEqual(calls["kwargs"]["beam_size"], 4)  # type: ignore[index]
            self.assertEqual(calls["kwargs"]["no_repeat_ngram_size"], 3)  # type: ignore[index]
            self.assertEqual(calls["kwargs"]["repetition_penalty"], 1.1)  # type: ignore[index]
            self.assertEqual(calls["decode_pieces"], ["▁Bạn", "▁khỏe", "▁không"])
            self.assertEqual(diagnostics["outerType"], "list")
            self.assertEqual(diagnostics["firstItemType"], "list")
            self.assertEqual(diagnostics["firstTokenType"], "str")
            self.assertEqual(result[1]["translatedText"], "Bạn khỏe không")
        finally:
            if old_spm is None:
                sys.modules.pop("sentencepiece", None)
            else:
                sys.modules["sentencepiece"] = old_spm
            if old_ct2 is None:
                sys.modules.pop("ctranslate2", None)
            else:
                sys.modules["ctranslate2"] = old_ct2


if __name__ == "__main__":
    unittest.main()
