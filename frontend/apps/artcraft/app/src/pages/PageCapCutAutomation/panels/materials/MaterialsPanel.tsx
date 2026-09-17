import { useState } from "react";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import {
  faImage,
  faMusic,
  faPlus,
  faVideo,
} from "@fortawesome/pro-solid-svg-icons";
import { twMerge } from "tailwind-merge";
import toast from "react-hot-toast";
import { useCapCutMate } from "../../api/CapCutMateContext";
import * as api from "../../api/capcutBeClient";
import { PanelGuide } from "../../shared/PanelGuide";
import { PanelStatusAside } from "../../shared/PanelStatusAside";
import { ResizableSplit } from "../../shared/ResizableSplit";

type Kind = "video" | "image" | "audio";

const KIND_LABEL: Record<Kind, string> = {
  video: "video",
  image: "ảnh",
  audio: "audio",
};

/** Thêm media bằng URL vào draft — BE: add_videos / add_images / add_audios */
export function MaterialsPanel() {
  const mate = useCapCutMate();
  const [kind, setKind] = useState<Kind>("video");
  const [url, setUrl] = useState("");
  const [durationSec, setDurationSec] = useState(5);
  const [transition, setTransition] = useState("");
  const [busy, setBusy] = useState(false);

  const tabs: { id: Kind; label: string; icon: typeof faVideo }[] = [
    { id: "video", label: "Video", icon: faVideo },
    { id: "image", label: "Ảnh", icon: faImage },
    { id: "audio", label: "Audio", icon: faMusic },
  ];

  const handleAdd = async () => {
    const mediaUrl = url.trim();
    if (!mediaUrl) {
      toast.error("Dán URL media (http/https)");
      return;
    }
    if (!mediaUrl.startsWith("http://") && !mediaUrl.startsWith("https://")) {
      toast.error("URL phải bắt đầu bằng http:// hoặc https://");
      return;
    }
    // TikTok page URL không phải file media trực tiếp
    if (
      /tiktok\.com|douyin\.com/i.test(mediaUrl) &&
      !/\.(mp4|mov|webm)(\?|$)/i.test(mediaUrl)
    ) {
      toast.error(
        "Link TikTok trang web không dùng được — cần URL file .mp4 trực tiếp (CDN)",
      );
      return;
    }

    setBusy(true);
    try {
      const draftUrl = mate.ensureDraft();
      const start = mate.timelineEndUs;
      let end = start + Math.max(0.1, durationSec) * api.US;

      if (kind === "audio") {
        try {
          const d = await api.getAudioDuration(mediaUrl);
          if (typeof d.duration === "number" && d.duration > 0) {
            end = start + d.duration;
            setDurationSec(d.duration / api.US);
          }
        } catch {
          /* dùng duration tay */
        }
      }

      if (kind === "video") {
        const item: Record<string, unknown> = {
          video_url: mediaUrl,
          start,
          end,
          duration: end - start,
        };
        if (transition.trim()) {
          item.transition = transition.trim();
          item.transition_duration = 500_000;
        }
        await api.addVideos(draftUrl, [item]);
        toast.success("Đã thêm video vào draft");
      } else if (kind === "image") {
        const item: Record<string, unknown> = {
          image_url: mediaUrl,
          start,
          end,
        };
        if (transition.trim()) {
          item.transition = transition.trim();
          item.transition_duration = 500_000;
        }
        await api.addImages(draftUrl, [item]);
        toast.success("Đã thêm ảnh vào draft");
      } else {
        await api.addAudios(draftUrl, [
          { audio_url: mediaUrl, start, end, volume: 1 },
        ]);
        toast.success("Đã thêm audio vào draft");
      }

      mate.setTimelineEndUs(end);
      setUrl("");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "Thêm media thất bại");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col bg-[#0d1017]">
      <PanelGuide
        what="Thêm video, ảnh, audio vào draft mate bằng URL file trực tiếp."
        how="① Tạo draft (thanh trên) · ② chọn loại · ③ dán URL http(s) · ④ thời lượng · ⑤ Thêm."
        need={
          mate.draftUrl
            ? "Đã có draft mate — dán link file .mp4/.jpg/.mp3 (không link trang TikTok)."
            : "Draft mate — bấm «Tạo draft» trước."
        }
        tone={mate.draftUrl ? "default" : "warn"}
      />
      <div className="border-b border-white/10 px-6 py-4">
        <h2 className="text-sm font-bold text-white tracking-wide">Nguyên Liệu Media</h2>
        <p className="mt-0.5 text-xs text-zinc-400">
          URL file →{" "}
          <code className="text-zinc-300">add_videos / add_images / add_audios</code>
        </p>
      </div>

      <ResizableSplit
        storageKey="capcut-split-materials"
        defaultWidth={280}
        minWidth={220}
        maxWidth={400}
        left={
      <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        <div className="mx-auto max-w-xl space-y-4">
          <div className="flex gap-1.5 rounded-xl border border-white/10 bg-[#10141e] p-1.5">
            {tabs.map((t) => (
              <button
                key={t.id}
                type="button"
                onClick={() => setKind(t.id)}
                className={twMerge(
                  "flex flex-1 items-center justify-center gap-2 rounded-lg py-2 text-xs font-semibold transition",
                  kind === t.id
                    ? "border border-white/10 bg-rose-500/10 text-white shadow-md shadow-rose-500/5"
                    : "text-zinc-400 hover:bg-white/[0.04] hover:text-zinc-100",
                )}
              >
                <FontAwesomeIcon icon={t.icon} />
                {t.label}
              </button>
            ))}
          </div>

          <div>
            <label className="mb-1.5 block text-xs font-medium text-zinc-400">
              URL media (file trực tiếp)
            </label>
            <input
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://…/clip.mp4"
              className="w-full rounded-lg border border-white/10 bg-[#0b0f17] px-3.5 py-2 font-mono text-xs text-zinc-200 outline-none focus:border-indigo-400/50"
            />
            <p className="mt-1 text-[10px] text-zinc-500">
              Cần link file (mp4/jpg/mp3…), không dùng link trang web
            </p>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="mb-1.5 block text-xs font-medium text-zinc-400">
                Thời lượng (giây)
              </label>
              <input
                type="number"
                min={0.1}
                step={0.1}
                value={durationSec}
                onChange={(e) => setDurationSec(Number(e.target.value) || 1)}
                className="w-full rounded-lg border border-white/10 bg-[#0b0f17] px-3 py-2 text-xs text-zinc-200 outline-none focus:border-indigo-400/50"
              />
              {kind === "audio" && (
                <p className="mt-1 text-[10px] text-zinc-500">
                  Sẽ thử đo duration tự động trước
                </p>
              )}
            </div>
            {(kind === "video" || kind === "image") && (
              <div>
                <label className="mb-1.5 block text-xs font-medium text-zinc-400">
                  Chuyển cảnh (tuỳ chọn)
                </label>
                <input
                  value={transition}
                  onChange={(e) => setTransition(e.target.value)}
                  placeholder="淡入淡出"
                  className="w-full rounded-lg border border-white/10 bg-[#0b0f17] px-3 py-2 text-xs text-zinc-200 outline-none focus:border-indigo-400/50"
                />
              </div>
            )}
          </div>

          <div className="rounded-xl border border-white/10 bg-[#10141e] px-3.5 py-2.5 text-xs text-zinc-400">
            Thêm vào timeline lúc{" "}
            <span className="font-mono font-semibold text-zinc-200">
              {(mate.timelineEndUs / api.US).toFixed(2)}s
            </span>
            {" · "}
            {mate.draftUrl ? (
              <span className="font-medium text-emerald-400">đã có draft</span>
            ) : (
              <span className="font-medium text-amber-400">hãy tạo draft trước</span>
            )}
          </div>

          <button
            type="button"
            disabled={busy}
            onClick={() => void handleAdd()}
            className="flex w-full items-center justify-center gap-2 rounded-xl bg-indigo-500/20 border border-indigo-500/30 py-2.5 text-xs font-semibold text-indigo-300 transition hover:bg-indigo-500/30 disabled:opacity-50"
          >
            <FontAwesomeIcon icon={faPlus} />
            {busy
              ? "Đang thêm…"
              : `Thêm ${KIND_LABEL[kind]} vào draft`}
          </button>
        </div>
      </div>
        }
        right={
          <PanelStatusAside tip="Nguyên liệu ghi vào draft mate (URL). Project CapCut local dùng panel khác." />
        }
      />
    </div>
  );
}
