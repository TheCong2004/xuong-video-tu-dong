import React from 'react';
import {
  Activity,
  AlertCircle,
  ArrowUpRight,
  CheckCircle2,
  Clock,
  ExternalLink,
  Layers,
  Play,
  Plus,
  RefreshCw,
  Sparkles,
  TrendingUp,
  Video,
  Zap,
} from 'lucide-react';
import { DetailedReadinessStatus, ContentPage } from '../../api/flowordClient';
import { WorkflowRun } from '../../services/workflowEngine';

interface DashboardViewProps {
  readiness: DetailedReadinessStatus;
  activeRuns: WorkflowRun[];
  pages: ContentPage[];
  onNewJob: () => void;
  onNavigateTab: (tab: 'jobs' | 'pages' | 'studio' | 'publish' | 'settings') => void;
  onSelectJob: (jobId: string) => void;
  onRefresh: () => void;
}

export const DashboardView: React.FC<DashboardViewProps> = ({
  readiness,
  activeRuns,
  pages,
  onNewJob,
  onNavigateTab,
  onSelectJob,
  onRefresh,
}) => {
  // Aggregate stats
  const totalJobs = activeRuns.length;
  const runningJobs = activeRuns.filter((r) => r.status === 'running').length;
  const queuedJobs = activeRuns.filter((r) => r.status === 'pending' || r.status === 'queued').length;
  const errorJobs = activeRuns.filter((r) => r.status === 'failed' || r.status === 'error').length;
  const completedJobs = activeRuns.filter((r) => r.status === 'completed' || r.status === 'complete_success').length;

  // Pipeline stage breakdown counts
  const stageCounts = {
    image: activeRuns.filter((r) => r.currentStage === 'image' || r.currentStage?.includes('image')).length,
    aspect: activeRuns.filter((r) => r.currentStage === 'aspect' || r.currentStage?.includes('9:16')).length,
    video: activeRuns.filter((r) => r.currentStage === 'video' || r.currentStage?.includes('video')).length,
    download: activeRuns.filter((r) => r.currentStage === 'download').length,
    readyToPost: activeRuns.filter((r) => r.status === 'completed' && !r.isPublished).length,
    posting: activeRuns.filter((r) => r.currentStage === 'publish' || r.currentStage === 'posting').length,
  };

  const getPageName = (pageId?: string) => {
    if (!pageId) return 'General';
    const found = pages.find((p) => p.id === pageId);
    return found ? found.name : pageId;
  };

  return (
    <div className="space-y-6 p-6 max-w-7xl mx-auto">
      {/* Top Banner / Hero */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 rounded-2xl bg-gradient-to-r from-[#171b26] via-[#141822] to-[#10141d] border border-white/[0.08] p-6 shadow-2xl">
        <div>
          <div className="flex items-center gap-3 mb-1.5">
            <span className="flex h-2.5 w-2.5 rounded-full bg-emerald-400 animate-pulse" />
            <h1 className="text-xl font-bold text-white tracking-tight">Floword Production Studio</h1>
            <span className="px-2.5 py-0.5 rounded-full text-[11px] font-semibold bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
              System Healthy
            </span>
          </div>
          <p className="text-sm text-zinc-400">
            Automated multi-channel video generation & publishing console.
          </p>
        </div>

        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={onRefresh}
            className="flex items-center gap-2 px-3.5 py-2 rounded-xl bg-white/[0.04] hover:bg-white/[0.08] text-zinc-300 text-xs font-medium border border-white/[0.08] transition"
            title="Refresh statistics"
          >
            <RefreshCw className="h-3.5 w-3.5" />
            Refresh
          </button>
          <button
            type="button"
            onClick={onNewJob}
            className="flex items-center gap-2 px-4 py-2 rounded-xl bg-gradient-to-r from-[#e54d5e] to-[#c23b4c] hover:from-[#f05c6d] hover:to-[#d04657] text-white text-xs font-semibold shadow-lg shadow-rose-500/20 transition transform active:scale-95"
          >
            <Plus className="h-4 w-4" />
            New Job
          </button>
        </div>
      </div>

      {/* KPI Counters */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="rounded-2xl border border-white/[0.08] bg-[#121622]/80 backdrop-blur p-5">
          <div className="flex items-center justify-between text-zinc-400 text-xs font-medium mb-2">
            <span>Total Production Jobs</span>
            <Layers className="h-4 w-4 text-zinc-500" />
          </div>
          <div className="text-3xl font-extrabold text-white">{totalJobs}</div>
          <div className="mt-2 flex items-center gap-1.5 text-[11px] text-zinc-500">
            <span>{pages.length} Pages active</span>
          </div>
        </div>

        <div className="rounded-2xl border border-blue-500/20 bg-blue-500/[0.03] backdrop-blur p-5">
          <div className="flex items-center justify-between text-blue-400 text-xs font-medium mb-2">
            <span>Running Now</span>
            <Activity className="h-4 w-4 text-blue-400 animate-spin" />
          </div>
          <div className="text-3xl font-extrabold text-blue-400">{runningJobs}</div>
          <div className="mt-2 text-[11px] text-blue-400/70">
            {queuedJobs} queued in pipeline
          </div>
        </div>

        <div className="rounded-2xl border border-emerald-500/20 bg-emerald-500/[0.03] backdrop-blur p-5">
          <div className="flex items-center justify-between text-emerald-400 text-xs font-medium mb-2">
            <span>Completed</span>
            <CheckCircle2 className="h-4 w-4 text-emerald-400" />
          </div>
          <div className="text-3xl font-extrabold text-emerald-400">{completedJobs}</div>
          <div className="mt-2 text-[11px] text-emerald-400/70">
            Ready or published
          </div>
        </div>

        <div className="rounded-2xl border border-rose-500/20 bg-rose-500/[0.03] backdrop-blur p-5">
          <div className="flex items-center justify-between text-rose-400 text-xs font-medium mb-2">
            <span>Requires Attention</span>
            <AlertCircle className="h-4 w-4 text-rose-400" />
          </div>
          <div className="text-3xl font-extrabold text-rose-400">{errorJobs}</div>
          <div className="mt-2 text-[11px] text-rose-400/70">
            {errorJobs > 0 ? 'Retry available' : 'Zero blockers'}
          </div>
        </div>
      </div>

      {/* Production Pipeline Breakdown & Readiness */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Stage counters */}
        <div className="lg:col-span-2 rounded-2xl border border-white/[0.08] bg-[#121622] p-5">
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-sm font-bold text-white flex items-center gap-2">
              <TrendingUp className="h-4 w-4 text-rose-400" />
              Production Pipeline Stages
            </h3>
            <button
              onClick={() => onNavigateTab('jobs')}
              className="text-xs text-rose-400 hover:text-rose-300 font-medium flex items-center gap-1 transition"
            >
              View all jobs <ArrowUpRight className="h-3 w-3" />
            </button>
          </div>

          <div className="grid grid-cols-2 sm:grid-cols-3 gap-3">
            <div className="p-3.5 rounded-xl bg-white/[0.02] border border-white/[0.06] hover:bg-white/[0.04] transition">
              <div className="text-[11px] text-zinc-400 font-medium">1. Generate Image</div>
              <div className="text-2xl font-bold text-white mt-1">{stageCounts.image}</div>
              <div className="text-[10px] text-zinc-500 mt-1">Grok / FLUX Worker</div>
            </div>

            <div className="p-3.5 rounded-xl bg-white/[0.02] border border-white/[0.06] hover:bg-white/[0.04] transition">
              <div className="text-[11px] text-zinc-400 font-medium">2. Convert 9:16</div>
              <div className="text-2xl font-bold text-white mt-1">{stageCounts.aspect}</div>
              <div className="text-[10px] text-zinc-500 mt-1">Smart Outpainting</div>
            </div>

            <div className="p-3.5 rounded-xl bg-white/[0.02] border border-white/[0.06] hover:bg-white/[0.04] transition">
              <div className="text-[11px] text-zinc-400 font-medium">3. Generate Video</div>
              <div className="text-2xl font-bold text-white mt-1">{stageCounts.video}</div>
              <div className="text-[10px] text-zinc-500 mt-1">Video Diffusion</div>
            </div>

            <div className="p-3.5 rounded-xl bg-white/[0.02] border border-white/[0.06] hover:bg-white/[0.04] transition">
              <div className="text-[11px] text-zinc-400 font-medium">4. Download / Merge</div>
              <div className="text-2xl font-bold text-white mt-1">{stageCounts.download}</div>
              <div className="text-[10px] text-zinc-500 mt-1">Local Storage</div>
            </div>

            <div className="p-3.5 rounded-xl bg-amber-500/[0.04] border border-amber-500/20 hover:bg-amber-500/[0.08] transition">
              <div className="text-[11px] text-amber-300 font-medium">5. Ready To Post</div>
              <div className="text-2xl font-bold text-amber-400 mt-1">{stageCounts.readyToPost}</div>
              <div className="text-[10px] text-amber-400/70 mt-1">Waiting Review</div>
            </div>

            <div className="p-3.5 rounded-xl bg-blue-500/[0.04] border border-blue-500/20 hover:bg-blue-500/[0.08] transition">
              <div className="text-[11px] text-blue-300 font-medium">6. Auto / Posting</div>
              <div className="text-2xl font-bold text-blue-400 mt-1">{stageCounts.posting}</div>
              <div className="text-[10px] text-blue-400/70 mt-1">FB / TikTok / YT</div>
            </div>
          </div>
        </div>

        {/* Worker & System Summary */}
        <div className="rounded-2xl border border-white/[0.08] bg-[#121622] p-5 flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-sm font-bold text-white flex items-center gap-2">
                <Zap className="h-4 w-4 text-amber-400" />
                Execution Engines
              </h3>
              <button
                onClick={() => onNavigateTab('settings')}
                className="text-xs text-zinc-400 hover:text-white"
              >
                Settings
              </button>
            </div>

            <div className="space-y-2.5">
              <div className="flex items-center justify-between p-2.5 rounded-xl bg-white/[0.02] border border-white/[0.05]">
                <div className="flex items-center gap-2">
                  <span className={`h-2 w-2 rounded-full ${readiness.overall === 'ready' ? 'bg-emerald-400' : 'bg-amber-400'}`} />
                  <span className="text-xs text-zinc-300">Unified Orchestrator</span>
                </div>
                <span className="text-[11px] font-mono text-zinc-500">:20128</span>
              </div>

              <div className="flex items-center justify-between p-2.5 rounded-xl bg-white/[0.02] border border-white/[0.05]">
                <div className="flex items-center gap-2">
                  <span className="h-2 w-2 rounded-full bg-emerald-400" />
                  <span className="text-xs text-zinc-300">Browser Extension Pool</span>
                </div>
                <span className="text-[11px] text-emerald-400">Online</span>
              </div>

              <div className="flex items-center justify-between p-2.5 rounded-xl bg-white/[0.02] border border-white/[0.05]">
                <div className="flex items-center gap-2">
                  <span className="h-2 w-2 rounded-full bg-emerald-400" />
                  <span className="text-xs text-zinc-300">CapCut Draft Injector</span>
                </div>
                <span className="text-[11px] text-emerald-400">Ready</span>
              </div>
            </div>
          </div>

          <div className="mt-4 pt-3 border-t border-white/[0.06] flex items-center justify-between text-xs text-zinc-500">
            <span>Review Inbox: {stageCounts.readyToPost} videos</span>
            <button
              onClick={() => onNavigateTab('publish')}
              className="text-rose-400 hover:text-rose-300 font-medium"
            >
              Open Inbox &rarr;
            </button>
          </div>
        </div>
      </div>

      {/* Recent Production Jobs */}
      <div className="rounded-2xl border border-white/[0.08] bg-[#121622] p-5">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-sm font-bold text-white flex items-center gap-2">
            <Clock className="h-4 w-4 text-zinc-400" />
            Recent Production Jobs
          </h3>
          <button
            onClick={() => onNavigateTab('jobs')}
            className="text-xs text-zinc-400 hover:text-white font-medium transition"
          >
            View all ({activeRuns.length})
          </button>
        </div>

        {activeRuns.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <Video className="h-10 w-10 text-zinc-600 mb-3" />
            <p className="text-sm font-medium text-zinc-400">Chưa có Job nào trong phiên làm việc.</p>
            <p className="text-xs text-zinc-500 mt-1">Bấm "New Job" để khởi chạy luồng tạo video đầu tiên.</p>
            <button
              type="button"
              onClick={onNewJob}
              className="mt-4 px-4 py-2 rounded-xl bg-rose-600 hover:bg-rose-500 text-white text-xs font-semibold transition"
            >
              Tạo Job Mới
            </button>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs">
              <thead>
                <tr className="border-b border-white/[0.06] text-zinc-500 font-medium">
                  <th className="pb-3">Job ID</th>
                  <th className="pb-3">Page</th>
                  <th className="pb-3">Current Stage</th>
                  <th className="pb-3">Progress</th>
                  <th className="pb-3">Status</th>
                  <th className="pb-3 text-right">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-white/[0.04]">
                {activeRuns.slice(0, 8).map((run) => (
                  <tr
                    key={run.id}
                    onClick={() => onSelectJob(run.id)}
                    className="hover:bg-white/[0.02] cursor-pointer transition group"
                  >
                    <td className="py-3 font-mono font-bold text-zinc-200">
                      {run.id.slice(0, 8)}...
                    </td>
                    <td className="py-3 text-zinc-300">
                      {getPageName(run.pageId)}
                    </td>
                    <td className="py-3 text-zinc-400 capitalize">
                      {run.currentStage || 'Initializing'}
                    </td>
                    <td className="py-3">
                      <div className="flex items-center gap-2">
                        <div className="w-24 h-1.5 rounded-full bg-white/[0.08] overflow-hidden">
                          <div
                            className={`h-full transition-all duration-300 ${
                              run.status === 'failed'
                                ? 'bg-rose-500'
                                : run.status === 'completed'
                                ? 'bg-emerald-500'
                                : 'bg-blue-500'
                            }`}
                            style={{ width: `${run.progressPercent || 0}%` }}
                          />
                        </div>
                        <span className="text-[11px] font-mono text-zinc-500">
                          {run.progressPercent || 0}%
                        </span>
                      </div>
                    </td>
                    <td className="py-3">
                      <span
                        className={`inline-flex items-center px-2 py-0.5 rounded-md text-[10px] font-semibold uppercase tracking-wider ${
                          run.status === 'running'
                            ? 'bg-blue-500/10 text-blue-400 border border-blue-500/20'
                            : run.status === 'completed'
                            ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                            : run.status === 'failed'
                            ? 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                            : 'bg-zinc-500/10 text-zinc-400 border border-zinc-500/20'
                        }`}
                      >
                        {run.status}
                      </span>
                    </td>
                    <td className="py-3 text-right">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          onSelectJob(run.id);
                        }}
                        className="text-xs text-zinc-400 group-hover:text-white font-medium hover:underline"
                      >
                        Inspect &rarr;
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};
