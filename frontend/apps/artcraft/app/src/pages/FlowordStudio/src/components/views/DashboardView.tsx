import React, { useEffect, useState, useCallback } from 'react';
import {
  getDashboardSummary,
  DashboardSummary,
  listContentPages,
  ContentPage,
  checkStorageHealth,
  StorageHealthReport,
} from '../../api/flowordClient';
import {
  BarChart3, RefreshCw, Layers, CheckCircle2, AlertTriangle, Clock,
  Play, Share2, Database, HardDrive, ShieldCheck, Sparkles, Film,
  ArrowUpRight, Filter, Calendar, Globe, Server, Check, X
} from 'lucide-react';

export const DashboardView: React.FC = () => {
  const [summary, setSummary] = useState<DashboardSummary | null>(null);
  const [pages, setPages] = useState<ContentPage[]>([]);
  const [selectedPageId, setSelectedPageId] = useState<string>('all');
  const [dateRange, setDateRange] = useState<'today' | '7d' | '30d' | 'all'>('all');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [platformFilter, setPlatformFilter] = useState<string>('all');
  const [loading, setLoading] = useState<boolean>(true);
  const [pageStorageHealth, setPageStorageHealth] = useState<StorageHealthReport | null>(null);

  const fetchSummary = useCallback(async () => {
    setLoading(true);
    try {
      let dateFrom: number | undefined;
      const now = Math.floor(Date.now() / 1000);
      if (dateRange === 'today') {
        const startOfDay = new Date();
        startOfDay.setHours(0, 0, 0, 0);
        dateFrom = Math.floor(startOfDay.getTime() / 1000);
      } else if (dateRange === '7d') {
        dateFrom = now - 7 * 86400;
      } else if (dateRange === '30d') {
        dateFrom = now - 30 * 86400;
      }

      const res = await getDashboardSummary({
        page_id: selectedPageId !== 'all' ? selectedPageId : undefined,
        date_from: dateFrom,
        status: statusFilter !== 'all' ? statusFilter : undefined,
        platform: platformFilter !== 'all' ? platformFilter : undefined,
      });
      setSummary(res);

      if (selectedPageId !== 'all') {
        try {
          const health = await checkStorageHealth(selectedPageId);
          setPageStorageHealth(health);
        } catch {
          setPageStorageHealth(null);
        }
      } else {
        setPageStorageHealth(null);
      }
    } catch (err) {
      console.error('Failed to load dashboard summary:', err);
    } finally {
      setLoading(false);
    }
  }, [selectedPageId, dateRange, statusFilter, platformFilter]);

  useEffect(() => {
    listContentPages(false).then((p) => setPages(p.pages ?? [])).catch(console.error);
  }, []);

  useEffect(() => {
    fetchSummary();
    const interval = setInterval(fetchSummary, 6000);
    return () => clearInterval(interval);
  }, [fetchSummary]);

  const selectedPage = pages.find((p) => p.id === selectedPageId);

  return (
    <div className="flex-1 flex flex-col h-full bg-[#0b0f17] text-slate-100 overflow-y-auto p-6 space-y-6">
      {/* Top Header & Real-time Filter Bar */}
      <div className="flex flex-col md:flex-row items-start md:items-center justify-between gap-4 bg-[#131926] p-4 rounded-xl border border-slate-800/80 shadow-lg">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 border border-indigo-500/30 flex items-center justify-center text-indigo-400">
            <BarChart3 className="w-5 h-5" />
          </div>
          <div>
            <h1 className="text-xl font-bold text-white tracking-wide">Production Operations Console</h1>
            <p className="text-xs text-slate-400">Authoritative real-time aggregation from SQLite tasks & publications</p>
          </div>
        </div>

        {/* Global Filters */}
        <div className="flex flex-wrap items-center gap-2">
          {/* Page Selector */}
          <div className="flex items-center gap-1.5 bg-[#0b0f17] px-3 py-1.5 rounded-lg border border-slate-700 text-xs">
            <Layers className="w-3.5 h-3.5 text-slate-400" />
            <select
              value={selectedPageId}
              onChange={(e) => setSelectedPageId(e.target.value)}
              className="bg-transparent text-slate-200 focus:outline-none cursor-pointer"
            >
              <option value="all">Tất cả Content Pages</option>
              {pages.map((p) => (
                <option key={p.id} value={p.id}>{p.name} ({p.target_platform || 'Universal'})</option>
              ))}
            </select>
          </div>

          {/* Date Filter */}
          <div className="flex items-center gap-1.5 bg-[#0b0f17] px-3 py-1.5 rounded-lg border border-slate-700 text-xs">
            <Calendar className="w-3.5 h-3.5 text-slate-400" />
            <select
              value={dateRange}
              onChange={(e) => setDateRange(e.target.value as any)}
              className="bg-transparent text-slate-200 focus:outline-none cursor-pointer"
            >
              <option value="all">Toàn bộ thời gian</option>
              <option value="today">Hôm nay (Today)</option>
              <option value="7d">7 ngày qua</option>
              <option value="30d">30 ngày qua</option>
            </select>
          </div>

          {/* Platform Filter */}
          <div className="flex items-center gap-1.5 bg-[#0b0f17] px-3 py-1.5 rounded-lg border border-slate-700 text-xs">
            <Globe className="w-3.5 h-3.5 text-slate-400" />
            <select
              value={platformFilter}
              onChange={(e) => setPlatformFilter(e.target.value)}
              className="bg-transparent text-slate-200 focus:outline-none cursor-pointer"
            >
              <option value="all">Tất cả Platforms</option>
              <option value="facebook">Facebook Reels</option>
              <option value="tiktok">TikTok Video</option>
              <option value="youtube">YouTube Shorts</option>
            </select>
          </div>

          <button
            onClick={fetchSummary}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-semibold shadow transition active:scale-95 disabled:opacity-50"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
            <span>Làm mới</span>
          </button>
        </div>
      </div>

      {/* Per-Page Specific Banner (if a single page is selected) */}
      {selectedPage && (
        <div className="bg-gradient-to-r from-indigo-950/40 via-[#131926] to-[#131926] p-4 rounded-xl border border-indigo-500/30 flex flex-wrap items-center justify-between gap-4">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <span className="text-xs font-bold px-2 py-0.5 rounded bg-indigo-500/20 text-indigo-300 border border-indigo-500/30">
                ACTIVE PAGE
              </span>
              <h2 className="text-lg font-bold text-white">{selectedPage.name}</h2>
              <span className="text-xs text-slate-400 font-mono">[{selectedPage.id}]</span>
            </div>
            <p className="text-xs text-slate-400">
              Browser Profile: <span className="text-slate-200 font-mono">{selectedPage.browser_profile_id || 'default'}</span> • Platform: <span className="text-slate-200 font-semibold">{selectedPage.target_platform || 'Universal'}</span>
            </p>
          </div>

          {/* Storage health check */}
          <div className="flex items-center gap-3 bg-[#0b0f17] px-4 py-2 rounded-lg border border-slate-800">
            <HardDrive className="w-5 h-5 text-indigo-400" />
            <div className="text-xs">
              <div className="text-slate-400">Output Storage:</div>
              <div className="font-mono text-slate-200 text-[11px] truncate max-w-[260px]">
                {pageStorageHealth?.target_path || selectedPage.output_root || 'Default Video Storage'}
              </div>
            </div>
            {pageStorageHealth ? (
              <span className={`px-2 py-0.5 rounded text-[10px] font-bold ${pageStorageHealth.writable ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/30' : 'bg-rose-500/10 text-rose-400 border border-rose-500/30'}`}>
                {pageStorageHealth.writable ? 'WRITABLE' : 'READ-ONLY/LOCKED'}
              </span>
            ) : (
              <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-slate-800 text-slate-400">PROBING</span>
            )}
          </div>
        </div>
      )}

      {/* Main Metric Cards Grid */}
      <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-4">
        <div className="bg-[#131926] p-4 rounded-xl border border-slate-800/80 shadow">
          <div className="flex items-center justify-between text-slate-400 text-xs font-semibold mb-2">
            <span>TOTAL JOBS</span>
            <Database className="w-4 h-4 text-indigo-400" />
          </div>
          <div className="text-3xl font-extrabold text-white tracking-tight">{summary?.total_jobs ?? 0}</div>
          <div className="text-[11px] text-slate-500 mt-1">Đơn hàng trong bộ lọc</div>
        </div>

        <div className="bg-[#131926] p-4 rounded-xl border border-slate-800/80 shadow">
          <div className="flex items-center justify-between text-amber-400 text-xs font-semibold mb-2">
            <span>QUEUED</span>
            <Clock className="w-4 h-4 text-amber-400" />
          </div>
          <div className="text-3xl font-extrabold text-amber-300 tracking-tight">{summary?.queued ?? 0}</div>
          <div className="text-[11px] text-slate-500 mt-1">Chờ Worker nhận việc</div>
        </div>

        <div className="bg-[#131926] p-4 rounded-xl border border-slate-800/80 shadow">
          <div className="flex items-center justify-between text-sky-400 text-xs font-semibold mb-2">
            <span>IN PROGRESS</span>
            <Play className="w-4 h-4 text-sky-400" />
          </div>
          <div className="text-3xl font-extrabold text-sky-300 tracking-tight">
            {(summary?.generating_image ?? 0) + (summary?.converting_9_16 ?? 0) + (summary?.generating_video ?? 0) + (summary?.downloading ?? 0) + (summary?.saving_local ?? 0)}
          </div>
          <div className="text-[11px] text-slate-500 mt-1">Đang tạo ảnh/video/tải</div>
        </div>

        <div className="bg-[#131926] p-4 rounded-xl border border-slate-800/80 shadow">
          <div className="flex items-center justify-between text-emerald-400 text-xs font-semibold mb-2">
            <span>VIDEO DONE</span>
            <CheckCircle2 className="w-4 h-4 text-emerald-400" />
          </div>
          <div className="text-3xl font-extrabold text-emerald-300 tracking-tight">{summary?.done ?? 0}</div>
          <div className="text-[11px] text-slate-500 mt-1">Đã lưu ổ đĩa thành công</div>
        </div>

        <div className="bg-[#131926] p-4 rounded-xl border border-slate-800/80 shadow">
          <div className="flex items-center justify-between text-purple-400 text-xs font-semibold mb-2">
            <span>READY TO POST</span>
            <Share2 className="w-4 h-4 text-purple-400" />
          </div>
          <div className="text-3xl font-extrabold text-purple-300 tracking-tight">{summary?.ready_to_post ?? 0}</div>
          <div className="text-[11px] text-slate-500 mt-1">Sẵn sàng xuất bản</div>
        </div>

        <div className="bg-[#131926] p-4 rounded-xl border border-slate-800/80 shadow">
          <div className="flex items-center justify-between text-rose-400 text-xs font-semibold mb-2">
            <span>ERROR / AUTH</span>
            <AlertTriangle className="w-4 h-4 text-rose-400" />
          </div>
          <div className="text-3xl font-extrabold text-rose-400 tracking-tight">
            {(summary?.error ?? 0) + (summary?.auth_required ?? 0)}
          </div>
          <div className="text-[11px] text-slate-500 mt-1">{summary?.auth_required ?? 0} cần đăng nhập</div>
        </div>
      </div>

      {/* Production Pipeline Funnel Details */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Core Video Generation Stage Funnel */}
        <div className="lg:col-span-2 bg-[#131926] p-5 rounded-xl border border-slate-800/80 shadow space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-bold text-white flex items-center gap-2">
              <Film className="w-4 h-4 text-indigo-400" />
              <span>Tiến trình Sản xuất Video Thực tế (Pipeline Stages)</span>
            </h3>
            <span className="text-xs text-slate-500 font-mono">Real-time Stage Counts</span>
          </div>

          <div className="grid grid-cols-2 sm:grid-cols-5 gap-3">
            <div className="bg-[#0b0f17] p-3 rounded-lg border border-slate-800">
              <div className="text-[11px] text-slate-400 font-medium">1. Grok Edit</div>
              <div className="text-xl font-bold text-sky-400 mt-1">{summary?.generating_image ?? 0}</div>
              <div className="text-[10px] text-slate-500 mt-0.5">Tạo / chỉnh ảnh</div>
            </div>

            <div className="bg-[#0b0f17] p-3 rounded-lg border border-slate-800">
              <div className="text-[11px] text-slate-400 font-medium">2. Expand 9:16</div>
              <div className="text-xl font-bold text-indigo-400 mt-1">{summary?.converting_9_16 ?? 0}</div>
              <div className="text-[10px] text-slate-500 mt-0.5">Mở rộng khung dọc</div>
            </div>

            <div className="bg-[#0b0f17] p-3 rounded-lg border border-slate-800">
              <div className="text-[11px] text-slate-400 font-medium">3. Grok Video</div>
              <div className="text-xl font-bold text-purple-400 mt-1">{summary?.generating_video ?? 0}</div>
              <div className="text-[10px] text-slate-500 mt-0.5">Sinh clip chuyển động</div>
            </div>

            <div className="bg-[#0b0f17] p-3 rounded-lg border border-slate-800">
              <div className="text-[11px] text-slate-400 font-medium">4. Auto Download</div>
              <div className="text-xl font-bold text-cyan-400 mt-1">{summary?.downloading ?? 0}</div>
              <div className="text-[10px] text-slate-500 mt-0.5">Tải video MP4</div>
            </div>

            <div className="bg-[#0b0f17] p-3 rounded-lg border border-slate-800">
              <div className="text-[11px] text-slate-400 font-medium">5. Save Local</div>
              <div className="text-xl font-bold text-emerald-400 mt-1">{summary?.saving_local ?? 0}</div>
              <div className="text-[10px] text-slate-500 mt-0.5">Lưu chuẩn Page/date</div>
            </div>
          </div>

          {/* Quick status banner */}
          <div className="bg-[#0b0f17] p-3.5 rounded-lg border border-slate-800 flex items-center justify-between text-xs">
            <div className="flex items-center gap-2 text-slate-300">
              <ShieldCheck className="w-4 h-4 text-emerald-400" />
              <span>Dữ liệu Authoritative truy vấn trực tiếp từ SQLite Database - Zero client-side stale mockup.</span>
            </div>
            <span className="text-slate-500 font-mono text-[11px]">Polling 6s</span>
          </div>
        </div>

        {/* Social Publishing Engine Status Breakdown */}
        <div className="bg-[#131926] p-5 rounded-xl border border-slate-800/80 shadow space-y-4">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-bold text-white flex items-center gap-2">
              <Share2 className="w-4 h-4 text-purple-400" />
              <span>Phân phối Đa nền tảng (Publishing)</span>
            </h3>
          </div>

          <div className="space-y-2.5">
            <div className="flex items-center justify-between bg-[#0b0f17] p-2.5 rounded-lg border border-slate-800 text-xs">
              <div className="flex items-center gap-2 text-slate-300">
                <span className="w-2 h-2 rounded-full bg-blue-500"></span>
                <span>Facebook Reels</span>
              </div>
              <span className="font-mono font-bold text-blue-400">{summary?.publications_facebook ?? 0}</span>
            </div>

            <div className="flex items-center justify-between bg-[#0b0f17] p-2.5 rounded-lg border border-slate-800 text-xs">
              <div className="flex items-center gap-2 text-slate-300">
                <span className="w-2 h-2 rounded-full bg-rose-500"></span>
                <span>TikTok Video</span>
              </div>
              <span className="font-mono font-bold text-rose-400">{summary?.publications_tiktok ?? 0}</span>
            </div>

            <div className="flex items-center justify-between bg-[#0b0f17] p-2.5 rounded-lg border border-slate-800 text-xs">
              <div className="flex items-center gap-2 text-slate-300">
                <span className="w-2 h-2 rounded-full bg-red-500"></span>
                <span>YouTube Shorts</span>
              </div>
              <span className="font-mono font-bold text-red-400">{summary?.publications_youtube ?? 0}</span>
            </div>

            <div className="pt-2 border-t border-slate-800/80 grid grid-cols-3 gap-2 text-center text-xs">
              <div className="bg-[#0b0f17] p-2 rounded border border-slate-800">
                <div className="text-slate-400 text-[10px]">Đã đăng</div>
                <div className="text-emerald-400 font-bold font-mono mt-0.5">{summary?.publications_posted ?? 0}</div>
              </div>
              <div className="bg-[#0b0f17] p-2 rounded border border-slate-800">
                <div className="text-slate-400 text-[10px]">Lên lịch</div>
                <div className="text-amber-400 font-bold font-mono mt-0.5">{summary?.publications_scheduled ?? 0}</div>
              </div>
              <div className="bg-[#0b0f17] p-2 rounded border border-slate-800">
                <div className="text-slate-400 text-[10px]">Chờ duyệt</div>
                <div className="text-indigo-400 font-bold font-mono mt-0.5">{summary?.publications_waiting_approval ?? 0}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
