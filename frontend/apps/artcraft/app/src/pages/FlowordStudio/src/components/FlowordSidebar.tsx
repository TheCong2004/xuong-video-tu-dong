import React from "react";
import {
  Activity,
  Boxes,
  ChevronLeft,
  ChevronRight,
  Globe,
  HardDrive,
  KeyRound,
  LayoutDashboard,
  Layers,
  ListChecks,
  Menu,
  PlaySquare,
  Radio,
  Send,
  Settings,
  Sparkles,
  Workflow,
  X,
  Clapperboard,
  FileSpreadsheet,
  History,
} from "lucide-react";
import { goToApp } from "~/config/appMenu";

export type FlowordView =
  | "dashboard"
  | "production"
  | "studio"
  | "bulk_import"
  | "jobs"
  | "pages"
  | "publish"
  | "history"
  | "settings";

interface FlowordSidebarProps {
  activeView: FlowordView;
  collapsed: boolean;
  mobileOpen: boolean;
  pendingPublishCount?: number;
  activeJobsCount?: number;
  onChange: (view: FlowordView) => void;
  onToggleCollapse: () => void;
  onCloseMobile: () => void;
  onOpenMobile: () => void;
}

const navItems = [
  {
    id: "dashboard" as const,
    label: "Tổng Quan",
    icon: LayoutDashboard,
    badge: null,
  },
  {
    id: "production" as const,
    label: "Xưởng Phim AI",
    icon: Clapperboard,
    badge: null,
  },
  {
    id: "studio" as const,
    label: "Studio Sản Xuất",
    icon: PlaySquare,
    badge: null,
  },
  {
    id: "bulk_import" as const,
    label: "Nhập Hàng Loạt",
    icon: FileSpreadsheet,
    badge: null,
  },
  {
    id: "jobs" as const,
    label: "Danh Sách Công Việc",
    icon: ListChecks,
    badge: "activeJobsCount",
  },
  {
    id: "pages" as const,
    label: "Quản Lý Page / Kênh",
    icon: Radio,
    badge: null,
  },
  {
    id: "publish" as const,
    label: "Hàng Đợi Đăng Bài",
    icon: Send,
    badge: "pendingPublishCount",
  },
  {
    id: "history" as const,
    label: "Lịch Sử Hoạt Động",
    icon: History,
    badge: null,
  },
  {
    id: "settings" as const,
    label: "Cài Đặt & Cấu Hình",
    icon: Settings,
    badge: null,
  },
];

export const FlowordSidebar: React.FC<FlowordSidebarProps> = ({
  activeView,
  collapsed,
  mobileOpen,
  pendingPublishCount = 0,
  activeJobsCount = 0,
  onChange,
  onToggleCollapse,
  onCloseMobile,
  onOpenMobile,
}) => {
  return (
    <>
      {/* Mobile toggle button */}
      <button
        type="button"
        onClick={onOpenMobile}
        className="text-zinc-200 fixed left-3 top-3 z-40 rounded-[9px] border border-white/10 bg-[#161b22] p-2 md:hidden"
        aria-label="Open navigation"
      >
        <Menu className="h-5 w-5" />
      </button>

      {mobileOpen && (
        <button
          type="button"
          className="fixed inset-0 z-40 bg-black/60 md:hidden"
          onClick={onCloseMobile}
          aria-label="Close navigation overlay"
        />
      )}

      <aside
        className={`fixed inset-y-0 left-0 z-50 flex shrink-0 flex-col border-r border-white/[0.08] bg-[#10141e] md:static ${
          collapsed ? "w-[72px]" : "w-[230px]"
        } ${mobileOpen ? "translate-x-0" : "-translate-x-full md:translate-x-0"} transition-all duration-200`}
      >
        {/* App Title / Logo */}
        <div className="flex h-16 items-center gap-3 border-b border-white/[0.08] px-4">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-[#e54d5e] to-[#a855f7] text-white shadow-lg shadow-rose-500/20">
            <Sparkles className="h-5 w-5" />
          </div>
          {!collapsed && (
            <div className="min-w-0">
              <div className="truncate text-sm font-bold tracking-tight text-white">
                Floword Studio
              </div>
              <div className="text-zinc-500 text-[10px] font-medium">
                Production Console
              </div>
            </div>
          )}
          <button
            type="button"
            onClick={onCloseMobile}
            className="text-zinc-400 ml-auto p-1 md:hidden"
            aria-label="Close navigation"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Core 6 Navigation Items */}
        <nav
          className="flex-1 space-y-1.5 overflow-y-auto p-3"
          aria-label="Floword Studio"
        >
          {!collapsed && (
            <div className="text-zinc-500 mb-2 px-3 text-[10px] font-bold uppercase tracking-[0.16em]">
              Console
            </div>
          )}

          {navItems.map((item) => {
            const Icon = item.icon;
            const active = activeView === item.id;
            let badgeValue: number | null = null;
            if (item.badge === "activeJobsCount" && activeJobsCount > 0)
              badgeValue = activeJobsCount;
            if (item.badge === "pendingPublishCount" && pendingPublishCount > 0)
              badgeValue = pendingPublishCount;

            return (
              <button
                key={item.id}
                type="button"
                title={collapsed ? item.label : undefined}
                aria-current={active ? "page" : undefined}
                onClick={() => {
                  onChange(item.id);
                  onCloseMobile();
                }}
                className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-xs font-semibold transition ${
                  active
                    ? "border border-rose-500/30 bg-rose-500/10 text-white shadow-md shadow-rose-500/5"
                    : "text-zinc-400 hover:text-zinc-100 hover:bg-white/[0.04]"
                }`}
              >
                <Icon
                  className={`h-4 w-4 shrink-0 ${active ? "text-rose-400" : "text-zinc-400"}`}
                />
                {!collapsed && (
                  <>
                    <span className="flex-1 truncate text-left">
                      {item.label}
                    </span>
                    {badgeValue !== null && (
                      <span
                        className={`rounded-full px-2 py-0.5 text-[10px] font-bold ${
                          item.id === "publish"
                            ? "bg-amber-500/20 text-amber-300"
                            : "bg-blue-500/20 text-blue-300"
                        }`}
                      >
                        {badgeValue}
                      </span>
                    )}
                  </>
                )}
              </button>
            );
          })}

          {/* AI Tools & Integrations Section */}
          <div className="mt-4 border-t border-white/[0.08] pt-4">
            {!collapsed && (
              <div className="text-zinc-500 mb-2 px-3 text-[10px] font-bold uppercase tracking-[0.16em]">
                AI & Routing
              </div>
            )}

            {/* OmniRoute Button */}
            <button
              type="button"
              title="Quản lý API Keys & Router AI (OmniRoute)"
              onClick={() => {
                goToApp("OMNI_ROUTE");
                onCloseMobile();
              }}
              className="group flex w-full items-center gap-3 rounded-xl border border-indigo-500/20 px-3 py-2.5 text-xs font-semibold text-indigo-300 transition hover:bg-indigo-500/10 hover:text-indigo-200"
            >
              <Globe className="h-4 w-4 shrink-0 text-indigo-400 transition-transform group-hover:scale-110" />
              {!collapsed && (
                <>
                  <span className="flex-1 truncate text-left font-medium">
                    AI Keys (OmniRoute)
                  </span>
                  <span className="rounded bg-indigo-500/20 px-1.5 py-0.5 text-[9px] font-bold text-indigo-300">
                    290+
                  </span>
                </>
              )}
            </button>

            {/* CapCut Studio Button */}
            <button
              type="button"
              title="CapCut Timeline & Draft Automation"
              onClick={() => {
                goToApp("CAPCUT_AUTOMATION");
                onCloseMobile();
              }}
              className="group mt-1.5 flex w-full items-center gap-3 rounded-xl border border-cyan-500/20 px-3 py-2.5 text-xs font-semibold text-cyan-300 transition hover:bg-cyan-500/10 hover:text-cyan-200"
            >
              <Clapperboard className="h-4 w-4 shrink-0 text-cyan-400 transition-transform group-hover:scale-110" />
              {!collapsed && (
                <span className="flex-1 truncate text-left font-medium">
                  CapCut Studio
                </span>
              )}
            </button>
          </div>
        </nav>

        {/* Sidebar Collapse Toggle */}
        <div className="border-t border-white/[0.08] p-3">
          <button
            type="button"
            onClick={onToggleCollapse}
            className="text-zinc-500 hover:text-zinc-200 hidden w-full items-center justify-center rounded-xl p-2 transition hover:bg-white/[0.04] md:flex"
            aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          >
            {collapsed ? (
              <ChevronRight className="h-4 w-4" />
            ) : (
              <ChevronLeft className="h-4 w-4" />
            )}
          </button>
        </div>
      </aside>
    </>
  );
};
