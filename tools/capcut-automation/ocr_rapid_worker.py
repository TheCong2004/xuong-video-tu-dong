"""ArtCraft-owned RapidOCR worker (single image and persistent stream)."""
from __future__ import annotations
import argparse, json, subprocess, sys
from pathlib import Path
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", line_buffering=True)

def regions(engine, frame, width, height):
    result, _ = engine(frame)
    out = []
    for index, item in enumerate(result or []):
        box, text, score = item
        xs = [float(point[0]) for point in box]; ys = [float(point[1]) for point in box]
        value = str(text).strip()
        if not value: continue
        out.append({"id": f"ocr-{index + 1}", "text": value,
                    "x": max(0.0, min(1.0, min(xs) / width)), "y": max(0.0, min(1.0, min(ys) / height)),
                    "width": max(0.0, min(1.0, (max(xs) - min(xs)) / width)),
                    "height": max(0.0, min(1.0, (max(ys) - min(ys)) / height)), "confidence": float(score)})
    return out

def main():
    parser = argparse.ArgumentParser(); parser.add_argument("--image"); parser.add_argument("--video"); parser.add_argument("--ffmpeg"); parser.add_argument("--width", type=int, default=960); parser.add_argument("--height", type=int, default=540); parser.add_argument("--fps", type=float, default=2.0); parser.add_argument("--min-ocr-interval-ms", type=int, default=2000); parser.add_argument("--change-threshold", type=float, default=0.012); args = parser.parse_args()
    import cv2
    import numpy as np
    from rapidocr_onnxruntime import RapidOCR
    engine = RapidOCR()
    if args.image:
        frame = cv2.imread(str(Path(args.image)))
        if frame is None: raise ValueError("CAPCUT_OCR_IMAGE_DECODE_FAILED")
        h, w = frame.shape[:2]; print(json.dumps({"engine":"rapidocr-onnxruntime", "regions":regions(engine, frame, w, h)}, ensure_ascii=False), flush=True); return 0
    if not args.video: raise ValueError("CAPCUT_OCR_INPUT_MISSING")
    w, h = args.width, args.height; size = w * h * 3
    child = subprocess.Popen([args.ffmpeg,"-hide_banner","-loglevel","error","-i",args.video,"-vf",f"fps={args.fps},scale={w}:{h}:flags=fast_bilinear","-pix_fmt","bgr24","-f","rawvideo","pipe:1"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    previous = None; last_ocr = -2000; index = 0; candidates = 0; ocr_count = 0
    try:
        while True:
            raw = child.stdout.read(size)
            if len(raw) != size: break
            frame = np.frombuffer(raw, dtype=np.uint8).reshape((h,w,3)); gray = cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY); small = cv2.resize(gray,(32,18),interpolation=cv2.INTER_AREA); delta = None if previous is None else cv2.absdiff(small, previous); diff = 1.0 if delta is None else float(delta.mean())/255.0
            # Whole-frame means can hide a small subtitle change. Add a coarse
            # tile signal while retaining the bounded 32x18 change buffer.
            tile_diff = diff if delta is None else max(float(delta[y:y + 6, x:x + 8].mean()) / 255.0 for y in (0, 6, 12) for x in (0, 8, 16, 24))
            media = round(index*1000.0/args.fps)
            # The change gate is the primary performance control.  Do not
            # force an OCR invocation merely because the interval elapsed:
            # long videos with static overlays would otherwise perform
            # thousands of redundant model calls and hit the stream timeout.
            candidate = previous is None or (media-last_ocr >= args.min_ocr_interval_ms and (diff >= args.change_threshold or tile_diff >= args.change_threshold * 1.5)); found = []
            if candidate: candidates += 1; found = regions(engine, frame, w, h); last_ocr = media; ocr_count += 1
            print(json.dumps({"frameIndex":index,"mediaPtsMs":media,"ocrPerformed":bool(candidate),"changeScore":diff,"tileChangeScore":tile_diff,"regions":found}, ensure_ascii=False), flush=True); previous=small; index += 1
        code = child.wait(timeout=30)
        if code != 0: raise RuntimeError(f"CAPCUT_OCR_SCAN_FFMPEG_FAILED:{code}")
        print(json.dumps({"summary":{"decodedFrameCount":index,"changeCandidateCount":candidates,"ocrFrameCount":ocr_count,"ffmpegScanProcessCount":1,"ocrWorkerStartCount":1,"onnxModelLoadCount":1}}), flush=True); return 0
    finally:
        if child.poll() is None: child.kill(); child.wait()
if __name__ == "__main__": raise SystemExit(main())
