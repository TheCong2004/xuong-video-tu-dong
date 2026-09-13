import { useEffect, useMemo, useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ModelStatusPanel } from "./ModelStatusPanel";
import {
  cancelNativeJob,
  listNativeAutomationJobs,
  listenNativeAutomationProgress,
  previewNativeAutomation,
  removeNativeJob,
  retryNativeJob,
  startNativeAutomationJob,
  transcribeCapCutAutomationLocal,
  translateCapCutAutomationLocal,
  ensureArtcraftSpeechRuntime,
  listVoiceStudioProfiles,
  uploadVoiceStudioClip,
  type VoiceStudioProfile,
  type LocalTranslationSegment,
  type LocalTranscriptSegment,
  type LocalAutomationPreset,
  type LocalAutomationReceipt,
  type NativeAutomationJob,
} from "../../api/pipelineClient";

type QueueState =
  | "QUEUED"
  | "PROBING"
  | "PREPARING"
  | "RENDERING"
  | "VERIFYING"
  | "COMPLETED"
  | "CANCEL_REQUESTED"
  | "FAILED"
  | "CANCELLED";
type QueueItem = {
  id: string;
  path: string;
  state: QueueState;
  progress: number;
  overallProgress?: number;
  stageProgress?: number;
  message?: string | null;
  stage?: string;
  processedMs?: number | null;
  expectedDurationMs?: number | null;
  decodedFrames?: number;
  changeCandidateFrames?: number;
  ocrFrames?: number;
  skippedDuplicateFrames?: number;
  rawDetectionCount?: number;
  mergedTrackCount?: number;
  processingFps?: number | null;
  speedRatio?: number | null;
  estimatedRemainingMs?: number | null;
  attempt?: number;
  attemptId?: string;
  receipt?: LocalAutomationReceipt;
  error?: string;
};

const DEFAULT_PRESET: LocalAutomationPreset = {
  schemaVersion: 1,
  presetId: "QUICK_LOCALIZED_VERTICAL_V1",
  name: "Vietnamese Short Video",
  mirrorHorizontal: true,
  playbackRate: 1.1,
  preserveAudioPitch: true,
  color: {
    brightness: 5,
    contrast: 3,
    saturation: -5,
    gamma: 0,
    hue: 0,
    temperature: 0,
    highlights: 0,
    shadows: 0,
  },
  localization: {
    enabled: true,
    sourceLanguage: "auto",
    targetLanguage: "vi",
    transcriptionEngine: "existing_or_local",
    translate: true,
    burnSubtitles: true,
    subtitleStyle: { fontName: "Arial", fontSize: 48, marginV: 80 },
    subtitlePath: null,
    manualCues: [],
  },
  hook: {
    enabled: true,
    text: "Hook mở đầu",
    startMs: 0,
    endMs: 3000,
    style: {
      fontName: "Arial",
      fontSize: 48,
      alignment: "top-center",
      margin: 80,
      outline: 2,
    },
  },
  foreignText: {
    enabled: true,
    detectionMode: "OCR_WITH_MANUAL_REVIEW",
    action: "BLUR",
    manualRegions: [],
    stickerPath: null,
    stickers: [],
  },
  output: {
    container: "mp4",
    videoCodec: "h264",
    audioCodec: "aac",
    ratio: "9:16",
    width: 1080,
    height: 1920,
    fps: "source_or_30",
    scaleMode: "fill",
    qualityPreset: "balanced",
  },
  audioPolicy: "KEEP_IF_RIGHTS_CONFIRMED",
  publishSchedule: null,
  autoTranscribe: true,
  sourceLanguage: "auto",
  targetLanguage: "vi",
  autoTranslate: true,
  autoOcr: true,
  ocrEngine: "rapidocr-onnxruntime",
  // OCR language is selected explicitly in the panel; never default a
  // Chinese job to the English model.
  ocrLanguages: [],
  ocrSampleIntervalMs: 1000,
  autoDiarize: true,
  autoTts: true,
  translationOutputMode: "DUBBED_AUDIO",
  originalAudioPolicy: "REMOVE_ORIGINAL",
  // TTS replaces the original track by default; mixing is explicit.
  ttsAudioMode: "REPLACE",
  originalAudioGain: 1,
  speakerVoiceAssignments: {},
  voiceProfileId: null,
  voiceProvider: "PIPER",
  voiceModel: "k2-fsa/OmniVoice",
};

export function NativeAutomationWorkspace() {
  const [preset, setPreset] = useState<LocalAutomationPreset>(DEFAULT_PRESET);
  const [items, setItems] = useState<QueueItem[]>([]);
  const [running, setRunning] = useState(false);
  const [previewing, setPreviewing] = useState(false);
  const [outputRoot, setOutputRoot] = useState("");
  const [transcript, setTranscript] = useState<LocalTranscriptSegment[]>([]);
  const [localAiBusy, setLocalAiBusy] = useState<"transcribe" | "translate" | null>(null);
  const [addingVideos, setAddingVideos] = useState(false);
  const [videoDialogError, setVideoDialogError] = useState<string | null>(null);
  const [voiceProfiles, setVoiceProfiles] = useState<VoiceStudioProfile[]>([]);
  const [voiceStudioMessage, setVoiceStudioMessage] = useState<string | null>(null);
  const [voiceProfileName, setVoiceProfileName] = useState("");
  const [voiceConsent, setVoiceConsent] = useState(false);
  const [voiceModelTermsAccepted, setVoiceModelTermsAccepted] = useState(false);

  const applyJob = (job: NativeAutomationJob) =>
    setItems((current) => {
      const next: QueueItem = {
        id: job.jobId,
        path: job.inputPath,
        state: job.state,
        progress: job.progress,
        overallProgress: job.overallProgress,
        stageProgress: job.stageProgress,
        message: job.message,
        stage: job.stage,
        processedMs: job.processedMs,
        expectedDurationMs: job.expectedDurationMs,
        decodedFrames: job.decodedFrames,
        changeCandidateFrames: job.changeCandidateFrames,
        ocrFrames: job.ocrFrames,
        skippedDuplicateFrames: job.skippedDuplicateFrames,
        rawDetectionCount: job.rawDetectionCount,
        mergedTrackCount: job.mergedTrackCount,
        processingFps: job.processingFps,
        speedRatio: job.speedRatio,
        estimatedRemainingMs: job.estimatedRemainingMs,
        attempt: job.attempt,
        attemptId: job.attemptId,
        receipt: job.receipt ?? undefined,
        error: job.error ?? undefined,
      };
      const index = current.findIndex((item) => item.id === job.jobId);
      if (index < 0) return [...current, next];
      const previous = current[index];
      if ((previous.attempt ?? 1) > (next.attempt ?? 1)) return current;
      if ((previous.attempt ?? 1) === (next.attempt ?? 1) && previous.state === "COMPLETED" && next.state !== "COMPLETED") return current;
      if ((previous.attempt ?? 1) === (next.attempt ?? 1) && (previous.progress ?? 0) > next.progress && next.state !== "FAILED" && next.state !== "CANCELLED") return current;
      const copy = [...current];
      copy[index] = { ...previous, ...next, progress: Math.max(previous.progress ?? 0, next.progress ?? 0) };
      return copy;
    });

  useEffect(() => {
    let stop: (() => void) | undefined;
    void listNativeAutomationJobs().then((jobs) => jobs.forEach(applyJob));
    void listenNativeAutomationProgress(applyJob).then((unlisten) => {
      stop = unlisten;
    });
    return () => stop?.();
  }, []);

  useEffect(() => {
    void ensureArtcraftSpeechRuntime()
      .then(listVoiceStudioProfiles)
      .then(setVoiceProfiles)
      .catch(() => {
        // VoiceStudio is optional; Piper/OmniRoute remain available when its
        // service is not running.
      });
  }, []);

  const refreshVoiceProfiles = async () => {
    try {
      await ensureArtcraftSpeechRuntime();
      setVoiceProfiles(await listVoiceStudioProfiles());
      setVoiceStudioMessage("Đã tải danh sách voice profile");
    } catch (error) {
      setVoiceStudioMessage(error instanceof Error ? error.message : "VoiceStudio chưa chạy");
    }
  };

  const uploadVoiceClip = async () => {
    if (!voiceConsent) {
      setVoiceStudioMessage("Xác nhận quyền sử dụng audio mẫu trước khi tạo giọng clone.");
      return;
    }
    if (!voiceModelTermsAccepted) {
      setVoiceStudioMessage("Xác nhận điều khoản model OmniVoice trước khi tạo giọng clone.");
      return;
    }
    const selected = await open({ multiple: false, directory: false, filters: [{ name: "Audio", extensions: ["wav", "mp3", "m4a", "flac", "ogg"] }] });
    if (typeof selected !== "string") return;
    try {
      const inferredName = selected.split(/[\\\\/]/).pop()?.replace(/\.[^.]+$/, "") || "Giọng ArtCraft";
      await ensureArtcraftSpeechRuntime(voiceModelTermsAccepted);
      const result = await uploadVoiceStudioClip({ path: selected, name: voiceProfileName.trim() || inferredName });
      const voiceId = typeof result.voice_id === "string" ? result.voice_id : typeof result.id === "string" ? result.id : null;
      if (voiceId) setPreset((current) => ({ ...current, voiceProvider: "ARTCRAFT_SPEECH", voiceProfileId: voiceId }));
      await refreshVoiceProfiles();
      setVoiceStudioMessage(voiceId ? "Đã tải clip và chọn voice profile mới" : "Đã tải clip lên VoiceStudio");
    } catch (error) {
      setVoiceStudioMessage(error instanceof Error ? error.message : "Không thể tải clip lên VoiceStudio");
    }
  };

  const queued = useMemo(
    () => items.filter((item) => item.state === "QUEUED"),
    [items],
  );
  const previewInput = items[0]?.path;
  const updateColor = (
    key: keyof LocalAutomationPreset["color"],
    value: number,
  ) =>
    setPreset((current) => ({
      ...current,
      color: { ...current.color, [key]: value },
    }));
  const updateOutput = (
    key: keyof LocalAutomationPreset["output"],
    value: string | number,
  ) =>
    setPreset((current) => ({
      ...current,
      output: { ...current.output, [key]: value },
    }));
  const setSourceLanguage = (value: string) =>
    setPreset((current) => ({
      ...current,
      sourceLanguage: value,
      localization: { ...current.localization, sourceLanguage: value },
      // RapidOCR is multilingual and does not consume Tesseract language
      // codes. Keep the language contract in sourceLanguage/ocrEngine rather
      // than sending a misleading chi_tra/chi_sim hint.
      ocrLanguages: current.ocrEngine?.toLowerCase().startsWith("rapidocr")
        ? []
        : value === "zt"
          ? ["chi_tra"]
          : value === "zh"
            ? ["chi_sim"]
            : value === "en"
              ? ["eng"]
              : [],
    }));
  const setSchedule = (enabled: boolean) =>
    setPreset((current) => ({
      ...current,
      publishSchedule: enabled
        ? current.publishSchedule ?? { timezone: "Asia/Ho_Chi_Minh", slots: ["11:30", "20:00"] }
        : null,
    }));
  const updateScheduleSlots = (value: string) =>
    setPreset((current) => ({
      ...current,
      publishSchedule: current.publishSchedule
        ? { ...current.publishSchedule, slots: value.split(",").map((slot) => slot.trim()).filter(Boolean) }
        : null,
    }));
  const addRegion = () =>
    setPreset((current) => ({
      ...current,
      foreignText: {
        ...current.foreignText,
        enabled: true,
        manualRegions: current.foreignText.manualRegions.length
          ? current.foreignText.manualRegions
          : [
              {
                startMs: 0,
                endMs: 3000,
                x: 0.1,
                y: 0.1,
                width: 0.3,
                height: 0.15,
              },
            ],
      },
    }));
  const updateRegion = (
    index: number,
    key: keyof LocalAutomationPreset["foreignText"]["manualRegions"][number],
    value: number,
  ) =>
    setPreset((current) => ({
      ...current,
      foreignText: {
        ...current.foreignText,
        manualRegions: current.foreignText.manualRegions.map(
          (region, regionIndex) =>
            regionIndex === index ? { ...region, [key]: value } : region,
        ),
      },
    }));

  const addVideos = async () => {
    if (addingVideos) return;
    setAddingVideos(true);
    setVideoDialogError(null);
    try {
      const selected = await open({
        multiple: true,
        directory: false,
        filters: [
          { name: "Video", extensions: ["mp4", "mov", "mkv", "webm", "avi"] },
        ],
      });
      const paths = Array.isArray(selected)
        ? selected
        : selected
          ? [selected]
          : [];
      setItems((current) => [
        ...current,
        ...paths
          .filter(
            (path): path is string =>
              typeof path === "string" &&
              !current.some((item) => item.path === path),
          )
          .map((path) => ({
            id: crypto.randomUUID(),
            path,
            state: "QUEUED" as const,
            progress: 0,
          })),
      ]);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.error("[CapCut Automation] failed to open video picker", error);
      setVideoDialogError(`Không mở được hộp thoại chọn video: ${message}`);
    } finally {
      setAddingVideos(false);
    }
  };

  const createFreshJobWithCurrentPreset = (item: QueueItem) => {
    // This deliberately creates a new identity. It never mutates or retries
    // the historical request stored on the failed card.
    setItems((current) => current.some((entry) => entry.path === item.path && entry.id !== item.id && entry.state === "QUEUED")
      ? current
      : [...current, { id: crypto.randomUUID(), path: item.path, state: "QUEUED", progress: 0 }]);
  };

  const addSubtitle = async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Subtitle", extensions: ["srt", "ass"] }],
    });
    if (typeof selected !== "string") return;
    setPreset((current) => ({
      ...current,
      localization: {
        ...current.localization,
        enabled: true,
        burnSubtitles: true,
        subtitlePath: selected,
      },
    }));
  };

  const addSticker = async () => {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Sticker", extensions: ["png", "webp"] }],
    });
    if (typeof selected !== "string") return;
    setPreset((current) => ({
      ...current,
      foreignText: {
        ...current.foreignText,
        enabled: true,
        action: "STICKER",
        stickerPath: selected,
        stickers: [
          ...(current.foreignText.stickers ?? []).filter((sticker) => sticker.path !== selected),
          { id: crypto.randomUUID(), path: selected, x: 0.1, y: 0.1, width: 0.25, height: 0.25, opacity: 1, startMs: 0, endMs: 3000, enabled: true },
        ],
      },
    }));
  };

  const addCue = () =>
    setPreset((current) => ({
      ...current,
      localization: {
        ...current.localization,
        enabled: true,
        burnSubtitles: true,
        manualCues: [
          ...(current.localization.manualCues ?? []),
          { startMs: 0, endMs: 3000, text: "", enabled: true },
        ],
      },
    }));
  const transcribe = async () => {
    if (!previewInput || localAiBusy) return;
    setLocalAiBusy("transcribe");
    try {
      const result = await transcribeCapCutAutomationLocal(previewInput, preset.localization.sourceLanguage === "auto" ? undefined : preset.localization.sourceLanguage);
      setTranscript(result.segments);
      setPreset((current) => ({ ...current, localization: { ...current.localization, enabled: true, burnSubtitles: true, manualCues: result.segments.map((segment) => ({ startMs: segment.start_ms, endMs: segment.end_ms, text: segment.text, enabled: true })) } }));
    } finally {
      setLocalAiBusy(null);
    }
  };
  const translate = async () => {
    if (!transcript.length || localAiBusy) return;
    setLocalAiBusy("translate");
    try {
      const result = await translateCapCutAutomationLocal(transcript as LocalTranslationSegment[], preset.localization.sourceLanguage || preset.sourceLanguage || "auto", preset.localization.targetLanguage || "vi");
      setTranscript(result.segments);
      setPreset((current) => ({ ...current, localization: { ...current.localization, enabled: true, burnSubtitles: true, translate: true, manualCues: result.segments.map((segment) => ({ startMs: segment.start_ms, endMs: segment.end_ms, text: segment.translated_text || segment.text, enabled: true })) } }));
    } finally {
      setLocalAiBusy(null);
    }
  };
  const updateCue = (index: number, patch: Partial<NonNullable<LocalAutomationPreset["localization"]["manualCues"]>[number]>) =>
    setPreset((current) => ({
      ...current,
      localization: {
        ...current.localization,
        manualCues: (current.localization.manualCues ?? []).map((cue, cueIndex) => cueIndex === index ? { ...cue, ...patch } : cue),
      },
    }));

  const runQueue = async () => {
    if (running || !queued.length) return;
    const source = (preset.localization.sourceLanguage || preset.sourceLanguage || "").trim();
    // Automatic detection is supported when Whisper transcription is enabled;
    // the backend resolves the effective language before selecting a route.
    if (!source || (source.toLowerCase() === "auto" && !preset.autoTranscribe)) {
      setItems((current) => current.map((item) => item.state === "QUEUED" ? { ...item, error: "CAPCUT_SOURCE_LANGUAGE_UNRESOLVED: không nhận diện được ngôn ngữ nguồn" } : item));
      return;
    }
    if (preset.autoTts && !preset.autoTranslate) {
      setItems((current) => current.map((item) => item.state === "QUEUED" ? { ...item, error: "CAPCUT_TTS_REQUIRES_TRANSLATION: hãy bật Dịch trước khi lồng tiếng" } : item));
      return;
    }
    const ocrEngine = (preset.ocrEngine || "rapidocr-onnxruntime").trim().toLowerCase();
    if (preset.autoOcr && ocrEngine !== "rapidocr-onnxruntime" && ocrEngine !== "rapidocr-onnx" && !(preset.ocrLanguages?.length)) {
      setItems((current) => current.map((item) => item.state === "QUEUED" ? { ...item, error: "CAPCUT_OCR_LANGUAGE_UNRESOLVED: chọn gói ngôn ngữ OCR" } : item));
      return;
    }
    setRunning(true);
    // Native jobs restored from persistence are already owned by the
    // backend FIFO. Re-submitting them on refresh produces a duplicate-ID
    // error and leaves a misleading local failure card.
    const persisted = new Map((await listNativeAutomationJobs()).map((job) => [job.jobId, job]));
    await Promise.all(
      queued.map(async (item) => {
        try {
          const existing = persisted.get(item.id);
          if (existing) {
            applyJob(existing);
            return;
          }
          applyJob(
            await startNativeAutomationJob({
              inputPath: item.path,
              outputRoot: outputRoot || undefined,
              pageName: "ArtCraft",
              jobId: item.id,
              requestId: item.id,
              preset,
            }),
          );
        } catch (error) {
          setItems((current) =>
            current.map((entry) =>
              entry.id === item.id
                ? {
                    ...entry,
                    state: "FAILED",
                    error:
                      error instanceof Error
                        ? error.message
                        : typeof error === "string"
                          ? error
                          : JSON.stringify(error),
                  }
                : entry,
            ),
          );
        }
      }),
    );
    setRunning(false);
  };

  const preview = async () => {
    if (!items.length || previewing) return;
    setPreviewing(true);
    await Promise.all(
      items.map(async (item) => {
        try {
          const result = await previewNativeAutomation(item.path, preset);
          setItems((current) =>
            current.map((entry) =>
              entry.id === item.id
                ? {
                    ...entry,
                    receipt: {
                      schemaVersion: 1,
                      jobId: entry.id,
                      requestId: entry.id,
                      inputSha256: "",
                      outputSha256: "",
                      outputMd5: "",
                      outputPath: result.path,
                      durationMs: 5000,
                      width: 540,
                      height: 540,
                      videoCodec: "h264",
                      audioCodec: "none",
                      playbackRate: preset.playbackRate,
                      subtitleBurned: preset.localization.burnSubtitles && (preset.localization.manualCues?.length ?? 0) > 0,
                      subtitleMode: (preset.localization.manualCues?.length ?? 0) > 0 ? "MANUAL" : "NONE",
                      subtitleCueCount: preset.localization.manualCues?.filter((cue) => cue.enabled).length ?? 0,
                      hookApplied: preset.hook.enabled,
                      foreignTextRegionsApplied: preset.foreignText.enabled ? preset.foreignText.manualRegions.length : 0,
                      terminal: false,
                      state: "PREVIEW",
                    },
                  }
                : entry,
            ),
          );
        } catch {
          /* Preview never changes terminal state. */
        }
      }),
    );
    setPreviewing(false);
  };

  const remove = (id: string) => {
    void removeNativeJob(id).then(() =>
      setItems((current) => current.filter((item) => item.id !== id)),
    );
  };

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-y-auto bg-[#0f171e] p-6 text-white">
      <div className="mb-5 flex items-center justify-between">
        <div>
          <h2 className="text-xl font-semibold">Tự động hóa nội bộ</h2>
          <p className="mt-1 text-sm text-white/55">
            Quy trình FFmpeg nội bộ của ArtCraft, hàng đợi FIFO, mặc định 3 video chạy song song (có thể cấu hình tối đa 10).
          </p>
        </div>
        <div className="flex items-center gap-3 rounded-xl border border-cyan-300/20 bg-cyan-300/5 px-3 py-2 text-xs">
          <span className="font-semibold text-cyan-100">Tự động toàn bộ</span>
          <Toggle label="Whisper" checked={Boolean(preset.autoTranscribe)} onChange={(value) => setPreset((current) => ({ ...current, autoTranscribe: value }))} />
          <Toggle label="Dịch" checked={Boolean(preset.autoTranslate)} onChange={(value) => setPreset((current) => ({ ...current, autoTranslate: value }))} />
          <Toggle label="OCR" checked={Boolean(preset.autoOcr)} onChange={(value) => setPreset((current) => ({ ...current, autoOcr: value }))} />
          <Toggle
            label="Giọng nói"
            checked={Boolean(preset.autoDiarize && preset.autoTts)}
            onChange={(value) => setPreset((current) => ({
              ...current,
              autoDiarize: value,
              autoTts: value,
              autoTranslate: value || Boolean(current.autoTranslate),
              localization: { ...current.localization, translate: value || current.localization.translate },
              translationOutputMode: value ? "DUBBED_AUDIO" : "SUBTITLE_ONLY",
              originalAudioPolicy: value ? "REMOVE_ORIGINAL" : "KEEP_ORIGINAL",
              ttsAudioMode: value ? "REPLACE" : current.ttsAudioMode,
            }))}
          />
        </div>
        <button
          type="button"
          onClick={() => setPreset(DEFAULT_PRESET)}
          className="rounded-lg border border-slate-700/70 bg-slate-900/50 px-3 py-2 text-xs text-slate-200 transition hover:border-cyan-300/40 hover:text-white"
        >
          Đặt lại cấu hình
        </button>
        <button
          type="button"
          disabled={running || !queued.length}
          onClick={() => void runQueue()}
          className="rounded-lg bg-violet-400 px-5 py-2.5 font-semibold text-[#160b24] shadow-[0_8px_20px_rgba(167,139,250,0.18)] transition hover:bg-violet-300 disabled:opacity-40"
        >
          Xử lý tự động toàn bộ
        </button>
      </div>
      <div className="grid grid-cols-1 gap-5 xl:grid-cols-3">
        <div className="space-y-4">
          <Panel title="Video / Hàng đợi">
            <button
              type="button"
              disabled={addingVideos}
              onClick={() => void addVideos()}
              className="w-full rounded-lg bg-cyan-400 px-3 py-2 text-sm font-semibold text-[#071018] shadow-[0_8px_20px_rgba(34,211,238,0.16)] transition hover:bg-cyan-300 disabled:cursor-wait disabled:opacity-60"
            >
              {addingVideos ? "Đang mở hộp thoại…" : "Thêm video"}
            </button>
            {videoDialogError && (
              <p className="mt-2 break-words text-xs text-rose-300" role="alert">
                {videoDialogError}
              </p>
            )}
            <div className="mt-3 max-h-72 space-y-2 overflow-y-auto">
              {items.map((item) => (
                <div
                  key={item.id}
                  className="rounded-lg border border-slate-700/70 bg-[#18212a] p-2 text-xs"
                >
                  <div className="flex justify-between gap-2">
                    <span className="truncate">
                      {item.path.split(/[\\/]/).pop()}
                    </span>
                    <span>
                      {item.state === "FAILED" || item.state === "CANCELLED"
                        ? item.state
                        : item.stage ?? item.state}
                    </span>
                  </div>
                  {(item.attempt ?? 1) > 1 && (
                    <p className="mt-1 text-[10px] text-white/45">
                      Lần thử {item.attempt}
                    </p>
                  )}
                  {item.processedMs != null &&
                    item.expectedDurationMs != null && (
                      <p className="mt-1 text-[10px] text-white/45">
                        {Math.round(item.processedMs / 1000)}s /{" "}
                        {Math.round(item.expectedDurationMs / 1000)}s
                      </p>
                    )}
                  {(item.decodedFrames ?? 0) > 0 && (
                    <p className="mt-1 text-[10px] text-cyan-100/70">
                      Khung: {item.decodedFrames} · OCR: {item.ocrFrames ?? 0} · Ứng viên: {item.changeCandidateFrames ?? 0} · Vùng: {item.mergedTrackCount ?? 0}
                    </p>
                  )}
                  {(item.receipt?.detectedLanguages?.length ?? 0) > 0 && (
                    <p className="mt-1 text-[10px] text-cyan-100/70">Ngôn ngữ: {item.receipt?.detectedLanguages?.join(", ")}</p>
                  )}
                  {item.estimatedRemainingMs != null && item.state !== "COMPLETED" && (
                    <p className="mt-1 text-[10px] text-white/45">Còn khoảng {Math.ceil(item.estimatedRemainingMs / 1000)}s</p>
                  )}
                  {(item.state === "PROBING" ||
                    item.state === "PREPARING" ||
                    item.state === "RENDERING" ||
                    item.state === "VERIFYING" ||
                    item.state === "CANCEL_REQUESTED") && (
                    <>
                      <div className="mt-2 h-1.5 rounded bg-white/10">
                        <div
                          className="h-full rounded bg-cyan-400"
                          style={{
                            width: `${Math.round(item.progress * 100)}%`,
                          }}
                        />
                      </div>
                      {item.state !== "CANCEL_REQUESTED" && (
                        <button
                          type="button"
                          onClick={() =>
                            void cancelNativeJob(item.id).then(applyJob)
                          }
                          className="mt-1 text-rose-300"
                        >
                          Hủy
                        </button>
                      )}
                    </>
                  )}
                  {item.error && (
                    <p className="mt-1 break-words text-rose-300">
                      {item.error}
                    </p>
                  )}
                  {(item.state === "FAILED" || item.state === "CANCELLED") && (
                    <>
                      <button
                        type="button"
                        disabled={Boolean(item.error?.includes("RETRY_DISABLED") || item.error?.includes("CAPCUT_SOURCE_LANGUAGE_UNRESOLVED") || item.error?.includes("CAPCUT_OCR_LANGUAGE_UNRESOLVED"))}
                        onClick={() => {
                          void retryNativeJob(item.id).then(applyJob).catch((error) =>
                            setItems((current) => current.map((entry) => entry.id === item.id
                              ? { ...entry, error: error instanceof Error ? error.message : "Không thể thử lại cấu hình cũ" }
                              : entry)),
                          );
                        }}
                        className="mr-2 mt-1 text-cyan-200 disabled:cursor-not-allowed disabled:text-white/30"
                        title="Chạy lại đúng cấu hình đã lưu trên công việc này"
                      >
                        Thử lại với cấu hình cũ
                      </button>
                      {(item.error?.includes("RETRY_DISABLED") || item.error?.includes("CAPCUT_SOURCE_LANGUAGE_UNRESOLVED") || item.error?.includes("CAPCUT_OCR_LANGUAGE_UNRESOLVED")) && (
                        <button
                          type="button"
                          onClick={() => createFreshJobWithCurrentPreset(item)}
                          className="mr-2 mt-1 text-emerald-300"
                        >
                          Tạo công việc mới với cấu hình hiện tại
                        </button>
                      )}
                      <button
                        type="button"
                        onClick={() => remove(item.id)}
                        className="mt-1 text-white/50"
                      >
                        Xóa
                      </button>
                    </>
                  )}
                  {item.state === "COMPLETED" && (
                    <button
                      type="button"
                      onClick={() => remove(item.id)}
                      className="mt-1 text-white/50"
                    >
                      Xóa
                    </button>
                  )}
                </div>
              ))}
            </div>
            <input
              value={outputRoot}
              onChange={(event) => setOutputRoot(event.target.value)}
              placeholder="Thư mục xuất (không bắt buộc)"
              className="mt-3 w-full rounded-lg border border-slate-700/70 bg-[#10171e] px-3 py-2 text-xs outline-none transition focus:border-cyan-300/60"
            />
          </Panel>
          <Panel title="Âm thanh">
            <select
              value={preset.audioPolicy}
              onChange={(event) =>
                setPreset((current) => ({
                  ...current,
                  audioPolicy: event.target
                    .value as LocalAutomationPreset["audioPolicy"],
                }))
              }
              className="w-full rounded bg-[#20232a] px-2 py-2 text-xs"
            >
              <option value="KEEP_IF_RIGHTS_CONFIRMED">
                Giữ âm thanh nếu đã xác nhận quyền
              </option>
              <option value="MUTE_ORIGINAL">Tắt âm thanh gốc</option>
              <option value="REPLACE_WITH_USER_AUDIO">
                Thay bằng âm thanh của bạn
              </option>
            </select>
            {preset.autoTts && (
              <select
                value={preset.ttsAudioMode ?? "REPLACE"}
                onChange={() =>
                  setPreset((current) => ({
                    ...current,
                    translationOutputMode: "DUBBED_AUDIO",
                    originalAudioPolicy: "REMOVE_ORIGINAL",
                    ttsAudioMode: "REPLACE",
                  }))
                }
                className="mt-2 w-full rounded bg-[#20232a] px-2 py-2 text-xs"
                aria-label="Chế độ âm thanh giọng nói"
              >
                <option value="REPLACE">Thay âm thanh gốc bằng giọng Việt</option>
              </select>
            )}
            {preset.autoTts && (
              <div className="mt-3 rounded-lg border border-cyan-300/20 bg-cyan-300/5 p-2">
                <label className="block text-xs text-white/75">
                  Bộ máy giọng nói
                  <select
                    value={preset.voiceProvider ?? "PIPER"}
                    onChange={(event) => setPreset((current) => ({ ...current, voiceProvider: event.target.value as "PIPER" | "ARTCRAFT_SPEECH" }))}
                    className="mt-1 w-full rounded bg-[#20232a] px-2 py-2 text-xs"
                    aria-label="Bộ máy giọng nói"
                  >
                    <option value="PIPER">Piper nội bộ</option>
                    <option value="ARTCRAFT_SPEECH">ArtCraft Voice Clone (OmniVoice)</option>
                  </select>
                </label>
                {(preset.voiceProvider ?? "PIPER") === "ARTCRAFT_SPEECH" && (
                  <>
                    <label className="mt-2 block text-xs text-white/75">
                      Tên voice profile mới
                      <input
                        value={voiceProfileName}
                        onChange={(event) => setVoiceProfileName(event.target.value)}
                        placeholder="Ví dụ: Giọng dẫn chuyện"
                        className="mt-1 w-full rounded bg-[#20232a] px-2 py-2 text-xs"
                      />
                    </label>
                    <label className="mt-2 flex items-start gap-2 text-[11px] text-white/70">
                      <input type="checkbox" aria-label="Xác nhận quyền audio mẫu" checked={voiceConsent} onChange={(event) => setVoiceConsent(event.target.checked)} className="mt-0.5" />
                      Tôi có quyền sử dụng audio mẫu và đồng ý tạo voice profile cục bộ.
                    </label>
                    <label className="mt-2 flex items-start gap-2 text-[11px] text-white/70">
                      <input type="checkbox" aria-label="Chấp nhận điều khoản model OmniVoice" checked={voiceModelTermsAccepted} onChange={(event) => setVoiceModelTermsAccepted(event.target.checked)} className="mt-0.5" />
                      Tôi đã đọc và chấp nhận điều khoản riêng của model OmniVoice để tổng hợp giọng cục bộ.
                    </label>
                  </>
                )}
                <label className="block text-xs text-white/75">
                  Voice profile VoiceStudio
                  <select
                    value={preset.voiceProfileId ?? ""}
                    onChange={(event) => setPreset((current) => ({ ...current, voiceProfileId: event.target.value || null }))}
                    disabled={(preset.voiceProvider ?? "PIPER") !== "ARTCRAFT_SPEECH"}
                    className="mt-1 w-full rounded bg-[#20232a] px-2 py-2 text-xs"
                  >
                    <option value="">Mặc định từ cấu hình môi trường</option>
                    {voiceProfiles.map((profile, index) => {
                      const id = typeof profile.voice_id === "string" ? profile.voice_id : typeof profile.id === "string" ? profile.id : "";
                      const label = typeof profile.name === "string" ? profile.name : typeof profile.profile_name === "string" ? profile.profile_name : id || `Voice ${index + 1}`;
                      return id ? <option key={id} value={id}>{label}</option> : null;
                    })}
                  </select>
                </label>
                <div className="mt-2 flex gap-2">
                  <button type="button" onClick={() => void refreshVoiceProfiles()} className="rounded border border-cyan-300/30 px-2 py-1 text-[11px] text-cyan-100">Tải profile</button>
                  <button type="button" onClick={() => void uploadVoiceClip()} className="rounded border border-cyan-300/30 px-2 py-1 text-[11px] text-cyan-100">Tải clip clone</button>
                </div>
                {voiceStudioMessage && <p className="mt-1 text-[10px] text-white/55">{voiceStudioMessage}</p>}
              </div>
            )}
            <p className="mt-2 text-xs text-white/55">
              {preset.autoTts
                ? "Dịch bằng giọng nói: thay âm thanh gốc bằng giọng Việt."
                : "Chỉ dịch phụ đề: giữ âm thanh gốc."}
            </p>
          </Panel>
        </div>
        <div className="space-y-4">
          <Panel title="Biến đổi / Màu sắc">
            <Toggle
              label="Lật ngang"
              checked={preset.mirrorHorizontal}
              onChange={(value) =>
                setPreset((current) => ({
                  ...current,
                  mirrorHorizontal: value,
                }))
              }
            />
            <Slider
              label="Độ sáng"
              value={preset.color.brightness}
              min={-100}
              max={100}
              valueLabel={formatSignedPercent(preset.color.brightness)}
              hint={`FFmpeg brightness: ${formatSignedDecimal(preset.color.brightness / 100)}`}
              onChange={(value) => updateColor("brightness", value)}
            />
            <Slider
              label="Độ tương phản"
              value={preset.color.contrast}
              min={-100}
              max={100}
              valueLabel={formatSignedPercent(preset.color.contrast)}
              hint={`FFmpeg contrast: ${(1 + preset.color.contrast / 100).toFixed(2)}`}
              onChange={(value) => updateColor("contrast", value)}
            />
            <Slider
              label="Độ bão hòa"
              value={preset.color.saturation}
              min={-100}
              max={100}
              valueLabel={formatSignedPercent(preset.color.saturation)}
              hint={`FFmpeg saturation: ${(1 + preset.color.saturation / 100).toFixed(2)}`}
              onChange={(value) => updateColor("saturation", value)}
            />
            <Slider
              label="Tốc độ phát"
              value={preset.playbackRate}
              min={0.5}
              max={2}
              step={0.05}
              valueLabel={formatPlaybackRate(preset.playbackRate)}
              hint="Range: 0.50x - 2.00x"
              onChange={(value) =>
                setPreset((current) => ({ ...current, playbackRate: value }))
              }
            />
            <div className="mt-2 flex flex-wrap gap-1.5" aria-label="Quick speed presets">
              {[1, 1.1, 1.25, 1.5, 2].map((rate) => (
                <button
                  key={rate}
                  type="button"
                  onClick={() =>
                    setPreset((current) => ({ ...current, playbackRate: rate }))
                  }
                  className={`rounded-md border px-2 py-1 text-[10px] transition ${
                    Math.abs(preset.playbackRate - rate) < 0.001
                      ? "border-cyan-300/70 bg-cyan-400/15 text-cyan-100"
                      : "border-slate-700/70 bg-slate-900/40 text-white/65 hover:border-cyan-300/50 hover:text-white"
                  }`}
                >
                  {formatPlaybackRate(rate)}
                </button>
              ))}
            </div>
          </Panel>
          <Panel title="Phụ đề / Hook mở đầu">
            <label className="mb-3 block text-xs text-white/75">
              Ngôn ngữ nguồn
              <select
                value={preset.localization.sourceLanguage || preset.sourceLanguage}
                onChange={(event) => setSourceLanguage(event.target.value)}
                className="mt-1 w-full rounded-lg border border-slate-700/70 bg-[#10171e] px-3 py-2 text-xs outline-none focus:border-cyan-300/60"
              >
                <option value="auto">Tự động phát hiện</option>
                <option value="zh">Tiếng Trung giản thể</option>
                <option value="zt">Tiếng Trung phồn thể</option>
                <option value="en">Tiếng Anh</option>
              </select>
              <span className="mt-1 block text-[10px] text-white/45">
                Giản thể dùng chi_sim; phồn thể dùng chi_tra và dịch qua tiếng Anh trước khi sang tiếng Việt.
              </span>
            </label>
            <div className="mb-3 flex flex-wrap gap-2">
              <button type="button" disabled={!previewInput || localAiBusy !== null} onClick={() => void transcribe()} className="rounded-lg border border-cyan-300/30 bg-cyan-300/5 px-3 py-2 text-xs text-cyan-100 disabled:opacity-50">{localAiBusy === "transcribe" ? "Đang nhận dạng…" : "Nhận dạng giọng nói"}</button>
              <button type="button" disabled={!transcript.length || localAiBusy !== null} onClick={() => void translate()} className="rounded-lg border border-violet-300/30 bg-violet-300/5 px-3 py-2 text-xs text-violet-100 disabled:opacity-50">{localAiBusy === "translate" ? "Đang dịch…" : "Dịch sang tiếng Việt"}</button>
            </div>
            <Toggle
              label="Bật phụ đề"
              checked={preset.localization.enabled}
              onChange={(value) =>
                setPreset((current) => ({
                  ...current,
                  localization: { ...current.localization, enabled: value },
                }))
              }
            />
            <Toggle
              label="Ghi phụ đề vào video"
              checked={preset.localization.burnSubtitles}
              onChange={(value) =>
                setPreset((current) => ({
                  ...current,
                  localization: {
                    ...current.localization,
                    burnSubtitles: value,
                  },
                }))
              }
            />
            <button
              type="button"
              onClick={() => void addSubtitle()}
              className="mt-3 rounded-lg border border-cyan-300/30 bg-cyan-300/5 px-3 py-2 text-xs text-cyan-100 transition hover:bg-cyan-300/10"
            >
              Nhập SRT/ASS
            </button>
            {preset.localization.subtitlePath && (
              <p className="mt-2 truncate text-[10px] text-white/50">
                {preset.localization.subtitlePath}
              </p>
            )}
            <div className="mt-3 rounded-lg border border-slate-700/70 bg-slate-950/20 p-2">
              <div className="flex items-center justify-between text-[10px] text-white/60">
                <span>Mốc phụ đề thủ công</span>
                <button type="button" onClick={addCue} className="text-cyan-200">Thêm mốc</button>
              </div>
              {(preset.localization.manualCues ?? []).map((cue, index) => (
                <div key={`cue-${index}`} className="mt-2 grid grid-cols-2 gap-1">
                  <NumberInput label="Bắt đầu (ms)" value={cue.startMs} onChange={(value) => updateCue(index, { startMs: value })} />
                  <NumberInput label="Kết thúc (ms)" value={cue.endMs} onChange={(value) => updateCue(index, { endMs: value })} />
                  <input aria-label={`Nội dung mốc ${index + 1}`} value={cue.text} onChange={(event) => updateCue(index, { text: event.target.value })} placeholder="Nội dung phụ đề" className="col-span-2 rounded bg-[#17191e] px-2 py-1.5 text-xs" />
                  <label className="col-span-2 flex items-center gap-2 text-[10px] text-white/60"><input type="checkbox" checked={cue.enabled} onChange={(event) => updateCue(index, { enabled: event.target.checked })} /> Đang bật</label>
                </div>
              ))}
            </div>
            <Toggle
              label="Hiện Hook mở đầu"
              checked={preset.hook.enabled}
              onChange={(value) =>
                setPreset((current) => ({
                  ...current,
                  hook: { ...current.hook, enabled: value },
                }))
              }
            />
            <input
              value={preset.hook.text}
              onChange={(event) =>
                setPreset((current) => ({
                  ...current,
                  hook: { ...current.hook, text: event.target.value },
                }))
              }
              className="mt-2 w-full rounded bg-[#17191e] px-2 py-2 text-xs"
            />
          </Panel>
          <Panel title="OCR / Làm mờ / Nhãn dán">
            <Toggle
              label="Bật xử lý chữ"
              checked={preset.foreignText.enabled}
              onChange={(value) =>
                setPreset((current) => ({
                  ...current,
                  foreignText: { ...current.foreignText, enabled: value },
                }))
              }
            />
            <select
              value={preset.foreignText.detectionMode}
              onChange={(event) =>
                setPreset((current) => ({
                  ...current,
                  foreignText: {
                    ...current.foreignText,
                    detectionMode: event.target
                      .value as LocalAutomationPreset["foreignText"]["detectionMode"],
                  },
                }))
              }
              className="mt-2 w-full rounded-lg border border-slate-700/70 bg-[#10171e] px-3 py-2 text-xs outline-none focus:border-cyan-300/60"
            >
              <option value="MANUAL">Vùng thủ công</option>
              <option value="OCR">OCR nội bộ</option>
              <option value="OCR_WITH_MANUAL_REVIEW">
                OCR + duyệt thủ công
              </option>
            </select>
            <select
              value={preset.foreignText.action}
              onChange={(event) =>
                setPreset((current) => ({
                  ...current,
                  foreignText: {
                    ...current.foreignText,
                    action: event.target
                      .value as LocalAutomationPreset["foreignText"]["action"],
                  },
                }))
              }
              className="mt-2 w-full rounded-lg border border-slate-700/70 bg-[#10171e] px-3 py-2 text-xs outline-none focus:border-cyan-300/60"
            >
              <option value="BLUR">Làm mờ vùng</option>
              <option value="COVER">Che phủ vùng</option>
              <option value="STICKER">Nhãn dán / mặt nạ</option>
            </select>
            <button
              type="button"
              onClick={addRegion}
              className="mr-2 mt-2 rounded-lg border border-cyan-300/30 bg-cyan-300/5 px-3 py-2 text-xs text-cyan-100 transition hover:bg-cyan-300/10"
            >
              Thêm vùng
            </button>
            <button
              type="button"
              onClick={() => void addSticker()}
              className="mt-2 rounded-lg border border-cyan-300/30 bg-cyan-300/5 px-3 py-2 text-xs text-cyan-100 transition hover:bg-cyan-300/10"
            >
              Chọn nhãn dán
            </button>
            {preset.foreignText.stickerPath && (
              <p className="mt-2 truncate text-[10px] text-white/50">
                {preset.foreignText.stickerPath}
              </p>
            )}
            {(preset.foreignText.stickers ?? []).map((sticker, index) => (
              <div key={sticker.id} className="mt-2 grid grid-cols-3 gap-1 text-[10px]">
                <NumberInput label="X" value={sticker.x} onChange={(value) => setPreset((current) => ({ ...current, foreignText: { ...current.foreignText, stickers: (current.foreignText.stickers ?? []).map((entry, i) => i === index ? { ...entry, x: value } : entry) } }))} />
                <NumberInput label="Y" value={sticker.y} onChange={(value) => setPreset((current) => ({ ...current, foreignText: { ...current.foreignText, stickers: (current.foreignText.stickers ?? []).map((entry, i) => i === index ? { ...entry, y: value } : entry) } }))} />
                <NumberInput label="Độ mờ" value={sticker.opacity} onChange={(value) => setPreset((current) => ({ ...current, foreignText: { ...current.foreignText, ...current.foreignText, stickers: (current.foreignText.stickers ?? []).map((entry, i) => i === index ? { ...entry, opacity: value } : entry) } }))} />
                <NumberInput label="Bắt đầu (ms)" value={sticker.startMs} onChange={(value) => setPreset((current) => ({ ...current, foreignText: { ...current.foreignText, stickers: (current.foreignText.stickers ?? []).map((entry, i) => i === index ? { ...entry, startMs: value } : entry) } }))} />
                <NumberInput label="Kết thúc (ms)" value={sticker.endMs} onChange={(value) => setPreset((current) => ({ ...current, foreignText: { ...current.foreignText, stickers: (current.foreignText.stickers ?? []).map((entry, i) => i === index ? { ...entry, endMs: value } : entry) } }))} />
                <label className="flex items-end gap-1 pb-1"><input type="checkbox" checked={sticker.enabled} onChange={(event) => setPreset((current) => ({ ...current, foreignText: { ...current.foreignText, stickers: (current.foreignText.stickers ?? []).map((entry, i) => i === index ? { ...entry, enabled: event.target.checked } : entry) } }))} /> Bật</label>
              </div>
            ))}
            {preset.foreignText.manualRegions.map((region, index) => (
              <div
                key={`${index}-${region.startMs}`}
                className="mt-2 grid grid-cols-3 gap-1 text-[10px]"
              >
                <NumberInput
                  label="Bắt đầu (ms)"
                  value={region.startMs}
                  onChange={(value) => updateRegion(index, "startMs", value)}
                />
                <NumberInput
                  label="Kết thúc (ms)"
                  value={region.endMs}
                  onChange={(value) => updateRegion(index, "endMs", value)}
                />
                <NumberInput
                  label="X"
                  value={region.x}
                  onChange={(value) => updateRegion(index, "x", value)}
                />
                <NumberInput
                  label="Y"
                  value={region.y}
                  onChange={(value) => updateRegion(index, "y", value)}
                />
                <NumberInput
                  label="W"
                  value={region.width}
                  onChange={(value) => updateRegion(index, "width", value)}
                />
                <NumberInput
                  label="H"
                  value={region.height}
                  onChange={(value) => updateRegion(index, "height", value)}
                />
              </div>
            ))}
          </Panel>
        </div>
        <div className="space-y-4">
          <ModelStatusPanel />
          <Panel title="Đầu ra">
            <div className="grid grid-cols-2 gap-2">
              <NumberInput
                label="Chiều rộng"
                value={preset.output.width}
                onChange={(value) => updateOutput("width", value)}
              />
              <NumberInput
                label="Chiều cao"
                value={preset.output.height}
                onChange={(value) => updateOutput("height", value)}
              />
            </div>
            <select
              value={preset.output.ratio}
              onChange={(event) => updateOutput("ratio", event.target.value)}
              className="mt-2 w-full rounded bg-[#20232a] px-2 py-2 text-xs"
            >
              <option>9:16</option>
              <option>16:9</option>
              <option>1:1</option>
              <option>4:5</option>
            </select>
            <label className="mt-3 block text-xs text-white/75">
              Cách đổi khung hình
              <select
                value={preset.output.scaleMode}
                onChange={(event) => updateOutput("scaleMode", event.target.value)}
                className="mt-1 w-full rounded-lg border border-slate-700/70 bg-[#10171e] px-3 py-2 text-xs outline-none focus:border-cyan-300/60"
              >
                <option value="fill">Cắt giữa (9:16)</option>
                <option value="fit_with_background">Vừa khung, nền hai bên</option>
                <option value="center_crop">Cắt giữa tự động</option>
                <option value="manual_crop">Cắt theo vùng đã chọn</option>
                <option value="keep_source_ratio">Giữ tỷ lệ gốc</option>
              </select>
            </label>
            <label className="mt-3 block text-xs text-white/75">
              Chất lượng xuất
              <select
                value={preset.output.qualityPreset}
                onChange={(event) => updateOutput("qualityPreset", event.target.value)}
                className="mt-1 w-full rounded-lg border border-slate-700/70 bg-[#10171e] px-3 py-2 text-xs outline-none focus:border-cyan-300/60"
              >
                <option value="fast">Nhanh</option>
                <option value="balanced">Cân bằng</option>
                <option value="quality">Chất lượng cao</option>
              </select>
            </label>
          </Panel>
          <Panel title="Lịch đăng (tùy chọn)">
            <Toggle label="Bật lịch nhắc đăng" checked={Boolean(preset.publishSchedule)} onChange={setSchedule} />
            {preset.publishSchedule && (
              <>
                <p className="mt-2 text-[11px] text-white/55">Múi giờ: Asia/Ho_Chi_Minh · ArtCraft chỉ lưu lịch, không tự đăng.</p>
                <label className="mt-2 block text-xs text-white/75">
                  Khung giờ (HH:mm, cách nhau bằng dấu phẩy)
                  <input
                    value={preset.publishSchedule.slots.join(", ")}
                    onChange={(event) => updateScheduleSlots(event.target.value)}
                    placeholder="11:30, 20:00"
                    className="mt-1 w-full rounded-lg border border-slate-700/70 bg-[#10171e] px-3 py-2 text-xs outline-none focus:border-cyan-300/60"
                  />
                </label>
              </>
            )}
          </Panel>
          <Panel title="Xem trước">
            {items.length ? (
              <div className="space-y-3">
                {items.map((item, previewIndex) => {
                  const inputName = item.path.split(/[\\/]/).pop() || item.path;
                  const outputPath = item.receipt?.outputPath;
                  return (
                    <div
                      key={item.id}
                      className="rounded-lg border border-slate-700/70 bg-slate-950/20 p-2"
                    >
                      <div className="mb-2 flex items-center justify-between gap-2 text-[10px] text-white/65">
                        <span className="truncate" title={inputName}>
                          Video {previewIndex + 1}
                        </span>
                        <span className="shrink-0 text-cyan-100">Xem trước: {item.state}</span>
                      </div>
                      <div className="grid grid-cols-2 gap-2">
                        <video
                          className="aspect-video w-full bg-black object-contain"
                          controls
                          src={convertFileSrc(item.path)}
                        />
                        {outputPath ? (
                          <video
                            className="aspect-video w-full bg-black object-contain"
                            controls
                            src={convertFileSrc(outputPath)}
                          />
                        ) : (
                          <div className="flex aspect-video items-center justify-center bg-black text-[10px] text-white/40">
                            Chưa có kết quả
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div className="flex aspect-video items-center justify-center bg-black text-xs text-white/40">
                Chưa có video trong hàng đợi
              </div>
            )}
          </Panel>
        </div>
      </div>
      <div className="mt-5 flex items-center gap-3">
        <button
          type="button"
          disabled={running || !queued.length}
          onClick={() => void runQueue()}
          className="rounded-lg bg-cyan-400 px-5 py-2.5 font-semibold text-[#071018] shadow-[0_8px_20px_rgba(34,211,238,0.16)] transition hover:bg-cyan-300 disabled:opacity-40"
        >
          {running ? "Đang xếp hàng…" : "Chạy hàng đợi"}
        </button>
        <button
          type="button"
          disabled={!previewInput || previewing}
          onClick={() => void preview()}
          className="rounded-lg border border-cyan-300/40 bg-cyan-300/5 px-4 py-2.5 text-cyan-100 transition hover:bg-cyan-300/10 disabled:opacity-40"
        >
          {previewing ? "Đang xem trước…" : "Xem trước 5 giây"}
        </button>
        <span className="text-xs text-white/50">
          {items.length} video · {queued.length} đang chờ · mặc định 3 chạy song song (tối đa 10 theo cấu hình) · tiến độ/hủy do máy xử lý
        </span>
      </div>
    </section>
  );
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="rounded-xl border border-slate-700/70 bg-[#18212a] p-4 shadow-[0_10px_30px_rgba(0,0,0,0.12)]">
      <h3 className="mb-3 text-sm font-semibold text-slate-100">{title}</h3>
      {children}
    </div>
  );
}
function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <label className="mt-2 flex items-center justify-between text-xs">
      <span>{label}</span>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
    </label>
  );
}
function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  valueLabel,
  hint,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  valueLabel?: string;
  hint?: string;
  onChange: (value: number) => void;
}) {
  return (
    <label className="mt-3 block text-xs">
      <span className="flex items-center justify-between gap-3">
        <span>{label}</span>
        <span className="font-mono text-cyan-100">{valueLabel ?? value}</span>
      </span>
      <input
        className="mt-1 w-full accent-cyan-400"
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-valuetext={valueLabel ?? String(value)}
        onChange={(event) => onChange(Number(event.target.value))}
      />
      <span className="mt-0.5 flex justify-between text-[10px] text-white/40">
        <span>{formatSliderBoundary(min, step)}</span>
        <span>{formatSliderBoundary(max, step)}</span>
      </span>
      {hint ? <span className="mt-1 block text-[10px] text-white/45">{hint}</span> : null}
    </label>
  );
}

function formatSignedPercent(value: number) {
  return `${value > 0 ? "+" : ""}${Math.round(value)}%`;
}

function formatSignedDecimal(value: number) {
  return `${value > 0 ? "+" : ""}${value.toFixed(2)}`;
}

function formatPlaybackRate(value: number) {
  return `${value.toFixed(2)}x`;
}

function formatSliderBoundary(value: number, step: number) {
  if (step < 1) return value.toFixed(2);
  return String(Math.round(value));
}
function NumberInput({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="text-[10px]">
      {label}
      <input
        type="number"
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        className="mt-1 w-full rounded bg-[#17191e] px-2 py-1.5 text-xs"
      />
    </label>
  );
}
