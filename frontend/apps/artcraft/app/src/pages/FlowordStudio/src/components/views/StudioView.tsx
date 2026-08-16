import React, { useState } from 'react';
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
} from 'lucide-react';
import { ContentPage } from '../../api/flowordClient';
import { WorkflowInput, WorkflowRun } from '../../services/workflowEngine';

interface StudioViewProps {
  pages: ContentPage[];
  activePageId?: string;
  onSelectPage: (pageId: string) => void;
  activeRun: WorkflowRun | null;
  isRunning: boolean;
  onRunWorkflow: (input: WorkflowInput) => Promise<void>;
  onCancelWorkflow: () => Promise<void>;
}

export const StudioView: React.FC<StudioViewProps> = ({
  pages,
  activePageId,
  onSelectPage,
  activeRun,
  isRunning,
  onRunWorkflow,
  onCancelWorkflow,
}) => {
  const [mode, setMode] = useState<'simple' | 'advanced'>('simple');
  const [topic, setTopic] = useState<string>('Top 5 Hidden Gems in Action Cinema 2026');
  const [imagePrompt, setImagePrompt] = useState<string>('Cinematic hero poster, dynamic rim light, 8k resolution');
  const [videoPrompt, setVideoPrompt] = useState<string>('Smooth drone zooming into cinematic cityscape, hyper-realistic');
  const [caption, setCaption] = useState<string>('Khám phá ngay danh sách siêu phẩm không thể bỏ lỡ! #cinema #review #movies');
  const [skipResearch, setSkipResearch] = useState<boolean>(true);
  const [preferredWorkerPool, setPreferredWorkerPool] = useState<string>('grok_browser_pool_01');

  const selectedPage = pages.find((p) => p.id === activePageId) || pages[0];

  const handleStartRun = async () => {
    await onRunWorkflow({
      topic,
      customPrompt: imagePrompt,
      platform: 'facebook',
      targetDurationSec: 45,
      voiceTone: 'cinematic_narrator',
      skipResearch,
      generateImage: true,
      generateDraft: true,
    });
  };

  const STAGES = [
    { key: 'input', label: '1. Ingest / Input', desc: 'Page & topic initialization' },
    { key: 'image', label: '2. Image AI', desc: 'High-res image generation' },
    { key: 'aspect', label: '3. 9:16 Transform', desc: 'Short-form framing outpaint' },
    { key: 'video', label: '4. Video Synthesis', desc: 'Video diffusion & motion' },
    { key: 'download', label: '5. Download & Merge', desc: 'Asset pack creation' },
    { key: 'publish', label: '6. Ready to Publish', desc: 'Inbox review & schedule' },
  ];

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
                {selectedPage?.name || 'General Production'}
              </span>
            </div>
            <div className="text-[11px] text-zinc-400 mt-0.5">
              Create, configure, and synthesize automated short-form video pipelines.
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
              Simple Mode
            </button>
            <button
              type="button"
              onClick={() => setMode('advanced')}
              className={`px-3 py-1 rounded-lg font-medium transition ${
                mode === 'advanced' ? 'bg-white/[0.1] text-white shadow' : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              Advanced Mode
            </button>
          </div>

          {isRunning ? (
            <button
              type="button"
              onClick={onCancelWorkflow}
              className="flex items-center gap-2 px-4 py-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 text-rose-400 text-xs font-semibold border border-rose-500/30 transition"
            >
              <RotateCcw className="h-3.5 w-3.5 animate-spin" />
              Stop Run
            </button>
          ) : (
            <button
              type="button"
              onClick={handleStartRun}
              className="flex items-center gap-2 px-5 py-2 rounded-xl bg-gradient-to-r from-[#e54d5e] to-[#c23b4c] hover:from-[#f05c6d] hover:to-[#d04657] text-white text-xs font-semibold shadow-lg shadow-rose-500/20 transition transform active:scale-95"
            >
              <Play className="h-4 w-4 fill-white" />
              Run Production Pipeline
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
              Pipeline Stages
            </div>

            <div className="space-y-2">
              {STAGES.map((st, idx) => {
                const isCurrent = isRunning && activeRun?.currentStage?.includes(st.key);
                const isDone = activeRun?.status === 'completed' || (activeRun?.progressPercent && activeRun.progressPercent >= (idx + 1) * 18);

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
                      <span className={`text-[11px] ${isDone ? 'text-emerald-400' : isCurrent ? 'text-blue-400' : 'text-zinc-600'}`}>
                        {isDone ? '✓' : isCurrent ? '●' : '○'}
                      </span>
                    </div>
                    <p className="text-[10px] text-zinc-500 mt-0.5">{st.desc}</p>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="mt-4 p-3 rounded-xl bg-white/[0.02] border border-white/[0.05] text-[11px] text-zinc-400">
            <div className="flex items-center justify-between">
              <span>Auto-Recovery:</span>
              <span className="text-emerald-400 font-semibold">Enabled (3 Retries)</span>
            </div>
          </div>
        </div>

        {/* PANE 2: Content & Prompt Editor (5 cols) */}
        <div className="lg:col-span-5 rounded-2xl bg-[#121622] border border-white/[0.08] p-4 overflow-y-auto space-y-4 text-xs">
          <div className="text-xs font-bold text-zinc-400 uppercase tracking-wider flex items-center gap-2">
            <FileText className="h-4 w-4 text-rose-400" />
            Content Specification
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">Target Page / Preset</label>
            <select
              value={activePageId || ''}
              onChange={(e) => onSelectPage(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-[#171b26] border border-white/[0.1] text-white focus:outline-none"
            >
              {pages.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.targetAudience || 'General'})
                </option>
              ))}
            </select>
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">Video Topic / Title</label>
            <input
              type="text"
              value={topic}
              onChange={(e) => setTopic(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500"
            />
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">Image Prompt (Grok / FLUX)</label>
            <textarea
              rows={3}
              value={imagePrompt}
              onChange={(e) => setImagePrompt(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500 resize-none font-mono text-[11px]"
            />
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">Video Motion Prompt</label>
            <textarea
              rows={2}
              value={videoPrompt}
              onChange={(e) => setVideoPrompt(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500 resize-none font-mono text-[11px]"
            />
          </div>

          <div>
            <label className="block text-zinc-400 mb-1 font-medium">Publishing Caption & Tags</label>
            <textarea
              rows={2}
              value={caption}
              onChange={(e) => setCaption(e.target.value)}
              className="w-full px-3 py-2 rounded-xl bg-white/[0.04] border border-white/[0.1] text-white focus:outline-none focus:border-rose-500 resize-none"
            />
          </div>

          {mode === 'advanced' && (
            <div className="pt-3 border-t border-white/[0.08] space-y-3 animate-in fade-in">
              <div className="text-[11px] font-bold text-amber-400 uppercase tracking-wider flex items-center gap-1.5">
                <Settings2 className="h-3.5 w-3.5" />
                Advanced Engine Overrides
              </div>

              <div>
                <label className="block text-zinc-400 mb-1 font-medium">Worker Pool</label>
                <select
                  value={preferredWorkerPool}
                  onChange={(e) => setPreferredWorkerPool(e.target.value)}
                  className="w-full px-3 py-1.5 rounded-xl bg-[#171b26] border border-white/[0.1] text-zinc-300 font-mono text-[11px]"
                >
                  <option value="grok_browser_pool_01">Grok Extension Pool (10 Profiles)</option>
                  <option value="local_openmontage_pool">OpenMontage Local Fast</option>
                  <option value="omniroute_cloud_pool">OmniRoute Direct Cloud API</option>
                </select>
              </div>

              <div className="flex items-center justify-between p-2 rounded-xl bg-white/[0.02]">
                <span className="text-zinc-400">Skip Research Crawl</span>
                <input
                  type="checkbox"
                  checked={skipResearch}
                  onChange={(e) => setSkipResearch(e.target.checked)}
                  className="h-4 w-4 rounded accent-rose-500"
                />
              </div>
            </div>
          )}
        </div>

        {/* PANE 3 & 4: Live Preview & Activity Log (4 cols) */}
        <div className="lg:col-span-4 flex flex-col gap-4 overflow-hidden">
          {/* Live Asset Preview */}
          <div className="flex-1 rounded-2xl bg-[#121622] border border-white/[0.08] p-4 flex flex-col overflow-hidden">
            <div className="text-xs font-bold text-zinc-400 uppercase tracking-wider mb-2 flex items-center gap-2">
              <Eye className="h-4 w-4 text-rose-400" />
              Live Stage Preview
            </div>

            <div className="flex-1 rounded-xl bg-black/40 border border-white/[0.05] flex flex-col items-center justify-center p-4 text-center">
              <Video className="h-8 w-8 text-zinc-600 mb-2" />
              <div className="text-xs font-medium text-zinc-400">
                {isRunning ? 'Synthesizing 9:16 Video Canvas...' : 'Ready for generation preview'}
              </div>
              <p className="text-[10px] text-zinc-600 mt-1 max-w-xs">
                Generated frames and video timeline drafts will stream directly into this view.
              </p>
            </div>
          </div>

          {/* Activity / Diagnostic Logs */}
          <div className="h-48 rounded-2xl bg-[#0d1017] border border-white/[0.08] p-3 flex flex-col font-mono text-[11px] overflow-hidden">
            <div className="text-zinc-500 text-[10px] font-bold uppercase tracking-wider mb-1 flex items-center gap-1.5">
              <Terminal className="h-3.5 w-3.5" />
              Execution Activity Log
            </div>
            <div className="flex-1 overflow-y-auto space-y-1 text-zinc-400 pt-1">
              <div className="text-zinc-600">[00:00] Floword Engine Ready.</div>
              <div className="text-zinc-600">[00:01] Target Profile Bound: {selectedPage?.name}</div>
              {isRunning && (
                <>
                  <div className="text-blue-400">[00:02] Initializing image diffusion pipeline...</div>
                  <div className="text-blue-400">[00:05] Applying 9:16 outpainting mask...</div>
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
