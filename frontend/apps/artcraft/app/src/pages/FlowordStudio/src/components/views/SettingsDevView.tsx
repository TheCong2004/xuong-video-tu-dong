import React, { useState, useEffect } from 'react';
import {
  Activity,
  Boxes,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Code2,
  Cpu,
  Database,
  ExternalLink,
  Folder,
  HardDrive,
  Layers,
  Network,
  RefreshCw,
  Server,
  Settings,
  Shield,
  Sliders,
  Terminal,
  Zap,
  Monitor,
  Check,
} from 'lucide-react';
import toast from 'react-hot-toast';
import { DetailedReadinessStatus, BrowserWorkerInfo, listBrowserWorkers, getFlowordSettings, updateFlowordSettings } from '../../api/flowordClient';

interface SettingsDevViewProps {
  readiness: DetailedReadinessStatus;
  onRefreshReadiness: () => void;
}

export const SettingsDevView: React.FC<SettingsDevViewProps> = ({
  readiness,
  onRefreshReadiness,
}) => {
  const [activeTab, setActiveTab] = useState<'general' | 'workers' | 'engines' | 'storage' | 'diagnostics'>('general');
  const [maxConcurrency, setMaxConcurrency] = useState<number>(3);
  const [savingSettings, setSavingSettings] = useState<boolean>(false);
  const [workers, setWorkers] = useState<BrowserWorkerInfo[]>([]);
  const [loadingWorkers, setLoadingWorkers] = useState<boolean>(false);

  useEffect(() => {
    getFlowordSettings()
      .then((res) => setMaxConcurrency(res.max_concurrent_jobs))
      .catch(() => setMaxConcurrency(3));

    setLoadingWorkers(true);
    listBrowserWorkers()
      .then(setWorkers)
      .catch(() => setWorkers([]))
      .finally(() => setLoadingWorkers(false));
  }, []);

  const handleSaveConcurrency = async (val: number) => {
    setMaxConcurrency(val);
    setSavingSettings(true);
    try {
      await updateFlowordSettings(val);
      toast.success(`Đã lưu cấu hình Max Concurrency: ${val} Jobs đồng thời.`);
    } catch (err) {
      toast.error('Lỗi khi lưu cấu hình');
    } finally {
      setSavingSettings(false);
    }
  };

  return (
    <div className="max-w-7xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-white tracking-tight">Settings & System Architecture</h1>
          <p className="text-xs text-zinc-400">
            Authoritative Bounded Concurrency, Browser Worker Pools & System Diagnostics.
          </p>
        </div>

        <button
          type="button"
          onClick={() => {
            onRefreshReadiness();
            listBrowserWorkers().then(setWorkers).catch(() => {});
          }}
          className="flex items-center gap-2 px-3.5 py-2 rounded-xl bg-white/[0.04] hover:bg-white/[0.08] text-zinc-300 text-xs font-medium border border-white/[0.08] transition"
        >
          <RefreshCw className="h-3.5 w-3.5" />
          Test All Services
        </button>
      </div>

      {/* Tabs Navigation */}
      <div className="flex items-center gap-2 border-b border-white/[0.08] pb-2 text-xs font-medium overflow-x-auto">
        <button
          type="button"
          onClick={() => setActiveTab('general')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl transition ${
            activeTab === 'general'
              ? 'bg-white/[0.08] text-white font-semibold'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          <Settings className="h-4 w-4" />
          General & Concurrency
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('workers')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl transition ${
            activeTab === 'workers'
              ? 'bg-white/[0.08] text-white font-semibold'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          <Monitor className="h-4 w-4 text-blue-400" />
          Donut Browser Workers
        </button>

        <button
          type="button"
          onClick={() => setActiveTab('diagnostics')}
          className={`flex items-center gap-2 px-4 py-2 rounded-xl transition ${
            activeTab === 'diagnostics'
              ? 'bg-rose-500/20 text-rose-300 font-semibold border border-rose-500/30'
              : 'text-zinc-400 hover:text-zinc-200'
          }`}
        >
          <Code2 className="h-4 w-4 text-rose-400" />
          Readiness Probes
        </button>
      </div>

      {/* Tab Contents */}
      {activeTab === 'general' && (
        <div className="rounded-2xl bg-[#121622] border border-white/[0.08] p-6 space-y-5 text-xs">
          <h3 className="text-sm font-bold text-white">Production Scheduler Configuration</h3>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <div className="p-4 rounded-xl bg-white/[0.02] border border-white/[0.05] space-y-3">
              <div>
                <span className="font-semibold text-white">Bounded Concurrent Job Scheduler (P0)</span>
                <p className="text-zinc-400 text-[11px] mt-0.5">
                  Số lượng Job tối đa Rust Worker Thread được phép thực thi đồng thời trong Tokio async pool. Giá trị được lưu trực tiếp vào SQLite <code>app_settings</code>.
                </p>
              </div>

              <div className="flex items-center gap-3">
                <select
                  value={maxConcurrency}
                  onChange={(e) => handleSaveConcurrency(Number(e.target.value))}
                  disabled={savingSettings}
                  className="px-3 py-2 rounded-xl bg-[#171b26] border border-white/[0.1] text-white font-semibold text-xs focus:outline-none focus:border-rose-500"
                >
                  <option value={1}>1 Job (Serial execution)</option>
                  <option value={2}>2 Concurrent Jobs</option>
                  <option value={3}>3 Concurrent Jobs (Mặc định)</option>
                  <option value={5}>5 Concurrent Jobs (Khuyên dùng 16GB+ RAM)</option>
                  <option value={8}>8 Concurrent Jobs (High performance)</option>
                  <option value={10}>10 Concurrent Jobs</option>
                  <option value={15}>15 Concurrent Jobs</option>
                  <option value={20}>20 Concurrent Jobs (Max)</option>
                </select>

                {savingSettings && <span className="text-zinc-400 text-[11px]">Đang lưu...</span>}
              </div>
            </div>

            <div className="p-4 rounded-xl bg-white/[0.02] border border-white/[0.05] space-y-2">
              <span className="font-semibold text-white">Output Root Path Policy</span>
              <p className="text-zinc-400 text-[11px]">
                Quy tắc lưu trữ tự động của hệ thống:
              </p>
              <div className="p-2.5 rounded-lg bg-black/40 border border-white/[0.05] font-mono text-[10px] text-zinc-300">
                &lt;page.output_root&gt;\&lt;DD-MM-YYYY&gt;\&lt;filename&gt;
              </div>
              <p className="text-[10px] text-zinc-500">
                Không tạo lồng trùng lặp tên Page nếu output_root đã chứa thư mục tương ứng.
              </p>
            </div>
          </div>
        </div>
      )}

      {activeTab === 'workers' && (
        <div className="rounded-2xl bg-[#121622] border border-white/[0.08] p-6 space-y-5 text-xs">
          <div className="flex items-center justify-between">
            <div>
              <h3 className="text-sm font-bold text-white">Active Donut Browser Worker Profiles</h3>
              <p className="text-zinc-400 text-[11px]">Danh sách các browser worker đang chạy và kết nối với Donut Browser runtime.</p>
            </div>
            <span className="px-3 py-1 rounded-full text-xs font-semibold bg-blue-500/10 text-blue-400 border border-blue-500/20">
              {workers.length} Workers Trực tuyến
            </span>
          </div>

          <div className="space-y-3">
            {workers.length === 0 ? (
              <div className="p-8 rounded-xl bg-white/[0.02] border border-white/[0.05] text-center text-xs text-zinc-500">
                {loadingWorkers ? 'Đang truy vấn Donut Browser runtime...' : 'Không có browser worker nào trực tuyến. Hãy khởi động Donut Browser.'}
              </div>
            ) : (
              workers.map((w) => (
                <div key={w.worker_id} className="flex items-center justify-between p-3.5 rounded-xl bg-white/[0.02] border border-white/[0.05]">
                  <div className="flex items-center gap-3">
                    <span className={`h-2.5 w-2.5 rounded-full ${w.grok_logged_in ? 'bg-emerald-400' : 'bg-amber-400'}`} />
                    <div>
                      <div className="font-semibold text-white flex items-center gap-2">
                        {w.profile_name || w.profile_id || w.worker_id}
                        {w.grok_logged_in ? (
                          <span className="px-1.5 py-0.2 rounded text-[9px] bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">Grok Logged In</span>
                        ) : (
                          <span className="px-1.5 py-0.2 rounded text-[9px] bg-amber-500/10 text-amber-400 border border-amber-500/20">Auth Required</span>
                        )}
                      </div>
                      <div className="text-[10px] text-zinc-500 font-mono">
                        Profile ID: {w.profile_id || 'N/A'} &bull; Worker ID: {w.worker_id} &bull; State: {w.state}
                      </div>
                    </div>
                  </div>
                  <span className="text-[10px] font-mono px-2 py-1 rounded bg-white/[0.06] text-zinc-300">
                    {w.has_extension ? 'Extension Ready' : 'No Extension'}
                  </span>
                </div>
              ))
            )}
          </div>
        </div>
      )}

      {activeTab === 'diagnostics' && (
        <div className="rounded-2xl bg-[#121622] border border-white/[0.08] p-6 space-y-4 text-xs">
          <h3 className="text-sm font-bold text-white">Backend Readiness Probes</h3>

          <div className="space-y-2">
            {readiness.services.map((svc) => (
              <div key={svc.id} className="flex items-center justify-between p-3 rounded-xl bg-white/[0.02] border border-white/[0.05]">
                <div className="flex items-center gap-2.5">
                  <span className={`h-2 w-2 rounded-full ${
                    svc.status === 'ready' ? 'bg-emerald-400' : svc.status === 'auth_required' ? 'bg-amber-400' : 'bg-zinc-600'
                  }`} />
                  <div>
                    <span className="font-semibold text-white uppercase">{svc.id}</span>
                    {svc.message && <p className="text-[10px] text-zinc-500">{svc.message}</p>}
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  <span className="text-[10px] font-mono text-zinc-500">{svc.latency_ms}ms</span>
                  <span className={`px-2 py-0.5 rounded text-[10px] font-mono font-bold uppercase ${
                    svc.status === 'ready'
                      ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                      : svc.status === 'auth_required'
                      ? 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
                      : 'bg-zinc-800 text-zinc-500 border border-zinc-700'
                  }`}>
                    {svc.status}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
