import React, { useState } from 'react';
import {
  Calendar,
  Check,
  CheckCircle2,
  Clock,
  Edit2,
  ExternalLink,
  Film,
  Play,
  Send,
  Share2,
  Trash2,
  UploadCloud,
  X,
} from 'lucide-react';
import { ContentPage } from '../../api/flowordClient';
import { WorkflowRun } from '../../services/workflowEngine';

interface PublishViewProps {
  runs: WorkflowRun[];
  pages: ContentPage[];
  onApprovePublish: (runId: string) => Promise<void>;
  onRejectPublish: (runId: string) => Promise<void>;
}

export const PublishView: React.FC<PublishViewProps> = ({
  runs,
  pages,
  onApprovePublish,
  onRejectPublish,
}) => {
  const [filterPage, setFilterPage] = useState<string>('all');

  const pendingPublish = runs.filter((r) => {
    if (r.status !== 'completed') return false;
    if (r.isPublished) return false;
    if (filterPage !== 'all' && r.pageId !== filterPage) return false;
    return true;
  });

  const getPageName = (pageId?: string) => {
    if (!pageId) return 'General';
    const found = pages.find((p) => p.id === pageId);
    return found ? found.name : pageId;
  };

  return (
    <div className="max-w-7xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-xl font-bold text-white tracking-tight">Review & Publish Inbox</h1>
            <span className="px-2.5 py-0.5 rounded-full text-xs font-bold bg-amber-500/10 text-amber-300 border border-amber-500/20">
              {pendingPublish.length} Ready for Approval
            </span>
          </div>
          <p className="text-xs text-zinc-400 mt-0.5">
            Review completed videos and approve for multi-platform posting (Review Before Post).
          </p>
        </div>

        <div className="flex items-center gap-2 text-xs">
          <span className="text-zinc-500">Filter by Page:</span>
          <select
            value={filterPage}
            onChange={(e) => setFilterPage(e.target.value)}
            className="px-3 py-1.5 rounded-xl bg-[#171b26] border border-white/[0.08] text-zinc-300 focus:outline-none"
          >
            <option value="all">All Pages</option>
            {pages.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      {/* Review Grid */}
      {pendingPublish.length === 0 ? (
        <div className="p-16 rounded-2xl bg-[#121622] border border-white/[0.08] text-center space-y-3">
          <CheckCircle2 className="h-10 w-10 text-emerald-400 mx-auto" />
          <h3 className="text-base font-bold text-white">Review Inbox trống!</h3>
          <p className="text-xs text-zinc-400 max-w-md mx-auto">
            Tất cả các video đã hoàn thành đều đã được duyệt và lên lịch đăng hoặc chưa có Job mới nào hoàn tất.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {pendingPublish.map((run) => (
            <div
              key={run.id}
              className="rounded-2xl bg-[#121622] border border-white/[0.08] overflow-hidden flex flex-col justify-between shadow-xl"
            >
              {/* Media Preview Thumbnail */}
              <div className="relative aspect-[9/16] max-h-56 bg-black/60 flex items-center justify-center border-b border-white/[0.06] overflow-hidden group">
                <Film className="h-12 w-12 text-zinc-600 group-hover:scale-110 transition duration-300" />
                <div className="absolute inset-0 bg-gradient-to-t from-black/80 via-transparent to-transparent flex items-end p-4">
                  <div className="w-full flex items-center justify-between text-white">
                    <span className="font-mono text-xs font-bold">{run.id.slice(0, 8)}</span>
                    <span className="px-2 py-0.5 rounded text-[10px] font-semibold bg-white/20 backdrop-blur">
                      {run.input?.targetDurationSeconds ? `${run.input.targetDurationSeconds}s` : '30s'} • {run.input?.aspectRatio || '9:16'}
                    </span>
                  </div>
                </div>
              </div>

              {/* Details & Caption */}
              <div className="p-4 space-y-3 text-xs flex-1">
                <div className="flex items-center justify-between">
                  <span className="font-bold text-white text-sm">{getPageName(run.pageId)}</span>
                  <span className="text-[11px] text-amber-400 flex items-center gap-1 font-medium">
                    <Clock className="h-3 w-3" /> Awaiting Approval
                  </span>
                </div>

                <p className="text-zinc-300 line-clamp-3 bg-white/[0.02] p-2.5 rounded-xl border border-white/[0.04]">
                  {run.input?.topic || run.input?.prompt || run.input?.customPrompt || 'Generated Video Post'}
                </p>

                {/* Target Channel */}
                <div className="flex items-center gap-1.5 pt-1">
                  <span className="px-2.5 py-0.5 rounded text-[10px] font-bold bg-rose-500/10 text-rose-300 border border-rose-500/20 uppercase">
                    Target: {run.input?.targetPlatform || 'TikTok'}
                  </span>
                  {run.finalDraftUrl ? (
                    <span className="px-2 py-0.5 rounded text-[10px] font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                      Draft Ready
                    </span>
                  ) : null}
                </div>
              </div>

              {/* Action Buttons */}
              <div className="p-4 border-t border-white/[0.06] bg-[#151926] flex items-center justify-between gap-2">
                <button
                  type="button"
                  onClick={() => onRejectPublish(run.id)}
                  className="px-3 py-2 rounded-xl bg-white/[0.04] hover:bg-white/[0.08] text-zinc-400 hover:text-white text-xs font-medium transition"
                >
                  Reject / Rerun
                </button>
                <button
                  type="button"
                  onClick={() => onApprovePublish(run.id)}
                  className="flex-1 flex items-center justify-center gap-1.5 px-4 py-2 rounded-xl bg-gradient-to-r from-emerald-600 to-teal-600 hover:from-emerald-500 hover:to-teal-500 text-white text-xs font-semibold shadow-lg shadow-emerald-500/20 transition transform active:scale-95"
                >
                  <Check className="h-4 w-4" />
                  Approve & Publish
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
