import React, { useState, useEffect } from 'react';
import { X, Folder, Layers, Check, Trash2 } from 'lucide-react';
import { ContentPage, CreateContentPageRequest, UpdateContentPageRequest } from '../api/flowordClient';

interface PageManagementModalProps {
  isOpen: boolean;
  onClose: () => void;
  pageToEdit?: ContentPage | null;
  onSavePage: (data: CreateContentPageRequest | UpdateContentPageRequest) => Promise<void>;
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
  const [defaultImagePrompt, setDefaultImagePrompt] = useState('');
  const [defaultExpand916Prompt, setDefaultExpand916Prompt] = useState('');
  const [defaultVideoPrompt, setDefaultVideoPrompt] = useState('');
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (pageToEdit) {
      setName(pageToEdit.name);
      setOutputRoot(pageToEdit.output_root);
      setTargetPlatform(pageToEdit.target_platform || 'tiktok');
      setDefaultLanguage(pageToEdit.default_language || 'vi');
      setDefaultTone(pageToEdit.default_tone || 'professional');
      setDefaultAspectRatio(pageToEdit.default_aspect_ratio || '9:16');
      setDefaultImagePrompt(pageToEdit.default_image_prompt || '');
      setDefaultExpand916Prompt(pageToEdit.default_expand_9_16_prompt || '');
      setDefaultVideoPrompt(pageToEdit.default_video_prompt || '');
    } else {
      setName('');
      setOutputRoot('D:\\');
      setTargetPlatform('tiktok');
      setDefaultLanguage('vi');
      setDefaultTone('professional');
      setDefaultAspectRatio('9:16');
      setDefaultImagePrompt('');
      setDefaultExpand916Prompt('');
      setDefaultVideoPrompt('');
    }
    setError(null);
  }, [pageToEdit, isOpen]);

  if (!isOpen) return null;

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

    setSaving(true);
    setError(null);
    try {
      if (pageToEdit) {
        await onSavePage({
          id: pageToEdit.id,
          name: name.trim(),
          output_root: outputRoot.trim(),
          target_platform: targetPlatform,
          default_language: defaultLanguage,
          default_tone: defaultTone,
          default_aspect_ratio: defaultAspectRatio,
          default_image_prompt: defaultImagePrompt.trim() || undefined,
          default_expand_9_16_prompt: defaultExpand916Prompt.trim() || undefined,
          default_video_prompt: defaultVideoPrompt.trim() || undefined,
        });
      } else {
        await onSavePage({
          name: name.trim(),
          output_root: outputRoot.trim(),
          target_platform: targetPlatform,
          default_language: defaultLanguage,
          default_tone: defaultTone,
          default_aspect_ratio: defaultAspectRatio,
          default_image_prompt: defaultImagePrompt.trim() || undefined,
          default_expand_9_16_prompt: defaultExpand916Prompt.trim() || undefined,
          default_video_prompt: defaultVideoPrompt.trim() || undefined,
        });
      }
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
      <div className="w-full max-w-lg max-h-[90vh] flex flex-col rounded-xl border border-white/[0.1] bg-[#12161f] shadow-2xl overflow-hidden">
        <div className="flex items-center justify-between border-b border-white/[0.08] px-5 py-4 shrink-0">
          <div className="flex items-center gap-2 text-white font-semibold text-base">
            <Layers className="h-5 w-5 text-indigo-400" />
            {pageToEdit ? 'Edit Content Page' : 'Create New Content Page'}
          </div>
          <button
            onClick={onClose}
            className="rounded-lg p-1 text-zinc-400 hover:bg-white/[0.05] hover:text-white transition"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="p-5 space-y-4 overflow-y-auto flex-1 text-xs">
          {error && (
            <div className="rounded-lg bg-red-500/10 border border-red-500/30 p-3 text-xs text-red-400">
              {error}
            </div>
          )}

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
                className="w-full rounded-lg border border-white/[0.1] bg-black/40 pl-9 pr-3 py-2 text-sm text-white placeholder-zinc-500 focus:border-indigo-500 focus:outline-none"
                required
              />
              <Folder className="absolute left-3 top-2.5 h-4 w-4 text-zinc-500" />
            </div>
            <p className="mt-1 text-[11px] text-zinc-400">
              Final outputs will automatically save to: <code className="text-indigo-300">{outputRoot.trim() ? outputRoot.trim() : 'D:\\'}\{name.trim() || '&lt;PageName&gt;'}\&lt;DD-MM-YYYY&gt;\</code>
            </p>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-zinc-300 mb-1">Target Platform</label>
              <select
                value={targetPlatform}
                onChange={(e) => setTargetPlatform(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#1a202c] px-3 py-2 text-sm text-white focus:border-indigo-500 focus:outline-none"
              >
                <option value="tiktok">TikTok</option>
                <option value="reels">Instagram Reels</option>
                <option value="youtube_shorts">YouTube Shorts</option>
              </select>
            </div>

            <div>
              <label className="block text-xs font-medium text-zinc-300 mb-1">Default Aspect Ratio</label>
              <select
                value={defaultAspectRatio}
                onChange={(e) => setDefaultAspectRatio(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#1a202c] px-3 py-2 text-sm text-white focus:border-indigo-500 focus:outline-none"
              >
                <option value="9:16">9:16 (Vertical)</option>
                <option value="16:9">16:9 (Horizontal)</option>
                <option value="1:1">1:1 (Square)</option>
              </select>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-zinc-300 mb-1">Default Language</label>
              <select
                value={defaultLanguage}
                onChange={(e) => setDefaultLanguage(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#1a202c] px-3 py-2 text-sm text-white focus:border-indigo-500 focus:outline-none"
              >
                <option value="vi">Tiếng Việt (vi)</option>
                <option value="en">English (en)</option>
                <option value="zh">Chinese (zh)</option>
              </select>
            </div>

            <div>
              <label className="block text-xs font-medium text-zinc-300 mb-1">Default Tone</label>
              <select
                value={defaultTone}
                onChange={(e) => setDefaultTone(e.target.value)}
                className="w-full rounded-lg border border-white/[0.1] bg-[#1a202c] px-3 py-2 text-sm text-white focus:border-indigo-500 focus:outline-none"
              >
                <option value="professional">Professional</option>
                <option value="storytelling">Storytelling</option>
                <option value="educational">Educational</option>
                <option value="review">Review</option>
                <option value="viral">Viral</option>
              </select>
            </div>
          </div>

          <div className="border-t border-white/[0.08] pt-3 space-y-3">
            <div className="text-xs font-semibold text-indigo-300">Grok Pipeline Default Prompts</div>

            <div>
              <label className="block text-[11px] text-zinc-400 mb-1">Default Image Prompt</label>
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
                className="flex items-center gap-1.5 px-4 py-2 text-xs font-medium text-white bg-indigo-600 hover:bg-indigo-500 rounded-lg shadow-lg shadow-indigo-600/30 transition disabled:opacity-50"
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
