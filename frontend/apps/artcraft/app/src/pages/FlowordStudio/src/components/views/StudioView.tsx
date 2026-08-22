import React, { useState, useEffect, useRef, useCallback } from 'react';
import {
  AlertCircle,
  ArrowRight,
  CheckCircle2,
  ChevronRight,
  Clock,
  Eye,
  FileText,
  Image as ImageIcon,
  Layers,
  Play,
  RotateCcw,
  Settings2,
  Sparkles,
  Terminal,
  Upload,
  Video,
  Wand2,
  Trash2,
  Copy,
} from 'lucide-react';
import toast from 'react-hot-toast';
import { ContentPage, IngestFlowordSourceImageResponse, ingestFlowordSourceImage, ArtifactRef as FlowordArtifactRef, DonutProfileEnriched } from '../../api/flowordClient';
import { WorkflowInput, WorkflowRun, ArtifactRef } from '../../services/workflowEngine';

interface StudioViewProps {
  pages: ContentPage[];
  activePageId?: string;
  onSelectPage: (pageId: string) => void;
  activeRun: WorkflowRun | null;
  isRunning: boolean;
  onRunWorkflow: (input: WorkflowInput) => Promise<void>;
  onCancelWorkflow: () => Promise<void>;
  profiles?: DonutProfileEnriched[];
}

export const StudioView: React.FC<StudioViewProps> = ({
  pages,
  activePageId,
  onSelectPage,
  activeRun,
  isRunning,
  onRunWorkflow,
  onCancelWorkflow,
  profiles = [],
}) => {
  const [pipelineType, setPipelineType] = useState<'grok_content_pipeline' | 'grok_image_edit' | 'floword_video_pipeline'>('grok_content_pipeline');
  const [mode, setMode] = useState<'simple' | 'advanced'>('simple');
  const [topic, setTopic] = useState<string>('Top 5 Hidden Gems in Action Cinema 2026');
  
  // Ingested Source Image State
  const [sourceImagePath, setSourceImagePath] = useState<string>('');
  const [sourceImagePreviewUrl, setSourceImagePreviewUrl] = useState<string>('');
  const [sourceImageArtifact, setSourceImageArtifact] = useState<FlowordArtifactRef | null>(null);
  const [isIngesting, setIsIngesting] = useState<boolean>(false);
  const [isDragging, setIsDragging] = useState<boolean>(false);
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  // Prompts & Publishing Metadata
  const [imagePrompt, setImagePrompt] = useState<string>('');
  const [expand916Prompt, setExpand916Prompt] = useState<string>('');
  const [videoPrompt, setVideoPrompt] = useState<string>('');
  const [title, setTitle] = useState<string>('');
  const [caption, setCaption] = useState<string>('Khám phá ngay danh sách siêu phẩm không thể bỏ lỡ! #cinema #review #movies');
  const [hashtags, setHashtags] = useState<string>('cinema, review, movies');
  const [description, setDescription] = useState<string>('');

  const selectedPage = pages.find((p) => p.id === activePageId) || pages[0];

  // Helper to ingest file from bytes/base64 or local path
  const handleIngestFile = useCallback(async (file: File) => {
    setIsIngesting(true);
    const toastId = toast.loading(`Đang tải ảnh: ${file.name}...`);
    try {
      const reader = new FileReader();
      reader.onload = async (e) => {
        const base64Data = e.target?.result as string;
        try {
          const res: IngestFlowordSourceImageResponse = await ingestFlowordSourceImage({
            base64_data: base64Data,
            file_name: file.name,
            page_id: selectedPage?.id,
          });
          setSourceImageArtifact(res.artifact);
          setSourceImagePath(res.artifact.location);
          setSourceImagePreviewUrl(base64Data || res.preview_url);
          toast.success('Đã lưu ảnh nguồn vào storage!', { id: toastId });
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          toast.error(`Lỗi tải ảnh: ${msg}`, { id: toastId });
        } finally {
          setIsIngesting(false);
        }
      };
      reader.onerror = () => {
        toast.error('Không thể đọc file ảnh', { id: toastId });
        setIsIngesting(false);
      };
      reader.readAsDataURL(file);
    } catch (err) {
      setIsIngesting(false);
      toast.error('Lỗi khi tải ảnh', { id: toastId });
    }
  }, [selectedPage?.id]);

  // Global paste handler
  useEffect(() => {
    const onPaste = (evt: ClipboardEvent) => {
      const items = evt.clipboardData?.items;
      if (!items) return;
      for (let i = 0; i < items.length; i++) {
        if (items[i].type.indexOf('image') !== -1) {
          const file = items[i].getAsFile();
          if (file) {
            evt.preventDefault();
            handleIngestFile(file);
            break;
          }
        }
      }
    };
    window.addEventListener('paste', onPaste);
    return () => window.removeEventListener('paste', onPaste);
  }, [handleIngestFile]);

  // Drag & drop handlers
  const onDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  };
  const onDragLeave = () => setIsDragging(false);
  const onDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const file = e.dataTransfer.files[0];
      if (file.type.startsWith('image/')) {
        handleIngestFile(file);
      } else {
        toast.error('Vui lòng kéo thả file ảnh hợp lệ (PNG, JPG, WEBP).');
      }
    }
  };

  const handleStartRun = async () => {
    if (!selectedPage) {
      toast.error('Vui lòng chọn Page trước khi khởi chạy.');
      return;
    }

    const hasSource = sourceImagePath.trim().length > 0 || sourceImageArtifact !== null;
    if (!hasSource) {
      toast.error('Vui lòng tải lên hoặc dán Source Image trước khi khởi chạy.');
      return;
    }

    const effImagePrompt = imagePrompt.trim() || selectedPage.default_image_prompt || '';
    if (!effImagePrompt) {
      toast.error('Vui lòng nhập Image Prompt hoặc cấu hình sẵn trong Page.');
      return;
    }

    const effExpandPrompt = expand916Prompt.trim() || selectedPage.default_expand_9_16_prompt || undefined;
    const effVideoPrompt = videoPrompt.trim() || selectedPage.default_video_prompt || undefined;

    // Normalize and extract structured hashtags (without leading #)
    const fromInput = hashtags
      .split(/[,\s]+/)
      .map((t) => t.replace(/^#+/, '').trim())
      .filter(Boolean);
    const fromCaption = (caption.match(/#[a-zA-Z0-9_\u00C0-\u024F\u1EA0-\u1EF9]+/g) || [])
      .map((t) => t.replace(/^#+/, '').trim())
      .filter(Boolean);
    const parsedHashtags = Array.from(new Set([...fromInput, ...fromCaption]));

    await onRunWorkflow({
      workflowMode: pipelineType,
      workflowName: pipelineType,
      pageId: selectedPage.id,
      prompt: effImagePrompt,
      imagePrompt: effImagePrompt,
      expand916Prompt: effExpandPrompt,
      videoPrompt: effVideoPrompt,
      sourceFiles: sourceImagePath.trim() ? [sourceImagePath.trim()] : [],
      sourceImageArtifact: sourceImageArtifact || undefined,
      sourceUrls: [],
      targetPlatform: (selectedPage.target_platform as any) || 'tiktok',
      targetDurationSeconds: 45,
      language: selectedPage.default_language || 'vi',
      tone: (selectedPage.default_tone as any) || 'professional',
      aspectRatio: '9:16',
      scriptMode: 'original',
      outputMode: 'render_video',
      researchEnabled: false,
      researchPlatform: 'xhs',
      researchQuery: '',
      researchMode: 'search',
      topic: topic.trim() || undefined,
      title: title.trim() || topic.trim() || undefined,
      caption: caption.trim() || undefined,
      hashtags: parsedHashtags.length > 0 ? parsedHashtags : undefined,
      description: description.trim() || undefined,
      customPrompt: effImagePrompt,
      platform: selectedPage.target_platform || 'tiktok',
      targetDurationSec: 45,
      voiceTone: 'cinematic_narrator',
      generateImage: true,
      generateDraft: true,
    });
  };

  const GROK_STAGES = [
    { key: 'QUEUED', label: '1. Nạp Ảnh & Gán Hồ Sơ', desc: 'Lưu trữ ảnh gốc & phân bổ trình duyệt' },
    { key: 'GENERATING_IMAGE', label: '2. Chỉnh Sửa Ảnh (Grok Flux)', desc: 'Tạo ảnh mới theo prompt từ ảnh nguồn' },
    { key: 'CONVERTING_9_16', label: '3. Mở Rộng Khung Dọc 9:16', desc: 'Mở rộng tỷ lệ 9:16 giữ nguyên chủ thể' },
    { key: 'GENERATING_VIDEO', label: '4. Tạo Chuyển Động Video', desc: 'Tạo chuyển động video bằng AI Grok' },
    { key: 'SAVING_LOCAL', label: '5. Lưu File Vào Máy', desc: 'Lưu video master vào thư mục Page' },
    { key: 'READY_TO_POST', label: '6. Sẵn Sàng Xuất Bản', desc: 'Video hoàn thiện sẵn sàng đăng tải' },
  ];

  const latestVideoArtifact = activeRun?.artifacts?.find(
    (a) => a.type === 'video' || a.type === 'rendered_video' || a.name.endsWith('.mp4')
  );
  const latestImageArtifact = activeRun?.artifacts?.find(
    (a) => a.type === 'image' || a.name.endsWith('.png') || a.name.endsWith('.jpg')
  );

  return (
    <div className="flex flex-col h-full max-w-7xl mx-auto p-6 space-y-4 overflow-hidden">
      {/* Studio Header */}
      <div className="flex items-center justify-between p-4 rounded-2xl bg-[#121622] border border-white/[0.08]">
        <div className="flex items-center gap-3">
          <div className="h-9 w-9 rounded-xl bg-gradient-to-br from-[#e54d5e] to-[#a855f7] flex items-center justify-center text-white shadow-md shadow-rose-500/20">
            <Wand2 className="h-5 w-5" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-bold text-white">Floword Studio Editor</h2>
              <span className="px-2 py-0.5 rounded-full text-[10px] font-semibold bg-white/[0.06] text-zinc-300">
                {selectedPage?.name || 'Sản Xuất Chung'}
              </span>
              {selectedPage?.browser_profile_id && (
                <span className="px-2 py-0.5 rounded-full text-[10px] font-mono bg-blue-500/10 text-blue-400 border border-blue-500/20">
                  Hồ Sơ: {selectedPage.browser_profile_id}
                </span>
              )}
            </div>
            <div className="text-[11px] text-zinc-400 mt-0.5">
              Quy Trình Tự Động: Nạp Ảnh &rarr; Sửa Grok &rarr; 9:16 &rarr; Tạo Video &rarr; Lưu Về Máy.
            </div>
          </div>
        </div>

        {/* Mode switch & Action */}
        <div className="flex items-center gap-3">
          <div className="flex items-center p-1 rounded-xl bg-white/[0.04] border border-white/[0.08] text-xs">
            <button
              type="button"
              onClick={() => setMode('simple')}
              className={`px-3 py-1 rounded-lg font-medium transition ${
                mode === 'simple' ? 'bg-white/[0.1] text-white shadow' : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              Chế Độ Cơ Bản
            </button>
            <button
              type="button"
              onClick={() => setMode('advanced')}
              className={`px-3 py-1 rounded-lg font-medium transition ${
                mode === 'advanced' ? 'bg-white/[0.1] text-white shadow' : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              Chế Độ Nâng Cao
            </button>
          </div>

          {isRunning ? (
            <button
              type="button"
              onClick={onCancelWorkflow}
              className="flex items-center gap-2 px-4 py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-rose-400 text-xs font-semibold border border-rose-500/30 transition"
            >
              <RotateCcw className="h-3.5 w-3.5 animate-spin" />
              Dừng Chạy
            </button>
          ) : (
            <button
              type="button"
              onClick={handleStartRun}
              className="flex items-center gap-2 px-5 py-2 rounded-xl bg-gradient-to-r from-[#e54d5e] to-[#c23b4c] hover:from-[#f05c6d] hover:to-[#d04657] text-white text-xs font-semibold shadow-lg shadow-rose-500/20 transition transform active:scale-95"
            >
              <Play className="h-4 w-4 fill-white" />
              Chạy Quy Trình Sản Xuất
            </button>
          )}
        </div>
      </div>

      {/* 4-Pane Grid Layout */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 flex-1 min-h-0 overflow-hidden">
        {/* PANE 1: Pipeline Stage Rail (3 cols) */}
        <div className="lg:col-span-3 rounded-2xl bg-[#121622] border border-white/[0.08] p-4 flex flex-col justify-between overflow-y-auto">
          <div>
            <div className="text-xs font-bold text-zinc-400 uppercase tracking-wider mb-3 flex items-center gap-2">
              <Layers className="h-4 w-4 text-rose-400" />
              Giai Đoạn Sản Xuất Grok
            </div>

            <div className="space-y-2">
              {GROK_STAGES.map((st, idx) => {
                const bStatus = activeRun?.businessStatus || '';
                const isCurrent = isRunning && (bStatus === st.key || bStatus.includes(st.key));
                const isDone = bStatus === 'READY_TO_POST' || bStatus === 'LOCAL_SAVED' || (activeRun?.progressPercent && activeRun.progressPercent >= (idx + 1) * 18);

                return (
                  <div
                    key={st.key}
                    className={`p-3 rounded-xl border transition ${
                      isCurrent
                        ? 'bg-blue-500/10 border-blue-500/30 shadow-md'
                        : isDone
                        ? 'bg-emerald-500/[0.04] border-emerald-500/20'
                        : 'bg-white/[0.02] border-white/[0.04]'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-semibold text-white">{st.label}</span>
                      <span className={`text-[11px] ${isDone ? 'text-emerald-400 font-bold' : isCurrent ? 'text-blue-400 font-bold animate-pulse' : 'text-zinc-600'}`}>
                        {isDone ? '✓' : isCurrent ? '●' : '○'}
                      </span>
                    </div>
                    <p className="text-[10px] text-zinc-500 mt-0.5">{st.desc}</p>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="mt-4 p-3 rounded-xl bg-white/[0.02] border border-white/[0.05] text-[11px] text-zinc-400 space-y-1">
            <div className="flex items-center justify-between">
              <span>Hồ Sơ Trình Duyệt:</span>
              <span className="text-blue-400 font-mono font-medium">{selectedPage?.browser_profile_id || 'Chưa gán hồ sơ'}</span>
            </div>
            <div className="flex items-center justify-between">
              <span>Thư Mục Đầu Ra:</span>
              <span className="text-zinc-400 font-mono text-[10px] truncate max-w-[140px]">{selectedPage?.output_root || 'Chưa thiết lập'}</span>
            </div>
          </div>
        </div>

        {/* PANE 2: Content & Prompt Editor (5 cols) */}
        <div className="lg:col-span-5 rounded-2xl bg-[#121622] border border-white/[0.08] p-4 overflow-y-auto space-y-4 text-xs">
          <div className="text-xs font-bold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
            <FileText className="h-4 w-4 text-rose-400" />
            Cấu Hình Nội Dung & Prompt
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">Chế Độ Quy Trình</label>
            <select
              value={pipelineType}
              onChange={(e) => setPipelineType(e.target.value as any)}
              className="w-full px-3 py-2 rounded-xl bg-[#171b26] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500 font-semibold"
            >
              <option value="grok_content_pipeline">Grok Tự Động Toàn Diện (Sửa Ảnh &rarr; 9:16 &rarr; Tạo Video &rarr; Lưu File)</option>
              <option value="grok_image_edit">Chỉ Chỉnh Sửa Ảnh Bằng Grok</option>
            </select>
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">Chọn Page / Kênh Đích</label>
            <select
              value={activePageId || ''}
              onChange={(e) => onSelectPage(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-[#171b26] border border-white/[0.1] text-white focus:outline-none"
            >
              {pages.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.target_platform || 'tiktok'}) {p.browser_profile_id ? `[${p.browser_profile_id}]` : ''}
                </option>
              ))}
            </select>
          </div>

          {/* DRAG / DROP / PASTE SOURCE IMAGE BOX */}
          <div>
            <label className="block text-zinc-400 mb-1 font-medium">
              Ảnh Nguồn <span className="text-rose-400">* (Kéo thả, Dán Ctrl+V hoặc bấm để chọn file)</span>
            </label>

            <input
              type="file"
              ref={fileInputRef}
              accept="image/png,image/jpeg,image/webp"
              className="hidden"
              onChange={(e) => {
                if (e.target.files && e.target.files.length > 0) {
                  handleIngestFile(e.target.files[0]);
                }
              }}
            />

            <div
              onDragOver={onDragOver}
              onDragLeave={onDragLeave}
              onDrop={onDrop}
              onClick={() => fileInputRef.current?.click()}
              className={`border-2 border-dashed rounded-2xl p-4 flex flex-col items-center justify-center cursor-pointer transition ${
                isDragging
                  ? 'border-rose-500 bg-rose-500/10'
                  : sourceImagePreviewUrl
                  ? 'border-emerald-500/40 bg-emerald-500/[0.02]'
                  : 'border-white/[0.12] hover:border-white/[0.25] bg-white/[0.02]'
              }`}
            >
              {sourceImagePreviewUrl ? (
                <div className="flex items-center gap-3 w-full">
                  <img
                    src={sourceImagePreviewUrl}
                    alt="Source Preview"
                    className="h-16 w-16 object-cover rounded-xl border border-white/[0.1]"
                  />
                  <div className="flex-1 min-w-0">
                    <div className="text-xs font-semibold text-emerald-400 flex items-center gap-1">
                      <CheckCircle2 className="h-3.5 w-3.5" /> Ảnh Nguồn Đã Sẵn Sàng
                    </div>
                    <div className="text-[10px] text-zinc-400 font-mono truncate mt-0.5">
                      {sourceImagePath}
                    </div>
                    {sourceImageArtifact && (
                      <div className="text-[9px] text-zinc-500 font-mono mt-0.5">
                        SHA256: {sourceImageArtifact.sha256?.slice(0, 16)}... | {(sourceImageArtifact.size_bytes / 1024).toFixed(1)} KB
                      </div>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      setSourceImagePath('');
                      setSourceImagePreviewUrl('');
                      setSourceImageArtifact(null);
                    }}
                    className="p-1.5 rounded-lg bg-zinc-800 hover:bg-rose-900/30 text-zinc-400 hover:text-rose-400 transition"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
              ) : (
                <div className="flex flex-col items-center text-center py-2">
                  <Upload className={`h-6 w-6 mb-2 ${isDragging ? 'text-rose-400' : 'text-zinc-500'}`} />
                  <div className="text-xs font-semibold text-zinc-200">
                    {isIngesting ? 'Đang lưu ảnh nguồn...' : 'Kéo thả ảnh hoặc dán Ctrl+V tại đây'}
                  </div>
                  <div className="text-[10px] text-zinc-500 mt-0.5">
                    Hỗ trợ PNG, JPG, WEBP &bull; Bấm để duyệt file
                  </div>
                </div>
              )}
            </div>
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">Chủ Đề / Tiêu Đề Video</label>
            <input
              type="text"
              value={topic}
              onChange={(e) => setTopic(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500"
            />
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">
              Prompt Chỉnh Sửa Ảnh (Grok FLUX)
              {selectedPage?.default_image_prompt && !imagePrompt && (
                <span className="text-[10px] text-zinc-500 ml-2">(Kế thừa từ Page)</span>
              )}
            </label>
            <textarea
              rows={3}
              value={imagePrompt}
              onChange={(e) => setImagePrompt(e.target.value)}
              placeholder={selectedPage?.default_image_prompt || 'Nhập mô tả / prompt để Grok chỉnh sửa ảnh...'}
              className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500 resize-none font-mono text-[11px]"
            />
          </div>

          {pipelineType === 'grok_content_pipeline' && (
            <div>
              <label className="block text-zinc-400 mb-1 font-medium">
                Prompt Mở Rộng Khung Dọc 9:16
                {selectedPage?.default_expand_9_16_prompt && !expand916Prompt && (
                  <span className="text-[10px] text-zinc-500 ml-2">(Kế thừa từ Page)</span>
                )}
              </label>
              <textarea
                rows={2}
                value={expand916Prompt}
                onChange={(e) => setExpand916Prompt(e.target.value)}
                placeholder={selectedPage?.default_expand_9_16_prompt || 'Nhập prompt mở rộng khung hình dọc 9:16...'}
                className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500 resize-none font-mono text-[11px]"
              />
            </div>
          )}

          {pipelineType === 'grok_content_pipeline' && (
            <div>
              <label className="block text-zinc-400 mb-1 font-medium">
                Prompt Tạo Chuyển Động Video
                {selectedPage?.default_video_prompt && !videoPrompt && (
                  <span className="text-[10px] text-zinc-500 ml-2">(Kế thừa từ Page)</span>
                )}
              </label>
              <textarea
                rows={2}
                value={videoPrompt}
                onChange={(e) => setVideoPrompt(e.target.value)}
                placeholder={selectedPage?.default_video_prompt || 'Nhập prompt mô tả chuyển động camera/nhân vật...'}
                className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500 resize-none font-mono text-[11px]"
              />
            </div>
          )}

          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <div>
              <label className="block text-zinc-400 mb-1 font-medium text-xs">Tiêu Đề Khi Xuất Bản</label>
              <input
                type="text"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                placeholder={topic || 'Tiêu đề video xuất bản...'}
                className="w-full px-3 py-1.5 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white text-xs focus:outline-none focus:border-rose-500"
              />
            </div>
            <div>
              <label className="block text-zinc-400 mb-1 font-medium text-xs">Hashtag Bài Đăng</label>
              <input
                type="text"
                value={hashtags}
                onChange={(e) => setHashtags(e.target.value)}
                placeholder="review, phim, viral, giaitri..."
                className="w-full px-3 py-1.5 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white text-xs focus:outline-none focus:border-rose-500"
              />
            </div>
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium text-xs">Nội Dung Bài Đăng (Caption)</label>
            <textarea
              rows={2}
              value={caption}
              onChange={(e) => setCaption(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white text-xs focus:outline-none focus:border-rose-500 resize-none"
            />
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium text-xs">Mô Tả Chi Tiết (YouTube / SEO)</label>
            <textarea
              rows={2}
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Mô tả chi tiết video khi đăng lên YouTube/Facebook..."
              className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white text-xs focus:outline-none focus:border-rose-500 resize-none"
            />
          </div>
        </div>

        {/* PANE 3 & 4: Live Preview & Activity Log (4 cols) */}
        <div className="lg:col-span-4 flex flex-col gap-4 overflow-hidden">
          {/* Live Asset Preview */}
          <div className="flex-1 rounded-2xl bg-[#121622] border border-white/[0.08] p-4 flex flex-col overflow-hidden">
            <div className="text-xs font-bold text-zinc-400 uppercase tracking-wider mb-2 flex items-center gap-2">
              <Eye className="h-4 w-4 text-rose-400" />
              Xem Trước Trực Tiếp
            </div>

            <div className="flex-1 rounded-xl bg-black/40 border border-white/[0.05] flex flex-col items-center justify-center p-3 text-center overflow-hidden">
              {latestVideoArtifact ? (
                <div className="w-full h-full flex flex-col items-center justify-center">
                  <div className="text-[11px] font-semibold text-emerald-400 mb-1 flex items-center gap-1">
                    <CheckCircle2 className="h-3.5 w-3.5" /> Video Đã Hoàn Tất: {latestVideoArtifact.name}
                  </div>
                  <div className="text-[9px] text-zinc-400 font-mono truncate max-w-full px-2">
                    {latestVideoArtifact.path}
                  </div>
                </div>
              ) : latestImageArtifact ? (
                <div className="w-full h-full flex flex-col items-center justify-center">
                  <div className="text-[11px] font-semibold text-blue-400 mb-1">
                    Khung Hình Đã Tạo: {latestImageArtifact.name}
                  </div>
                  <div className="text-[9px] text-zinc-400 font-mono truncate max-w-full px-2">
                    {latestImageArtifact.path}
                  </div>
                </div>
              ) : (
                <>
                  <Video className="h-8 w-8 text-zinc-600 mb-2" />
                  <div className="text-xs font-medium text-zinc-400">
                    {isRunning ? `Giai đoạn: ${activeRun?.businessStatus || activeRun?.currentStage}...` : 'Sẵn sàng xem trước kết quả'}
                  </div>
                  <p className="text-[10px] text-zinc-600 mt-1 max-w-xs">
                    Hình ảnh và video tạo bởi Grok sẽ hiển thị trực tiếp tại đây.
                  </p>
                </>
              )}
            </div>
          </div>

          {/* Activity / Diagnostic Logs */}
          <div className="h-48 rounded-2xl bg-[#0d1017] border border-white/[0.08] p-3 flex flex-col font-mono text-[11px] overflow-hidden">
            <div className="text-zinc-500 text-[10px] font-bold uppercase tracking-wider mb-1 flex items-center justify-between">
              <span className="flex items-center gap-1.5">
                <Terminal className="h-3.5 w-3.5 text-zinc-400" />
                Nhật Ký Tiến Trình Thực Thi
              </span>
              <div className="flex items-center gap-2">
                {activeRun?.businessStatus && (
                  <span className="text-rose-400 font-bold">{activeRun.businessStatus}</span>
                )}
                <button
                  type="button"
                  title="Sao chép toàn bộ nhật ký"
                  onClick={() => {
                    const lines = ['[00:00] Động cơ Floword đã sẵn sàng.'];
                    if (selectedPage) {
                      lines.push(`[00:01] Page đích: ${selectedPage.name}`);
                      const profileId = selectedPage.browser_profile_id;
                      const boundProfile = profileId ? profiles.find((p) => p.id === profileId) : undefined;
                      if (profileId) {
                        lines.push(`[00:02] Hồ sơ trình duyệt: ${boundProfile?.name ?? profileId} / ${profileId}`);
                        lines.push(`[00:03] Trạng thái: ${boundProfile?.is_running ? `${boundProfile.worker_state || 'Đang chạy'}${boundProfile.grok_logged_in ? ' ● Grok ✓' : ''}${boundProfile.extension_ready ? ' ● Ext ✓' : ''}` : 'Đang tắt'}`);
                      } else {
                        lines.push('[00:02] Hồ sơ trình duyệt: (chưa gán)');
                      }
                    }
                    if (activeRun) {
                      lines.push(`[CÔNG VIỆC] ID: ${activeRun.id}`);
                      lines.push(`[TRẠNG THÁI] ${activeRun.businessStatus || activeRun.status}`);
                      if (activeRun.errorMessage) {
                        lines.push(`[LỖI] ${activeRun.errorMessage}`);
                      }
                    }
                    navigator.clipboard.writeText(lines.join('\n'));
                    toast.success('Đã sao chép nhật ký!');
                  }}
                  className="flex items-center gap-1 px-1.5 py-0.5 rounded bg-white/[0.06] hover:bg-white/[0.12] text-zinc-300 hover:text-white transition text-[10px] font-sans font-medium"
                >
                  <Copy className="h-3 w-3" />
                  <span>Sao chép</span>
                </button>
              </div>
            </div>
            <div className="flex-1 overflow-y-auto space-y-1 text-zinc-400 pt-1">
              <div className="text-zinc-600">[00:00] Động cơ Floword đã sẵn sàng.</div>
              {selectedPage && (() => {
                const profileId = selectedPage.browser_profile_id;
                const boundProfile = profileId
                  ? profiles.find((p) => p.id === profileId)
                  : undefined;
                return (
                  <>
                    <div className="text-zinc-500">[00:01] Page đích: {selectedPage.name}</div>
                    {profileId ? (
                      <>
                        <div className="text-blue-400">
                          [00:02] Hồ sơ trình duyệt: {boundProfile?.name ?? profileId} / {profileId}
                        </div>
                        <div className={boundProfile?.is_running ? 'text-emerald-400' : 'text-zinc-500'}>
                          [00:03] Trạng thái: {boundProfile?.is_running
                            ? `${boundProfile.worker_state || 'Đang chạy'}${
                                boundProfile.grok_logged_in ? ' ● Grok ✓' : ''
                              }${
                                boundProfile.extension_ready ? ' ● Ext ✓' : ''
                              }`
                            : 'Đang tắt — hồ sơ sẽ tự động bật khi bắt đầu công việc'}
                        </div>
                      </>
                    ) : (
                      <div className="text-amber-400">[00:02] Hồ sơ trình duyệt: (chưa gán — vào Cài đặt Page để gán)</div>
                    )}
                  </>
                );
              })()}
              {activeRun && (
                <>
                  <div className="text-blue-400">[CÔNG VIỆC] ID: {activeRun.id}</div>
                  <div className="text-emerald-400">[TRẠNG THÁI] {activeRun.businessStatus || activeRun.status}</div>
                  {activeRun.errorMessage && (
                    <div className="text-rose-400">[LỖI] {activeRun.errorMessage}</div>
                  )}
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
