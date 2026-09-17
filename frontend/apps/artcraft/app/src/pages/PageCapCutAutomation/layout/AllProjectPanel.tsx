import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  RefreshCw,
  SlidersHorizontal,
  Check,
  ChevronRight,
  ChevronLeft,
  FolderOpen,
  HardDrive,
  Plus,
  Trash2,
  Search,
  ExternalLink,
} from "lucide-react";
import { twMerge } from "tailwind-merge";
import toast from "react-hot-toast";
import { useCapCutMate } from "../api/CapCutMateContext";
import * as local from "../api/capcutLocalClient";
import type { LocalProjectItem } from "../api/capcutLocalClient";
import {
  formatDurationUs,
  localProjectCoverUrl,
} from "../api/capcutLocalClient";

const MIN_WIDTH = 220;
const MAX_WIDTH = 560;
const DEFAULT_WIDTH = 300;
const COLLAPSED_WIDTH = 40;
const STORAGE_KEY = "capcut-all-projects-width";
const DRAFTS_DIR_KEY = "capcut-custom-drafts-dir";

function loadWidth(): number {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_WIDTH;
    const n = Number(raw);
    if (Number.isFinite(n)) {
      return Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, n));
    }
  } catch {
    /* ignore */
  }
  return DEFAULT_WIDTH;
}

function loadCustomDir(): string {
  try {
    return localStorage.getItem(DRAFTS_DIR_KEY) || "";
  } catch {
    return "";
  }
}

function projectPath(p: LocalProjectItem): string {
  return (p.project || p.path || "").trim();
}

function displayName(p: LocalProjectItem): string {
  return (p.name || p.folder || "—").trim() || "—";
}

/**
 * Rail phải — quét project CapCut/JianYing thật qua BE `/v1/local/projects`.
 * Click = set Draft local (path folder) cho các panel Apply.
 */
export function AllProjectPanel() {
  const mate = useCapCutMate();
  const [search, setSearch] = useState("");
  const [backup, setBackup] = useState(false);
  const [collapsed, setCollapsed] = useState(false);
  const [width, setWidth] = useState(loadWidth);
  const [dragging, setDragging] = useState(false);
  const dragStartX = useRef(0);
  const dragStartWidth = useRef(DEFAULT_WIDTH);

  const [projects, setProjects] = useState<LocalProjectItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [customDir, setCustomDir] = useState(loadCustomDir);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());

  const activePath = mate.localProject.trim();
  const mateLocalProject = mate.localProject;
  const setMateLocalProject = mate.setLocalProject;

  const refresh = useCallback(
    async (opts?: { drafts_dir?: string; silent?: boolean }) => {
      setLoading(true);
      setError(null);
      try {
        const dir = (opts?.drafts_dir ?? customDir).trim();
        const res = await local.localProjects({
          names: true,
          ...(dir ? { drafts_dir: dir } : {}),
          ...(search.trim() ? { query: search.trim() } : {}),
        });
        const list = res.projects ?? [];
        setProjects(list);
        if (list.length > 0 && !mateLocalProject) {
          const firstPath = (list[0].project || list[0].path || "").replace(/[/\\]draft_content\.json$/i, "");
          if (firstPath) {
            setMateLocalProject(firstPath);
            setSelectedIds(new Set([firstPath]));
          }
        }
        if (!opts?.silent) {
          toast.success(
            list.length
              ? `Tìm thấy ${list.length} dự án`
              : "Không có project (mở CapCut lưu 1 lần, hoặc chọn folder)",
          );
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : "Quét project thất bại";
        setError(msg);
        setProjects([]);
        if (!opts?.silent) toast.error(msg);
      } finally {
        setLoading(false);
      }
    },
    [customDir, search, mateLocalProject, setMateLocalProject],
  );

  // Auto-scan khi mở panel / BE online
  useEffect(() => {
    if (collapsed) return;
    void refresh({ silent: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collapsed, mate.online]);

  useEffect(() => {
    if (collapsed) return;
    try {
      localStorage.setItem(STORAGE_KEY, String(width));
    } catch {
      /* ignore */
    }
  }, [width, collapsed]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return projects;
    return projects.filter((p) => {
      const n = displayName(p).toLowerCase();
      const path = projectPath(p).toLowerCase();
      return n.includes(q) || path.includes(q);
    });
  }, [projects, search]);

  const toggleMulti = (p: LocalProjectItem, e: React.MouseEvent) => {
    e.stopPropagation();
    const id = (p.project || projectPath(p)).replace(
      /[/\\]draft_content\.json$/i,
      "",
    );
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const selectProject = (p: LocalProjectItem) => {
    const folder = (p.project || projectPath(p)).replace(
      /[/\\]draft_content\.json$/i,
      "",
    );
    mate.setLocalProject(folder);
    setSelectedIds(new Set([folder]));
    toast.success(`Đã chọn: ${displayName(p)}`);
  };

  const useSelectedAsLocal = () => {
    const first = [...selectedIds][0];
    if (!first) {
      toast.error("Chưa tick chọn project nào");
      return;
    }
    mate.setLocalProject(first);
    toast.success("Đã set Draft local từ lựa chọn");
  };

  const deleteProjects = async (paths: string[]) => {
    const unique = [...new Set(paths.map((p) => p.trim()).filter(Boolean))];
    if (!unique.length) {
      toast.error("Chọn project cần xóa (tick checkbox)");
      return;
    }
    const preview = unique
      .slice(0, 5)
      .map((p) => `• ${p}`)
      .join("\n");
    const more =
      unique.length > 5 ? `\n… và ${unique.length - 5} project nữa` : "";
    const ok = window.confirm(
      `XÓA VĨNH VIỄN ${unique.length} project trên đĩa?\n\n${preview}${more}\n\nKhông hoàn tác được. CapCut đang mở project này có thể lỗi — nên đóng CapCut trước.`,
    );
    if (!ok) return;
    const ok2 = window.confirm(
      "Xác nhận lần 2: Xóa folder draft (có draft_content.json)?",
    );
    if (!ok2) return;

    setLoading(true);
    let okCount = 0;
    const errors: string[] = [];
    for (const path of unique) {
      try {
        await local.localDeleteProject(path, true);
        okCount += 1;
        if (activePath === path || activePath.startsWith(path)) {
          mate.setLocalProject("");
        }
      } catch (e) {
        errors.push(
          `${path}: ${e instanceof Error ? e.message : String(e)}`,
        );
      }
    }
    setSelectedIds(new Set());
    await refresh({ silent: true });
    setLoading(false);
    if (okCount) toast.success(`Đã xóa ${okCount} project`);
    if (errors.length) toast.error(errors[0]);
  };

  const promptCustomDir = () => {
    const next = window.prompt(
      "Path folder chứa các draft CapCut (com.lveditor.draft hoặc folder cha):",
      customDir ||
        "%LOCALAPPDATA%\\CapCut\\User Data\\Projects\\com.lveditor.draft",
    );
    if (next == null) return;
    const cleaned = next.trim();
    setCustomDir(cleaned);
    try {
      if (cleaned) localStorage.setItem(DRAFTS_DIR_KEY, cleaned);
      else localStorage.removeItem(DRAFTS_DIR_KEY);
    } catch {
      /* ignore */
    }
    void refresh({ drafts_dir: cleaned });
  };

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (collapsed) return;
      e.preventDefault();
      setDragging(true);
      dragStartX.current = e.clientX;
      dragStartWidth.current = width;
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [collapsed, width],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging) return;
      const delta = dragStartX.current - e.clientX;
      const next = Math.min(
        MAX_WIDTH,
        Math.max(MIN_WIDTH, dragStartWidth.current + delta),
      );
      setWidth(next);
    },
    [dragging],
  );

  const onPointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    setDragging(false);
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
  }, []);

  const onDoubleClick = useCallback(() => {
    setCollapsed((c) => !c);
  }, []);

  if (collapsed) {
    return (
      <aside
        className="relative flex shrink-0 flex-col items-center border-l border-white/10 bg-[#10141e] py-4"
        style={{ width: COLLAPSED_WIDTH }}
      >
        <ResizeHandle
          collapsed
          onPointerDown={(e) => {
            e.preventDefault();
          }}
          onPointerMove={() => undefined}
          onPointerUp={() => undefined}
          onDoubleClick={onDoubleClick}
          onClick={() => setCollapsed(false)}
        />
        <button
          type="button"
          title="Mở rộng danh sách dự án"
          onClick={() => setCollapsed(false)}
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-white/10 text-zinc-400 hover:bg-white/5 hover:text-white transition"
        >
          <ChevronLeft className="h-4 w-4" />
        </button>
        <span
          className="mt-4 origin-center rotate-90 whitespace-nowrap text-[10px] font-bold uppercase tracking-wider text-zinc-500"
          style={{ writingMode: "vertical-rl" }}
        >
          Tất cả dự án
        </span>
        {projects.length > 0 && (
          <span className="mt-3 rounded-full bg-indigo-500/20 px-1.5 py-0.5 text-[9px] font-bold text-indigo-300">
            {projects.length}
          </span>
        )}
      </aside>
    );
  }

  return (
    <aside
      className={twMerge(
        "relative flex shrink-0 flex-col border-l border-white/10 bg-[#10141e]",
        dragging && "select-none",
      )}
      style={{ width }}
    >
      <ResizeHandle
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onDoubleClick={onDoubleClick}
        dragging={dragging}
      />

      {/* Header matching Floword Header height & border */}
      <div className="flex h-16 shrink-0 items-center justify-between gap-2 border-b border-white/10 px-3.5">
        <div className="flex min-w-0 items-center gap-2">
          <button
            type="button"
            onClick={() => setCollapsed(true)}
            className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-white/10 text-zinc-400 hover:bg-white/5 hover:text-white transition"
            title="Thu gọn panel"
          >
            <ChevronRight className="h-3.5 w-3.5" />
          </button>
          <span className="truncate text-xs font-bold text-white tracking-wide">
            Dự Án CapCut
          </span>
          <span className="shrink-0 rounded-md bg-white/10 px-1.5 py-0.5 text-[10px] font-semibold text-zinc-300">
            {loading ? "…" : filtered.length}
          </span>
        </div>

        <div className="flex items-center gap-1">
          <IconBtn
            icon={Trash2}
            title="Xóa project đã chọn trên đĩa"
            onClick={() =>
              void deleteProjects(
                selectedIds.size
                  ? [...selectedIds]
                  : activePath
                    ? [activePath]
                    : [],
              )
            }
          />
          <IconBtn
            icon={FolderOpen}
            title="Chọn folder CapCut (drafts_dir)"
            onClick={promptCustomDir}
          />
          <IconBtn
            icon={Plus}
            title="Dùng project đã chọn làm Draft local"
            onClick={useSelectedAsLocal}
          />
          <IconBtn
            icon={RefreshCw}
            title="Quét lại (BE /local/projects)"
            onClick={() => void refresh()}
            spin={loading}
          />
        </div>
      </div>

      {/* Search Bar */}
      <div className="p-3 border-b border-white/10 space-y-1.5">
        <div className="relative flex items-center">
          <Search className="absolute left-2.5 h-3.5 w-3.5 text-zinc-500 pointer-events-none" />
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void refresh();
            }}
            placeholder="Tìm dự án… (Enter để quét)"
            className="w-full rounded-lg border border-white/10 bg-[#0b0f17] pl-8 pr-3 py-1.5 text-xs text-zinc-200 outline-none placeholder:text-zinc-500 focus:border-indigo-400/50"
          />
        </div>
        {customDir ? (
          <p className="truncate font-mono text-[9px] text-indigo-300/80 px-1" title={customDir}>
            dir: {customDir}
          </p>
        ) : (
          <p className="text-[9px] text-zinc-500 px-1">
            Mặc định: CapCut / JianYing local drafts
          </p>
        )}
      </div>

      {/* Active Local Project Banner */}
      {activePath ? (
        <div className="mx-3 my-2 flex items-start gap-2.5 rounded-xl border border-emerald-500/20 bg-emerald-500/10 p-2.5 text-xs">
          <HardDrive className="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400" />
          <div className="min-w-0 flex-1">
            <div className="text-[10px] font-semibold uppercase tracking-wider text-emerald-300">
              Draft Local Đang Dùng
            </div>
            <div className="truncate font-mono text-[10px] text-emerald-200/80" title={activePath}>
              {activePath}
            </div>
          </div>
        </div>
      ) : null}

      {/* Projects List */}
      <div className="min-h-0 flex-1 overflow-y-auto p-2.5 space-y-1.5">
        {loading && (
          <p className="px-4 py-8 text-center text-xs text-zinc-500">
            Đang quét project…
          </p>
        )}

        {!loading && error && (
          <div className="flex flex-col items-center px-4 py-8 text-center">
            <p className="text-xs font-semibold text-rose-300">
              Không quét được project
            </p>
            <p className="mt-2 max-w-[220px] text-[11px] leading-relaxed text-zinc-400">
              {error}
            </p>
            <p className="mt-2 text-[10px] text-zinc-500">
              BE phải chạy trên máy có CapCut (:30000).
            </p>
            <div className="mt-4 w-full max-w-[200px] space-y-2">
              <ActionBtn label="Thử lại" onClick={() => void refresh()} />
              <ActionBtn label="Chọn folder…" onClick={promptCustomDir} />
            </div>
          </div>
        )}

        {!loading && !error && filtered.length === 0 && (
          <div className="flex flex-col items-center px-4 py-8 text-center">
            <p className="text-xs font-semibold text-zinc-300">
              Không thấy dự án CapCut
            </p>
            <p className="mt-2 max-w-[220px] text-[11px] leading-relaxed text-zinc-400">
              Mở CapCut Desktop, tạo/lưu project một lần để BE phát hiện.
            </p>
            <div className="mt-4 flex w-full max-w-[200px] flex-col gap-2">
              <ActionBtn
                label="Tìm folder CapCut"
                onClick={promptCustomDir}
              />
              <ActionBtn
                label="Làm mới danh sách"
                onClick={() => void refresh()}
              />
            </div>
          </div>
        )}

        {!loading &&
          !error &&
          filtered.map((p) => {
            const path = (p.project || projectPath(p)).replace(
              /[/\\]draft_content\.json$/i,
              "",
            );
            const isActive =
              activePath.length > 0 &&
              (activePath === path ||
                activePath.replace(/[/\\]+$/, "") === path.replace(/[/\\]+$/, ""));
            const isMulti = selectedIds.has(path);
            const coverSrc =
              p.has_cover || p.cover_path
                ? localProjectCoverUrl(path)
                : null;
            const isEmpty = p.empty === true || (p.duration_us ?? 0) === 0;
            const durLabel = formatDurationUs(p.duration_us);
            const mediaHint = p.media
              ? [
                  p.media.videos ? `${p.media.videos}v` : null,
                  p.media.audios ? `${p.media.audios}a` : null,
                  p.media.images ? `${p.media.images}i` : null,
                  p.media.segments ? `${p.media.segments}seg` : null,
                ]
                  .filter(Boolean)
                  .join(" · ")
              : "";

            return (
              <button
                key={path || p.folder}
                type="button"
                onClick={() => selectProject(p)}
                className={twMerge(
                  "flex w-full items-center gap-2.5 rounded-xl border p-2 text-left transition",
                  isActive
                    ? "border-rose-500/40 bg-rose-500/10 shadow-sm shadow-rose-500/10"
                    : "border-white/10 bg-white/[0.02] hover:bg-white/[0.05]",
                )}
              >
                {/* Selection Checkbox */}
                <span
                  role="checkbox"
                  aria-checked={isMulti}
                  tabIndex={0}
                  onClick={(e) => toggleMulti(p, e)}
                  onKeyDown={(e) => {
                    if (e.key === " " || e.key === "Enter") {
                      e.preventDefault();
                      toggleMulti(p, e as unknown as React.MouseEvent);
                    }
                  }}
                  className={twMerge(
                    "flex h-4 w-4 shrink-0 items-center justify-center rounded border transition text-[10px]",
                    isMulti
                      ? "border-rose-500 bg-rose-500 text-white"
                      : "border-white/20 bg-white/[0.04] text-transparent hover:border-white/40",
                  )}
                >
                  <Check className="h-3 w-3" />
                </span>

                {/* Thumbnail */}
                <div className="relative h-11 w-16 shrink-0 overflow-hidden rounded-lg border border-white/10 bg-[#0b0f17]">
                  {coverSrc ? (
                    <img
                      src={coverSrc}
                      alt=""
                      loading="lazy"
                      className="h-full w-full object-cover"
                      onError={(e) => {
                        (e.target as HTMLImageElement).style.display = "none";
                      }}
                    />
                  ) : (
                    <div className="flex h-full w-full flex-col items-center justify-center gap-0.5 text-zinc-500">
                      <FolderOpen className="h-3.5 w-3.5" />
                      <span className="text-[8px] font-medium uppercase tracking-wide">
                        {isEmpty ? "trống" : "chưa có bìa"}
                      </span>
                    </div>
                  )}
                </div>

                {/* Info */}
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate text-xs font-semibold text-white">
                      {displayName(p)}
                    </span>
                    {isActive && (
                      <span className="shrink-0 rounded-full bg-emerald-500/20 px-1.5 py-0.2 text-[8px] font-bold uppercase text-emerald-300">
                        đang dùng
                      </span>
                    )}
                    {isEmpty && (
                      <span className="shrink-0 rounded-full bg-amber-500/20 px-1.5 py-0.2 text-[8px] font-bold uppercase text-amber-300">
                        trống
                      </span>
                    )}
                  </div>
                  <div className="mt-0.5 flex flex-wrap items-center gap-x-2 text-[10px] text-zinc-400">
                    <span
                      className={twMerge(
                        "font-mono",
                        isEmpty ? "text-amber-300/80" : "text-zinc-300",
                      )}
                    >
                      {durLabel || "—"}
                    </span>
                    {mediaHint ? (
                      <span className="text-zinc-500">{mediaHint}</span>
                    ) : null}
                    {p.root && (
                      <span className="text-zinc-500" title={path}>
                        {p.root === "mate-output" ? "mate" : p.root}
                      </span>
                    )}
                  </div>
                </div>
              </button>
            );
          })}
      </div>

      {/* Footer */}
      <div className="border-t border-white/10 bg-[#10141e] px-3.5 py-2.5">
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2 text-xs">
            <span className="text-zinc-400 font-medium">Đã chọn:</span>
            <span className="rounded-md bg-white/10 px-1.5 py-0.5 text-[10px] font-bold text-zinc-200">
              {selectedIds.size || (activePath ? 1 : 0)}
            </span>
          </div>

          <div className="flex items-center gap-2">
            <span className="text-xs text-zinc-400 font-medium">Sao lưu</span>
            <button
              type="button"
              role="switch"
              aria-checked={backup}
              onClick={() => {
                setBackup((v) => !v);
                toast(
                  backup
                    ? "Đã tắt flag sao lưu"
                    : "Đã bật flag sao lưu (backup .bak trước Apply)",
                );
              }}
              className={twMerge(
                "relative h-5 w-9 rounded-full transition-colors",
                backup ? "bg-indigo-500" : "bg-white/20",
              )}
            >
              <span
                className={twMerge(
                  "absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white shadow transition-transform",
                  backup && "translate-x-4",
                )}
              />
            </button>
            <button
              type="button"
              className="flex h-7 w-7 items-center justify-center rounded-lg border border-white/10 text-zinc-400 hover:bg-white/5 hover:text-white transition"
              title="Dùng lựa chọn làm draft"
              onClick={useSelectedAsLocal}
            >
              <ExternalLink className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </div>
    </aside>
  );
}

function ResizeHandle({
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onDoubleClick,
  onClick,
  dragging,
  collapsed,
}: {
  onPointerDown: (e: React.PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (e: React.PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (e: React.PointerEvent<HTMLDivElement>) => void;
  onDoubleClick: () => void;
  onClick?: () => void;
  dragging?: boolean;
  collapsed?: boolean;
}) {
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Đổi kích thước bảng dự án"
      title="Kéo để đổi kích thước · Nhấp đúp để thu gọn"
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      onDoubleClick={onDoubleClick}
      onClick={onClick}
      className={twMerge(
        "absolute top-0 bottom-0 left-0 z-20 w-1.5 -translate-x-1/2 cursor-col-resize touch-none",
        "hover:bg-indigo-400/50",
        dragging && "bg-indigo-400/70",
        collapsed && "w-2 translate-x-0 left-0",
      )}
    >
      <div className="absolute inset-y-0 -left-1.5 -right-1.5" />
    </div>
  );
}

function IconBtn({
  icon: Icon,
  title,
  onClick,
  spin,
}: {
  icon: React.ElementType;
  title: string;
  onClick?: () => void;
  spin?: boolean;
}) {
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      className="flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border border-white/10 text-zinc-400 hover:bg-white/5 hover:text-white transition"
    >
      <Icon className={`h-3.5 w-3.5 ${spin ? "animate-spin" : ""}`} />
    </button>
  );
}

function ActionBtn({
  label,
  onClick,
}: {
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="w-full rounded-lg border border-white/10 bg-white/[0.04] px-3 py-2 text-xs font-semibold text-zinc-300 hover:bg-white/[0.08] hover:text-white transition"
    >
      {label}
    </button>
  );
}
