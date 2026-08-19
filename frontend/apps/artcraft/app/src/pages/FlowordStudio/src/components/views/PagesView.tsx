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
  Monitor,
} from 'lucide-react';
import {
  ContentPage,
  CreateContentPageRequest,
  UpdateContentPageRequest,
} from '../../api/flowordClient';
import { PageManagementModal } from '../PageManagementModal';

interface PagesViewProps {
  pages: ContentPage[];
  activePageId?: string;
  onSelectPage: (pageId: string) => void;
  onCreatePage: (req: CreateContentPageRequest) => Promise<ContentPage>;
  onUpdatePage: (pageId: string, req: UpdateContentPageRequest) => Promise<ContentPage>;
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
  const [modalOpen, setModalOpen] = useState<boolean>(false);
  const [selectedPageToEdit, setSelectedPageToEdit] = useState<ContentPage | null>(null);

  const openCreateModal = () => {
    setSelectedPageToEdit(null);
    setModalOpen(true);
  };

  const openEditModal = (page: ContentPage) => {
    setSelectedPageToEdit(page);
    setModalOpen(true);
  };

  const handleSavePage = async (req: CreateContentPageRequest | UpdateContentPageRequest): Promise<ContentPage> => {
    if ('id' in req && req.id) {
      return await onUpdatePage(req.id, req as UpdateContentPageRequest);
    } else {
      return await onCreatePage(req as CreateContentPageRequest);
    }
  };

  return (
    <div className="max-w-7xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-bold text-white tracking-tight">Channel & Page Management</h1>
          <p className="text-xs text-zinc-400">
            Cấu hình Page, thư mục lưu trữ cục bộ, Donut Browser Profile affinity và Prompt mặc định.
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
                    <p className="text-xs text-zinc-400 mt-1 line-clamp-1 font-mono">
                      {page.output_root}
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

                {/* Profile Affinity & Platform */}
                <div className="mt-4 pt-3 border-t border-white/[0.06] space-y-2">
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-zinc-500">Platform:</span>
                    <span className="text-zinc-300 font-medium uppercase">{page.target_platform || 'TikTok'}</span>
                  </div>
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-zinc-500">Browser Profile:</span>
                    <span className="font-mono text-blue-400 font-medium">
                      {page.browser_profile_id || 'Default Pool'}
                    </span>
                  </div>
                </div>

                {/* Output Directory Template */}
                <div className="mt-3 p-2.5 rounded-xl bg-black/40 border border-white/[0.04] text-[10px] font-mono text-zinc-400">
                  <div className="text-zinc-500 font-sans font-semibold mb-0.5">Layout lưu file:</div>
                  <div className="truncate text-indigo-300">
                    {page.output_root}\{page.name}\&lt;DD-MM-YYYY&gt;\
                  </div>
                </div>
              </div>

              {/* Card Footer */}
              <div className="mt-5 pt-3 border-t border-white/[0.06] flex items-center justify-between">
                <span className="text-[11px] text-zinc-500">
                  {isActive ? '● Currently Active' : 'Click to select'}
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

      {/* Authoritative Page Modal */}
      <PageManagementModal
        isOpen={modalOpen}
        onClose={() => setModalOpen(false)}
        pageToEdit={selectedPageToEdit}
        onSavePage={handleSavePage}
        onArchivePage={onArchivePage}
      />
    </div>
  );
};
