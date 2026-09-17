import React from "react";
import {
  Boxes,
  FolderOpen,
  Video,
  RefreshCw,
  Subtitles,
  Wand2,
  Sliders,
  Smile,
  Layers,
  PlaySquare,
  Volume2,
  Image as ImageIcon,
  Diamond,
  Palette,
  Sparkles,
  Puzzle,
  Clapperboard,
  Globe,
} from "lucide-react";
import { goToApp } from "~/config/appMenu";
import type { SideNavId } from "../types";

interface CapCutSideNavProps {
  activeId: SideNavId;
  onSelect: (id: SideNavId) => void;
}

interface NavItemDef {
  id: SideNavId;
  label: string;
  icon: React.ElementType;
  badge?: string;
}

const NAV_ITEMS: NavItemDef[] = [
  { id: "local-draft", label: "Bản nháp nội bộ", icon: FolderOpen },
  { id: "materials", label: "Nguyên liệu", icon: Boxes },
  { id: "auto-render", label: "Xuất video", icon: Video },
  { id: "sync", label: "Đồng bộ", icon: RefreshCw },
  { id: "caption", label: "Phụ đề", icon: Subtitles },
  { id: "effects", label: "Hiệu ứng", icon: Wand2 },
  { id: "filters", label: "Bộ lọc", icon: Sliders },
  { id: "stickers", label: "Nhãn dán", icon: Smile },
  { id: "transitions", label: "Chuyển cảnh", icon: Layers },
  { id: "animations", label: "Hoạt ảnh", icon: PlaySquare },
  { id: "sounds", label: "Âm thanh", icon: Volume2 },
  { id: "media", label: "Tệp / Mặt nạ", icon: ImageIcon },
  { id: "keyframe", label: "Khung khóa", icon: Diamond },
  { id: "adjustment", label: "Chỉnh màu", icon: Palette, badge: "Soon" },
  { id: "ai-generate", label: "AI tạo", icon: Sparkles, badge: "Soon" },
  { id: "extension", label: "Tiện ích", icon: Puzzle, badge: "Soon" },
];

export function CapCutSideNav({ activeId, onSelect }: CapCutSideNavProps) {
  return (
    <aside className="flex h-full min-h-0 w-[230px] shrink-0 flex-col border-r border-white/10 bg-[#10141e]">
      {/* App Header / Logo */}
      <div className="flex h-16 shrink-0 items-center gap-3 border-b border-white/10 px-4">
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-[#e54d5e] to-[#a855f7] text-white shadow-lg shadow-rose-500/20">
          <Clapperboard className="h-5 w-5" />
        </div>
        <div className="min-w-0">
          <div className="truncate text-sm font-bold tracking-tight text-white">
            CapCut Studio
          </div>
          <div className="text-[10px] font-medium text-zinc-500">
            Xưởng Sản Xuất Video
          </div>
        </div>
      </div>

      {/* Navigation list */}
      <nav className="flex-1 space-y-1.5 overflow-y-auto p-3">
        <div className="mb-2 px-3 text-[10px] font-bold uppercase tracking-[0.16em] text-zinc-500">
          CÔNG CỤ BIÊN TẬP
        </div>

        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const active = activeId === item.id;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() => onSelect(item.id)}
              className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-xs font-semibold transition ${
                active
                  ? "border border-white/10 bg-rose-500/10 text-white shadow-md shadow-rose-500/5"
                  : "text-zinc-400 hover:bg-white/[0.04] hover:text-zinc-100"
              }`}
            >
              <Icon
                className={`h-4 w-4 shrink-0 ${active ? "text-rose-400" : "text-zinc-400"}`}
              />
              <span className="flex-1 truncate text-left">{item.label}</span>
              {item.badge && (
                <span className="rounded-full bg-white/10 px-1.5 py-0.5 text-[9px] font-semibold uppercase tracking-wide text-zinc-400">
                  {item.badge === "Soon" ? "Sắp có" : item.badge}
                </span>
              )}
            </button>
          );
        })}

        {/* AI Tools & Integrations Section */}
        <div className="mt-4 border-t border-white/10 pt-4">
          <div className="mb-2 px-3 text-[10px] font-bold uppercase tracking-[0.16em] text-zinc-500">
            AI & Routing
          </div>

          {/* OmniRoute Button */}
          <button
            type="button"
            title="Quản lý API Keys & Router AI (OmniRoute)"
            onClick={() => goToApp("OMNI_ROUTE")}
            className="group flex w-full items-center gap-3 rounded-xl border border-white/10 px-3 py-2.5 text-xs font-semibold text-indigo-300 transition hover:bg-indigo-500/10 hover:text-indigo-200"
          >
            <Globe className="h-4 w-4 shrink-0 text-indigo-400 transition-transform group-hover:scale-110" />
            <span className="flex-1 truncate text-left font-medium">
              AI Keys (OmniRoute)
            </span>
            <span className="rounded bg-indigo-500/20 px-1.5 py-0.5 text-[9px] font-bold text-indigo-300">
              290+
            </span>
          </button>

          {/* Floword Studio Button */}
          <button
            type="button"
            title="Quay lại Floword Production Studio"
            onClick={() => goToApp("FLOWORD_STUDIO")}
            className="group mt-1.5 flex w-full items-center gap-3 rounded-xl border border-white/10 px-3 py-2.5 text-xs font-semibold text-rose-300 transition hover:bg-rose-500/10 hover:text-rose-200"
          >
            <Sparkles className="h-4 w-4 shrink-0 text-rose-400 transition-transform group-hover:scale-110" />
            <span className="flex-1 truncate text-left font-medium">
              Floword Studio
            </span>
          </button>
        </div>
      </nav>

      {/* Footer info card */}
      <div className="mt-auto border-t border-white/10 p-3">
        <div className="rounded-xl border border-white/10 bg-white/[0.02] p-3">
          <div className="flex items-center justify-between">
            <span className="text-[11px] font-medium text-zinc-300">CapCut Engine</span>
            <span className="flex items-center gap-1.5 text-[10px] font-bold text-emerald-400">
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />
              :30000
            </span>
          </div>
          <p className="mt-1 text-[10px] text-zinc-500">
            Không gian biên tập & draft nội bộ
          </p>
        </div>
      </div>
    </aside>
  );
}
