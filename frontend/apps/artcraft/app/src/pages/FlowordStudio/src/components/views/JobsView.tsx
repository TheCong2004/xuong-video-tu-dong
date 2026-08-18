import React, { useState } from 'react';
import {
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  Clock,
  ExternalLink,
  Filter,
  Layers,
  MoreVertical,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Share2,
  Trash2,
  Video,
  X,
} from 'lucide-react';
import { ContentPage } from '../../api/flowordClient';
import { WorkflowRun, ArtifactRef } from '../../services/workflowEngine';

interface JobsViewProps {
  runs: WorkflowRun[];
  pages: ContentPage[];
  selectedJobId?: string;
  onSelectJob: (jobId?: string) => void;
  onNewJob: () => void;
  onRetryStep: (runId: string, stepId: string) => void;
  onCancelJob: (runId: string) => void;
}

export const JobsView: React.FC<JobsViewProps> = ({
  runs,
  pages,
  selectedJobId,
  onSelectJob,
  onNewJob,
  onRetryStep,
  onCancelJob,
}) => {
  const [filterPage, setFilterPage] = useState<string>('all');
  const [filterStatus, setFilterStatus] = useState<string>('all');
  const [searchQuery, setSearchQuery] = useState<string>('');

  const filteredRuns = runs.filter((r) => {
    if (filterPage !== 'all' && r.pageId !== filterPage) return false;
    if (filterStatus !== 'all' && r.status !== filterStatus) return false;
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      const matchId = r.id.toLowerCase().includes(q);
      const matchPage = pages.find((p) => p.id === r.pageId)?.name.toLowerCase().includes(q);
      if (!matchId && !matchPage) return false;
    }
    return true;
  });

  const selectedRun = runs.find((r) => r.id === selectedJobId);

  const getPageName = (pageId?: string) => {
    if (!pageId) return 'General';
    const found = pages.find((p) => p.id === pageId);
    return found ? found.name : pageId;
  };

  const STAGES = [
    { key: 'image', label: 'IMAGE' },
    { key: 'aspect', label: '9:16' },
    { key: 'video', label: 'VIDEO' },
    { key: 'download', label: 'DOWNLOAD' },
    { key: 'publish', label: 'POST' },
  ];

  return (
    <div className="flex h-full max-w-7xl mx-auto p-6 gap-6 overflow-hidden">
      {/* Main Jobs Table / List */}
      <div className={`flex-1 flex flex-col space-y-4 overflow-y-auto ${selectedRun ? 'hidden lg:flex' : 'flex'}`}>
        {/* Header & Controls */}
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
          <div>
            <h1 className="text-xl font-bold text-white tracking-tight">Production Jobs</h1>
            <p className="text-xs text-zinc-400">Track and manage multi-stage video pipelines.</p>
          </div>

          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onNewJob}
              className="flex items-center gap-2 px-4 py-2 rounded-xl bg-gradient-to-r from-[#e54d5e] to-[#c23b4c] hover:from-[#f05c6d] hover:to-[#d04657] text-white text-xs font-semibold shadow-lg shadow-rose-500/20 transition"
            >
              <Plus className="h-4 w-4" />
              New Job
            </button>
          </div>
        </div>

        {/* Filters Bar */}
        <div className="flex flex-wrap items-center gap-3 p-3 rounded-2xl bg-[#121622] border border-white/[0.08]">
          <div className="relative flex-1 min-w-[200px]">
            <Search className="absolute left-3 top-2.5 h-4 w-4 text-zinc-500" />
            <input
              type="text"
              placeholder="Search by Job ID or Page..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="w-full pl-9 pr-3 py-1.5 rounded-xl bg-white/[0.04] border border-white/[0.08] text-xs text-white placeholder-zinc-500 focus:outline-none focus:border-rose-500/50"
            />
          </div>

          <div className="flex items-center gap-2">
            <span className="text-xs text-zinc-500">Page:</span>
            <select
              value={filterPage}
              onChange={(e) => setFilterPage(e.target.value)}
              className="px-3 py-1.5 rounded-xl bg-[#171b26] border border-white/[0.08] text-xs text-zinc-300 focus:outline-none"
            >
              <option value="all">All Pages</option>
              {pages.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>

          <div className="flex items-center gap-2">
            <span className="text-xs text-zinc-500">Status:</span>
            <select
              value={filterStatus}
              onChange={(e) => setFilterStatus(e.target.value)}
              className="px-3 py-1.5 rounded-xl bg-[#171b26] border border-white/[0.08] text-xs text-zinc-300 focus:outline-none"
            >
              <option value="all">All Statuses</option>
              <option value="running">Running</option>
              <option value="completed">Completed</option>
              <option value="failed">Failed</option>
              <option value="pending">Pending</option>
            </select>
          </div>
        </div>

        {/* Jobs List */}
        <div className="space-y-3">
          {filteredRuns.length === 0 ? (
            <div className="p-12 rounded-2xl bg-[#121622] border border-white/[0.08] text-center">
              <Layers className="h-8 w-8 text-zinc-600 mx-auto mb-2" />
              <p className="text-sm font-medium text-zinc-400">Không tìm thấy Job nào phù hợp.</p>
              <p className="text-xs text-zinc-600 mt-1">Hãy thử thay đổi bộ lọc hoặc tạo Job mới.</p>
            </div>
          ) : (
            filteredRuns.map((run) => {
              const isSelected = run.id === selectedJobId;
              const pageName = getPageName(run.pageId);

              return (
                <div
                  key={run.id}
                  onClick={() => onSelectJob(run.id)}
                  className={`p-4 rounded-2xl border transition cursor-pointer ${
                    isSelected
                      ? 'bg-rose-500/[0.06] border-rose-500/40 shadow-lg shadow-rose-500/5'
                      : 'bg-[#121622] border-white/[0.08] hover:border-white/[0.16]'
                  }`}
                >
                  <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                    <div className="flex items-center gap-3">
                      <div
                        className={`h-3 w-3 rounded-full shrink-0 ${
                          run.status === 'running'
                            ? 'bg-blue-400 animate-pulse'
                            : run.status === 'completed'
                            ? 'bg-emerald-400'
                            : run.status === 'failed'
                            ? 'bg-rose-400'
                            : 'bg-zinc-500'
                        }`}
                      />
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="font-mono text-xs font-bold text-white">
                            {run.id}
                          </span>
                          <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-white/[0.06] text-zinc-300">
                            {pageName}
                          </span>
                        </div>
                        <div className="text-[11px] text-zinc-500 mt-0.5">
                          {run.status === 'running'
                            ? `Generating ${run.currentStage || 'video'} • In progress`
                            : run.status === 'failed'
                            ? `Error at stage ${run.currentStage || 'processing'}`
                            : 'Ready / Complete'}
                        </div>
                      </div>
                    </div>

                    {/* Mini Progress Rail */}
                    <div className="flex items-center gap-1 sm:gap-2">
                      {STAGES.map((s, idx) => {
                        const stageOrder = ['image', 'aspect', 'video', 'download', 'publish'];
                        const currentStage = (run.currentStage || '').toLowerCase();
                        let currentIdx = 0;
                        if (currentStage.includes('aspect') || currentStage.includes('9:16') || currentStage.includes('timeline') || currentStage.includes('converting_9_16')) {
                          currentIdx = 1;
                        } else if (currentStage.includes('video') || currentStage.includes('draft') || currentStage.includes('render') || currentStage.includes('generating_video')) {
                          currentIdx = 2;
                        } else if (currentStage.includes('download') || currentStage.includes('completed')) {
                          currentIdx = 3;
                        } else if (currentStage.includes('publish') || currentStage.includes('post')) {
                          currentIdx = 4;
                        }

                        const isDone = run.status === 'completed' || (run.status !== 'cancelled' && idx < currentIdx);
                        const isCurrent = run.status === 'running' && idx === currentIdx;
                        const isFailed = run.status === 'failed' && idx === currentIdx;

                        return (
                          <div
                            key={s.key}
                            className={`px-2 py-1 rounded-md text-[10px] font-mono font-bold flex items-center gap-1 ${
                              isDone
                                ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                                : isFailed
                                ? 'bg-rose-500/20 text-rose-300 border border-rose-500/40'
                                : isCurrent
                                ? 'bg-blue-500/20 text-blue-300 border border-blue-500/40 animate-pulse'
                                : 'bg-white/[0.02] text-zinc-600 border border-white/[0.04]'
                            }`}
                          >
                            <span>{s.label}</span>
                            <span>{isDone ? '✓' : isFailed ? '✕' : isCurrent ? '●' : '○'}</span>
                          </div>
                        );
                      })}
                    </div>
                  </div>

                  {run.status === 'failed' && (
                    <div className="mt-3 pt-3 border-t border-rose-500/20 flex items-center justify-between">
                      <span className="text-[11px] text-rose-400 font-medium">
                        Quá trình tạo gặp lỗi • Nhấn để xem chi tiết
                      </span>
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onRetryStep(run.id, run.currentStage || 'video');
                        }}
                        className="px-3 py-1 rounded-lg bg-rose-600 hover:bg-rose-500 text-white text-xs font-medium flex items-center gap-1.5 transition"
                      >
                        <RotateCcw className="h-3 w-3" />
                        Retry Stage
                      </button>
                    </div>
                  )}
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Job Inspector Panel */}
      {selectedRun ? (
        <div className="w-full lg:w-[420px] shrink-0 rounded-2xl bg-[#121622] border border-white/[0.1] flex flex-col h-full overflow-hidden shadow-2xl">
          {/* Inspector Header */}
          <div className="p-4 border-b border-white/[0.08] flex items-center justify-between bg-[#151926]">
            <div>
              <div className="text-[11px] font-semibold uppercase tracking-wider text-rose-400">
                Job Inspector
              </div>
              <div className="text-sm font-bold text-white font-mono">{selectedRun.id}</div>
            </div>
            <button
              onClick={() => onSelectJob(undefined)}
              className="p-1 rounded-lg hover:bg-white/[0.06] text-zinc-400 hover:text-white"
            >
              <X className="h-4 w-4" />
            </button>
          </div>

          {/* Inspector Body */}
          <div className="flex-1 overflow-y-auto p-4 space-y-5 text-xs">
            {/* Page & Status Summary */}
            <div className="p-3 rounded-xl bg-white/[0.02] border border-white/[0.06] space-y-2">
              <div className="flex justify-between">
                <span className="text-zinc-400">Target Page:</span>
                <span className="font-semibold text-white">{getPageName(selectedRun.pageId)}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-zinc-400">Status:</span>
                <span className="capitalize font-bold text-white">{selectedRun.status}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-zinc-400">Progress:</span>
                <span className="font-mono text-zinc-300">{selectedRun.progressPercent || 0}%</span>
              </div>
            </div>

            {/* Lineage & Artifacts */}
            <div>
              <div className="text-zinc-400 font-semibold text-[11px] uppercase tracking-wider mb-2">
                Pipeline Lineage
              </div>
              <div className="space-y-2.5">
                <div className="p-3 rounded-xl bg-black/20 border border-white/[0.04]">
                  <div className="text-[10px] text-zinc-500 uppercase font-bold">1. Input Image / Prompt</div>
                  <p className="text-zinc-300 mt-1 line-clamp-2">
                    {selectedRun.input?.topic || selectedRun.input?.customPrompt || 'Default preset prompt'}
                  </p>
                </div>

                <div className="p-3 rounded-xl bg-black/20 border border-white/[0.04]">
                  <div className="text-[10px] text-zinc-500 uppercase font-bold">2. Video Aspect Ratio</div>
                  <p className="text-zinc-300 mt-1 font-mono">9:16 (Short-form portrait)</p>
                </div>

                <div className="p-3 rounded-xl bg-black/20 border border-white/[0.04]">
                  <div className="text-[10px] text-zinc-500 uppercase font-bold">3. Generated Artifacts</div>
                  {selectedRun.artifacts && selectedRun.artifacts.length > 0 ? (
                    <div className="mt-2 space-y-1.5">
                      {selectedRun.artifacts.map((art) => (
                        <div key={art.id} className="flex items-center justify-between text-[11px] text-zinc-300 bg-white/[0.02] p-2 rounded-lg">
                          <span className="truncate">{art.name}</span>
                          <span className="font-mono text-[10px] text-zinc-500">{art.type}</span>
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div className="text-zinc-600 italic text-[11px] mt-1">Pending generation output</div>
                  )}
                </div>
              </div>
            </div>

            {/* Publishing Channels */}
            <div>
              <div className="text-zinc-400 font-semibold text-[11px] uppercase tracking-wider mb-2">
                Publishing Target
              </div>
              <div className="p-3 rounded-xl bg-white/[0.02] border border-white/[0.05] space-y-2">
                <div className="flex items-center justify-between text-xs">
                  <span className="text-zinc-300">Target Channel:</span>
                  <span className="font-semibold text-white uppercase">{selectedRun.input?.targetPlatform || 'TikTok'}</span>
                </div>
                <div className="flex items-center justify-between text-xs">
                  <span className="text-zinc-300">Publishing Mode:</span>
                  <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-amber-500/10 text-amber-400 border border-amber-500/20">
                    Review Before Post
                  </span>
                </div>
                <div className="text-[10px] text-zinc-500 pt-1.5 border-t border-white/[0.04]">
                  Automated posting is triggered via dedicated browser profile upon manual approval in Publish View.
                </div>
              </div>
            </div>

            {/* Audit History Timeline */}
            <div>
              <div className="text-zinc-400 font-semibold text-[11px] uppercase tracking-wider mb-2">
                Execution History
              </div>
              <div className="space-y-2 border-l border-white/[0.08] pl-3 ml-2">
                <div className="relative">
                  <span className="absolute -left-[17px] top-1 h-2 w-2 rounded-full bg-blue-400" />
                  <div className="text-[11px] text-zinc-300">Job Enqueued</div>
                  <div className="text-[10px] text-zinc-500">Worker Pool assigned</div>
                </div>
                {selectedRun.steps?.map((step) => (
                  <div key={step.id} className="relative">
                    <span className={`absolute -left-[17px] top-1 h-2 w-2 rounded-full ${
                      step.status === 'succeeded' ? 'bg-emerald-400' : step.status === 'failed' ? 'bg-rose-400' : 'bg-zinc-500'
                    }`} />
                    <div className="text-[11px] text-zinc-300 capitalize">{step.id} — {step.status}</div>
                    <div className="text-[10px] text-zinc-500">Duration: {step.durationMs ? `${Math.round(step.durationMs / 1000)}s` : 'active'}</div>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Inspector Footer Actions */}
          <div className="p-4 border-t border-white/[0.08] bg-[#151926] flex items-center justify-between gap-3">
            <button
              type="button"
              onClick={() => onCancelJob(selectedRun.id)}
              className="px-3 py-2 rounded-xl bg-white/[0.04] hover:bg-white/[0.08] text-rose-400 text-xs font-medium border border-white/[0.08] transition"
            >
              Cancel Job
            </button>
            <button
              type="button"
              onClick={() => onRetryStep(selectedRun.id, selectedRun.currentStage || 'video')}
              className="px-4 py-2 rounded-xl bg-rose-600 hover:bg-rose-500 text-white text-xs font-semibold shadow transition"
            >
              Retry Full Job
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
};
