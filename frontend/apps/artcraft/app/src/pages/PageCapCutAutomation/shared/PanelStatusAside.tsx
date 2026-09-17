import { useCapCutMate } from "../api/CapCutMateContext";

/** Cột phải gọn: trạng thái draft mate / local — để mọi mục 1 cột vẫn kéo thu phóng được. */
export function PanelStatusAside({ tip }: { tip?: string }) {
  const mate = useCapCutMate();
  return (
    <aside className="flex h-full min-h-0 w-full flex-col gap-3 overflow-y-auto border-l border-white/10 bg-[#10141e] px-4 py-4">
      <h3 className="text-xs font-bold uppercase tracking-wider text-zinc-300">
        Trạng Thái
      </h3>
      <div className="space-y-2.5 text-xs leading-relaxed text-zinc-400">
        <div className="rounded-xl border border-white/10 bg-white/[0.02] p-2.5">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Backend :30000
          </div>
          <div className="mt-1 flex items-center gap-1.5 font-mono text-xs font-semibold">
            <span
              className={`h-2 w-2 rounded-full ${
                mate.online
                  ? "bg-emerald-400 shadow-[0_0_8px_rgba(52,211,153,0.5)]"
                  : mate.online === false
                  ? "bg-rose-400"
                  : "bg-amber-400"
              }`}
            />
            <span className={mate.online ? "text-emerald-300" : "text-zinc-400"}>
              {mate.online === true
                ? "Hoạt động"
                : mate.online === false
                  ? "Ngoại tuyến"
                  : "Đang kiểm tra…"}
            </span>
          </div>
        </div>

        <div className="rounded-xl border border-white/10 bg-white/[0.02] p-2.5">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Draft Mate
          </div>
          <div className="mt-1 break-all font-mono text-xs text-indigo-300">
            {mate.draftUrl
              ? mate.draftUrl.match(/draft_id=([^&]+)/)?.[1] || "Đã sẵn sàng"
              : "Chưa kết nối"}
          </div>
        </div>

        <div className="rounded-xl border border-white/10 bg-white/[0.02] p-2.5">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Draft Local
          </div>
          <div className="mt-1 break-all font-mono text-xs text-emerald-300">
            {mate.localProject.trim() || "Chưa chọn"}
          </div>
        </div>

        <div className="rounded-xl border border-white/10 bg-white/[0.02] p-2.5">
          <div className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Timeline
          </div>
          <div className="mt-1 font-mono text-xs font-semibold text-zinc-200">
            {(mate.timelineEndUs / 1_000_000).toFixed(2)}s
          </div>
        </div>

        {tip ? (
          <p className="rounded-lg border border-white/10 bg-white/[0.01] p-2 text-[11px] text-zinc-400">
            {tip}
          </p>
        ) : null}
      </div>
      <p className="mt-auto text-[10px] text-zinc-600">
        Kéo mép giữa cột để thu phóng panel
      </p>
    </aside>
  );
}
