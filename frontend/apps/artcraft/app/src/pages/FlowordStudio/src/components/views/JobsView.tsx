import React, { useEffect, useState, useCallback } from 'react';
import toast from 'react-hot-toast';
import {
  listPipelineJobsPaginated,
  listContentPages,
  listPipelineJobEvents,
  retryFlowordJobFromStart,
  cancelFlowordWorkflow,
  ContentPage,
  PipelineJobEvent,
} from '../../api/flowordClient';
import {
  Search, RefreshCw, Layers, ChevronLeft, ChevronRight, Eye, Play,
  AlertTriangle, CheckCircle2, Clock, RotateCcw, XCircle, FileText,
  Video, ExternalLink, Hash, Activity, Film, Filter, Copy
} from 'lucide-react';

interface PaginatedJobItem {
  id: { pipeline_job_id: string } | string;
  status: string;
  current_stage: string;
  maybe_page_id?: string;
  maybe_input_payload?: string;
  maybe_stage_outputs?: string;
  maybe_on_failure_message?: string;
  maybe_page_snapshot?: string;
  maybe_business_status?: string;
  maybe_started_at?: number;
  maybe_failure_code?: string;
  maybe_failure_stage?: string;
  created_at: number;
  updated_at: number;
  maybe_completed_at?: number;
}

export const JobsView: React.FC = () => {
  const [jobs, setJobs] = useState<PaginatedJobItem[]>([]);
  const [totalCount, setTotalCount] = useState<number>(0);
  const [pageIndex, setPageIndex] = useState<number>(0);
  const [pageSize, setPageSize] = useState<number>(20);
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [selectedPageId, setSelectedPageId] = useState<string>('all');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [pages, setPages] = useState<ContentPage[]>([]);
  const [loading, setLoading] = useState<boolean>(true);

  // Inspector Drawer State
  const [inspectingJob, setInspectingJob] = useState<PaginatedJobItem | null>(null);
  const [jobEvents, setJobEvents] = useState<PipelineJobEvent[]>([]);
  const [loadingEvents, setLoadingEvents] = useState<boolean>(false);

  const fetchJobs = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listPipelineJobsPaginated({
        page_id: selectedPageId !== 'all' ? selectedPageId : undefined,
        status: statusFilter !== 'all' ? statusFilter : undefined,
        search_query: searchQuery.trim() !== '' ? searchQuery.trim() : undefined,
        limit: pageSize,
        offset: pageIndex * pageSize,
      });

      setJobs(res.jobs ?? []);
      setTotalCount(res.total_count ?? 0);
    } catch (err) {
      console.error('Failed to load paginated jobs:', err);
    } finally {
      setLoading(false);
    }
  }, [pageIndex, pageSize, selectedPageId, statusFilter, searchQuery]);

  useEffect(() => {
    listContentPages(false).then((p) => setPages(p.pages ?? [])).catch(console.error);
  }, []);

  useEffect(() => {
    fetchJobs();
  }, [fetchJobs]);

  const openInspector = async (job: PaginatedJobItem) => {
    setInspectingJob(job);
    setLoadingEvents(true);
    const rawId = typeof job.id === 'object' ? (job.id as any).pipeline_job_id : job.id;
    try {
      const events = await listPipelineJobEvents(rawId);
      setJobEvents(events);
    } catch (e) {
      console.error('Failed to load job events:', e);
      setJobEvents([]);
    } finally {
      setLoadingEvents(false);
    }
  };

  const [actionError, setActionError] = useState<string | null>(null);

  const handleRetry = async (jobId: string) => {
    setActionError(null);
    try {
      await retryFlowordJobFromStart(jobId);
      fetchJobs();
      if (inspectingJob) {
        const events = await listPipelineJobEvents(jobId);
        setJobEvents(events);
      }
    } catch (e) {
      setActionError(`Lỗi retry job: ${e}`);
    }
  };

  const handleCancel = async (jobId: string) => {
    setActionError(null);
    try {
      await cancelFlowordWorkflow(jobId);
      fetchJobs();
    } catch (e) {
      setActionError(`Lỗi hủy job: ${e}`);
    }
  };

  const getJobIdString = (job: PaginatedJobItem) => {
    return typeof job.id === 'object' ? (job.id as any).pipeline_job_id : String(job.id);
  };

  const parseInputPayload = (raw?: string) => {
    if (!raw) return null;
    try {
      return JSON.parse(raw);
    } catch {
      return null;
    }
  };

  const parseStageOutputs = (raw?: string) => {
    if (!raw) return null;
    try {
      return JSON.parse(raw);
    } catch {
      return null;
    }
  };

  const totalPages = Math.ceil(totalCount / pageSize);

  return (
    <div className="flex-1 flex flex-col h-full bg-[#0b0f17] text-slate-100 overflow-hidden">
      {actionError && (
        <div className="px-4 py-2.5 bg-rose-950/90 border-b border-rose-500/50 text-rose-300 text-xs flex items-center justify-between">
          <span>{actionError}</span>
          <button onClick={() => setActionError(null)} className="text-slate-400 hover:text-white">✕</button>
        </div>
      )}
      {/* Top Filter & Search Bar */}
      <div className="p-4 bg-[#131926] border-b border-slate-800 flex flex-wrap items-center justify-between gap-3 shadow-md">
        <div className="flex items-center gap-3 flex-1 min-w-[280px] max-w-md">
          <div className="relative w-full">
            <Search className="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              placeholder="Tìm theo Job ID, prompt..."
              value={searchQuery}
              onChange={(e) => {
                setSearchQuery(e.target.value);
                setPageIndex(0);
              }}
              className="w-full bg-[#0b0f17] text-xs text-slate-200 pl-9 pr-3 py-2 rounded-lg border border-slate-700 focus:outline-none focus:border-indigo-500 transition"
            />
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Page Filter */}
          <div className="flex items-center gap-1.5 bg-[#0b0f17] px-3 py-1.5 rounded-lg border border-slate-700 text-xs">
            <Layers className="w-3.5 h-3.5 text-slate-400" />
            <select
              value={selectedPageId}
              onChange={(e) => {
                setSelectedPageId(e.target.value);
                setPageIndex(0);
              }}
              className="bg-transparent text-slate-200 focus:outline-none cursor-pointer"
            >
              <option value="all">Tất cả Content Pages</option>
              {pages.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </div>

          {/* Status Filter */}
          <div className="flex items-center gap-1.5 bg-[#0b0f17] px-3 py-1.5 rounded-lg border border-slate-700 text-xs">
            <Filter className="w-3.5 h-3.5 text-slate-400" />
            <select
              value={statusFilter}
              onChange={(e) => {
                setStatusFilter(e.target.value);
                setPageIndex(0);
              }}
              className="bg-transparent text-slate-200 focus:outline-none cursor-pointer"
            >
              <option value="all">Tất cả trạng thái</option>
              <option value="QUEUED">QUEUED</option>
              <option value="RUNNING">RUNNING</option>
              <option value="DONE">DONE</option>
              <option value="ERROR">ERROR</option>
              <option value="AUTH_REQUIRED">AUTH_REQUIRED</option>
            </select>
          </div>

          <button
            onClick={fetchJobs}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow transition active:scale-95 disabled:opacity-50"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
            <span>Làm mới</span>
          </button>
        </div>
      </div>

      {/* Main Jobs Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-left border-collapse text-xs">
          <thead>
            <tr className="bg-[#101522] border-b border-slate-800 text-slate-400 sticky top-0 z-10 select-none">
              <th className="py-3 px-4 font-semibold">JOB ID</th>
              <th className="py-3 px-4 font-semibold">PAGE</th>
              <th className="py-3 px-4 font-semibold">PREVIEW / PROMPT</th>
              <th className="py-3 px-4 font-semibold">STATUS</th>
              <th className="py-3 px-4 font-semibold">CURRENT STAGE</th>
              <th className="py-3 px-4 font-semibold">OUTPUT / SAVED</th>
              <th className="py-3 px-4 font-semibold">CREATED</th>
              <th className="py-3 px-4 text-right font-semibold">ACTIONS</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60">
            {loading && jobs.length === 0 ? (
              <tr>
                <td colSpan={8} className="text-center py-16 text-slate-500">
                  <RefreshCw className="w-6 h-6 animate-spin mx-auto mb-2 text-indigo-400" />
                  <span>Đang tải danh sách Jobs từ SQLite...</span>
                </td>
              </tr>
            ) : jobs.length === 0 ? (
              <tr>
                <td colSpan={8} className="text-center py-16 text-slate-500">
                  Không tìm thấy Job nào phù hợp với bộ lọc.
                </td>
              </tr>
            ) : (
              jobs.map((job) => {
                const jobIdStr = getJobIdString(job);
                const input = parseInputPayload(job.maybe_input_payload);
                const outputs = parseStageOutputs(job.maybe_stage_outputs);
                const page = pages.find((p) => p.id === job.maybe_page_id);

                const statusColor =
                  job.status === 'DONE' || job.status === 'COMPLETED'
                    ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                    : job.status === 'RUNNING'
                    ? 'bg-sky-500/10 text-sky-400 border-sky-500/30 animate-pulse'
                    : job.status === 'AUTH_REQUIRED'
                    ? 'bg-amber-500/10 text-amber-400 border-amber-500/30'
                    : job.status === 'ERROR' || job.status === 'FAILED'
                    ? 'bg-rose-500/10 text-rose-400 border-rose-500/30'
                    : 'bg-slate-800 text-slate-400 border-slate-700';

                return (
                  <tr
                    key={jobIdStr}
                    onClick={() => openInspector(job)}
                    className="hover:bg-[#131926]/80 cursor-pointer transition"
                  >
                    <td className="py-3 px-4 font-mono font-bold text-indigo-300">
                      {jobIdStr.slice(0, 16)}...
                    </td>
                    <td className="py-3 px-4">
                      <div className="font-semibold text-slate-200">{page?.name || job.maybe_page_id || 'Universal'}</div>
                      <div className="text-[10px] text-slate-500">{page?.target_platform || 'All Platforms'}</div>
                    </td>
                    <td className="py-3 px-4 max-w-[260px]">
                      <div className="truncate text-slate-300 font-medium">
                        {input?.video_prompt || input?.image_prompt || input?.prompt || 'No Prompt Provided'}
                      </div>
                      {input?.source_image && (
                        <div className="text-[10px] text-indigo-400 truncate mt-0.5">
                          Source: {input.source_image}
                        </div>
                      )}
                    </td>
                    <td className="py-3 px-4">
                      <span className={`px-2 py-0.5 rounded text-[10px] font-bold border ${statusColor}`}>
                        {job.status}
                      </span>
                    </td>
                    <td className="py-3 px-4 font-mono text-[11px] text-slate-300">
                      {job.current_stage || 'QUEUED'}
                    </td>
                    <td className="py-3 px-4 max-w-[200px]">
                      {outputs?.save_local?.saved_file_path ? (
                        <div className="text-emerald-400 text-[11px] truncate font-mono flex items-center gap-1">
                          <Film className="w-3 h-3 flex-shrink-0" />
                          <span className="truncate">{outputs.save_local.saved_file_path}</span>
                        </div>
                      ) : (
                        <span className="text-slate-600 italic text-[11px]">Chưa hoàn tất</span>
                      )}
                    </td>
                    <td className="py-3 px-4 text-[11px] text-slate-400 whitespace-nowrap">
                      {new Date(job.created_at * 1000).toLocaleString('vi-VN')}
                    </td>
                    <td className="py-3 px-4 text-right whitespace-nowrap space-x-1" onClick={(e) => e.stopPropagation()}>
                      <button
                        onClick={() => openInspector(job)}
                        className="p-1.5 rounded hover:bg-slate-800 text-slate-300 hover:text-white"
                        title="Xem chi tiết"
                      >
                        <Eye className="w-3.5 h-3.5" />
                      </button>
                      {(job.status === 'ERROR' || job.status === 'FAILED' || job.status === 'AUTH_REQUIRED') && (
                        <button
                          onClick={() => handleRetry(jobIdStr)}
                          className="p-1.5 rounded hover:bg-indigo-950 text-indigo-400 hover:text-indigo-300"
                          title="Thử lại từ đầu"
                        >
                          <RotateCcw className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {/* Pagination Footer */}
      <div className="p-3 bg-[#131926] border-t border-slate-800 flex items-center justify-between text-xs text-slate-400 select-none">
        <div className="flex items-center gap-2">
          <span>Tổng cộng: <strong className="text-white font-mono">{totalCount}</strong> Jobs</span>
          <span>•</span>
          <span>Trang <strong className="text-white font-mono">{pageIndex + 1}</strong> / {totalPages || 1}</span>
        </div>

        <div className="flex items-center gap-2">
          <select
            value={pageSize}
            onChange={(e) => {
              setPageSize(Number(e.target.value));
              setPageIndex(0);
            }}
            className="bg-[#0b0f17] border border-slate-700 rounded px-2 py-1 text-slate-300 focus:outline-none"
          >
            <option value={10}>10 / trang</option>
            <option value={20}>20 / trang</option>
            <option value={50}>50 / trang</option>
            <option value={100}>100 / trang</option>
          </select>

          <button
            onClick={() => setPageIndex((p) => Math.max(0, p - 1))}
            disabled={pageIndex === 0}
            className="p-1.5 rounded border border-slate-700 bg-[#0b0f17] text-slate-300 hover:bg-slate-800 disabled:opacity-40 disabled:pointer-events-none"
          >
            <ChevronLeft className="w-4 h-4" />
          </button>
          <button
            onClick={() => setPageIndex((p) => Math.min(totalPages - 1, p + 1))}
            disabled={pageIndex >= totalPages - 1}
            className="p-1.5 rounded border border-slate-700 bg-[#0b0f17] text-slate-300 hover:bg-slate-800 disabled:opacity-40 disabled:pointer-events-none"
          >
            <ChevronRight className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Job Inspector Drawer */}
      {inspectingJob && (
        <div className="fixed inset-y-0 right-0 w-full max-w-xl bg-[#101522] border-l border-slate-800 shadow-2xl z-50 flex flex-col animate-in slide-in-from-right duration-200">
          <div className="p-4 border-b border-slate-800 flex items-center justify-between bg-[#131926]">
            <div>
              <h2 className="text-sm font-bold text-white flex items-center gap-2">
                <FileText className="w-4 h-4 text-indigo-400" />
                <span>Job Inspector</span>
              </h2>
              <div className="text-[11px] font-mono text-slate-400 truncate max-w-sm mt-0.5">
                {getJobIdString(inspectingJob)}
              </div>
            </div>
            <button
              onClick={() => setInspectingJob(null)}
              className="p-1.5 rounded-lg hover:bg-slate-800 text-slate-400 hover:text-white"
            >
              <XCircle className="w-5 h-5" />
            </button>
          </div>

          <div className="flex-1 overflow-y-auto p-5 space-y-5 text-xs">
            {/* Summary card */}
            <div className="bg-[#0b0f17] p-3.5 rounded-lg border border-slate-800 space-y-2">
              <div className="flex justify-between">
                <span className="text-slate-400">Status:</span>
                <span className="font-bold text-indigo-400">{inspectingJob.status}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-slate-400">Stage:</span>
                <span className="font-mono text-slate-200">{inspectingJob.current_stage}</span>
              </div>
              <div className="flex justify-between">
                <span className="text-slate-400">Page ID:</span>
                <span className="font-mono text-slate-200">{inspectingJob.maybe_page_id || 'None'}</span>
              </div>
              {inspectingJob.maybe_on_failure_message && (
                <div className="p-2.5 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-300">
                  <div className="flex items-center justify-between font-bold text-[10px] uppercase tracking-wider mb-1">
                    <span>Chi Tiết Lỗi (Failure Details)</span>
                    <button
                      type="button"
                      onClick={() => {
                        navigator.clipboard.writeText(inspectingJob.maybe_on_failure_message || '');
                        toast.success('Đã sao chép chi tiết lỗi!');
                      }}
                      className="flex items-center gap-1 px-1.5 py-0.5 rounded bg-rose-500/20 hover:bg-rose-500/30 text-rose-200 transition text-[10px] font-sans font-medium"
                    >
                      <Copy className="h-3 w-3" />
                      <span>Sao chép</span>
                    </button>
                  </div>
                  <div className="mt-0.5 font-mono text-[11px] break-all select-text">{inspectingJob.maybe_on_failure_message}</div>
                </div>
              )}
            </div>

            {/* Audit Trail Events */}
            <div className="space-y-3">
              <h3 className="text-xs font-bold text-slate-300 uppercase tracking-wider flex items-center gap-2">
                <Activity className="w-3.5 h-3.5 text-indigo-400" />
                <span>Lịch sử Audit Events (Durable SQLite Events)</span>
              </h3>

              {loadingEvents ? (
                <div className="text-center py-8 text-slate-500">
                  <RefreshCw className="w-4 h-4 animate-spin mx-auto mb-1" />
                  <span>Đang tải events...</span>
                </div>
              ) : jobEvents.length === 0 ? (
                <div className="text-center py-6 text-slate-500 italic bg-[#0b0f17] rounded-lg border border-slate-800">
                  Chưa có audit event nào được ghi nhận.
                </div>
              ) : (
                <div className="space-y-2">
                  {jobEvents.map((evt) => (
                    <div
                      key={evt.id}
                      className="bg-[#0b0f17] p-3 rounded-lg border border-slate-800/80 flex flex-col gap-1"
                    >
                      <div className="flex items-center justify-between">
                        <span className="font-mono font-bold text-indigo-400 text-[11px]">
                          [{evt.event_type}]
                        </span>
                        <span className="text-slate-500 text-[10px]">
                          {new Date(evt.created_at * 1000).toLocaleTimeString('vi-VN')}
                        </span>
                      </div>
                      <p className="text-slate-300 text-xs">{evt.message}</p>
                      {evt.metadata_json && (
                        <pre className="text-[10px] font-mono bg-black/40 p-1.5 rounded text-slate-400 overflow-x-auto">
                          {evt.metadata_json}
                        </pre>
                      )}
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
