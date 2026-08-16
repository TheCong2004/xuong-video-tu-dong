import React, { useState } from 'react';
import {
  Calendar,
  Check,
  Edit2,
  Folder,
  Globe,
  HardDrive,
  Layers,
  Plus,
  Radio,
  Save,
  Share2,
  Sliders,
  Trash2,
  UserCheck,
  X,
  Zap,
} from 'lucide-react';
import {
  ContentPage,
  CreateContentPageRequest,
  UpdateContentPageRequest,
} from '../../api/flowordClient';

interface PagesViewProps {
  pages: ContentPage[];
  activePageId?: string;
  onSelectPage: (pageId: string) => void;
  onCreatePage: (req: CreateContentPageRequest) => Promise<void>;
  onUpdatePage: (pageId: string, req: UpdateContentPageRequest) => Promise<void>;
  onArchivePage: (pageId: string) => Promise<void>;
}

export const PagesView: React.FC<PagesViewProps> = ({
  pages,
  activePageId,
  onSelectPage,
  onCreatePage,
  onUpdatePage,
  onArchivePage,
}) => {
  const [isEditing, setIsEditing] = useState<boolean>(false);
  const [editingPageId, setEditingPageId] = useState<string | null>(null);

  // Form state
  const [formData, setFormData] = useState<{
    name: string;
    description: string;
    targetAudience: string;
    storagePath: string;
    browserProfile: string;
    defaultImagePrompt: string;
    defaultVideoPrompt: string;
    postMode: 'auto' | 'review';
    slots: string;
  }>({
    name: '',
    description: '',
    targetAudience: 'Movie & Entertainment',
    storagePath: 'D:\\Floword_Media\\',
    browserProfile: 'PROFILE_01',
    defaultImagePrompt: 'Cinematic wide angle frame, 8k resolution, photorealistic lighting',
    defaultVideoPrompt: 'Smooth camera panning, vivid 4k cinematic movement',
    postMode: 'review',
    slots: '08:30, 12:00, 17:30, 21:00',
  });

  const openCreateModal = () => {
    setEditingPageId(null);
    setFormData({
      name: '',
      description: '',
      targetAudience: 'General Audience',
      storagePath: 'D:\\Floword_Media\\',
      browserProfile: 'PROFILE_DEFAULT',
      defaultImagePrompt: 'Cinematic shot, hyper-realistic, vivid atmosphere',
      defaultVideoPrompt: 'Dynamic motion, smooth camera pan',
      postMode: 'review',
      slots: '08:30, 12:00, 17:30, 21:00',
    });
    setIsEditing(true);
  };

  const openEditModal = (page: ContentPage) => {
    setEditingPageId(page.id);
    setFormData({
      name: page.name,
      description: page.description || '',
      targetAudience: page.targetAudience || 'General Audience',
      storagePath: 'D:\\Floword_Media\\' + page.name,
      browserProfile: 'PROFILE_' + page.name.toUpperCase().replace(/\s+/g, '_'),
      defaultImagePrompt: 'Cinematic shot, hyper-realistic, vivid atmosphere',
      defaultVideoPrompt: 'Dynamic motion, smooth camera pan',
      postMode: 'review',
      slots: '08:30, 12:00, 17:30, 21:00',
    });
    setIsEditing(true);
  };

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!formData.name.trim()) return;

    if (editingPageId) {
      await onUpdatePage(editingPageId, {
        name: formData.name,
        description: formData.description,
        targetAudience: formData.targetAudience,
      });
    } else {
      await onCreatePage({
        name: formData.name,
        description: formData.description,
        targetAudience: formData.targetAudience,
      });
    }
    setIsEditing(false);
  };

  return (
    <div className="max-w-7xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-white tracking-tight">Channel & Page Management</h1>
          <p className="text-xs text-zinc-400">
            Configure target media channels, automated prompt presets, storage, and publishing rules.
          </p>
        </div>

        <button
          type="button"
          onClick={openCreateModal}
          className="flex items-center gap-2 px-4 py-2 rounded-xl bg-gradient-to-r from-[#e54d5e] to-[#c23b4c] hover:from-[#f05c6d] hover:to-[#d04657] text-white text-xs font-semibold shadow-lg shadow-rose-500/20 transition"
        >
          <Plus className="h-4 w-4" />
          Create New Page
        </button>
      </div>

      {/* Pages Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-5">
        {pages.map((page) => {
          const isActive = page.id === activePageId;

          return (
            <div
              key={page.id}
              onClick={() => onSelectPage(page.id)}
              className={`p-5 rounded-2xl border transition flex flex-col justify-between cursor-pointer ${
                isActive
                  ? 'bg-rose-500/[0.06] border-rose-500/40 shadow-xl'
                  : 'bg-[#121622] border-white/[0.08] hover:border-white/[0.16]'
              }`}
            >
              <div>
                {/* Page Title & Status */}
                <div className="flex items-start justify-between gap-2">
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="h-2.5 w-2.5 rounded-full bg-emerald-400" />
                      <h3 className="font-bold text-base text-white">{page.name}</h3>
                    </div>
                    <p className="text-xs text-zinc-400 mt-1 line-clamp-2">
                      {page.description || 'No description configured.'}
                    </p>
                  </div>

                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      openEditModal(page);
                    }}
                    className="p-1.5 rounded-lg bg-white/[0.04] hover:bg-white/[0.08] text-zinc-400 hover:text-white transition"
                    title="Edit Page Defaults"
                  >
                    <Edit2 className="h-3.5 w-3.5" />
                  </button>
                </div>

                {/* Target Audience & Preset Badges */}
                <div className="mt-4 pt-3 border-t border-white/[0.06] space-y-2">
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-zinc-500">Audience:</span>
                    <span className="text-zinc-300 font-medium">{page.targetAudience || 'General'}</span>
                  </div>
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-zinc-500">Post Mode:</span>
                    <span className="px-2 py-0.5 rounded text-[10px] font-bold bg-amber-500/10 text-amber-300 border border-amber-500/20">
                      Review Before Post
                    </span>
                  </div>
                </div>

                {/* Connected Channels */}
                <div className="mt-4 pt-3 border-t border-white/[0.06]">
                  <div className="text-[10px] text-zinc-500 font-semibold uppercase tracking-wider mb-2">
                    Publishing Channels
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="px-2 py-1 rounded-md text-[10px] font-bold bg-blue-500/10 text-blue-400 border border-blue-500/20">
                      Facebook Page
                    </span>
                    <span className="px-2 py-1 rounded-md text-[10px] font-bold bg-pink-500/10 text-pink-400 border border-pink-500/20">
                      TikTok
                    </span>
                    <span className="px-2 py-1 rounded-md text-[10px] font-bold bg-red-500/10 text-red-400 border border-red-500/20">
                      YouTube
                    </span>
                  </div>
                </div>
              </div>

              {/* Card Footer */}
              <div className="mt-5 pt-3 border-t border-white/[0.06] flex items-center justify-between">
                <span className="text-[11px] text-zinc-500">
                  {isActive ? '● Currently Active' : 'Click to activate'}
                </span>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    openEditModal(page);
                  }}
                  className="text-xs text-rose-400 hover:text-rose-300 font-medium"
                >
                  Page Settings &rarr;
                </button>
              </div>
            </div>
          );
        })}
      </div>

      {/* Page Edit / Create Modal */}
      {isEditing && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4">
          <div className="w-full max-w-xl rounded-2xl bg-[#151926] border border-white/[0.12] p-6 shadow-2xl space-y-5 animate-in fade-in zoom-in-95">
            <div className="flex items-center justify-between border-b border-white/[0.08] pb-3">
              <h3 className="text-base font-bold text-white">
                {editingPageId ? 'Page Preset Settings' : 'Create New Channel / Page'}
              </h3>
              <button
                type="button"
                onClick={() => setIsEditing(false)}
                className="p-1 text-zinc-400 hover:text-white"
              >
                <X className="h-5 w-5" />
              </button>
            </div>

            <form onSubmit={handleSave} className="space-y-4 text-xs">
              <div>
                <label className="block text-zinc-400 mb-1 font-medium">Page / Channel Name</label>
                <input
                  type="text"
                  required
                  placeholder="e.g. Movie Feed, Celebrity World..."
                  value={formData.name}
                  onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                  className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500"
                />
              </div>

              <div>
                <label className="block text-zinc-400 mb-1 font-medium">Topic / Description</label>
                <input
                  type="text"
                  placeholder="Short description of the content theme..."
                  value={formData.description}
                  onChange={(e) => setFormData({ ...formData, description: e.target.value })}
                  className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-zinc-400 mb-1 font-medium">Dedicated Storage Directory</label>
                  <input
                    type="text"
                    value={formData.storagePath}
                    onChange={(e) => setFormData({ ...formData, storagePath: e.target.value })}
                    className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-zinc-300 font-mono text-[11px] focus:outline-none"
                  />
                </div>
                <div>
                  <label className="block text-zinc-400 mb-1 font-medium">Browser Posting Profile</label>
                  <input
                    type="text"
                    value={formData.browserProfile}
                    onChange={(e) => setFormData({ ...formData, browserProfile: e.target.value })}
                    className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-zinc-300 font-mono text-[11px] focus:outline-none"
                  />
                </div>
              </div>

              <div>
                <label className="block text-zinc-400 mb-1 font-medium">Default Prompt Template</label>
                <textarea
                  rows={2}
                  value={formData.defaultImagePrompt}
                  onChange={(e) => setFormData({ ...formData, defaultImagePrompt: e.target.value })}
                  className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500 resize-none"
                />
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-zinc-400 mb-1 font-medium">Publishing Workflow</label>
                  <select
                    value={formData.postMode}
                    onChange={(e) => setFormData({ ...formData, postMode: e.target.value as any })}
                    className="w-full px-3 py-2 rounded-xl bg-[#1b2030] border border-white/[0.1] text-white focus:outline-none"
                  >
                    <option value="review">Review Before Post (Recommended)</option>
                    <option value="auto">Auto-Post Immediately</option>
                  </select>
                </div>
                <div>
                  <label className="block text-zinc-400 mb-1 font-medium">Default Schedule Slots</label>
                  <input
                    type="text"
                    value={formData.slots}
                    onChange={(e) => setFormData({ ...formData, slots: e.target.value })}
                    className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white font-mono text-[11px] focus:outline-none"
                  />
                </div>
              </div>

              <div className="pt-4 border-t border-white/[0.08] flex items-center justify-between">
                {editingPageId ? (
                  <button
                    type="button"
                    onClick={() => {
                      onArchivePage(editingPageId);
                      setIsEditing(false);
                    }}
                    className="px-3 py-2 rounded-xl bg-rose-500/10 hover:bg-rose-500/20 text-rose-400 text-xs font-medium transition"
                  >
                    Archive Page
                  </button>
                ) : <div />}

                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={() => setIsEditing(false)}
                    className="px-4 py-2 rounded-xl bg-white/[0.04] hover:bg-white/[0.08] text-zinc-300 text-xs font-medium transition"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    className="px-5 py-2 rounded-xl bg-rose-600 hover:bg-rose-500 text-white text-xs font-semibold shadow transition"
                  >
                    Save Page Preset
                  </button>
                </div>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
};
