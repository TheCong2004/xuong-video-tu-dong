import React, { useCallback, useMemo, useRef, useState } from "react";
import {
  AudioLines,
  CheckCircle2,
  Clapperboard,
  FileText,
  Film,
  Image as ImageIcon,
  Loader2,
  Mic2,
  Play,
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
} from "../../api/flowordClient";
import { WorkflowInput, WorkflowRun } from "../../services/workflowEngine";

type PipelinePhase = {
  id: string;
  title: string;
  description: string;
  stages: string[];
};

const phases: PipelinePhase[] = [
  {
    id: "research",
    title: "Nguồn & ý tưởng",
    description: "Trend, link nguồn và tư liệu đầu vào.",
    stages: ["RESEARCH", "INGEST", "ANALYZE"],
  },
  {
    id: "script",
    title: "Kịch bản phân cảnh",
    description: "Brief được chuyển thành kịch bản và scene.",
    stages: ["SCRIPT", "SCENE"],
  },
  {
    id: "assets",
    title: "Tư liệu & bối cảnh",
    description: "Ảnh, stock và asset phục vụ từng cảnh.",
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
    description: "Visual provider tạo asset theo scene plan đã lưu.",
    stages: ["GROK", "GENERATING", "VIDEO", "MEDIA"],
  },
  {
    id: "voice",
    title: "Giọng nói & phụ đề",
    description: "Lồng tiếng, nhịp câu và subtitle.",
    stages: ["VOICE", "TTS", "CAPTION", "SUBTITLE"],
  },
  {
    id: "assembly",
    title: "Dựng & xuất",
    description: "Timeline, kiểm tra output và video hoàn chỉnh.",
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

interface Props {
  pages: ContentPage[];
  activePageId?: string;
  onSelectPage: (id: string) => void;
  activeRun: WorkflowRun | null;
  isRunning: boolean;
  onRunWorkflow: (input: WorkflowInput) => Promise<void>;
  onCancelWorkflow: () => Promise<void>;
  profiles: DonutProfileEnriched[];
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
}) => {
  const [brief, setBrief] = useState("");
  const [sources, setSources] = useState("");
  const [useTrendResearch, setUseTrendResearch] = useState(true);
  const [sceneCount, setSceneCount] = useState(6);
  const [duration, setDuration] = useState(60);
  const [voiceStyle, setVoiceStyle] = useState("storytelling_vietnamese");
  const [anchor, setAnchor] = useState<StoredArtifactRef | null>(null);
  const [anchorPreview, setAnchorPreview] = useState("");
  const [ingesting, setIngesting] = useState(false);
  const fileInput = useRef<HTMLInputElement | null>(null);
  const page =
    pages.find((candidate) => candidate.id === activePageId) ?? pages[0];
  const profile = profiles.find(
    (candidate) => candidate.id === page?.browser_profile_id,
  );
  const donutReady = Boolean(
    profile?.is_running && profile.grok_logged_in && profile.extension_ready,
  );
  const video = useMemo(
    () =>
      activeRun?.artifacts.find(
        (artifact) =>
          artifact.type === "rendered_video" || artifact.type === "video",
      ),
    [activeRun],
  );

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
    await onRunWorkflow({
      workflowName: "floword_feature_film_pipeline",
      workflowMode: "floword_video_pipeline",
      pageId: page.id,
      prompt: brief.trim(),
      topic: brief.trim(),
      sourceUrls: parseSources(sources),
      sourceFiles: [anchor.location],
      sourceImageArtifact: anchor,
      targetPlatform: "tiktok",
      targetDurationSeconds: duration,
      language: page.default_language || "vi",
      tone: "storytelling",
      aspectRatio: "9:16",
      scriptMode: "original",
      contentSource: useTrendResearch ? "trend_research" : "prompt_only",
      outputMode: "render_video",
      researchEnabled: useTrendResearch,
      researchPlatform: "xhs",
      researchQuery: brief.trim(),
      researchMode: "search",
      voiceId: voiceStyle,
      customPrompt: `${brief.trim()}\nScene count: ${sceneCount}. Character anchor: ${anchor.artifact_id}.`,
      generateImage: true,
      generateDraft: true,
    });
  };

  return (
    <div className="mx-auto flex w-full max-w-[1500px] flex-col gap-5 p-5 lg:p-7">
      <header className="rounded-2xl border border-white/[0.08] bg-[#121622] p-5 shadow-xl shadow-black/10">
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
          <div className="rounded-2xl border border-white/[0.08] bg-[#121622] p-5">
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
                className="placeholder:text-zinc-600 mt-2 w-full resize-y rounded-xl border border-white/[0.1] bg-[#0d1017] p-3 text-sm text-white outline-none focus:border-fuchsia-400"
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
                  className="placeholder:text-zinc-600 mt-2 w-full resize-y rounded-xl border border-white/[0.1] bg-[#0d1017] p-3 text-sm text-white outline-none focus:border-fuchsia-400"
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
                    className="mt-2 w-full rounded-xl border border-white/[0.1] bg-[#0d1017] p-3 text-white outline-none focus:border-fuchsia-400"
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
                    className="mt-2 w-full rounded-xl border border-white/[0.1] bg-[#0d1017] p-3 text-white outline-none focus:border-fuchsia-400"
                  />
                </label>
                <label className="text-zinc-300 col-span-2 text-sm">
                  Giọng đọc
                  <select
                    value={voiceStyle}
                    onChange={(event) => setVoiceStyle(event.target.value)}
                    className="mt-2 w-full rounded-xl border border-white/[0.1] bg-[#0d1017] p-3 text-white outline-none focus:border-fuchsia-400"
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
          <div className="rounded-2xl border border-white/[0.08] bg-[#121622] p-5">
            <div className="mb-4 flex items-center gap-2">
              <ImageIcon className="h-4 w-4 text-fuchsia-300" />
              <h2 className="font-bold text-white">2. Anchor nhân vật</h2>
            </div>
            <div className="grid gap-4 md:grid-cols-[minmax(0,1fr)_220px]">
              <div>
                <p className="text-zinc-400 text-sm">
                  Anchor là artifact cục bộ được gắn vào request. Nó là điều
                  kiện cần để Donut/Grok có thể giữ nhận diện qua các scene.
                </p>
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
                  className="mt-4 inline-flex items-center gap-2 rounded-xl border border-fuchsia-400/40 bg-fuchsia-500/10 px-4 py-2.5 text-sm font-semibold text-fuchsia-200 transition hover:bg-fuchsia-500/20 disabled:opacity-60"
                >
                  {ingesting ? (
                    <Loader2 className="h-4 w-4 animate-spin" />
                  ) : (
                    <Upload className="h-4 w-4" />
                  )}
                  {anchor ? "Thay anchor" : "Thêm anchor nhân vật"}
                </button>
                {anchor && (
                  <p className="mt-3 break-all font-mono text-[11px] text-emerald-300">
                    Artifact: {anchor.artifact_id} · SHA:{" "}
                    {anchor.sha256?.slice(0, 12) || "chưa có SHA"}
                    {anchor.sha256 ? "…" : ""}
                  </p>
                )}
              </div>
              <div className="flex min-h-36 items-center justify-center overflow-hidden rounded-xl border border-dashed border-white/[0.15] bg-black/20">
                {anchorPreview ? (
                  <img
                    src={anchorPreview}
                    alt="Anchor nhân vật"
                    className="h-40 w-full object-cover"
                  />
                ) : (
                  <span className="text-zinc-600 px-4 text-center text-xs">
                    Chưa có ảnh anchor
                  </span>
                )}
              </div>
            </div>
          </div>
        </div>
        <aside className="space-y-5">
          <div className="rounded-2xl border border-white/[0.08] bg-[#121622] p-5">
            <div className="flex items-center gap-2">
              <Film className="h-4 w-4 text-fuchsia-300" />
              <h2 className="font-bold text-white">Điều phối Donut Browser</h2>
            </div>
            <label className="text-zinc-500 mt-4 block text-xs">
              Page/Kênh
              <select
                value={page?.id || ""}
                onChange={(event) => onSelectPage(event.target.value)}
                className="mt-1.5 w-full rounded-xl border border-white/[0.1] bg-[#0d1017] p-2.5 text-sm text-white outline-none"
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
                  profile?.grok_logged_in ? "Đã xác nhận" : "Chưa xác nhận"
                }
                good={Boolean(profile?.grok_logged_in)}
              />
              <StatusRow
                label="Extension"
                value={profile?.extension_ready ? "Sẵn sàng" : "Chưa sẵn sàng"}
                good={Boolean(profile?.extension_ready)}
              />
            </dl>
            {!donutReady && (
              <p className="mt-4 rounded-xl border border-amber-500/20 bg-amber-500/10 p-3 text-xs leading-5 text-amber-200">
                Donut chưa sẵn sàng cho bridge Grok. Điều này không chặn luồng
                phim hiện tại: worker dùng visual provider nội bộ/OmniRoute.
                Chỉ khi adapter Donut trả receipt và artifact thật, bridge mới
                được dùng để gửi từng scene sang Grok.
              </p>
            )}
          </div>
          <div className="rounded-2xl border border-white/[0.08] bg-[#121622] p-5">
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
                className="mt-4 inline-flex w-full items-center justify-center gap-2 rounded-xl border border-rose-400/30 bg-rose-500/10 px-4 py-3 font-semibold text-rose-200"
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
        </aside>
      </section>
      <section className="rounded-2xl border border-white/[0.08] bg-[#121622] p-5">
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
        {activeRun?.errorMessage && (
          <p className="mt-4 rounded-xl border border-rose-500/30 bg-rose-500/10 p-3 text-sm text-rose-200">
            {activeRun.errorMessage}
          </p>
        )}
        {video && (
          <p className="mt-4 rounded-xl border border-emerald-500/30 bg-emerald-500/10 p-3 text-sm text-emerald-200">
            Video output: {video.name}
          </p>
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
    <div className="rounded-xl border border-white/[0.08] bg-black/20 px-3 py-2">
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
      ? "border-emerald-500/30 bg-emerald-500/[0.06]"
      : state === "running"
        ? "border-sky-500/40 bg-sky-500/[0.08]"
        : state === "failed"
          ? "border-rose-500/40 bg-rose-500/[0.08]"
          : "border-white/[0.07] bg-black/10";
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
              : "Chờ job"}
      </p>
    </div>
  );
}
