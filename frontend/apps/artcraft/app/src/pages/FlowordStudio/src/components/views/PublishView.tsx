import React, { useState, useEffect, useCallback } from 'react';
import {
  CheckCircle2,
  Clock,
  ExternalLink,
  Film,
  Folder,
  Layers,
  Video,
  Copy,
  Share2,
  Send,
  AlertTriangle,
  RefreshCw,
  XCircle,
  Play,
  Calendar,
  Check,
  ShieldAlert,
} from 'lucide-react';
import toast from 'react-hot-toast';
import {
  ContentPage,
  JobPublication,
  PublicationStatus,
  listJobPublications,
  approvePublication,
  rejectPublication,
  schedulePublication,
  retryPublication,
  postNowPublication,
} from '../../api/flowordClient';
import { WorkflowRun } from '../../services/workflowEngine';

interface PublishViewProps {
  runs: WorkflowRun[];
  pages: ContentPage[];
}

export const PublishView: React.FC<PublishViewProps> = ({
  runs,
  pages,
}) => {
  const [publications, setPublications] = useState<JobPublication[]>([]);
  const [loading, setLoading] = useState(true);
  const [platformFilter, setPlatformFilter] = useState<'all' | 'facebook' | 'tiktok' | 'youtube'>('all');
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [pageFilter, setPageFilter] = useState<string>('all');
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  const fetchPublications = useCallback(async () => {
    try {
      const items = await listJobPublications({
        platform: platformFilter === 'all' ? undefined : platformFilter,
        status: statusFilter === 'all' ? undefined : statusFilter,
        page_id: pageFilter === 'all' ? undefined : pageFilter,
      });
      setPublications(items);
    } catch (err) {
      console.error('Failed to load publications:', err);
    } finally {
      setLoading(false);
    }
  }, [platformFilter, statusFilter, pageFilter]);

  useEffect(() => {
    fetchPublications();
    const timer = setInterval(fetchPublications, 3000);
    return () => clearInterval(timer);
  }, [fetchPublications]);

  const handleApprove = async (pubId: string) => {
    setActionLoading(pubId);
    try {
      await approvePublication(pubId);
      toast.success('Đã duyệt bài đăng! Đang chuyển vào hàng đợi xuất bản.');
      await fetchPublications();
    } catch (err: unknown) {
      toast.error(`Duyệt thất bại: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setActionLoading(null);
    }
  };

  const handleScheduleDefaultSlot = async (pubId: string) => {
    setActionLoading(pubId);
    try {
      await schedulePublication(pubId, undefined, true);
      toast.success('Đã xếp lịch đăng vào khung giờ mặc định khả dụng gần nhất!');
      await fetchPublications();
    } catch (err: unknown) {
      toast.error(`Lên lịch thất bại: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setActionLoading(null);
    }
  };

  const handlePostNow = async (pubId: string) => {
    setActionLoading(pubId);
    try {
      await postNowPublication(pubId);
      toast.success('Đã chuyển trạng thái sang Đăng Ngay!');
      await fetchPublications();
    } catch (err: unknown) {
      toast.error(`Đăng ngay thất bại: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setActionLoading(null);
    }
  };

  const handleReject = async (pubId: string) => {
    const reason = window.prompt('Nhập lý do từ chối (tùy chọn):');
    if (reason === null) return;
    setActionLoading(pubId);
    try {
      await rejectPublication(pubId, reason || undefined);
      toast.success('Đã từ chối bài đăng.');
      await fetchPublications();
    } catch (err: unknown) {
      toast.error(`Từ chối thất bại: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setActionLoading(null);
    }
  };

  const handleRetry = async (pubId: string) => {
    setActionLoading(pubId);
    try {
      await retryPublication(pubId);
      toast.success('Đã thử lại lệnh xuất bản!');
      await fetchPublications();
    } catch (err: unknown) {
      toast.error(`Thử lại thất bại: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setActionLoading(null);
    }
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    toast.success('Đã sao chép vào clipboard!');
  };

  const getPageName = (pageId: string) => {
    const found = pages.find((p) => p.id === pageId);
    return found ? found.name : pageId;
  };

  const getStatusBadge = (status: PublicationStatus) => {
    switch (status) {
      case 'WAITING_APPROVAL':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-amber-500/10 text-amber-300 border border-amber-500/30 flex items-center gap-1">
            <Clock className="h-3 w-3" /> Chờ Phê Duyệt
          </span>
        );
      case 'SCHEDULED':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-blue-500/10 text-blue-300 border border-blue-500/30 flex items-center gap-1">
            <Calendar className="h-3 w-3" /> Đã Xếp Lịch
          </span>
        );
      case 'READY_TO_POST':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-emerald-500/10 text-emerald-300 border border-emerald-500/30 flex items-center gap-1">
            <Send className="h-3 w-3" /> Sẵn Sàng Đăng
          </span>
        );
      case 'POSTING':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-purple-500/10 text-purple-300 border border-purple-500/30 flex items-center gap-1 animate-pulse">
            <RefreshCw className="h-3 w-3 animate-spin" /> Đang Đăng...
          </span>
        );
      case 'POSTED':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-emerald-500/20 text-emerald-300 border border-emerald-500/40 flex items-center gap-1">
            <CheckCircle2 className="h-3 w-3" /> Đã Đăng Thành Công
          </span>
        );
      case 'AUTH_REQUIRED':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-red-500/15 text-red-300 border border-red-500/40 flex items-center gap-1">
            <ShieldAlert className="h-3 w-3" /> Cần Đăng Nhập Profile
          </span>
        );
      case 'VERIFY_REQUIRED':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-orange-500/15 text-orange-300 border border-orange-500/40 flex items-center gap-1">
            <AlertTriangle className="h-3 w-3" /> Cần Xác Minh
          </span>
        );
      case 'POST_ERROR':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-red-500/10 text-red-300 border border-red-500/30 flex items-center gap-1">
            <AlertTriangle className="h-3 w-3" /> Lỗi Đăng Bài
          </span>
        );
      case 'CANCELLED':
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-bold bg-zinc-500/10 text-zinc-400 border border-zinc-500/20 flex items-center gap-1">
            <XCircle className="h-3 w-3" /> Đã Hủy
          </span>
        );
      default:
        return (
          <span className="px-2.5 py-1 rounded-full text-[11px] font-medium bg-zinc-500/10 text-zinc-300">
            {status}
          </span>
        );
    }
  };

  const getPlatformIcon = (platform: string) => {
    switch (platform.toLowerCase()) {
      case 'facebook':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-blue-600/30 text-blue-300 border border-blue-500/30">FACEBOOK REELS</span>;
      case 'tiktok':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-pink-600/30 text-pink-300 border border-pink-500/30">TIKTOK</span>;
      case 'youtube':
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-red-600/30 text-red-300 border border-red-500/30">YOUTUBE SHORTS</span>;
      default:
        return <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-zinc-700 text-zinc-300">{platform.toUpperCase()}</span>;
    }
  };

  return (
    <div className="max-w-7xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <div className="flex items-center gap-2">
            <h1 className="text-xl font-bold text-white tracking-tight flex items-center gap-2">
              <Share2 className="h-5 w-5 text-emerald-400" />
              Floword Multi-Platform Publishing Engine
            </h1>
            <span className="px-2.5 py-0.5 rounded-full text-xs font-bold bg-emerald-500/10 text-emerald-300 border border-emerald-500/20">
              {publications.length} Publications
            </span>
          </div>
          <p className="text-xs text-zinc-400 mt-0.5">
            Quản lý xuất bản video thật lên Facebook Reels, TikTok và YouTube Shorts qua browser profile Donut isolated.
          </p>
        </div>

        <button
          onClick={fetchPublications}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-white/[0.05] hover:bg-white/[0.1] text-xs text-zinc-300 transition"
        >
          <RefreshCw className="h-3.5 w-3.5" /> Làm mới
        </button>
      </div>

      {/* Filters Bar */}
      <div className="flex flex-wrap items-center gap-3 bg-[#121622] p-3 rounded-2xl border border-white/[0.08]">
        {/* Platform Tabs */}
        <div className="flex items-center gap-1 bg-black/40 p-1 rounded-xl border border-white/[0.05]">
          {(['all', 'facebook', 'tiktok', 'youtube'] as const).map((p) => (
            <button
              key={p}
              onClick={() => setPlatformFilter(p)}
              className={`px-3 py-1 text-xs font-semibold rounded-lg transition capitalize ${
                platformFilter === p
                  ? 'bg-emerald-600 text-white shadow'
                  : 'text-zinc-400 hover:text-white'
              }`}
            >
              {p === 'all' ? 'All Platforms' : p}
            </button>
          ))}
        </div>

        {/* Status Filter */}
        <select
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value)}
          className="px-3 py-1.5 rounded-xl bg-black/40 border border-white/[0.08] text-xs text-zinc-300 focus:outline-none"
        >
          <option value="all">Tất cả trạng thái</option>
          <option value="WAITING_APPROVAL">Chờ Phê Duyệt (Review)</option>
          <option value="SCHEDULED">Đã Xếp Lịch (Scheduled)</option>
          <option value="READY_TO_POST">Sẵn Sàng Đăng (Ready)</option>
          <option value="POSTING">Đang Đăng (Posting)</option>
          <option value="POSTED">Đã Đăng Thành Công (Posted)</option>
          <option value="AUTH_REQUIRED">Cần Đăng Nhập (Auth Required)</option>
          <option value="VERIFY_REQUIRED">Cần Xác Minh (Verify Required)</option>
          <option value="POST_ERROR">Lỗi Đăng Bài (Error)</option>
          <option value="CANCELLED">Đã Hủy (Cancelled)</option>
        </select>

        {/* Page Filter */}
        <select
          value={pageFilter}
          onChange={(e) => setPageFilter(e.target.value)}
          className="px-3 py-1.5 rounded-xl bg-black/40 border border-white/[0.08] text-xs text-zinc-300 focus:outline-none"
        >
          <option value="all">Tất cả Content Pages</option>
          {pages.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </div>

      {/* Publications Grid */}
      {loading ? (
        <div className="p-16 rounded-2xl bg-[#121622] border border-white/[0.08] text-center space-y-2">
          <RefreshCw className="h-8 w-8 text-indigo-400 animate-spin mx-auto" />
          <p className="text-xs text-zinc-400">Đang tải danh sách bài đăng...</p>
        </div>
      ) : publications.length === 0 ? (
        <div className="p-16 rounded-2xl bg-[#121622] border border-white/[0.08] text-center space-y-3">
          <Film className="h-10 w-10 text-zinc-600 mx-auto" />
          <h3 className="text-base font-bold text-white">Chưa có bài đăng nào phù hợp bộ lọc</h3>
          <p className="text-xs text-zinc-400 max-w-md mx-auto">
            Khi Video hoàn tất chu trình ở Studio với Page đã cấu hình Publish Targets, bản ghi xuất bản sẽ tự động hiển thị tại đây.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {publications.map((pub) => {
            const isLoading = actionLoading === pub.id;

            return (
              <div
                key={pub.id}
                className="rounded-2xl bg-[#121622] border border-white/[0.08] overflow-hidden flex flex-col justify-between shadow-xl transition hover:border-white/[0.15]"
              >
                {/* Header Badge */}
                <div className="p-3.5 border-b border-white/[0.06] bg-black/20 flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    {getPlatformIcon(pub.platform)}
                    <span className="text-xs font-bold text-white truncate max-w-[130px]">
                      {getPageName(pub.page_id)}
                    </span>
                  </div>
                  {getStatusBadge(pub.status)}
                </div>

                {/* Details & Video Meta */}
                <div className="p-4 space-y-3 text-xs flex-1">
                  {/* Title & Caption */}
                  <div>
                    <h4 className="font-bold text-white text-sm line-clamp-1 mb-1">
                      {pub.title || 'Floword Video Master'}
                    </h4>
                    <p className="text-zinc-300 line-clamp-3 bg-white/[0.02] p-2.5 rounded-xl border border-white/[0.04] text-[11px]">
                      {pub.caption || pub.description || 'Không có mô tả phụ đề.'}
                    </p>
                  </div>

                  {/* Browser Profile & Target */}
                  <div className="grid grid-cols-2 gap-2 text-[11px] bg-black/30 p-2.5 rounded-xl border border-white/[0.04]">
                    <div>
                      <span className="text-zinc-500 block text-[10px]">Browser Profile:</span>
                      <span className="font-mono text-zinc-300 truncate block">
                        {pub.browser_profile_id || 'Chưa gán profile'}
                      </span>
                    </div>
                    <div>
                      <span className="text-zinc-500 block text-[10px]">Attempts:</span>
                      <span className="font-mono text-zinc-300 block">
                        {pub.attempt_count} lần thử
                      </span>
                    </div>
                  </div>

                  {/* Scheduled info if available */}
                  {pub.scheduled_at && (
                    <div className="flex items-center gap-1.5 text-blue-300 bg-blue-500/10 p-2 rounded-lg border border-blue-500/20 text-[11px]">
                      <Clock className="h-3.5 w-3.5" />
                      <span>
                        Lên lịch lúc: <b>{new Date(pub.scheduled_at * 1000).toLocaleString()}</b>
                      </span>
                    </div>
                  )}

                  {/* Error display if failed */}
                  {pub.last_error_message && (
                    <div className="p-2.5 rounded-xl bg-red-500/10 border border-red-500/20 text-red-300 text-[11px] space-y-1">
                      <div className="flex items-center gap-1 font-bold">
                        <AlertTriangle className="h-3.5 w-3.5 text-red-400" />
                        <span>Mã lỗi: {pub.last_error_code || 'ERROR'}</span>
                      </div>
                      <p className="line-clamp-2">{pub.last_error_message}</p>
                    </div>
                  )}

                  {/* Posted link if success */}
                  {pub.status === 'POSTED' && pub.post_url && (
                    <a
                      href={pub.post_url}
                      target="_blank"
                      rel="noreferrer"
                      className="flex items-center justify-between p-2.5 rounded-xl bg-emerald-500/10 border border-emerald-500/30 text-emerald-300 hover:bg-emerald-500/20 transition font-bold text-xs"
                    >
                      <span className="flex items-center gap-1.5">
                        <ExternalLink className="h-3.5 w-3.5" /> Xem bài đăng trực tiếp
                      </span>
                      <span className="text-[10px] opacity-75">{pub.platform_post_id}</span>
                    </a>
                  )}

                  {/* Local video path */}
                  {pub.video_path && (
                    <div className="flex items-center justify-between bg-black/40 p-2 rounded-lg border border-white/[0.05] text-[10px]">
                      <span className="font-mono text-zinc-400 truncate max-w-[220px]">
                        {pub.video_path}
                      </span>
                      <button
                        type="button"
                        onClick={() => copyToClipboard(pub.video_path || '')}
                        className="p-1 rounded text-zinc-400 hover:text-white transition"
                        title="Sao chép đường dẫn"
                      >
                        <Copy className="h-3 w-3" />
                      </button>
                    </div>
                  )}
                </div>

                {/* Card Action Buttons */}
                <div className="p-3 border-t border-white/[0.06] bg-black/30 flex flex-wrap items-center justify-end gap-2">
                  {pub.status === 'WAITING_APPROVAL' && (
                    <>
                      <button
                        type="button"
                        disabled={isLoading}
                        onClick={() => handleReject(pub.id)}
                        className="px-3 py-1.5 rounded-lg text-xs font-semibold text-red-400 hover:bg-red-500/10 transition"
                      >
                        Từ chối
                      </button>
                      <button
                        type="button"
                        disabled={isLoading}
                        onClick={() => handleScheduleDefaultSlot(pub.id)}
                        className="px-3 py-1.5 rounded-lg text-xs font-semibold text-blue-300 bg-blue-600/20 hover:bg-blue-600/30 border border-blue-500/30 transition flex items-center gap-1"
                      >
                        <Calendar className="h-3 w-3" /> Xếp Lịch
                      </button>
                      <button
                        type="button"
                        disabled={isLoading}
                        onClick={() => handleApprove(pub.id)}
                        className="px-3.5 py-1.5 rounded-lg text-xs font-bold text-white bg-emerald-600 hover:bg-emerald-500 shadow-lg shadow-emerald-600/20 transition flex items-center gap-1"
                      >
                        <Check className="h-3 w-3" /> Phê Duyệt
                      </button>
                    </>
                  )}

                  {pub.status === 'SCHEDULED' && (
                    <>
                      <button
                        type="button"
                        disabled={isLoading}
                        onClick={() => handleReject(pub.id)}
                        className="px-3 py-1.5 rounded-lg text-xs font-semibold text-zinc-400 hover:bg-white/[0.05] transition"
                      >
                        Hủy Lịch
                      </button>
                      <button
                        type="button"
                        disabled={isLoading}
                        onClick={() => handlePostNow(pub.id)}
                        className="px-3.5 py-1.5 rounded-lg text-xs font-bold text-white bg-indigo-600 hover:bg-indigo-500 transition flex items-center gap-1"
                      >
                        <Send className="h-3 w-3" /> Đăng Ngay
                      </button>
                    </>
                  )}

                  {(pub.status === 'POST_ERROR' || pub.status === 'AUTH_REQUIRED' || pub.status === 'VERIFY_REQUIRED' || pub.status === 'CANCELLED') && (
                    <button
                      type="button"
                      disabled={isLoading}
                      onClick={() => handleRetry(pub.id)}
                      className="px-3.5 py-1.5 rounded-lg text-xs font-bold text-white bg-amber-600 hover:bg-amber-500 transition flex items-center gap-1"
                    >
                      <RefreshCw className={`h-3 w-3 ${isLoading ? 'animate-spin' : ''}`} /> Thử lại
                    </button>
                  )}

                  {pub.status === 'POSTING' && (
                    <div className="text-[11px] text-purple-300 flex items-center gap-1 py-1">
                      <RefreshCw className="h-3 w-3 animate-spin" /> Đang tiến hành đăng bài...
                    </div>
                  )}

                  {pub.status === 'READY_TO_POST' && (
                    <div className="text-[11px] text-emerald-400 flex items-center gap-1 py-1 font-medium">
                      <Clock className="h-3 w-3" /> Đang xếp hàng chờ Worker xử lý...
                    </div>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};
