import React from 'react';
import { Settings2, Save, Play, Square, Layers, Plus, Edit2 } from 'lucide-react';
import { ContentPage } from '../api/flowordClient';

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
}) => {
  const activePage = pages.find((p) => p.id === activePageId);

  return (
    <header className="flex min-h-16 shrink-0 flex-wrap items-center justify-between gap-3 border-b border-white/[0.08] bg-[#0b0e14]/95 px-4 py-3 md:px-6">
      <div className="flex flex-wrap items-center gap-3">
        <div className="mr-1 text-sm font-semibold tracking-tight text-white flex items-center gap-2">
          <span>Floword Studio</span>
        </div>

        {/* Minimal Page Switcher */}
        <div className="flex items-center gap-1.5 rounded-lg border border-indigo-500/30 bg-indigo-950/30 px-2.5 py-1 text-xs">
          <Layers className="h-3.5 w-3.5 text-indigo-400" />
          <span className="text-zinc-400 font-medium">Page:</span>
          {pages.length > 0 ? (
            <div className="flex items-center gap-1">
              <select
                value={activePageId || ''}
                onChange={(e) => onSelectPage(e.target.value)}
                className="bg-transparent font-medium text-indigo-200 outline-none cursor-pointer text-xs"
              >
                {!activePageId && <option value="" disabled className="bg-[#12161f] text-zinc-400">Select a Page...</option>}
                {pages.map((page) => (
                  <option key={page.id} value={page.id} className="bg-[#12161f] text-white">
                    {page.name}
                  </option>
                ))}
              </select>
              {activePage && (
                <button
                  type="button"
                  onClick={() => onOpenEditPage(activePage)}
                  title="Edit Page Settings"
                  className="rounded p-0.5 text-zinc-400 hover:bg-white/[0.05] hover:text-white transition"
                >
                  <Edit2 className="h-3 w-3" />
                </button>
              )}
            </div>
          ) : (
            <span className="text-amber-400 font-medium text-xs">No pages created</span>
          )}

          <button
            type="button"
            onClick={onOpenCreatePage}
            className="ml-1 flex items-center gap-0.5 rounded bg-indigo-600/60 px-1.5 py-0.5 text-[11px] font-medium text-white hover:bg-indigo-600 transition"
          >
            <Plus className="h-3 w-3" /> New Page
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-2 text-xs text-zinc-400">
          {[
            ['CapCut', status.mateOnline],
            ['OmniRoute', status.omniOnline],
            ['Pipeline', status.rustPipelineOnline],
          ].map(([label, online]) => (
            <span key={String(label)} className="inline-flex items-center gap-1.5 rounded-full bg-white/[0.05] px-2.5 py-1 font-medium">
              <span className={`h-1.5 w-1.5 rounded-full ${online ? 'bg-green-500' : 'bg-zinc-600'}`} />
              {label}
            </span>
          ))}
          {activeDraftUrl && <span className="hidden max-w-48 truncate text-zinc-500 lg:inline">{activeDraftUrl}</span>}
        </div>
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={onConfigure}
          className="floword-button floword-button-secondary text-zinc-200"
        >
          <Settings2 className="h-3.5 w-3.5" /> Configure
        </button>

        <button
          onClick={onSaveWorkflow}
          className="floword-button floword-button-secondary text-zinc-200"
        >
          <Save className="h-3.5 w-3.5" /> Save
        </button>

        {running ? (
          <button
            onClick={onRunWorkflow}
            className="floword-button bg-red-500 text-white hover:bg-red-600"
          >
            <Square className="h-3.5 w-3.5 fill-white" /> Stop
          </button>
        ) : (
          <button
            onClick={onRunWorkflow}
            disabled={!activePageId}
            title={!activePageId ? 'Select a Page before running the workflow.' : undefined}
            className={`floword-button ${!activePageId ? 'bg-zinc-700 text-zinc-400 cursor-not-allowed opacity-60' : 'floword-button-primary'}`}
          >
            <Play className="h-3.5 w-3.5 fill-white" /> Run
          </button>
        )}
      </div>
    </header>
  );
};
