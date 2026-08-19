import React from 'react';
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
} from 'lucide-react';
import { goToApp } from '~/config/appMenu';

export type FlowordView =
  | 'dashboard'
  | 'studio'
  | 'bulk_import'
  | 'jobs'
  | 'pages'
  | 'publish'
  | 'history'
  | 'settings';

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
    id: 'dashboard' as const,
    label: 'Dashboard',
    icon: LayoutDashboard,
    badge: null,
  },
  {
    id: 'studio' as const,
    label: 'Studio',
    icon: PlaySquare,
    badge: null,
  },
  {
    id: 'bulk_import' as const,
    label: 'Bulk Import',
    icon: FileSpreadsheet,
    badge: null,
  },
  {
    id: 'jobs' as const,
    label: 'Jobs Table',
    icon: ListChecks,
    badge: 'activeJobsCount',
  },
  {
    id: 'pages' as const,
    label: 'Pages',
    icon: Radio,
    badge: null,
  },
  {
    id: 'publish' as const,
    label: 'Publish Queue',
    icon: Send,
    badge: 'pendingPublishCount',
  },
  {
    id: 'history' as const,
    label: 'History Logs',
    icon: History,
    badge: null,
  },
  {
    id: 'settings' as const,
    label: 'Settings & Ops',
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
        className="fixed left-3 top-3 z-40 rounded-[9px] border border-white/10 bg-[#161b22] p-2 text-zinc-200 md:hidden"
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
          collapsed ? 'w-[72px]' : 'w-[230px]'
        } ${mobileOpen ? 'translate-x-0' : '-translate-x-full md:translate-x-0'} transition-all duration-200`}
      >
        {/* App Title / Logo */}
        <div className="flex h-16 items-center gap-3 border-b border-white/[0.08] px-4">
          <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-[#e54d5e] to-[#a855f7] text-white shadow-lg shadow-rose-500/20">
            <Sparkles className="h-5 w-5" />
          </div>
          {!collapsed && (
            <div className="min-w-0">
              <div className="truncate text-sm font-bold text-white tracking-tight">Floword Studio</div>
              <div className="text-[10px] text-zinc-500 font-medium">Production Console</div>
            </div>
          )}
          <button
            type="button"
            onClick={onCloseMobile}
            className="ml-auto p-1 text-zinc-400 md:hidden"
            aria-label="Close navigation"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Core 6 Navigation Items */}
        <nav className="flex-1 space-y-1.5 overflow-y-auto p-3" aria-label="Floword Studio">
          {!collapsed && (
            <div className="mb-2 px-3 text-[10px] font-bold uppercase tracking-[0.16em] text-zinc-500">
              Console
            </div>
          )}

          {navItems.map((item) => {
            const Icon = item.icon;
            const active = activeView === item.id;
            let badgeValue: number | null = null;
            if (item.badge === 'activeJobsCount' && activeJobsCount > 0) badgeValue = activeJobsCount;
            if (item.badge === 'pendingPublishCount' && pendingPublishCount > 0) badgeValue = pendingPublishCount;

            return (
              <button
                key={item.id}
                type="button"
                title={collapsed ? item.label : undefined}
                aria-current={active ? 'page' : undefined}
                onClick={() => {
                  onChange(item.id);
                  onCloseMobile();
                }}
                className={`flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-xs font-semibold transition ${
                  active
                    ? 'bg-rose-500/10 text-white border border-rose-500/30 shadow-md shadow-rose-500/5'
                    : 'text-zinc-400 hover:bg-white/[0.04] hover:text-zinc-100'
                }`}
              >
                <Icon className={`h-4 w-4 shrink-0 ${active ? 'text-rose-400' : 'text-zinc-400'}`} />
                {!collapsed && (
                  <>
                    <span className="truncate flex-1 text-left">{item.label}</span>
                    {badgeValue !== null && (
                      <span className={`px-2 py-0.5 rounded-full text-[10px] font-bold ${
                        item.id === 'publish' ? 'bg-amber-500/20 text-amber-300' : 'bg-blue-500/20 text-blue-300'
                      }`}>
                        {badgeValue}
                      </span>
                    )}
                  </>
                )}
              </button>
            );
          })}

          {/* AI Tools & Integrations Section */}
          <div className="pt-4 mt-4 border-t border-white/[0.08]">
            {!collapsed && (
              <div className="mb-2 px-3 text-[10px] font-bold uppercase tracking-[0.16em] text-zinc-500">
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
              className="flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-xs font-semibold text-indigo-300 hover:bg-indigo-500/10 hover:text-indigo-200 border border-indigo-500/20 transition group"
            >
              <Globe className="h-4 w-4 shrink-0 text-indigo-400 group-hover:scale-110 transition-transform" />
              {!collapsed && (
                <>
                  <span className="truncate flex-1 text-left font-medium">AI Keys (OmniRoute)</span>
                  <span className="px-1.5 py-0.5 rounded bg-indigo-500/20 text-[9px] font-bold text-indigo-300">
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
              className="mt-1.5 flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-xs font-semibold text-cyan-300 hover:bg-cyan-500/10 hover:text-cyan-200 border border-cyan-500/20 transition group"
            >
              <Clapperboard className="h-4 w-4 shrink-0 text-cyan-400 group-hover:scale-110 transition-transform" />
              {!collapsed && (
                <span className="truncate flex-1 text-left font-medium">CapCut Studio</span>
              )}
            </button>
          </div>
        </nav>

        {/* Sidebar Collapse Toggle */}
        <div className="border-t border-white/[0.08] p-3">
          <button
            type="button"
            onClick={onToggleCollapse}
            className="hidden w-full items-center justify-center rounded-xl p-2 text-zinc-500 hover:bg-white/[0.04] hover:text-zinc-200 transition md:flex"
            aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
          >
            {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronLeft className="h-4 w-4" />}
          </button>
        </div>
      </aside>
    </>
  );
};
