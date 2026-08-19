import React, { useState, useEffect } from 'react';
import { X, Folder, Layers, Check, Trash2, Monitor, Share2, Plus, Clock } from 'lucide-react';
import toast from 'react-hot-toast';
import {
  ContentPage,
  CreateContentPageRequest,
  UpdateContentPageRequest,
  BrowserWorkerInfo,
  listBrowserWorkers,
  ContentPagePublishTarget,
  listContentPagePublishTargets,
  upsertContentPagePublishTarget,
} from '../api/flowordClient';

interface PageManagementModalProps {
  isOpen: boolean;
  onClose: () => void;
  pageToEdit?: ContentPage | null;
  onSavePage: (data: CreateContentPageRequest | UpdateContentPageRequest) => Promise<ContentPage>;
  onArchivePage?: (pageId: string) => Promise<void>;
}

export const PageManagementModal: React.FC<PageManagementModalProps> = ({
  isOpen,
  onClose,
  pageToEdit,
  onSavePage,
  onArchivePage,
}) => {
  const [name, setName] = useState('');
  const [outputRoot, setOutputRoot] = useState('D:\\');
  const [targetPlatform, setTargetPlatform] = useState('tiktok');
  const [defaultLanguage, setDefaultLanguage] = useState('vi');
  const [defaultTone, setDefaultTone] = useState('professional');
  const [defaultAspectRatio, setDefaultAspectRatio] = useState('9:16');
  const [browserProfileId, setBrowserProfileId] = useState('');
  const [defaultImagePrompt, setDefaultImagePrompt] = useState('');
  const [defaultExpand916Prompt, setDefaultExpand916Prompt] = useState('');
  const [defaultVideoPrompt, setDefaultVideoPrompt] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [workers, setWorkers] = useState<BrowserWorkerInfo[]>([]);

  // Platform Publish Targets State
  const [fbEnabled, setFbEnabled] = useState(true);
  const [fbProfile, setFbProfile] = useState('');
  const [fbDestination, setFbDestination] = useState('');
  const [fbMode, setFbMode] = useState<'auto' | 'review'>('review');
  const [fbSlots, setFbSlots] = useState('08:30, 10:00, 17:00, 22:00');

  const [ttEnabled, setTtEnabled] = useState(true);
  const [ttProfile, setTtProfile] = useState('');
  const [ttDestination, setTtDestination] = useState('');
  const [ttMode, setTtMode] = useState<'auto' | 'review'>('review');
  const [ttSlots, setTtSlots] = useState('08:30, 10:00, 17:00, 22:00');

  const [ytEnabled, setYtEnabled] = useState(false);
  const [ytProfile, setYtProfile] = useState('');
  const [ytDestination, setYtDestination] = useState('');
  const [ytMode, setYtMode] = useState<'auto' | 'review'>('review');
  const [ytSlots, setYtSlots] = useState('08:30, 10:00, 17:00, 22:00');

  useEffect(() => {
    if (isOpen) {
      listBrowserWorkers()
        .then(setWorkers)
        .catch(() => setWorkers([]));
    }
  }, [isOpen]);

  useEffect(() => {
    if (pageToEdit && isOpen) {
      setName(pageToEdit.name);
      setOutputRoot(pageToEdit.output_root);
      setTargetPlatform(pageToEdit.target_platform || 'tiktok');
      setDefaultLanguage(pageToEdit.default_language || 'vi');
      setDefaultTone(pageToEdit.default_tone || 'professional');
      setDefaultAspectRatio(pageToEdit.default_aspect_ratio || '9:16');
      setBrowserProfileId(pageToEdit.browser_profile_id || '');
      setDefaultImagePrompt(pageToEdit.default_image_prompt || '');
      setDefaultExpand916Prompt(pageToEdit.default_expand_9_16_prompt || '');
      setDefaultVideoPrompt(pageToEdit.default_video_prompt || '');

      // Load existing publish targets
      listContentPagePublishTargets(pageToEdit.id).then((targets) => {
        const fb = targets.find((t) => t.platform === 'facebook');
        if (fb) {
          setFbEnabled(fb.enabled);
          setFbProfile(fb.browser_profile_id);
          setFbDestination(fb.destination_id);
          setFbMode(fb.post_mode as any);
          try {
            setFbSlots(JSON.parse(fb.default_slots_json).join(', '));
          } catch {
            setFbSlots(fb.default_slots_json);
          }
        }
        const tt = targets.find((t) => t.platform === 'tiktok');
        if (tt) {
          setTtEnabled(tt.enabled);
          setTtProfile(tt.browser_profile_id);
          setTtDestination(tt.destination_id);
          setTtMode(tt.post_mode as any);
          try {
            setTtSlots(JSON.parse(tt.default_slots_json).join(', '));
          } catch {
            setTtSlots(tt.default_slots_json);
          }
        }
        const yt = targets.find((t) => t.platform === 'youtube');
        if (yt) {
          setYtEnabled(yt.enabled);
          setYtProfile(yt.browser_profile_id);
          setYtDestination(yt.destination_id);
          setYtMode(yt.post_mode as any);
          try {
            setYtSlots(JSON.parse(yt.default_slots_json).join(', '));
          } catch {
            setYtSlots(yt.default_slots_json);
          }
        }
      }).catch(() => {});
    } else {
      setName('');
      setOutputRoot('D:\\');
      setTargetPlatform('tiktok');
      setDefaultLanguage('vi');
      setDefaultTone('professional');
      setDefaultAspectRatio('9:16');
      setBrowserProfileId('');
      setDefaultImagePrompt('');
      setDefaultExpand916Prompt('');
      setDefaultVideoPrompt('');
      setFbEnabled(true);
      setFbProfile('');
      setFbDestination('');
      setFbMode('review');
      setTtEnabled(true);
      setTtProfile('');
      setTtDestination('');
      setTtMode('review');
      setYtEnabled(false);
      setYtProfile('');
      setYtDestination('');
      setYtMode('review');
    }
    setError(null);
  }, [pageToEdit, isOpen]);

  if (!isOpen) return null;

  const parseSlots = (slotsStr: string): string => {
    const list = slotsStr.split(',').map((s) => s.trim()).filter(Boolean);
    return JSON.stringify(list.length > 0 ? list : ['08:30', '10:00', '17:00', '22:00']);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError('Page name cannot be empty');
      return;
    }
    if (!outputRoot.trim()) {
      setError('Output root path cannot be empty');
      return;
    }

    // Validate target browser profiles: no fake "default"
    if (fbEnabled) {
      const fbEffectiveProfile = fbProfile.trim() || browserProfileId.trim();
      if (!fbEffectiveProfile) {
        setError('Facebook publish target requires a browser profile to be selected');
        return;
      }
    }
    if (ttEnabled) {
      const ttEffectiveProfile = ttProfile.trim() || browserProfileId.trim();
      if (!ttEffectiveProfile) {
        setError('TikTok publish target requires a browser profile to be selected');
        return;
      }
    }
    if (ytEnabled) {
      const ytEffectiveProfile = ytProfile.trim() || browserProfileId.trim();
      if (!ytEffectiveProfile) {
        setError('YouTube publish target requires a browser profile to be selected');
        return;
      }
    }

    setSaving(true);
    setError(null);
    try {
      const savedPage: ContentPage = pageToEdit
        ? await onSavePage({
            id: pageToEdit.id,
            name: name.trim(),
            output_root: outputRoot.trim(),
            target_platform: targetPlatform,
            default_language: defaultLanguage,
            default_tone: defaultTone,
            default_aspect_ratio: defaultAspectRatio,
            browser_profile_id: browserProfileId.trim() || undefined,
            default_image_prompt: defaultImagePrompt.trim() || undefined,
            default_expand_9_16_prompt: defaultExpand916Prompt.trim() || undefined,
            default_video_prompt: defaultVideoPrompt.trim() || undefined,
          })
        : await onSavePage({
            name: name.trim(),
            output_root: outputRoot.trim(),
            target_platform: targetPlatform,
            default_language: defaultLanguage,
            default_tone: defaultTone,
            default_aspect_ratio: defaultAspectRatio,
            browser_profile_id: browserProfileId.trim() || undefined,
            default_image_prompt: defaultImagePrompt.trim() || undefined,
            default_expand_9_16_prompt: defaultExpand916Prompt.trim() || undefined,
            default_video_prompt: defaultVideoPrompt.trim() || undefined,
          });

      const savedPageId = savedPage.id;

      // Upsert publish targets using the authoritative saved page id
      try {
        if (fbEnabled || fbDestination.trim()) {
          const profile = fbProfile.trim() || browserProfileId.trim();
          if (profile) {
            await upsertContentPagePublishTarget({
              page_id: savedPageId,
              platform: 'facebook',
              enabled: fbEnabled,
              destination_id: fbDestination.trim() || savedPageId,
              browser_profile_id: profile,
              post_mode: fbMode,
              default_slots_json: parseSlots(fbSlots),
            });
          }
        }
        if (ttEnabled || ttDestination.trim()) {
          const profile = ttProfile.trim() || browserProfileId.trim();
          if (profile) {
            await upsertContentPagePublishTarget({
              page_id: savedPageId,
              platform: 'tiktok',
              enabled: ttEnabled,
              destination_id: ttDestination.trim() || savedPageId,
              browser_profile_id: profile,
              post_mode: ttMode,
              default_slots_json: parseSlots(ttSlots),
            });
          }
        }
        if (ytEnabled || ytDestination.trim()) {
          const profile = ytProfile.trim() || browserProfileId.trim();
          if (profile) {
            await upsertContentPagePublishTarget({
              page_id: savedPageId,
              platform: 'youtube',
              enabled: ytEnabled,
              destination_id: ytDestination.trim() || savedPageId,
              browser_profile_id: profile,
              post_mode: ytMode,
              default_slots_json: parseSlots(ytSlots),
            });
          }
        }
      } catch (targetErr) {
        const tMsg = targetErr instanceof Error ? targetErr.message : String(targetErr);
        toast.error(`Page đã lưu (id: ${savedPageId}), nhưng lỗi lưu publish target: ${tMsg}`);
        setError(`Page saved, but publish target failed: ${tMsg}`);
        return;
      }

      toast.success('Đã lưu cấu hình Page và Publish Targets thành công!');
      onClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleArchive = async () => {
    if (!pageToEdit || !onArchivePage) return;
    if (!window.confirm(`Are you sure you want to archive page "${pageToEdit.name}"? Existing jobs and outputs will be preserved.`)) {
      return;
    }
    setSaving(true);
    try {
      await onArchivePage(pageToEdit.id);
      onClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4 overflow-y-auto">
      <div className="w-full max-w-2xl max-h-[92vh] flex flex-col rounded-2xl border border-white/[0.1] bg-[#12161f] shadow-2xl overflow-hidden">
        <div className="flex items-center justify-between border-b border-white/[0.08] px-5 py-4 shrink-0">
          <div className="flex items-center gap-2 text-white font-semibold text-base">
            <Layers className="h-5 w-5 text-indigo-400" />
            {pageToEdit ? 'Edit Content Page & Publish Targets' : 'Create New Content Page'}
          </div>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-zinc-400 hover:bg-white/[0.05] hover:text-white transition"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="p-5 space-y-5 overflow-y-auto flex-1 text-xs">
          {error && (
            <div className="rounded-lg bg-red-500/10 border border-red-500/30 p-3 text-xs text-red-400">
              {error}
            </div>
          )}

          {/* BASIC PAGE SETTINGS */}
          <div className="space-y-3">
            <div>
              <label className="block text-xs font-medium text-zinc-300 mb-1">
                Page Name <span className="text-red-400">*</span>
              </label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Stage & Screen Feed, Hollywood Flash"
                className="w-full rounded-lg border border-white/[0.1] bg-black/40 px-3 py-2 text-sm text-white placeholder-zinc-500 focus:border-indigo-500 focus:outline-none"
                required
              />
            </div>

            <div>
              <label className="block text-xs font-medium text-zinc-300 mb-1">
                Output Root Directory <span className="text-red-400">*</span>
              </label>
              <div className="relative">
                <input
                  type="text"
                  value={outputRoot}
                  onChange={(e) => setOutputRoot(e.target.value)}
                  placeholder="e.g. D:\ or D:\FlowordOutputs"
                  className="w-full rounded-lg border border-white/[0.1] bg-black/40 pl-9 pr-3 py-2 text-sm text-white placeholder-zinc-500 focus:border-indigo-500 focus:outline-none font-mono text-xs"
                  required
                />
                <Folder className="absolute left-3 top-2.5 h-4 w-4 text-zinc-500" />
              </div>
              <p className="mt-1 text-[11px] text-zinc-400">
                Layout: <code className="text-indigo-300">{outputRoot.trim() || 'D:\\'}\{name.trim() || '&lt;PageName&gt;'}\&lt;DD-MM-YYYY&gt;\&lt;filename&gt;</code>
              </p>
            </div>
          </div>

          {/* PUBLISH TARGETS CONFIGURATION */}
          <div className="border border-white/[0.08] rounded-xl p-4 bg-white/[0.02] space-y-4">
            <div className="text-xs font-bold text-emerald-400 uppercase tracking-wider flex items-center gap-1.5">
              <Share2 className="h-4 w-4" />
              Multi-Platform Publish Targets
            </div>

            {/* Facebook Target */}
            <div className="p-3 rounded-xl bg-black/30 border border-white/[0.05] space-y-2">
              <div className="flex items-center justify-between">
                <label className="flex items-center gap-2 font-bold text-zinc-200">
                  <input
                    type="checkbox"
                    checked={fbEnabled}
                    onChange={(e) => setFbEnabled(e.target.checked)}
                    className="h-4 w-4 rounded accent-indigo-500"
                  />
                  Facebook Reels / Page Video
                </label>
                <select
                  value={fbMode}
                  onChange={(e) => setFbMode(e.target.value as any)}
                  className="px-2 py-1 rounded bg-[#1a202c] border border-white/[0.1] text-zinc-300 text-[11px]"
                >
                  <option value="review">Review Before Post</option>
                  <option value="auto">Auto-Post Immediately</option>
                </select>
              </div>

              <div className="grid grid-cols-2 gap-2 pt-1">
                <div>
                  <span className="text-[10px] text-zinc-500 block mb-0.5">Target Page ID / Destination</span>
                  <input
                    type="text"
                    value={fbDestination}
                    onChange={(e) => setFbDestination(e.target.value)}
                    placeholder="e.g. 104829384918239 or Page Name"
                    className="w-full px-2.5 py-1.5 rounded-lg bg-black/40 border border-white/[0.1] text-white text-xs font-mono"
                  />
                </div>
                <div>
                  <span className="text-[10px] text-zinc-500 block mb-0.5">Browser Profile</span>
                  <select
                    value={fbProfile}
                    onChange={(e) => setFbProfile(e.target.value)}
                    className="w-full px-2.5 py-1.5 rounded-lg bg-[#1a202c] border border-white/[0.1] text-white text-xs font-mono"
                  >
                    <option value="">-- Chưa gán Profile --</option>
                    {workers.map((w) => (
                      <option key={w.worker_id} value={w.profile_id || w.worker_id}>
                        {w.profile_name || w.profile_id || w.worker_id}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            </div>

            {/* TikTok Target */}
            <div className="p-3 rounded-xl bg-black/30 border border-white/[0.05] space-y-2">
              <div className="flex items-center justify-between">
                <label className="flex items-center gap-2 font-bold text-zinc-200">
                  <input
                    type="checkbox"
                    checked={ttEnabled}
                    onChange={(e) => setTtEnabled(e.target.checked)}
                    className="h-4 w-4 rounded accent-indigo-500"
                  />
                  TikTok Video Publish
                </label>
                <select
                  value={ttMode}
                  onChange={(e) => setTtMode(e.target.value as any)}
                  className="px-2 py-1 rounded bg-[#1a202c] border border-white/[0.1] text-zinc-300 text-[11px]"
                >
                  <option value="review">Review Before Post</option>
                  <option value="auto">Auto-Post Immediately</option>
                </select>
              </div>

              <div className="grid grid-cols-2 gap-2 pt-1">
                <div>
                  <span className="text-[10px] text-zinc-500 block mb-0.5">TikTok Handle / Account</span>
                  <input
                    type="text"
                    value={ttDestination}
                    onChange={(e) => setTtDestination(e.target.value)}
                    placeholder="e.g. @movie_cinema_vn"
                    className="w-full px-2.5 py-1.5 rounded-lg bg-black/40 border border-white/[0.1] text-white text-xs font-mono"
                  />
                </div>
                <div>
                  <span className="text-[10px] text-zinc-500 block mb-0.5">Browser Profile</span>
                  <select
                    value={ttProfile}
                    onChange={(e) => setTtProfile(e.target.value)}
                    className="w-full px-2.5 py-1.5 rounded-lg bg-[#1a202c] border border-white/[0.1] text-white text-xs font-mono"
                  >
                    <option value="">-- Chưa gán Profile --</option>
                    {workers.map((w) => (
                      <option key={w.worker_id} value={w.profile_id || w.worker_id}>
                        {w.profile_name || w.profile_id || w.worker_id}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            </div>

            {/* YouTube Target */}
            <div className="p-3 rounded-xl bg-black/30 border border-white/[0.05] space-y-2">
              <div className="flex items-center justify-between">
                <label className="flex items-center gap-2 font-bold text-zinc-200">
                  <input
                    type="checkbox"
                    checked={ytEnabled}
                    onChange={(e) => setYtEnabled(e.target.checked)}
                    className="h-4 w-4 rounded accent-indigo-500"
                  />
                  YouTube Shorts
                </label>
                <select
                  value={ytMode}
                  onChange={(e) => setYtMode(e.target.value as any)}
                  className="px-2 py-1 rounded bg-[#1a202c] border border-white/[0.1] text-zinc-300 text-[11px]"
                >
                  <option value="review">Review Before Post</option>
                  <option value="auto">Auto-Post Immediately</option>
                </select>
              </div>

              <div className="grid grid-cols-2 gap-2 pt-1">
                <div>
                  <span className="text-[10px] text-zinc-500 block mb-0.5">Channel ID / Name</span>
                  <input
                    type="text"
                    value={ytDestination}
                    onChange={(e) => setYtDestination(e.target.value)}
                    placeholder="e.g. UCxxxxxx or Channel Name"
                    className="w-full px-2.5 py-1.5 rounded-lg bg-black/40 border border-white/[0.1] text-white text-xs font-mono"
                  />
                </div>
                <div>
                  <span className="text-[10px] text-zinc-500 block mb-0.5">Browser Profile</span>
                  <select
                    value={ytProfile}
                    onChange={(e) => setYtProfile(e.target.value)}
                    className="w-full px-2.5 py-1.5 rounded-lg bg-[#1a202c] border border-white/[0.1] text-white text-xs font-mono"
                  >
                    <option value="">-- Chưa gán Profile --</option>
                    {workers.map((w) => (
                      <option key={w.worker_id} value={w.profile_id || w.worker_id}>
                        {w.profile_name || w.profile_id || w.worker_id}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            </div>
          </div>

          {/* DEFAULT PROMPT TEMPLATES */}
          <div className="border-t border-white/[0.08] pt-3 space-y-3">
            <div className="text-xs font-semibold text-indigo-300">Grok Pipeline Default Prompts</div>

            <div>
              <label className="block text-[11px] text-zinc-400 mb-1">Default Image Edit Prompt</label>
              <textarea
                rows={2}
                value={defaultImagePrompt}
                onChange={(e) => setDefaultImagePrompt(e.target.value)}
                placeholder="Default fallback prompt when creating/editing Grok images for this page"
                className="w-full rounded-lg border border-white/[0.1] bg-black/40 px-3 py-1.5 text-xs text-white placeholder-zinc-600 focus:border-indigo-500 focus:outline-none resize-none font-mono"
              />
            </div>

            <div>
              <label className="block text-[11px] text-zinc-400 mb-1">Default 9:16 Outpaint Prompt</label>
              <textarea
                rows={2}
                value={defaultExpand916Prompt}
                onChange={(e) => setDefaultExpand916Prompt(e.target.value)}
                placeholder="Default fallback for 9:16 vertical expansion"
                className="w-full rounded-lg border border-white/[0.1] bg-black/40 px-3 py-1.5 text-xs text-white placeholder-zinc-600 focus:border-indigo-500 focus:outline-none resize-none font-mono"
              />
            </div>

            <div>
              <label className="block text-[11px] text-zinc-400 mb-1">Default Video Animation Prompt</label>
              <textarea
                rows={2}
                value={defaultVideoPrompt}
                onChange={(e) => setDefaultVideoPrompt(e.target.value)}
                placeholder="Default fallback for Grok video animation"
                className="w-full rounded-lg border border-white/[0.1] bg-black/40 px-3 py-1.5 text-xs text-white placeholder-zinc-600 focus:border-indigo-500 focus:outline-none resize-none font-mono"
              />
            </div>
          </div>

          {/* MODAL FOOTER */}
          <div className="flex items-center justify-between pt-3 border-t border-white/[0.08]">
            {pageToEdit && onArchivePage ? (
              <button
                type="button"
                onClick={handleArchive}
                disabled={saving}
                className="flex items-center gap-1.5 px-3 py-2 text-xs font-medium text-red-400 hover:text-red-300 hover:bg-red-500/10 rounded-lg transition"
              >
                <Trash2 className="h-3.5 w-3.5" />
                Archive Page
              </button>
            ) : (
              <div />
            )}

            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={onClose}
                disabled={saving}
                className="px-4 py-2 text-xs font-medium text-zinc-300 hover:bg-white/[0.05] rounded-lg transition"
              >
                Cancel
              </button>
              <button
                type="submit"
                disabled={saving}
                className="flex items-center gap-1.5 px-5 py-2 text-xs font-semibold text-white bg-indigo-600 hover:bg-indigo-500 rounded-lg shadow-lg shadow-indigo-600/30 transition disabled:opacity-50"
              >
                <Check className="h-3.5 w-3.5" />
                {saving ? 'Saving...' : pageToEdit ? 'Save Changes' : 'Create Page'}
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>
  );
};
