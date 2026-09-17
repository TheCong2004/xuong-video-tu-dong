import React, { useState } from "react";
import {
  Save,
  Plus,
  Settings2,
  RefreshCw,
  FolderOpen,
  CheckCircle2,
  AlertCircle,
  Copy,
  Ratio,
} from "lucide-react";
import toast from "react-hot-toast";
import { useCapCutMate } from "../api/CapCutMateContext";

const PRESETS: { label: string; w: number; h: number }[] = [
  { label: "9:16 (1080p Dọc)", w: 1080, h: 1920 },
  { label: "16:9 (1080p Ngang)", w: 1920, h: 1080 },
  { label: "1:1 (1080p Vuông)", w: 1080, h: 1080 },
  { label: "16:9 (720p)", w: 1280, h: 720 },
];

export function ProjectBar() {
  const mate = useCapCutMate();
  const [showSettings, setShowSettings] = useState(false);
  const [urlEdit, setUrlEdit] = useState(mate.baseUrl);

  const draftId =
    mate.draftUrl?.match(/draft_id=([^&]+)/)?.[1] ??
    (mate.draftUrl ? "draft" : null);

  const draftName = mate.localProject
    ? mate.localProject.split(/[/\\]/).filter(Boolean).pop()
    : draftId;

  return (
    <header className="flex h-16 shrink-0 flex-wrap items-center justify-between gap-3 border-b border-white/10 px-4 md:px-6">
      {/* Left controls: BE status, Canvas ratio, Active draft pill */}
      <div className="flex flex-wrap items-center gap-2.5">
        {/* Backend Status indicator */}
        <button
          type="button"
          onClick={() => void mate.refreshOnline()}
          title="Kiểm tra trạng thái kết nối backend"
          className="flex items-center gap-2 rounded-lg border border-white/10 bg-transparent px-2.5 py-1.5 text-xs transition hover:bg-white/[0.04]"
        >
          <span
            className={`h-2 w-2 rounded-full ${
              mate.online
                ? "bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.5)]"
                : mate.online === false
                ? "bg-rose-400"
                : "bg-amber-400"
            }`}
          />
          <span className="font-medium text-zinc-300">
            {mate.checking
              ? "Đang dò BE…"
              : mate.online
              ? "BE Hoạt động"
              : "BE Ngoại tuyến"}
          </span>
          <RefreshCw
            className={`h-3 w-3 text-zinc-500 ${
              mate.checking ? "animate-spin" : ""
            }`}
          />
        </button>

        {/* Canvas Ratio Preset */}
        <div className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-transparent px-2.5 py-1.5 text-xs text-zinc-300">
          <Ratio className="h-3.5 w-3.5 text-indigo-400" />
          <select
            value={`${mate.width}x${mate.height}`}
            onChange={(e) => {
              const p = PRESETS.find((x) => `${x.w}x${x.h}` === e.target.value);
              if (p) mate.setCanvasSize(p.w, p.h);
            }}
            className="cursor-pointer bg-transparent font-medium text-zinc-300 outline-none"
            title="Kích thước khung hình canvas"
          >
            {PRESETS.map((p) => (
              <option
                key={p.label}
                value={`${p.w}x${p.h}`}
                className="bg-[#12161f] text-white"
              >
                {p.label}
              </option>
            ))}
          </select>
        </div>

        <span className="font-light text-zinc-600">/</span>

        {/* Current Draft info */}
        <div className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-transparent px-2.5 py-1.5 text-xs">
          <FolderOpen className="h-3.5 w-3.5 text-rose-400" />
          <span className="font-medium text-zinc-400">Draft:</span>
          {draftName ? (
            <span
              className="max-w-xs truncate font-mono text-xs font-semibold text-rose-300"
              title={mate.localProject || mate.draftUrl || undefined}
            >
              {draftName}
            </span>
          ) : (
            <span className="text-xs text-amber-400/90">Chưa chọn draft</span>
          )}
        </div>
      </div>

      {/* Right actions: Tạo draft, Lưu, Settings */}
      <div className="flex items-center gap-2">
        <button
          type="button"
          disabled={mate.busy}
          onClick={() => void mate.createProject()}
          className="flex items-center gap-1.5 rounded-lg bg-indigo-500/20 px-3 py-1.5 text-xs font-semibold text-indigo-300 transition hover:bg-indigo-500/30 disabled:opacity-50"
        >
          <Plus className="h-3.5 w-3.5" /> Tạo draft
        </button>

        <button
          type="button"
          disabled={mate.busy || (!mate.draftUrl && !mate.localProject)}
          onClick={() => void mate.saveProject()}
          className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-white/[0.04] px-3 py-1.5 text-xs font-semibold text-zinc-200 transition hover:bg-white/[0.08] disabled:opacity-40"
        >
          <Save className="h-3.5 w-3.5" /> Lưu
        </button>

        <button
          type="button"
          onClick={() => {
            setUrlEdit(mate.baseUrl);
            setShowSettings((v) => !v);
          }}
          className={`flex items-center gap-1.5 rounded-lg border border-white/10 px-3 py-1.5 text-xs font-semibold transition ${
            showSettings
              ? "bg-white/10 text-white"
              : "bg-white/[0.04] text-zinc-200 hover:bg-white/[0.08]"
          }`}
          title="Cài đặt kết nối backend"
        >
          <Settings2 className="h-3.5 w-3.5" /> Cấu hình
        </button>
      </div>

      {/* Collapsible Backend Settings */}
      {showSettings && (
        <div className="flex w-full items-center gap-2.5 rounded-xl border border-white/10 bg-[#141a24] p-3 text-xs">
          <span className="shrink-0 text-zinc-400">Địa chỉ Backend :30000</span>
          <input
            value={urlEdit}
            onChange={(e) => setUrlEdit(e.target.value)}
            className="min-w-0 flex-1 rounded-lg border border-white/10 bg-[#0d1017] px-3 py-1.5 font-mono text-xs text-white outline-none focus:border-indigo-400/50"
            placeholder="http://127.0.0.1:30000"
          />
          <button
            type="button"
            onClick={() => {
              mate.setBaseUrl(urlEdit.trim() || "http://127.0.0.1:30000");
              toast.success("Đã lưu URL backend");
              void mate.refreshOnline();
            }}
            className="rounded-lg bg-indigo-500/20 px-3 py-1.5 text-xs font-semibold text-indigo-300 transition hover:bg-indigo-500/30"
          >
            Áp dụng
          </button>
          {mate.draftUrl && (
            <button
              type="button"
              onClick={() => {
                void navigator.clipboard.writeText(mate.draftUrl!);
                toast.success("Đã copy draft_url");
              }}
              className="flex items-center gap-1 rounded-lg border border-white/10 bg-transparent px-2.5 py-1.5 text-xs text-zinc-400 transition hover:bg-white/5 hover:text-white"
            >
              <Copy className="h-3 w-3" /> Copy URL
            </button>
          )}
        </div>
      )}
    </header>
  );
}
