import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

type ModelStatus = "NOT_INSTALLED" | "DOWNLOADING" | "VERIFYING" | "READY" | "CORRUPTED" | "LICENSE_REQUIRED" | "FAILED";
type ModelRecord = {
  id: string;
  capability: string;
  engine: string;
  version: string;
  relative_path: string;
  download_url: string;
  size?: number | null;
  license: string;
  license_url: string;
  status: ModelStatus;
  progress: number;
  error?: string | null;
};

const statusLabel: Record<ModelStatus, string> = {
  NOT_INSTALLED: "Chưa cài",
  DOWNLOADING: "Đang tải",
  VERIFYING: "Đang xác minh",
  READY: "Sẵn sàng",
  CORRUPTED: "Lỗi checksum",
  LICENSE_REQUIRED: "Cần xem giấy phép",
  FAILED: "Thất bại",
};

export function ModelStatusPanel() {
  const [models, setModels] = useState<ModelRecord[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const refresh = () => {
    // The panel is also rendered by the browser-based component tests and by
    // the Vite preview.  Tauri commands are only available in the desktop
    // shell, so keep the model list empty until that bridge exists.
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window || "__TAURI__" in window)) {
      setModels([]);
      return;
    }
    void invoke<ModelRecord[]>("list_capcut_ai_models").then(setModels).catch(() => setModels([]));
  };
  useEffect(() => { refresh(); }, []);
  const install = async (id: string) => {
    setBusy(id);
    try {
      await invoke("download_capcut_ai_model", { id });
      refresh();
    } finally {
      setBusy(null);
    }
  };
  const cancel = async (id: string) => {
    setBusy(id);
    try { await invoke("cancel_capcut_ai_model_download", { id }); refresh(); } finally { setBusy(null); }
  };
  const remove = async (id: string) => {
    setBusy(id);
    try { await invoke("remove_capcut_ai_model", { id }); refresh(); } finally { setBusy(null); }
  };
  return (
    <Panel title="Mô hình AI nội bộ">
      <p className="mb-3 text-[11px] text-slate-400">Tài nguyên thuộc Xưởng Sản Xuất Video, tải qua HTTPS và kiểm tra SHA trước khi dùng.</p>
      <div className="space-y-2">
        {models.map((model) => (
          <div key={model.id} className="rounded-lg border border-white/10 bg-slate-950/30 p-2 text-xs">
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium text-slate-100">{model.engine}</span>
              <span className={model.status === "READY" ? "text-emerald-300" : "text-amber-200"}>{statusLabel[model.status]}</span>
            </div>
            <div className="mt-1 truncate text-[10px] text-slate-500">{model.version} · {model.relative_path}{model.size ? ` · ${(model.size / 1048576).toFixed(1)} MB` : ""}</div>
            {model.status === "DOWNLOADING" && <div className="mt-1 h-1 rounded bg-white/10"><div className="h-full rounded bg-cyan-400" style={{ width: `${model.progress}%` }} /></div>}
            {model.status === "DOWNLOADING" && <button type="button" disabled={busy === model.id} onClick={() => void cancel(model.id)} className="mt-2 mr-2 rounded-lg border border-rose-500/30 bg-rose-500/20 px-2.5 py-1 text-[11px] font-semibold text-rose-300 disabled:opacity-50">Hủy tải</button>}
            {(model.status === "NOT_INSTALLED" || model.status === "CORRUPTED") && (
              <button type="button" disabled={busy === model.id} onClick={() => void install(model.id)} className="mt-2 rounded-lg border border-indigo-500/30 bg-indigo-500/20 px-2.5 py-1 text-[11px] font-semibold text-indigo-300 hover:bg-indigo-500/30 disabled:opacity-50">
                {busy === model.id ? "Đang cài…" : "Cài mô hình"}
              </button>
            )}
            {(model.status === "READY" || model.status === "CORRUPTED") && <button type="button" disabled={busy === model.id} onClick={() => void remove(model.id)} className="mt-2 ml-2 rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1 text-[11px] font-semibold text-zinc-300 hover:bg-white/[0.08] disabled:opacity-50">Gỡ mô hình</button>}
          </div>
        ))}
      </div>
    </Panel>
  );
}

function Panel({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="rounded-2xl border border-white/10 bg-[#121622] p-5 shadow-xl shadow-black/10">
      <h3 className="mb-4 text-sm font-bold text-white tracking-wide">{title}</h3>
      {children}
    </div>
  );
}
