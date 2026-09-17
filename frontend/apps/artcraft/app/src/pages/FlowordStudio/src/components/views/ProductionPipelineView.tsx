import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AudioLines,
  CheckCircle2,
  Clapperboard,
  FileText,
  Film,
  FolderOpen,
  Image as ImageIcon,
  Loader2,
  Mic2,
  Play,
  RefreshCw,
  Upload,
  Wand2,
  XCircle,
} from "lucide-react";
import toast from "react-hot-toast";
import {
  ArtifactRef as StoredArtifactRef,
  ContentPage,
  DonutProfileEnriched,
  IngestFlowordSourceImageResponse,
  ingestFlowordSourceImage,
  EditingPreset,
  openFlowordCapcutDraft,
  rebuildFlowordCapcutDraft,
} from "../../api/flowordClient";
import { WorkflowInput, WorkflowRun } from "../../services/workflowEngine";
import { ScriptMarketItem } from "./ScriptMarketView";

type PipelinePhase = {
  id: string;
  title: string;
  description: string;
  stages: string[];
};

type VisualEngine = "grok_web" | "omniroute_video";

const phases: PipelinePhase[] = [
  {
    id: "research",
    title: "Nguồn & ý tưởng",
    description: "Xu hướng, liên kết tham khảo và tư liệu đầu vào.",
    stages: ["RESEARCH", "INGEST", "ANALYZE"],
  },
  {
    id: "script",
    title: "Kịch bản phân cảnh",
    description: "Yêu cầu được chuyển thành kịch bản và từng cảnh.",
    stages: ["SCRIPT", "SCENE"],
  },
  {
    id: "assets",
    title: "Tư liệu & bối cảnh",
    description: "Ảnh, tư liệu có sẵn và bối cảnh cho từng cảnh.",
    stages: ["MEDIA", "ASSET"],
  },
  {
    id: "character",
    title: "Nhân vật & anchor",
    description: "Ảnh tham chiếu giữ nhận diện nhân vật.",
    stages: ["IMAGE", "CHARACTER"],
  },
  {
    id: "render",
    title: "Sinh hình & bối cảnh",
    description: "Dịch vụ tạo hình tạo tư liệu theo kế hoạch cảnh đã lưu.",
    stages: ["GROK", "GENERATING", "VIDEO", "MEDIA"],
  },
  {
    id: "voice",
    title: "Giọng nói & phụ đề",
    description: "Lồng tiếng, nhịp câu và phụ đề.",
    stages: ["VOICE", "TTS", "CAPTION", "SUBTITLE"],
  },
  {
    id: "assembly",
    title: "Dựng & xuất",
    description: "Dòng thời gian, kiểm tra kết quả và video hoàn chỉnh.",
    stages: ["TIMELINE", "DRAFT", "SAVING", "LOCAL"],
  },
];

function matches(value: string | undefined, phase: PipelinePhase): boolean {
  const normalized = (value || "").toUpperCase();
  return phase.stages.some((stage) => normalized.includes(stage));
}

function getPhaseState(
  run: WorkflowRun | null,
  phase: PipelinePhase,
): "pending" | "running" | "done" | "failed" {
  if (!run) return "pending";
  if (
    run.status === "failed" &&
    (matches(run.failureStage, phase) || matches(run.currentStage, phase))
  )
    return "failed";
  const step = run.steps?.find(
    (candidate) =>
      matches(candidate.id, phase) || matches(candidate.title, phase),
  );
  if (step?.status === "failed") return "failed";
  if (
    step?.status === "running" ||
    matches(run.currentStage, phase) ||
    matches(run.businessStatus, phase)
  )
    return "running";
  if (
    step?.status === "succeeded" ||
    run.status === "completed" ||
    run.status === "draft_ready"
  )
    return "done";
  return "pending";
}

function parseSources(value: string): string[] {
  return value
    .split(/\r?\n|,/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function translatePipelineError(message: string): string {
  const normalized = message.toLowerCase();
  if (
    normalized.includes("source video has no audio stream") ||
    normalized.includes("vynaro_audio_missing")
  ) {
    return "Video nguồn không có âm thanh nên không thể nhận dạng hoặc lồng tiếng. Ảnh anchor nhân vật không phải video nguồn; hãy tạo công việc mới sau khi cập nhật màn này.";
  }
  if (
    normalized.includes("visual_provider_unavailable") ||
    normalized.includes("original creation needs at least one active") ||
    normalized.includes("video-provider connection") ||
    normalized.includes("add and test veo")
  ) {
    return "Chưa có dịch vụ tạo video khả dụng trong OmniRoute. Hãy kết nối và kiểm tra ít nhất một dịch vụ tạo video trước khi chạy.";
  }
  if (normalized.includes("prompt_required")) {
    return "Chưa có nội dung yêu cầu để tạo kịch bản. Hãy nhập brief hoặc chọn một mẫu trong Chợ kịch bản.";
  }
  if (normalized.includes("source_required")) {
    return "Thiếu tư liệu nguồn cho chế độ đang chọn. Hãy nhập brief, liên kết nguồn hoặc đổi sang chế độ tạo nội dung gốc.";
  }
  if (normalized.includes("audio") && normalized.includes("missing")) {
    return "Không tìm thấy âm thanh cần thiết cho bước xử lý giọng nói.";
  }
  return "Công việc chưa hoàn tất. Mở Chi tiết kỹ thuật bên dưới nếu cần gửi lỗi để kiểm tra.";
}

interface Props {
  pages: ContentPage[];
  activePageId?: string;
  onSelectPage: (id: string) => void;
  activeRun: WorkflowRun | null;
  isRunning: boolean;
  onRunWorkflow: (input: WorkflowInput) => Promise<void>;
  onCancelWorkflow: () => Promise<void>;
  profiles: DonutProfileEnriched[];
  scriptToLoad?: ScriptMarketItem | null;
  onScriptLoaded: () => void;
}

export const ProductionPipelineView: React.FC<Props> = ({
  pages,
  activePageId,
  onSelectPage,
  activeRun,
  isRunning,
  onRunWorkflow,
  onCancelWorkflow,
  profiles,
  scriptToLoad,
  onScriptLoaded,
}) => {
  const [brief, setBrief] = useState("");
  const [sources, setSources] = useState("");
  const [useTrendResearch, setUseTrendResearch] = useState(true);
  const [sceneCount, setSceneCount] = useState(6);
  const [duration, setDuration] = useState(60);
  const [voiceStyle, setVoiceStyle] = useState("storytelling_vietnamese");
  const [editingPreset, setEditingPreset] = useState<EditingPreset>({
    aspect: "9:16",
    subtitle_style: "dynamic",
    transition_style: "smooth",
    music_volume: 0.18,
    video_speed: 1,
  });
  const [draftBusy, setDraftBusy] = useState(false);
  const [rebuiltDraftPath, setRebuiltDraftPath] = useState("");
  // Existing Grok inputs live in the main production surface so there is no
  // second, competing Studio workflow.
  const [visualEngine, setVisualEngine] = useState<VisualEngine>("grok_web");
  const [imagePrompt, setImagePrompt] = useState("");
  const [expand916Prompt, setExpand916Prompt] = useState("");
  const [videoPrompt, setVideoPrompt] = useState("");
  const [anchor, setAnchor] = useState<StoredArtifactRef | null>(null);
  const [anchorPreview, setAnchorPreview] = useState("");
  const [ingesting, setIngesting] = useState(false);
  const [dismissedErrorRunId, setDismissedErrorRunId] = useState<string | null>(null);
  const fileInput = useRef<HTMLInputElement | null>(null);
  const page =
    pages.find((candidate) => candidate.id === activePageId) ?? pages[0];
  const profile = profiles.find(
    (candidate) => candidate.id === page?.browser_profile_id,
  );
  // OmniBridge controls the selected Donut profile via CDP/Playwright.
  // Profile running is sufficient; if Grok is not yet logged in, the browser tab
  // will open directly to Grok for the user to login or continue automatically.
  const donutReady = Boolean(profile?.is_running);
  const video = useMemo(
    () =>
      activeRun?.artifacts.find(
        (artifact) =>
          artifact.type === "rendered_video" || artifact.type === "video",
      ),
    [activeRun],
  );
  const draftPath = rebuiltDraftPath || activeRun?.finalDraftUrl;

  useEffect(() => {
    setRebuiltDraftPath("");
  }, [activeRun?.id]);

  useEffect(() => {
    if (!scriptToLoad) return;
    setBrief(scriptToLoad.brief);
    setSources(scriptToLoad.sources.join("\n"));
    setUseTrendResearch(scriptToLoad.useTrendResearch);
    setSceneCount(scriptToLoad.sceneCount);
    setDuration(scriptToLoad.duration);
    setVoiceStyle(scriptToLoad.voiceStyle);
    onScriptLoaded();
    toast.success(`Đã nạp kịch bản: ${scriptToLoad.title}`);
  }, [onScriptLoaded, scriptToLoad]);

  const ingestAnchor = useCallback(
    async (file: File) => {
      if (!file.type.startsWith("image/")) {
        toast.error("Anchor phải là ảnh PNG, JPG hoặc WEBP.");
        return;
      }
      setIngesting(true);
      const toastId = toast.loading(`Đang lưu anchor ${file.name}…`);
      try {
        const base64 = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onerror = () => reject(new Error("Không thể đọc ảnh anchor"));
          reader.onload = () => resolve(String(reader.result || ""));
          reader.readAsDataURL(file);
        });
        const result: IngestFlowordSourceImageResponse =
          await ingestFlowordSourceImage({
            base64_data: base64,
            file_name: file.name,
            page_id: page?.id,
          });
        setAnchor(result.artifact);
        setAnchorPreview(base64 || result.preview_url);
        toast.success("Đã lưu anchor vào kho artifact.", { id: toastId });
      } catch (error) {
        toast.error(
          `Không thể lưu anchor: ${error instanceof Error ? error.message : String(error)}`,
          { id: toastId },
        );
      } finally {
        setIngesting(false);
      }
    },
    [page?.id],
  );

  const run = async () => {
    if (!page) return toast.error("Tạo hoặc chọn Page/Kênh trước khi chạy.");
    if (!brief.trim())
      return toast.error("Nhập brief để tạo kịch bản phân cảnh.");
    if (!anchor)
      return toast.error("Thêm anchor nhân vật trước khi chạy luồng phim AI.");
    if (visualEngine === "grok_web" && !page.browser_profile_id) {
      return toast.error(
        "Page chưa được gán Profile Donut Browser. Vui lòng bấm ✏️ bên cạnh tên Page để chọn Profile.",
      );
    }
    const usesGrok = visualEngine === "grok_web";
    await onRunWorkflow({
      workflowName: usesGrok
        ? "grok_feature_film_pipeline"
        : "floword_feature_film_pipeline",
      workflowMode: usesGrok
        ? "grok_feature_film_pipeline"
        : "original_creation",
      pageId: page.id,
      prompt: brief.trim(),
      topic: brief.trim(),
      title: brief.trim(),
      caption: brief.trim(),
      sourceUrls: parseSources(sources),
      // An anchor is a character reference, never a source video. Sending it
      // as sourceFiles makes the ingest worker try to extract audio from JPG/PNG.
      sourceFiles: [],
      sourceImageArtifact: anchor,
      targetPlatform: (page.target_platform as any) || "tiktok",
      targetDurationSeconds: duration,
      language: page.default_language || "vi",
      tone: "storytelling",
      aspectRatio: "9:16",
      editingPreset,
      scriptMode: "original",
      contentSource: useTrendResearch ? "trend_research" : "prompt_only",
      outputMode: "render_video",
      researchEnabled: useTrendResearch,
      researchPlatform: "xhs",
      researchQuery: brief.trim(),
      researchMode: "search",
      voiceId: voiceStyle,
      imagePrompt: usesGrok ? imagePrompt.trim() || brief.trim() : undefined,
      expand916Prompt: usesGrok ? expand916Prompt.trim() || undefined : undefined,
      videoPrompt: usesGrok ? videoPrompt.trim() || undefined : undefined,
      customPrompt: brief.trim(),
      generateImage: true,
      generateDraft: true,
    });
  };

  const openDraft = async () => {
    if (!draftPath) return;
    try {
      await openFlowordCapcutDraft(draftPath);
      toast.success("Đã mở CapCut và vị trí project.");
    } catch (error) {
      toast.error(`Không thể mở project: ${error instanceof Error ? error.message : String(error)}`);
    }
  };

  const rebuildDraft = async () => {
    if (!activeRun?.id) return;
    setDraftBusy(true);
    const toastId = toast.loading("Đang dựng lại project CapCut…");
    try {
      const result = await rebuildFlowordCapcutDraft(activeRun.id, editingPreset);
      setRebuiltDraftPath(result.draft_path);
      toast.success(`Đã dựng lại project ${result.draft_id.slice(0, 8)}.`, { id: toastId });
    } catch (error) {
      toast.error(`Dựng lại thất bại: ${error instanceof Error ? error.message : String(error)}`, { id: toastId });
    } finally {
      setDraftBusy(false);
    }
  };

  return (
    <div className="mx-auto flex w-full max-w-[1500px] flex-col gap-5 p-5 lg:p-7">
      <header className="rounded-2xl border border-white/10 bg-[#121622] p-5 shadow-xl shadow-black/10">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
          <div className="flex gap-3">
            <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-fuchsia-500 to-rose-500 text-white shadow-lg shadow-fuchsia-500/20">
              <Clapperboard className="h-6 w-6" />
            </div>
            <div>
              <p className="text-xs font-semibold uppercase tracking-[0.18em] text-fuchsia-300">
                Luồng chính
              </p>
              <h1 className="mt-1 text-xl font-bold text-white">
                Xưởng Phim AI
              </h1>
              <p className="text-zinc-400 mt-1 max-w-3xl text-sm">
                Một job ledger xuyên suốt từ brief đến video; mỗi trạng thái đến
                từ job và artifact đã lưu, không phải mock UI.
              </p>
            </div>
          </div>
          <div className="grid grid-cols-3 gap-2 text-xs">
            <Metric label="Page" value={page?.name || "Chưa chọn"} />
            <Metric
              label="Donut"
              value={donutReady ? "Sẵn sàng" : "Chưa sẵn sàng"}
              good={donutReady}
            />
            <Metric
              label="Job"
              value={activeRun?.id?.slice(0, 8) || "Chưa tạo"}
              mono
            />
          </div>
        </div>
      </header>

      <section className="grid gap-5 xl:grid-cols-[minmax(0,1.5fr)_minmax(360px,0.9fr)]">
        <div className="space-y-5">
          <div className="rounded-2xl border border-white/10 bg-[#121622] p-5">
            <div className="mb-4 flex items-center gap-2">
              <Wand2 className="h-4 w-4 text-fuchsia-300" />
              <h2 className="font-bold text-white">
                1. Brief, nguồn và kịch bản
              </h2>
            </div>
            <label className="text-zinc-300 text-sm">
              Brief / chủ đề
              <textarea
                value={brief}
                onChange={(event) => setBrief(event.target.value)}
                rows={5}
                placeholder="Ví dụ: Câu chuyện lịch sử 60 giây, 6 cảnh, nhân vật chính xuyên suốt…"
                className="placeholder:text-zinc-600 mt-2 w-full resize-y rounded-xl border border-white/10 bg-[#0d1017] p-3 text-sm text-white outline-none focus:border-fuchsia-400"
              />
            </label>
            <div className="mt-4 grid gap-4 md:grid-cols-2">
              <label className="text-zinc-300 text-sm">
                Nguồn / URL tham khảo{" "}
                <span className="text-zinc-600">(mỗi dòng một URL)</span>
                <textarea
                  value={sources}
                  onChange={(event) => setSources(event.target.value)}
                  rows={3}
                  placeholder="https://…"
                  className="placeholder:text-zinc-600 mt-2 w-full resize-y rounded-xl border border-white/10 bg-[#0d1017] p-3 text-sm text-white outline-none focus:border-fuchsia-400"
                />
                <span className="mt-2 flex items-center gap-2 text-xs text-zinc-400">
                  <input
                    type="checkbox"
                    checked={useTrendResearch}
                    onChange={(event) => setUseTrendResearch(event.target.checked)}
                    className="h-4 w-4 accent-fuchsia-500"
                  />
                  Khai thác trend/tư liệu liên quan trước khi viết kịch bản
                </span>
              </label>
              <div className="grid grid-cols-2 gap-3 self-end">
                <label className="text-zinc-300 text-sm">
                  Số cảnh
                  <input
                    type="number"
                    min={1}
                    max={30}
                    value={sceneCount}
                    onChange={(event) =>
                      setSceneCount(
                        Math.max(1, Number(event.target.value) || 1),
                      )
                    }
                    className="mt-2 w-full rounded-xl border border-white/10 bg-[#0d1017] p-3 text-white outline-none focus:border-fuchsia-400"
                  />
                </label>
                <label className="text-zinc-300 text-sm">
                  Thời lượng (giây)
                  <input
                    type="number"
                    min={10}
                    max={7200}
                    value={duration}
                    onChange={(event) =>
                      setDuration(
                        Math.max(10, Number(event.target.value) || 10),
                      )
                    }
                    className="mt-2 w-full rounded-xl border border-white/10 bg-[#0d1017] p-3 text-white outline-none focus:border-fuchsia-400"
                  />
                </label>
                <label className="text-zinc-300 col-span-2 text-sm">
                  Giọng đọc
                  <select
                    value={voiceStyle}
                    onChange={(event) => setVoiceStyle(event.target.value)}
                    className="mt-2 w-full rounded-xl border border-white/10 bg-[#0d1017] p-3 text-white outline-none focus:border-fuchsia-400"
                  >
                    <option value="storytelling_vietnamese">
                      Kể chuyện tiếng Việt, có ngắt nghỉ
                    </option>
                    <option value="natural_vietnamese">
                      Tiếng Việt tự nhiên
                    </option>
                    <option value="cinematic_narrator">
                      Thuyết minh điện ảnh
                    </option>
                  </select>
                </label>
              </div>
            </div>
          </div>
          <div className="rounded-2xl border border-white/10 bg-[#121622] p-5">
            <div className="mb-4 flex items-center justify-between">
              <div className="flex items-center gap-2">
                <ImageIcon className="h-4 w-4 text-fuchsia-300" />
                <h2 className="font-bold text-white">2. Nhân vật & Động cơ Video</h2>
              </div>
              <span className="text-xs px-2.5 py-0.5 rounded-full bg-fuchsia-500/10 text-fuchsia-300 border border-white/10 font-medium">
                {visualEngine === "grok_web" ? "Grok Web (Donut)" : "OmniRoute Video API"}
              </span>
            </div>

            <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_180px] items-start">
              <div>
                <p className="text-zinc-400 text-sm">
                  Tải 1 ảnh chân dung/nhân vật mẫu. AI Grok sẽ giữ nhận diện khuôn mặt và phong cách nhân vật này xuyên suốt mọi cảnh video.
                </p>

                <div className="mt-4 flex flex-wrap items-center gap-3">
                  <input
                    ref={fileInput}
                    type="file"
                    accept="image/png,image/jpeg,image/webp"
                    className="hidden"
                    onChange={(event) => {
                      const file = event.target.files?.[0];
                      if (file) void ingestAnchor(file);
                      event.currentTarget.value = "";
                    }}
                  />
                  <button
                    type="button"
                    disabled={ingesting}
                    onClick={() => fileInput.current?.click()}
                    className="inline-flex items-center gap-2 rounded-xl border border-white/10 bg-fuchsia-500/10 px-4 py-2.5 text-sm font-semibold text-fuchsia-200 transition hover:bg-fuchsia-500/20 disabled:opacity-60 cursor-pointer"
                  >
                    {ingesting ? (
                      <Loader2 className="h-4 w-4 animate-spin" />
                    ) : (
                      <Upload className="h-4 w-4" />
                    )}
                    {anchor ? "Thay ảnh nhân vật" : "Tải ảnh nhân vật mẫu (Anchor)"}
                  </button>

                  <select
                    value={visualEngine}
                    onChange={(event) =>
                      setVisualEngine(event.target.value as VisualEngine)
                    }
                    className="rounded-xl border border-white/10 bg-[#0d1017] px-3 py-2.5 text-xs text-zinc-300 outline-none focus:border-fuchsia-400"
                  >
                    <option value="grok_web">Động cơ: Grok Web qua Donut</option>
                    <option value="omniroute_video">Động cơ: OmniRoute Video API</option>
                  </select>
                </div>

                {anchor && (
                  <p className="mt-2.5 break-all font-mono text-[11px] text-emerald-300">
                    ✓ Đã nhận diện Anchor: {anchor.artifact_id.slice(0, 16)}…
                  </p>
                )}
              </div>

              <div className="flex h-28 w-full max-w-[180px] items-center justify-center overflow-hidden rounded-xl border border-dashed border-white/10 bg-black/30">
                {anchorPreview ? (
                  <img
                    src={anchorPreview}
                    alt="Anchor nhân vật"
                    className="h-full w-full object-cover"
                  />
                ) : (
                  <div className="text-center p-2 text-zinc-600">
                    <ImageIcon className="h-6 w-6 mx-auto mb-1 opacity-40" />
                    <span className="text-[11px]">Chưa có ảnh</span>
                  </div>
                )}
              </div>
            </div>

            {visualEngine === "grok_web" && (
              <details className="mt-4 pt-4 border-t border-white/10 group text-xs">
                <summary className="cursor-pointer font-medium text-zinc-400 hover:text-zinc-200 transition select-none flex items-center gap-1.5">
                  <span className="text-fuchsia-400 group-open:rotate-90 transition-transform inline-block">▸</span>
                  Tùy chỉnh Prompt nâng cao (Không bắt buộc - Tự động tạo từ Brief nếu để trống)
                </summary>
                <div className="mt-3 grid gap-3 pl-3">
                  <label className="text-zinc-300 text-xs">
                    Prompt tạo ảnh Grok (để trống sẽ dùng Brief)
                    <textarea
                      value={imagePrompt}
                      onChange={(event) => setImagePrompt(event.target.value)}
                      rows={2}
                      placeholder="Giữ khuôn mặt, trang phục từ anchor; mô tả bối cảnh…"
                      className="placeholder:text-zinc-600 mt-1.5 w-full resize-y rounded-xl border border-white/10 bg-[#0d1017] p-2.5 text-xs text-white outline-none focus:border-fuchsia-400"
                    />
                  </label>
                  <div className="grid gap-3 md:grid-cols-2">
                    <label className="text-zinc-300 text-xs">
                      Prompt mở rộng 9:16 (tùy chọn)
                      <textarea
                        value={expand916Prompt}
                        onChange={(event) => setExpand916Prompt(event.target.value)}
                        rows={2}
                        placeholder="Mở rộng 9:16 giữ chủ thể…"
                        className="placeholder:text-zinc-600 mt-1.5 w-full resize-y rounded-xl border border-white/10 bg-[#0d1017] p-2.5 text-xs text-white outline-none focus:border-fuchsia-400"
                      />
                    </label>
                    <label className="text-zinc-300 text-xs">
                      Prompt chuyển động video (tùy chọn)
                      <textarea
                        value={videoPrompt}
                        onChange={(event) => setVideoPrompt(event.target.value)}
                        rows={2}
                        placeholder="Chuyển động điện ảnh mượt mà…"
                        className="placeholder:text-zinc-600 mt-1.5 w-full resize-y rounded-xl border border-white/10 bg-[#0d1017] p-2.5 text-xs text-white outline-none focus:border-fuchsia-400"
                      />
                    </label>
                  </div>
                </div>
              </details>
            )}
          </div>
        </div>
        <aside className="space-y-5">
          <div className="rounded-2xl border border-white/10 bg-[#121622] p-5">
            <div className="flex items-center gap-2">
              <Film className="h-4 w-4 text-fuchsia-300" />
              <h2 className="font-bold text-white">Điều phối Donut Browser</h2>
            </div>
            <label className="text-zinc-500 mt-4 block text-xs">
              Page/Kênh
              <select
                value={page?.id || ""}
                onChange={(event) => onSelectPage(event.target.value)}
                className="mt-1.5 w-full rounded-xl border border-white/10 bg-[#0d1017] p-2.5 text-sm text-white outline-none"
              >
                {pages.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
            <dl className="mt-4 space-y-3 text-sm">
              <StatusRow
                label="Profile gắn Page"
                value={profile?.name || page?.browser_profile_id || "Chưa gắn"}
              />
              <StatusRow
                label="Runtime"
                value={profile?.is_running ? "Đang chạy" : "Chưa chạy"}
                good={Boolean(profile?.is_running)}
              />
              <StatusRow
                label="Grok đăng nhập"
                value={
                  profile?.grok_logged_in
                    ? "Đã xác nhận"
                    : profile?.is_running
                      ? "Sẵn sàng (Tự mở tab)"
                      : "Chờ bật profile"
                }
                good={Boolean(profile?.is_running)}
              />
              <StatusRow
                label="Động cơ hình"
                value={
                  visualEngine === "grok_web"
                    ? "Grok Web / Donut"
                    : "Video API / OmniRoute"
                }
                good={visualEngine === "grok_web" ? Boolean(profile?.is_running) : undefined}
              />
            </dl>
            {visualEngine === "grok_web" && (
              <p className="mt-4 rounded-xl border border-white/10 bg-fuchsia-500/10 p-3 text-xs leading-5 text-fuchsia-200">
                💡 <strong>Quy trình Grok Web:</strong> OmniBridge sẽ kết nối trực tiếp với Profile Donut. Nếu phiên trình duyệt chưa đăng nhập, tab Grok sẽ mở ra để bạn đăng nhập và tự động tiếp tục tác vụ ngay sau đó.
              </p>
            )}
            {visualEngine === "omniroute_video" && (
              <p className="mt-4 rounded-xl border border-white/10 bg-cyan-500/10 p-3 text-xs leading-5 text-cyan-100">
                Chế độ Video API không cần Donut. Pipeline sẽ chạy preflight bằng
                provider video thật của OmniRoute trước khi sinh cảnh.
              </p>
            )}
          </div>
          <div className="rounded-2xl border border-white/10 bg-[#121622] p-5">
            <div className="flex items-center gap-2">
              <AudioLines className="h-4 w-4 text-fuchsia-300" />
              <h2 className="font-bold text-white">Lệnh sản xuất</h2>
            </div>
            <p className="text-zinc-400 mt-2 text-sm leading-6">
              Tạo job Floword có audit trail, cho phép hủy/retry theo stage.
            </p>
            {isRunning ? (
              <button
                type="button"
                onClick={() => void onCancelWorkflow()}
                className="mt-4 inline-flex w-full items-center justify-center gap-2 rounded-xl border border-white/10 bg-rose-500/10 px-4 py-3 font-semibold text-rose-200"
              >
                <XCircle className="h-4 w-4" />
                Hủy job hiện tại
              </button>
            ) : (
              <button
                type="button"
                onClick={() => void run()}
                className="mt-4 inline-flex w-full items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-fuchsia-600 to-rose-500 px-4 py-3 font-semibold text-white shadow-lg shadow-fuchsia-500/20 transition hover:brightness-110"
              >
                <Play className="h-4 w-4 fill-current" />
                Tạo job phim AI
              </button>
            )}
          </div>
          <div className="rounded-2xl border border-white/10 bg-[#121622] p-5">
            <div className="flex items-center gap-2">
              <Clapperboard className="h-4 w-4 text-fuchsia-300" />
              <h2 className="font-bold text-white">Preset dựng phim</h2>
            </div>
            <div className="mt-4 grid grid-cols-2 gap-3 text-xs">
              <label className="text-zinc-400">
                Tỷ lệ
                <select value={editingPreset.aspect} onChange={(event) => setEditingPreset((current) => ({ ...current, aspect: event.target.value as EditingPreset["aspect"] }))} className="mt-1.5 w-full rounded-lg border border-white/10 bg-[#0d1017] p-2.5 text-white">
                  <option value="9:16">9:16 Dọc</option>
                  <option value="16:9">16:9 Ngang</option>
                  <option value="1:1">1:1 Vuông</option>
                  <option value="4:5">4:5 Dọc</option>
                </select>
              </label>
              <label className="text-zinc-400">
                Phụ đề
                <select value={editingPreset.subtitle_style} onChange={(event) => setEditingPreset((current) => ({ ...current, subtitle_style: event.target.value as EditingPreset["subtitle_style"] }))} className="mt-1.5 w-full rounded-lg border border-white/10 bg-[#0d1017] p-2.5 text-white">
                  <option value="dynamic">Năng động</option>
                  <option value="cinematic">Điện ảnh</option>
                  <option value="clean">Tối giản</option>
                </select>
              </label>
              <label className="col-span-2 text-zinc-400">
                Chuyển cảnh
                <select value={editingPreset.transition_style} onChange={(event) => setEditingPreset((current) => ({ ...current, transition_style: event.target.value }))} className="mt-1.5 w-full rounded-lg border border-white/10 bg-[#0d1017] p-2.5 text-white">
                  <option value="smooth">Mượt (Dissolve)</option>
                  <option value="fade">Fade đen</option>
                  <option value="flash">Flash trắng</option>
                  <option value="none">Không chuyển cảnh</option>
                </select>
              </label>
              <label className="col-span-2 text-zinc-400">
                Tốc độ video: {editingPreset.video_speed.toFixed(2)}x
                <input type="range" min="0.5" max="2" step="0.05" value={editingPreset.video_speed} onChange={(event) => setEditingPreset((current) => ({ ...current, video_speed: Number(event.target.value) }))} className="mt-2 w-full accent-fuchsia-500" />
              </label>
            </div>
          </div>
        </aside>
      </section>
      <section className="rounded-2xl border border-white/10 bg-[#121622] p-5">
        <div className="mb-4 flex items-center gap-2">
          <FileText className="h-4 w-4 text-fuchsia-300" />
          <h2 className="font-bold text-white">Tiến trình job thực tế</h2>
        </div>
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
          {phases.map((phase, index) => (
            <PhaseCard
              key={phase.id}
              index={index}
              phase={phase}
              state={getPhaseState(activeRun, phase)}
            />
          ))}
        </div>
        {activeRun?.errorMessage && dismissedErrorRunId !== activeRun.id && (
          <div className="mt-4 rounded-xl border border-white/10 bg-rose-500/10 p-4 text-sm text-rose-200 flex flex-col gap-2 relative">
            <div className="flex items-start justify-between gap-3">
              <div className="flex-1 min-w-0">
                <p className="font-medium">{translatePipelineError(activeRun.errorMessage)}</p>
                <p className="text-xs text-rose-300/80 mt-1">
                  (Đây là log của tác vụ cũ <code className="bg-black/30 px-1 py-0.5 rounded font-mono">#{activeRun.id?.slice(0, 8)}</code>. Bạn có thể đóng lại hoặc nhập Brief để bấm chạy tác vụ mới)
                </p>
              </div>
              <button
                type="button"
                onClick={() => setDismissedErrorRunId(activeRun.id)}
                className="px-2.5 py-1 rounded-lg bg-rose-500/20 hover:bg-rose-500/30 text-rose-200 text-xs font-semibold transition cursor-pointer shrink-0 border border-white/10"
              >
                ✕ Đóng thông báo
              </button>
            </div>
            <details className="mt-1 text-xs text-rose-200/70">
              <summary className="cursor-pointer font-mono">Chi tiết kỹ thuật</summary>
              <p className="mt-1 break-words font-mono bg-black/30 p-2 rounded-lg border border-white/10">{activeRun.errorMessage}</p>
            </details>
          </div>
        )}
        {video && (
          <p className="mt-4 rounded-xl border border-white/10 bg-emerald-500/10 p-3 text-sm text-emerald-200">
            Video output: {video.name}
          </p>
        )}
        {draftPath && (
          <div className="mt-4 rounded-xl border border-white/10 bg-fuchsia-500/[0.08] p-4">
            <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
              <div className="min-w-0">
                <p className="font-semibold text-white">Project CapCut có thể chỉnh sửa đã sẵn sàng</p>
                <p className="mt-1 truncate font-mono text-[11px] text-zinc-400">{draftPath}</p>
                <p className="mt-2 text-xs text-emerald-300">✓ Video · ✓ Giọng đọc · ✓ Phụ đề · ✓ Timeline · ✓ CapCut Draft</p>
              </div>
              <div className="flex shrink-0 gap-2">
                <button type="button" disabled={draftBusy} onClick={() => void rebuildDraft()} className="inline-flex items-center gap-2 rounded-lg border border-white/10 bg-white/[0.06] px-3 py-2 text-xs font-semibold text-zinc-200 hover:bg-white/10 disabled:opacity-50">
                  <RefreshCw className={`h-4 w-4 ${draftBusy ? "animate-spin" : ""}`} />
                  Dựng lại Draft
                </button>
                <button type="button" onClick={() => void openDraft()} className="inline-flex items-center gap-2 rounded-lg bg-fuchsia-600 px-3 py-2 text-xs font-semibold text-white hover:bg-fuchsia-500">
                  <FolderOpen className="h-4 w-4" />
                  Mở trong CapCut
                </button>
              </div>
            </div>
          </div>
        )}
      </section>
    </div>
  );
};

function Metric({
  label,
  value,
  good,
  mono = false,
}: {
  label: string;
  value: string;
  good?: boolean;
  mono?: boolean;
}) {
  return (
    <div className="rounded-xl border border-white/10 bg-black/20 px-3 py-2">
      <span className="text-zinc-500 block">{label}</span>
      <span
        className={`${good === undefined ? "text-zinc-200" : good ? "text-emerald-400" : "text-amber-300"} ${mono ? "font-mono text-[11px]" : "font-semibold"} block max-w-28 truncate`}
      >
        {value}
      </span>
    </div>
  );
}
function StatusRow({
  label,
  value,
  good,
}: {
  label: string;
  value: string;
  good?: boolean;
}) {
  return (
    <div className="flex justify-between gap-4">
      <dt className="text-zinc-500">{label}</dt>
      <dd
        className={
          good === undefined
            ? "max-w-[190px] truncate font-medium text-white"
            : good
              ? "font-medium text-emerald-400"
              : "font-medium text-amber-300"
        }
      >
        {value}
      </dd>
    </div>
  );
}
function PhaseCard({
  index,
  phase,
  state,
}: {
  index: number;
  phase: PipelinePhase;
  state: ReturnType<typeof getPhaseState>;
}) {
  const style =
    state === "done"
      ? "border-white/10 bg-emerald-500/[0.06]"
      : state === "running"
        ? "border-white/10 bg-sky-500/[0.08]"
        : state === "failed"
          ? "border-white/10 bg-rose-500/[0.08]"
          : "border-white/10 bg-black/10";
  const Icon =
    state === "done"
      ? CheckCircle2
      : state === "failed"
        ? XCircle
        : state === "running"
          ? Loader2
          : index === 4
            ? ImageIcon
            : index === 5
              ? Mic2
              : FileText;
  return (
    <div className={`rounded-xl border p-4 ${style}`}>
      <div className="flex items-center justify-between gap-2">
        <span className="text-zinc-500 text-[11px] font-bold uppercase tracking-wider">
          {String(index + 1).padStart(2, "0")}
        </span>
        <Icon
          className={`h-4 w-4 ${state === "running" ? "animate-spin text-sky-300" : state === "done" ? "text-emerald-300" : state === "failed" ? "text-rose-300" : "text-zinc-500"}`}
        />
      </div>
      <h3 className="mt-3 font-semibold text-white">{phase.title}</h3>
      <p className="text-zinc-400 mt-1 text-xs leading-5">
        {phase.description}
      </p>
      <p className="text-zinc-500 mt-3 text-[11px] font-semibold uppercase tracking-wide">
        {state === "running"
          ? "Đang chạy"
          : state === "done"
            ? "Hoàn tất"
            : state === "failed"
              ? "Cần xử lý"
              : "Chờ công việc"}
      </p>
    </div>
  );
}
