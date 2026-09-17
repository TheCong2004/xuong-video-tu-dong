import React from "react";
import {
  Settings2,
  Save,
  Play,
  Square,
  Layers,
  Plus,
  Edit2,
} from "lucide-react";
import { ContentPage } from "../api/flowordClient";
import { goToApp } from "~/config/appMenu";

interface FlowordHeaderProps {
  status: {
    mateOnline: boolean;
    omniOnline: boolean;
    rustPipelineOnline: boolean;
  };
  activeDraftUrl: string;
  running: boolean;
  pages: ContentPage[];
  activePageId?: string | null;
  onSelectPage: (pageId: string) => void;
  onOpenCreatePage: () => void;
  onOpenEditPage: (page: ContentPage) => void;
  onRunWorkflow: () => void;
  onSaveWorkflow: () => void;
  onConfigure: () => void;
  activeView?: string;
  onChangeView?: (view: any) => void;
}

export const FlowordHeader: React.FC<FlowordHeaderProps> = ({
  status,
  activeDraftUrl,
  running,
  pages,
  activePageId,
  onSelectPage,
  onOpenCreatePage,
  onOpenEditPage,
  onRunWorkflow,
  onSaveWorkflow,
  onConfigure,
  activeView,
  onChangeView,
}) => {
  const activePage = pages.find((p) => p.id === activePageId);

  return (
    <header className="flex min-h-16 shrink-0 flex-wrap items-center justify-between gap-3 px-4 py-3 md:px-6">
      <div className="flex flex-wrap items-center gap-3">
        {/* Minimal Page Switcher */}
        <div className="flex items-center gap-1.5 rounded-lg border border-white/10 bg-transparent px-2.5 py-1.5 text-xs">
          <Layers className="h-3.5 w-3.5 text-indigo-400" />
          <span className="text-zinc-400 font-medium">Page</span>
          {pages.length > 0 ? (
            <div className="flex items-center gap-1">
              <select
                value={activePageId || ""}
                onChange={(e) => onSelectPage(e.target.value)}
                className="cursor-pointer bg-transparent text-xs font-medium text-zinc-300 outline-none"
              >
                {!activePageId && (
                  <option
                    value=""
                    disabled
                    className="text-zinc-400 bg-[#12161f]"
                  >
                    Select a Page...
                  </option>
                )}
                {pages.map((page) => (
                  <option
                    key={page.id}
                    value={page.id}
                    className="bg-[#12161f] text-white"
                  >
                    {page.name}
                  </option>
                ))}
              </select>
              {activePage && (
                <button
                  type="button"
                  onClick={() => onOpenEditPage(activePage)}
                  title="Edit Page Settings"
                  className="text-zinc-400 rounded p-0.5 transition hover:bg-white/[0.05] hover:text-white"
                >
                  <Edit2 className="h-3 w-3" />
                </button>
              )}
            </div>
          ) : (
            <span className="text-xs font-medium text-amber-400">
              No pages created
            </span>
          )}

          <span className="text-zinc-600 font-light px-0.5">/</span>

          <button
            type="button"
            onClick={onOpenCreatePage}
            className="flex items-center gap-1 rounded bg-indigo-500/20 px-2 py-0.5 text-[11px] font-semibold text-indigo-300 transition hover:bg-indigo-500/30"
          >
            <Plus className="h-3 w-3" /> Thêm Page
          </button>
        </div>

        <div className="text-zinc-400 flex flex-wrap items-center gap-2 text-xs">
          {activeDraftUrl && (
            <span className="text-zinc-500 hidden max-w-48 truncate lg:inline">
              {activeDraftUrl}
            </span>
          )}
        </div>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={onConfigure}
          className="floword-button floword-button-secondary text-zinc-200"
        >
          <Settings2 className="h-3.5 w-3.5" /> Cấu Hình
        </button>

        <button
          onClick={onSaveWorkflow}
          className="floword-button floword-button-secondary text-zinc-200"
        >
          <Save className="h-3.5 w-3.5" /> Lưu
        </button>
      </div>
    </header>
  );
};
