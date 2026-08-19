import React, { useEffect, useState, useCallback } from 'react';
import {
  listJobPublications,
  JobPublication,
  listPipelineJobsPaginated,
  listContentPages,
  ContentPage,
  retryPublication,
} from '../../api/flowordClient';
import {
  History, Share2, Activity, RefreshCw, Layers, ExternalLink,
  RotateCcw, CheckCircle2, AlertTriangle, Clock, Globe, Filter, Search
} from 'lucide-react';

export const HistoryView: React.FC = () => {
  const [tab, setTab] = useState<'publishing' | 'jobs'>('publishing');
  const [publications, setPublications] = useState<JobPublication[]>([]);
  const [pages, setPages] = useState<ContentPage[]>([]);
  const [selectedPageId, setSelectedPageId] = useState<string>('all');
  const [platformFilter, setPlatformFilter] = useState<string>('all');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [loading, setLoading] = useState<boolean>(true);

  const fetchPublications = useCallback(async () => {
    setLoading(true);
    try {
      const res = await listJobPublications({
        page_id: selectedPageId !== 'all' ? selectedPageId : undefined,
        platform: platformFilter !== 'all' ? platformFilter : undefined,
        status: statusFilter !== 'all' ? statusFilter : undefined,
      });
      setPublications(res);
    } catch (e) {
      console.error('Failed to load publishing history:', e);
    } finally {
      setLoading(false);
    }
  }, [selectedPageId, platformFilter, statusFilter]);

  useEffect(() => {
    listContentPages(false).then((p) => setPages(p.pages ?? [])).catch(console.error);
  }, []);

  useEffect(() => {
    if (tab === 'publishing') {
      fetchPublications();
    }
  }, [tab, fetchPublications]);

  const [historyError, setHistoryError] = useState<string | null>(null);

  const handleRetryPub = async (pubId: string) => {
    setHistoryError(null);
    try {
      await retryPublication(pubId);
      fetchPublications();
    } catch (e) {
      setHistoryError(`Lỗi retry publication: ${e}`);
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full bg-[#0b0f17] text-slate-100 overflow-hidden">
      {historyError && (
        <div className="px-4 py-2.5 bg-rose-950/90 border-b border-rose-500/50 text-rose-300 text-xs flex items-center justify-between">
          <span>{historyError}</span>
          <button onClick={() => setHistoryError(null)} className="text-slate-400 hover:text-white">✕</button>
        </div>
      )}
      {/* Header & Tabs */}
      <div className="p-4 bg-[#131926] border-b border-slate-800 flex flex-wrap items-center justify-between gap-4 shadow-md">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 border border-indigo-500/30 flex items-center justify-center text-indigo-400">
            <History className="w-5 h-5" />
          </div>
          <div>
            <h1 className="text-xl font-bold text-white tracking-wide">Operations & Publishing History</h1>
            <p className="text-xs text-slate-400">Durable database records of publications, post IDs, and execution logs</p>
          </div>
        </div>

        {/* Tab Switcher */}
        <div className="flex items-center bg-[#0b0f17] p-1 rounded-lg border border-slate-800 text-xs">
          <button
            onClick={() => setTab('publishing')}
            className={`px-3 py-1.5 rounded-md font-semibold transition ${tab === 'publishing' ? 'bg-indigo-600 text-white shadow' : 'text-slate-400 hover:text-slate-200'}`}
          >
            Lịch sử Xuất bản (Publishing)
          </button>
        </div>
      </div>

      {/* Filter Bar */}
      <div className="p-3 bg-[#101522] border-b border-slate-800 flex flex-wrap items-center justify-between gap-3 text-xs">
        <div className="flex flex-wrap items-center gap-2">
          {/* Page Filter */}
          <div className="flex items-center gap-1.5 bg-[#0b0f17] px-3 py-1.5 rounded-lg border border-slate-700">
            <Layers className="w-3.5 h-3.5 text-slate-400" />
            <select
              value={selectedPageId}
              onChange={(e) => setSelectedPageId(e.target.value)}
              className="bg-transparent text-slate-200 focus:outline-none cursor-pointer"
            >
              <option value="all">Tất cả Content Pages</option>
              {pages.map((p) => (
                <option key={p.id} value={p.id}>{p.name}</option>
              ))}
            </select>
          </div>

          {/* Platform Filter */}
          <div className="flex items-center gap-1.5 bg-[#0b0f17] px-3 py-1.5 rounded-lg border border-slate-700">
            <Globe className="w-3.5 h-3.5 text-slate-400" />
            <select
              value={platformFilter}
              onChange={(e) => setPlatformFilter(e.target.value)}
              className="bg-transparent text-slate-200 focus:outline-none cursor-pointer"
            >
              <option value="all">Tất cả Platforms</option>
              <option value="facebook">Facebook</option>
              <option value="tiktok">TikTok</option>
              <option value="youtube">YouTube</option>
            </select>
          </div>

          {/* Status Filter */}
          <div className="flex items-center gap-1.5 bg-[#0b0f17] px-3 py-1.5 rounded-lg border border-slate-700">
            <Filter className="w-3.5 h-3.5 text-slate-400" />
            <select
              value={statusFilter}
              onChange={(e) => setStatusFilter(e.target.value)}
              className="bg-transparent text-slate-200 focus:outline-none cursor-pointer"
            >
              <option value="all">Tất cả Trạng thái</option>
              <option value="POSTED">POSTED (Thành công)</option>
              <option value="POSTING">POSTING (Đang đăng)</option>
              <option value="SCHEDULED">SCHEDULED (Lên lịch)</option>
              <option value="FAILED">FAILED (Thất bại)</option>
              <option value="WAITING_APPROVAL">WAITING_APPROVAL (Chờ duyệt)</option>
            </select>
          </div>
        </div>

        <button
          onClick={fetchPublications}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white font-semibold transition active:scale-95 disabled:opacity-50"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          <span>Làm mới</span>
        </button>
      </div>

      {/* Main Publishing History Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-left border-collapse text-xs">
          <thead>
            <tr className="bg-[#101522] border-b border-slate-800 text-slate-400 sticky top-0 z-10 select-none">
              <th className="py-3 px-4 font-semibold">RECORD ID</th>
              <th className="py-3 px-4 font-semibold">PAGE & PLATFORM</th>
              <th className="py-3 px-4 font-semibold">JOB ID</th>
              <th className="py-3 px-4 font-semibold">STATUS</th>
              <th className="py-3 px-4 font-semibold">POST ID / LINK</th>
              <th className="py-3 px-4 font-semibold">SCHEDULED / POSTED AT</th>
              <th className="py-3 px-4 text-right font-semibold">ACTIONS</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60">
            {loading && publications.length === 0 ? (
              <tr>
                <td colSpan={7} className="text-center py-16 text-slate-500">
                  <RefreshCw className="w-6 h-6 animate-spin mx-auto mb-2 text-indigo-400" />
                  <span>Đang tải lịch sử xuất bản từ SQLite...</span>
                </td>
              </tr>
            ) : publications.length === 0 ? (
              <tr>
                <td colSpan={7} className="text-center py-16 text-slate-500">
                  Chưa có bản ghi xuất bản nào trong bộ lọc.
                </td>
              </tr>
            ) : (
              publications.map((pub) => {
                const page = pages.find((p) => p.id === pub.page_id);
                const statusColor =
                  pub.status === 'POSTED'
                    ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30'
                    : pub.status === 'POSTING'
                    ? 'bg-sky-500/10 text-sky-400 border-sky-500/30 animate-pulse'
                    : pub.status === 'SCHEDULED'
                    ? 'bg-amber-500/10 text-amber-400 border-amber-500/30'
                    : pub.status === 'FAILED'
                    ? 'bg-rose-500/10 text-rose-400 border-rose-500/30'
                    : 'bg-slate-800 text-slate-400 border-slate-700';

                return (
                  <tr key={pub.id} className="hover:bg-[#131926]/80 transition">
                    <td className="py-3 px-4 font-mono text-indigo-300 font-bold">
                      {pub.id.slice(0, 14)}...
                    </td>
                    <td className="py-3 px-4">
                      <div className="font-semibold text-slate-200">{page?.name || pub.page_id}</div>
                      <div className="text-[10px] uppercase font-bold text-slate-400 tracking-wider">
                        {pub.platform}
                      </div>
                    </td>
                    <td className="py-3 px-4 font-mono text-slate-400">
                      {pub.job_id.slice(0, 14)}...
                    </td>
                    <td className="py-3 px-4">
                      <span className={`px-2 py-0.5 rounded text-[10px] font-bold border ${statusColor}`}>
                        {pub.status}
                      </span>
                    </td>
                    <td className="py-3 px-4 max-w-[200px]">
                      {pub.post_id ? (
                        <div className="space-y-0.5">
                          <div className="font-mono text-[11px] text-slate-300 font-semibold truncate">
                            ID: {pub.post_id}
                          </div>
                          {pub.post_url && (
                            <a
                              href={pub.post_url}
                              target="_blank"
                              rel="noreferrer"
                              className="text-[10px] text-indigo-400 hover:text-indigo-300 flex items-center gap-1 truncate"
                            >
                              <ExternalLink className="w-2.5 h-2.5 flex-shrink-0" />
                              <span className="truncate">{pub.post_url}</span>
                            </a>
                          )}
                        </div>
                      ) : pub.error_message ? (
                        <span className="text-rose-400 text-[11px] truncate block" title={pub.error_message}>
                          {pub.error_message}
                        </span>
                      ) : (
                        <span className="text-slate-600 italic">Chưa có ID</span>
                      )}
                    </td>
                    <td className="py-3 px-4 text-[11px] text-slate-400 whitespace-nowrap">
                      {pub.posted_at ? (
                        <div>Đăng: {new Date(pub.posted_at * 1000).toLocaleString('vi-VN')}</div>
                      ) : pub.scheduled_at ? (
                        <div className="text-amber-300">Lịch: {new Date(pub.scheduled_at * 1000).toLocaleString('vi-VN')}</div>
                      ) : (
                        <div>Tạo: {new Date(pub.created_at * 1000).toLocaleString('vi-VN')}</div>
                      )}
                    </td>
                    <td className="py-3 px-4 text-right whitespace-nowrap">
                      {pub.status === 'FAILED' && (
                        <button
                          onClick={() => handleRetryPub(pub.id)}
                          className="px-2.5 py-1 rounded bg-indigo-950 hover:bg-indigo-900 border border-indigo-500/30 text-indigo-300 text-[11px] font-semibold transition"
                        >
                          <RotateCcw className="w-3 h-3 inline mr-1" />
                          <span>Retry</span>
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
    </div>
  );
};
