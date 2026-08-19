import React, { useEffect, useState } from 'react';
import {
  checkSystemReadiness,
  SystemReadinessReport,
  listPromptTemplates,
  upsertPromptTemplate,
  deletePromptTemplate,
  PromptTemplate,
  getFlowordSystemSetting,
  updateFlowordSystemSetting,
  getFlowordSettings,
  updateFlowordSettings,
} from '../../api/flowordClient';
import {
  Settings, ShieldCheck, Activity, Cpu, Bell, BookmarkPlus,
  Trash2, Edit3, Plus, RefreshCw, CheckCircle2, AlertTriangle,
  Server, HardDrive, Database, Globe, Sliders, Check
} from 'lucide-react';

export const SettingsView: React.FC = () => {
  const [readiness, setReadiness] = useState<SystemReadinessReport | null>(null);
  const [probing, setProbing] = useState<boolean>(false);

  // Concurrency state
  const [maxConcurrentJobs, setMaxConcurrentJobs] = useState<number>(5);
  const [grokMaxConcurrent, setGrokMaxConcurrent] = useState<number>(3);
  const [savingConcurrency, setSavingConcurrency] = useState<boolean>(false);

  // Notification toggles
  const [notifyJobFailed, setNotifyJobFailed] = useState<boolean>(true);
  const [notifyAuthRequired, setNotifyAuthRequired] = useState<boolean>(true);
  const [notifyPostFailed, setNotifyPostFailed] = useState<boolean>(true);
  const [notifyBatchCompleted, setNotifyBatchCompleted] = useState<boolean>(true);
  const [savingNotifications, setSavingNotifications] = useState<boolean>(false);

  // Prompt Templates state
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [loadingTemplates, setLoadingTemplates] = useState<boolean>(false);
  const [editingTemplate, setEditingTemplate] = useState<Partial<PromptTemplate> | null>(null);

  const fetchReadiness = async () => {
    setProbing(true);
    try {
      const rep = await checkSystemReadiness();
      setReadiness(rep);
    } catch (e) {
      console.error('Failed to probe system readiness:', e);
    } finally {
      setProbing(false);
    }
  };

  const fetchSettingsAndTemplates = async () => {
    // 1. Fetch Concurrency
    try {
      const res = await getFlowordSettings();
      if (res?.max_concurrent_jobs) {
        setMaxConcurrentJobs(res.max_concurrent_jobs);
      }
    } catch {}

    // 2. Fetch Notifications from system settings table
    try {
      const notifSetting = await getFlowordSystemSetting('notifications');
      if (notifSetting?.value_json) {
        const parsed = JSON.parse(notifSetting.value_json);
        setNotifyJobFailed(parsed.notify_on_job_failed ?? true);
        setNotifyAuthRequired(parsed.notify_on_auth_required ?? true);
        setNotifyPostFailed(parsed.notify_on_post_failed ?? true);
        setNotifyBatchCompleted(parsed.notify_on_batch_completed ?? true);
      }
    } catch {}

    // 3. Fetch Templates
    setLoadingTemplates(true);
    try {
      const tpls = await listPromptTemplates();
      setTemplates(tpls);
    } catch (e) {
      console.error('Failed to load templates:', e);
    } finally {
      setLoadingTemplates(false);
    }
  };

  useEffect(() => {
    fetchReadiness();
    fetchSettingsAndTemplates();
  }, []);

  const [statusMessage, setStatusMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);

  const showStatus = (type: 'success' | 'error', text: string) => {
    setStatusMessage({ type, text });
    setTimeout(() => setStatusMessage(null), 4000);
  };

  const handleSaveConcurrency = async () => {
    setSavingConcurrency(true);
    try {
      await updateFlowordSettings(maxConcurrentJobs);
      await updateFlowordSystemSetting(
        'concurrency',
        JSON.stringify({
          global_concurrency: maxConcurrentJobs,
          grok_concurrency: grokMaxConcurrent,
        })
      );
      showStatus('success', 'Đã lưu cấu hình Concurrency vào Database!');
    } catch (e) {
      showStatus('error', `Lỗi lưu Concurrency: ${e}`);
    } finally {
      setSavingConcurrency(false);
    }
  };

  const handleSaveNotifications = async () => {
    setSavingNotifications(true);
    try {
      await updateFlowordSystemSetting(
        'notifications',
        JSON.stringify({
          notify_on_job_failed: notifyJobFailed,
          notify_on_auth_required: notifyAuthRequired,
          notify_on_post_failed: notifyPostFailed,
          notify_on_batch_completed: notifyBatchCompleted,
        })
      );
      showStatus('success', 'Đã lưu cấu hình Notifications vào Database!');
    } catch (e) {
      showStatus('error', `Lỗi lưu Notifications: ${e}`);
    } finally {
      setSavingNotifications(false);
    }
  };

  const handleSaveTemplate = async () => {
    if (!editingTemplate || !editingTemplate.name || !editingTemplate.video_prompt) {
      showStatus('error', 'Vui lòng nhập tên template và video prompt.');
      return;
    }
    try {
      await upsertPromptTemplate({
        id: editingTemplate.id,
        name: editingTemplate.name,
        image_prompt: editingTemplate.image_prompt || '',
        expand_prompt: editingTemplate.expand_prompt,
        video_prompt: editingTemplate.video_prompt,
      });
      setEditingTemplate(null);
      const tpls = await listPromptTemplates();
      setTemplates(tpls);
      showStatus('success', 'Đã lưu template thành công!');
    } catch (e) {
      showStatus('error', `Lỗi lưu template: ${e}`);
    }
  };

  const handleDeleteTemplate = async (id: string) => {
    if (!confirm('Bạn có chắc muốn xóa template này?')) return;
    try {
      await deletePromptTemplate(id);
      const tpls = await listPromptTemplates();
      setTemplates(tpls);
      showStatus('success', 'Đã xóa template thành công!');
    } catch (e) {
      showStatus('error', `Lỗi xóa template: ${e}`);
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full bg-[#0b0f17] text-slate-100 overflow-y-auto p-6 space-y-6">
      {/* Toast Notification */}
      {statusMessage && (
        <div
          className={`p-3 rounded-lg border text-xs font-semibold flex items-center justify-between transition-all ${
            statusMessage.type === 'success'
              ? 'bg-emerald-950/80 border-emerald-500/50 text-emerald-300'
              : 'bg-rose-950/80 border-rose-500/50 text-rose-300'
          }`}
        >
          <span>{statusMessage.text}</span>
          <button onClick={() => setStatusMessage(null)} className="text-slate-400 hover:text-white">✕</button>
        </div>
      )}

      {/* Top Header */}
      <div className="bg-[#131926] p-5 rounded-xl border border-slate-800/80 shadow-lg flex flex-col md:flex-row items-start md:items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-indigo-500/10 border border-indigo-500/30 flex items-center justify-center text-indigo-400">
            <Settings className="w-5 h-5" />
          </div>
          <div>
            <h1 className="text-xl font-bold text-white tracking-wide">System & Operations Settings</h1>
            <p className="text-xs text-slate-400">Quản lý Concurrency, Prompt Templates, Desktop Notifications & Real System Probes</p>
          </div>
        </div>

        <button
          onClick={fetchReadiness}
          disabled={probing}
          className="flex items-center gap-2 px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-bold shadow transition active:scale-95 disabled:opacity-50"
        >
          <RefreshCw className={`w-4 h-4 ${probing ? 'animate-spin' : ''}`} />
          <span>Kiểm tra Readiness Thực tế</span>
        </button>
      </div>

      {/* Real System Readiness Probes */}
      <div className="bg-[#131926] p-5 rounded-xl border border-slate-800 shadow space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <h2 className="text-sm font-bold text-white flex items-center gap-2">
              <ShieldCheck className="w-4 h-4 text-emerald-400" />
              <span>Authoritative System Readiness</span>
            </h2>
          </div>
          {readiness && (
            <div className="flex items-center gap-2 text-xs">
              <div className="px-2.5 py-1 rounded bg-black/40 border border-slate-800 flex items-center gap-1.5">
                <span className="text-slate-400 text-[11px]">Core Production:</span>
                <span className={`font-bold text-[11px] ${readiness.core_generation_ready ? 'text-emerald-400' : 'text-rose-400'}`}>
                  {readiness.core_generation_ready ? 'READY' : 'NOT READY'}
                </span>
              </div>
              <div className="px-2.5 py-1 rounded bg-black/40 border border-slate-800 flex items-center gap-1.5">
                <span className="text-slate-400 text-[11px]">Publishing Orchestrator:</span>
                <span className={`font-bold text-[11px] ${readiness.publishing_orchestrator_ready ? 'text-emerald-400' : 'text-rose-400'}`}>
                  {readiness.publishing_orchestrator_ready ? 'READY' : 'NOT READY'}
                </span>
              </div>
              <div className="px-2.5 py-1 rounded bg-black/40 border border-slate-800 flex items-center gap-1.5">
                <span className="text-slate-400 text-[11px]">System:</span>
                <span className={`font-bold text-[11px] ${readiness.overall_ready ? 'text-emerald-400' : 'text-amber-400'}`}>
                  {readiness.overall_ready ? 'READY' : 'DEGRADED'}
                </span>
              </div>
            </div>
          )}
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
          {readiness?.details?.map((probe, idx) => (
            <div key={idx} className="bg-[#0b0f17] p-3.5 rounded-lg border border-slate-800 flex flex-col justify-between gap-2">
              <div className="flex items-center justify-between">
                <span className="text-xs font-bold text-slate-300">{probe.service}</span>
                <span className={`px-2 py-0.5 rounded text-[10px] font-bold border ${probe.ready ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/30' : 'bg-rose-500/10 text-rose-400 border-rose-500/30'}`}>
                  {probe.ready ? 'ONLINE' : 'DEGRADED'}
                </span>
              </div>
              <p className="text-[11px] text-slate-400 line-clamp-2">{probe.message}</p>
              {probe.latency_ms != null && (
                <div className="text-[10px] text-slate-500 font-mono">
                  Latency: <strong className="text-emerald-400">{probe.latency_ms}ms</strong>
                </div>
              )}
            </div>
          ))}
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Scale & Concurrency Controller */}
        <div className="bg-[#131926] p-5 rounded-xl border border-slate-800 shadow space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-bold text-white flex items-center gap-2">
              <Cpu className="w-4 h-4 text-indigo-400" />
              <span>Cấu hình Tải & Concurrency (Backpressure)</span>
            </h2>
          </div>

          <div className="space-y-4 text-xs">
            <div>
              <label className="block text-slate-300 font-semibold mb-1">
                Global Pipeline Concurrency (Số Job chạy song song): <strong className="text-indigo-400">{maxConcurrentJobs}</strong>
              </label>
              <input
                type="range"
                min={1}
                max={20}
                value={maxConcurrentJobs}
                onChange={(e) => setMaxConcurrentJobs(Number(e.target.value))}
                className="w-full accent-indigo-500"
              />
              <span className="text-[11px] text-slate-500">Khuyến nghị: 3-10 jobs để tránh nghẽn RAM & GPU local</span>
            </div>

            <div>
              <label className="block text-slate-300 font-semibold mb-1">
                Grok Max Browser Profile Concurrency: <strong className="text-sky-400">{grokMaxConcurrent}</strong>
              </label>
              <input
                type="range"
                min={1}
                max={5}
                value={grokMaxConcurrent}
                onChange={(e) => setGrokMaxConcurrent(Number(e.target.value))}
                className="w-full accent-sky-500"
              />
              <span className="text-[11px] text-slate-500">Tối đa 5 profiles Grok hoạt động đồng thời</span>
            </div>

            <button
              onClick={handleSaveConcurrency}
              disabled={savingConcurrency}
              className="px-4 py-2 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white font-bold transition active:scale-95 disabled:opacity-50"
            >
              {savingConcurrency ? 'Đang lưu...' : 'Lưu Cấu hình Concurrency'}
            </button>
          </div>
        </div>

        {/* Notifications Setting */}
        <div className="bg-[#131926] p-5 rounded-xl border border-slate-800 shadow space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-bold text-white flex items-center gap-2">
              <Bell className="w-4 h-4 text-amber-400" />
              <span>Desktop Notifications (Thông báo Hệ thống)</span>
            </h2>
          </div>

          <div className="space-y-3 text-xs">
            <label className="flex items-center gap-2.5 cursor-pointer bg-[#0b0f17] p-2.5 rounded-lg border border-slate-800 hover:border-slate-700">
              <input
                type="checkbox"
                checked={notifyJobFailed}
                onChange={(e) => setNotifyJobFailed(e.target.checked)}
                className="rounded accent-indigo-500 w-4 h-4"
              />
              <span className="text-slate-300">Thông báo khi Job bị lỗi (Job Failed)</span>
            </label>

            <label className="flex items-center gap-2.5 cursor-pointer bg-[#0b0f17] p-2.5 rounded-lg border border-slate-800 hover:border-slate-700">
              <input
                type="checkbox"
                checked={notifyAuthRequired}
                onChange={(e) => setNotifyAuthRequired(e.target.checked)}
                className="rounded accent-indigo-500 w-4 h-4"
              />
              <span className="text-slate-300">Thông báo khi phiên đăng nhập hết hạn (Auth Required)</span>
            </label>

            <label className="flex items-center gap-2.5 cursor-pointer bg-[#0b0f17] p-2.5 rounded-lg border border-slate-800 hover:border-slate-700">
              <input
                type="checkbox"
                checked={notifyPostFailed}
                onChange={(e) => setNotifyPostFailed(e.target.checked)}
                className="rounded accent-indigo-500 w-4 h-4"
              />
              <span className="text-slate-300">Thông báo khi xuất bản mạng xã hội thất bại (Publish Failed)</span>
            </label>

            <label className="flex items-center gap-2.5 cursor-pointer bg-[#0b0f17] p-2.5 rounded-lg border border-slate-800 hover:border-slate-700">
              <input
                type="checkbox"
                checked={notifyBatchCompleted}
                onChange={(e) => setNotifyBatchCompleted(e.target.checked)}
                className="rounded accent-indigo-500 w-4 h-4"
              />
              <span className="text-slate-300">Thông báo khi hoàn tất toàn bộ Batch Import</span>
            </label>

            <button
              onClick={handleSaveNotifications}
              disabled={savingNotifications}
              className="px-4 py-2 rounded-lg bg-amber-600 hover:bg-amber-500 text-white font-bold transition active:scale-95 disabled:opacity-50"
            >
              {savingNotifications ? 'Đang lưu...' : 'Lưu Tùy chọn Thông báo'}
            </button>
          </div>
        </div>
      </div>

      {/* Prompt Templates Manager */}
      <div className="bg-[#131926] p-5 rounded-xl border border-slate-800 shadow space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-sm font-bold text-white flex items-center gap-2">
              <BookmarkPlus className="w-4 h-4 text-indigo-400" />
              <span>Prompt Templates Library (Snapshot Copy)</span>
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">Các mẫu prompt lưu sẵn, khi đưa vào Job sẽ copy snapshot độc lập</p>
          </div>

          <button
            onClick={() =>
              setEditingTemplate({
                name: 'Template mới',
                image_prompt: '',
                expand_prompt: '',
                video_prompt: '',
              })
            }
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-bold shadow transition"
          >
            <Plus className="w-3.5 h-3.5" />
            <span>Thêm Template Mới</span>
          </button>
        </div>

        {/* Template List */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
          {templates.map((tpl) => (
            <div key={tpl.id} className="bg-[#0b0f17] p-3.5 rounded-lg border border-slate-800 flex flex-col justify-between gap-2 text-xs">
              <div className="flex items-center justify-between">
                <span className="font-bold text-white text-sm">{tpl.name}</span>
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => setEditingTemplate(tpl)}
                    className="p-1 text-slate-400 hover:text-white"
                  >
                    <Edit3 className="w-3.5 h-3.5" />
                  </button>
                  <button
                    onClick={() => handleDeleteTemplate(tpl.id)}
                    className="p-1 text-rose-400 hover:text-rose-300"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>
              <p className="text-slate-400 font-mono text-[11px] truncate">Video: {tpl.video_prompt}</p>
              {tpl.image_prompt && <p className="text-slate-500 font-mono text-[11px] truncate">Image: {tpl.image_prompt}</p>}
            </div>
          ))}
        </div>
      </div>

      {/* Edit Template Modal */}
      {editingTemplate && (
        <div className="fixed inset-0 bg-black/70 backdrop-blur-sm z-50 flex items-center justify-center p-4">
          <div className="bg-[#131926] border border-slate-700 rounded-xl p-6 max-w-lg w-full space-y-4 text-xs shadow-2xl">
            <h3 className="text-sm font-bold text-white">
              {editingTemplate.id ? 'Chỉnh sửa Template' : 'Thêm Template Mới'}
            </h3>

            <div className="space-y-3">
              <div>
                <label className="block text-slate-400 mb-1">Tên Template</label>
                <input
                  type="text"
                  value={editingTemplate.name || ''}
                  onChange={(e) => setEditingTemplate({ ...editingTemplate, name: e.target.value })}
                  className="w-full bg-[#0b0f17] text-slate-200 p-2 rounded border border-slate-700 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <label className="block text-slate-400 mb-1">Image Prompt</label>
                <textarea
                  rows={2}
                  value={editingTemplate.image_prompt || ''}
                  onChange={(e) => setEditingTemplate({ ...editingTemplate, image_prompt: e.target.value })}
                  className="w-full bg-[#0b0f17] text-slate-200 p-2 rounded border border-slate-700 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <label className="block text-slate-400 mb-1">Expand 9:16 Prompt</label>
                <textarea
                  rows={2}
                  value={editingTemplate.expand_prompt || ''}
                  onChange={(e) => setEditingTemplate({ ...editingTemplate, expand_prompt: e.target.value })}
                  className="w-full bg-[#0b0f17] text-slate-200 p-2 rounded border border-slate-700 focus:outline-none focus:border-indigo-500"
                />
              </div>

              <div>
                <label className="block text-slate-400 mb-1">Video Prompt *</label>
                <textarea
                  rows={3}
                  value={editingTemplate.video_prompt || ''}
                  onChange={(e) => setEditingTemplate({ ...editingTemplate, video_prompt: e.target.value })}
                  className="w-full bg-[#0b0f17] text-slate-200 p-2 rounded border border-slate-700 focus:outline-none focus:border-indigo-500"
                />
              </div>
            </div>

            <div className="flex justify-end gap-2 pt-2 border-t border-slate-800">
              <button
                onClick={() => setEditingTemplate(null)}
                className="px-4 py-2 rounded bg-slate-800 text-slate-300 hover:bg-slate-700"
              >
                Hủy
              </button>
              <button
                onClick={handleSaveTemplate}
                className="px-4 py-2 rounded bg-indigo-600 text-white font-bold hover:bg-indigo-500"
              >
                Lưu Template
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
