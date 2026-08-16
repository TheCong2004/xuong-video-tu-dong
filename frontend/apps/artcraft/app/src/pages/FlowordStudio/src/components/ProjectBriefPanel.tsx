import React, { useEffect, useState } from 'react';
import { FolderOpen, RefreshCw, Save } from 'lucide-react';
import { open } from '@tauri-apps/plugin-dialog';

import {
  fetchOmniRouteModels,
  getResearchCapabilities,
  OmniRouteModel,
  ResearchCapabilities,
  ContentPage,
  resolveFlowordOutputPath,
} from '../api/flowordClient';
import { WorkflowInput } from '../services/workflowEngine';
import { Folder } from 'lucide-react';

interface ProjectBriefPanelProps {
  input: WorkflowInput;
  onChangeInput: (newInput: WorkflowInput) => void;
  onSaveConfig: () => void;
  onLoadConfig: () => void;
  activePage?: ContentPage | null;
  onOpenEditPage?: (page: ContentPage) => void;
}

const fieldLabel = 'mb-1.5 block text-xs font-medium text-zinc-300';
const control = 'floword-control w-full px-3 py-2 text-sm placeholder:text-zinc-600';

export const ProjectBriefPanel: React.FC<ProjectBriefPanelProps> = ({
  input,
  onChangeInput,
  onSaveConfig,
  onLoadConfig,
  activePage,
  onOpenEditPage,
}) => {
  const [models, setModels] = useState<OmniRouteModel[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [researchCapabilities, setResearchCapabilities] = useState<ResearchCapabilities>({ platforms: [], modes: [] });
  const [researchCapabilitiesUnavailable, setResearchCapabilitiesUnavailable] = useState(false);

  const reloadModels = async () => {
    setLoadingModels(true);
    try {
      const result = await fetchOmniRouteModels();
      setModels(result);
    } catch {
      setModels([]);
    } finally {
      setLoadingModels(false);
    }
  };

  useEffect(() => {
    void reloadModels();
    void getResearchCapabilities()
      .then((capabilities) => {
        setResearchCapabilities(capabilities);
        setResearchCapabilitiesUnavailable(false);
      })
      .catch(() => setResearchCapabilitiesUnavailable(true));
  }, []);

  const change = <K extends keyof WorkflowInput>(field: K, value: WorkflowInput[K]) => {
    onChangeInput({ ...input, [field]: value });
  };

  const contentSource = input.contentSource || 'auto';

  return (
    <section className="floword-card p-5 md:p-6">
      <div className="mb-6 flex flex-wrap items-start justify-between gap-3 border-b border-white/[0.08] pb-4">
        <div>
          <h2 className="text-base font-semibold text-white">Project Brief</h2>
          <p className="mt-1 text-xs leading-5 text-zinc-500">Source and production settings for the next workflow run.</p>
        </div>
        <div className="flex gap-2">
          <button type="button" onClick={onLoadConfig} className="floword-button floword-button-secondary text-zinc-300">
            <RefreshCw className="h-3.5 w-3.5" /> Load
          </button>
          <button type="button" onClick={onSaveConfig} className="floword-button floword-button-secondary text-zinc-300">
            <Save className="h-3.5 w-3.5" /> Save
          </button>
        </div>
      </div>

      <div className="grid gap-5 lg:grid-cols-2">
        <div className="space-y-5">
          <div>
            <label className={fieldLabel} htmlFor="floword-project-name">Project</label>
            <input id="floword-project-name" className={control} value={input.workflowName} onChange={(event) => change('workflowName', event.target.value)} />
          </div>

          <div className="grid gap-4 sm:grid-cols-2">
            <div>
              <label className={fieldLabel} htmlFor="floword-workflow-mode">Workflow Mode</label>
              <select id="floword-workflow-mode" className={control} value={input.scriptMode} onChange={(event) => change('scriptMode', event.target.value as WorkflowInput['scriptMode'])}>
                <option value="original">Original creation</option>
                <option value="source_based">Source-based</option>
                <option value="commentary">Commentary</option>
                <option value="remix">Remix</option>
              </select>
            </div>

            <div>
              <label className={fieldLabel} htmlFor="floword-content-source">Content Source</label>
              <select
                id="floword-content-source"
                className={control}
                value={contentSource}
                onChange={(event) => {
                  const source = event.target.value as WorkflowInput['contentSource'];
                  change('contentSource', source);
                  if (source === 'trend_research') {
                    change('researchEnabled', true);
                  }
                }}
              >
                <option value="auto">Auto (detect from input)</option>
                <option value="prompt_only">Prompt Only</option>
                <option value="trend_research">Trend Research (MediaCrawler)</option>
                <option value="web_story">Web Story / Article</option>
                <option value="video_url">Video URL</option>
                <option value="local_media">Local Media</option>
              </select>
            </div>
          </div>

          {/* Conditional Source Controls */}
          {contentSource === 'trend_research' && (
            <div className="rounded-lg border border-white/[0.08] bg-black/20 p-4 space-y-4">
              <div className="grid gap-4 sm:grid-cols-2">
                <div>
                  <label className={fieldLabel} htmlFor="floword-research-platform">Research Platform</label>
                  <select id="floword-research-platform" className={control} value={input.researchPlatform} disabled={researchCapabilities.platforms.length === 0} onChange={(event) => change('researchPlatform', event.target.value as WorkflowInput['researchPlatform'])}>
                    {researchCapabilities.platforms.map((platform) => <option key={platform.value} value={platform.value}>{platform.label}</option>)}
                  </select>
                </div>
                <div>
                  <label className={fieldLabel} htmlFor="floword-research-mode">Research Mode</label>
                  <select id="floword-research-mode" className={control} value={input.researchMode} disabled={researchCapabilities.modes.length === 0} onChange={(event) => change('researchMode', event.target.value as WorkflowInput['researchMode'])}>
                    {researchCapabilities.modes.map((mode) => <option key={mode.value} value={mode.value}>{mode.label}</option>)}
                  </select>
                </div>
              </div>
              {input.researchPlatform === 'xhs' && (
                <div>
                  <label className={fieldLabel} htmlFor="floword-xhs-variant">XHS Variant</label>
                  <select
                    id="floword-xhs-variant"
                    className={control}
                    value={input.xhsVariant || 'mainland'}
                    onChange={(event) => change('xhsVariant', event.target.value as WorkflowInput['xhsVariant'])}
                  >
                    <option value="mainland">Xiaohongshu Mainland</option>
                    <option value="international">RedNote International</option>
                  </select>
                </div>
              )}
              <div>
                <label className={fieldLabel} htmlFor="floword-research-query">Research Query</label>
                <input id="floword-research-query" className={control} value={input.researchQuery} onChange={(event) => change('researchQuery', event.target.value)} placeholder="Keywords or trends to research" />
              </div>
            </div>
          )}

          {contentSource === 'web_story' && (
            <div>
              <label className={fieldLabel} htmlFor="floword-story-url">Web Story / Article URL</label>
              <input
                id="floword-story-url"
                type="url"
                className={`${control} font-mono text-xs`}
                value={input.storyUrl || input.sourceUrls[0] || ''}
                onChange={(event) => {
                  const url = event.target.value.trim();
                  change('storyUrl', url);
                  change('sourceUrls', url ? [url] : []);
                }}
                placeholder="https://example.com/article/tech-news"
              />
              <p className="mt-1.5 text-[11px] text-zinc-500">Floword extracts article text, title, and structure automatically.</p>
            </div>
          )}

          {contentSource === 'video_url' && (
            <div>
              <label className={fieldLabel} htmlFor="floword-video-url">Video URL</label>
              <input
                id="floword-video-url"
                type="url"
                className={`${control} font-mono text-xs`}
                value={input.sourceUrls[0] || ''}
                onChange={(event) => change('sourceUrls', event.target.value.trim() ? [event.target.value.trim()] : [])}
                placeholder="https://www.youtube.com/watch?v=..."
              />
            </div>
          )}

          {contentSource === 'local_media' && (
            <div>
              <label className={fieldLabel}>Local Media File</label>
              <button
                type="button"
                onClick={async () => {
                  const selected = await open({ multiple: true, filters: [{ name: 'Media', extensions: ['mp4', 'mov', 'mkv', 'webm', 'mp3', 'wav', 'm4a'] }] });
                  if (typeof selected === 'string') change('sourceFiles', [selected]);
                  else if (Array.isArray(selected)) change('sourceFiles', selected);
                }}
                className="floword-button floword-button-secondary w-full justify-between text-zinc-300"
              >
                <span className="flex min-w-0 items-center gap-2"><FolderOpen className="h-4 w-4 shrink-0" /><span className="truncate">{input.sourceFiles.length ? input.sourceFiles.join(', ') : 'Choose local file'}</span></span>
                <span className="text-xs text-zinc-500">Browse</span>
              </button>
            </div>
          )}

          {contentSource === 'auto' && (
            <div>
              <label className={fieldLabel} htmlFor="floword-source">Source URL</label>
              <textarea
                id="floword-source"
                rows={3}
                className={`${control} resize-y font-mono text-xs leading-5`}
                value={input.sourceUrls.join('\n')}
                onChange={(event) => change('sourceUrls', event.target.value.split('\n').map((url) => url.trim()).filter(Boolean))}
                placeholder="One source URL per line (video or article)"
              />
              <button
                type="button"
                onClick={async () => {
                  const selected = await open({ multiple: true, filters: [{ name: 'Media', extensions: ['mp4', 'mov', 'mkv', 'webm', 'mp3', 'wav', 'm4a'] }] });
                  if (typeof selected === 'string') change('sourceFiles', [selected]);
                  else if (Array.isArray(selected)) change('sourceFiles', selected);
                }}
                className="floword-button floword-button-secondary mt-2 w-full justify-between text-zinc-300"
              >
                <span className="flex min-w-0 items-center gap-2"><FolderOpen className="h-4 w-4 shrink-0" /><span className="truncate">{input.sourceFiles.length ? input.sourceFiles.join(', ') : 'Choose local file'}</span></span>
                <span className="text-xs text-zinc-500">Browse</span>
              </button>
            </div>
          )}

          <div>
            <label className={fieldLabel} htmlFor="floword-prompt">Prompt</label>
            <textarea
              id="floword-prompt"
              rows={contentSource === 'prompt_only' ? 9 : 6}
              required
              className={`${control} resize-y leading-6`}
              value={input.prompt}
              onChange={(event) => change('prompt', event.target.value)}
              placeholder="Describe the video, audience, structure, and call to action…"
            />
          </div>
        </div>

        <div className="grid content-start gap-5 sm:grid-cols-2">
          <div className="sm:col-span-2">
            <label className={fieldLabel} htmlFor="floword-model">AI Model</label>
            <select id="floword-model" className={control} value={input.modelId || 'auto'} onChange={(event) => change('modelId', event.target.value)}>
              <option value="auto">Auto route</option>
              {models.map((model) => <option key={model.id} value={model.id}>{model.id}</option>)}
            </select>
            {models.length === 0 && (
              <div className="mt-1.5 flex items-center justify-between text-[11px] text-zinc-500">
                <span>OmniRoute model catalog is currently unavailable / degraded.</span>
                <button type="button" onClick={reloadModels} className="text-primary hover:underline flex items-center gap-1">
                  <RefreshCw className={`h-3 w-3 ${loadingModels ? 'animate-spin' : ''}`} /> Retry
                </button>
              </div>
            )}
          </div>

          <div>
            <label className={fieldLabel} htmlFor="floword-voice">Voice</label>
            <select id="floword-voice" className={control} value={input.tone} onChange={(event) => change('tone', event.target.value as WorkflowInput['tone'])}>
              <option value="professional">Professional</option>
              <option value="storytelling">Storytelling</option>
              <option value="educational">Educational</option>
              <option value="review">Review</option>
              <option value="viral">Viral</option>
            </select>
          </div>

          <div>
            <label className={fieldLabel} htmlFor="floword-language">Language</label>
            <input id="floword-language" className={control} value={input.language} onChange={(event) => change('language', event.target.value)} />
          </div>

          <div>
            <label className={fieldLabel} htmlFor="floword-duration">Duration</label>
            <div className="relative"><input id="floword-duration" type="number" min={1} className={`${control} pr-14`} value={input.targetDurationSeconds} onChange={(event) => change('targetDurationSeconds', Number(event.target.value))} /><span className="pointer-events-none absolute right-3 top-2.5 text-xs text-zinc-500">sec</span></div>
          </div>

          <div>
            <label className={fieldLabel} htmlFor="floword-platform">Target Platform</label>
            <select id="floword-platform" className={control} value={input.targetPlatform} onChange={(event) => change('targetPlatform', event.target.value as WorkflowInput['targetPlatform'])}>
              <option value="tiktok">TikTok</option><option value="reels">Instagram Reels</option><option value="youtube_shorts">YouTube Shorts</option>
            </select>
          </div>

          <div>
            <label className={fieldLabel} htmlFor="floword-aspect">Format</label>
            <select id="floword-aspect" className={control} value={input.aspectRatio} onChange={(event) => change('aspectRatio', event.target.value as WorkflowInput['aspectRatio'])}>
              <option value="9:16">9:16 Vertical</option><option value="16:9">16:9 Landscape</option><option value="1:1">1:1 Square</option>
            </select>
          </div>

          <div>
            <label className={fieldLabel} htmlFor="floword-output">Output Mode</label>
            <select id="floword-output" className={control} value={input.outputMode} onChange={(event) => change('outputMode', event.target.value as WorkflowInput['outputMode'])}>
              <option value="draft_only">CapCut draft</option><option value="render_video">Rendered video</option>
            </select>
          </div>

          <div className="sm:col-span-2 rounded-lg border border-indigo-500/20 bg-indigo-950/20 p-3.5">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-1.5 text-xs font-semibold text-indigo-300">
                <Folder className="h-3.5 w-3.5" />
                <span>Output Destination</span>
              </div>
              {activePage && onOpenEditPage && (
                <button
                  type="button"
                  onClick={() => onOpenEditPage(activePage)}
                  className="text-[11px] font-medium text-indigo-400 hover:text-indigo-200 transition"
                >
                  Change Page Settings
                </button>
              )}
            </div>
            {activePage ? (
              <div className="mt-2 space-y-1">
                <div className="rounded bg-black/40 px-2.5 py-1.5 font-mono text-[11px] text-indigo-200 truncate select-all border border-indigo-500/10">
                  {activePage.output_root.replace(/[\\/]+$/, '')}\{activePage.name}\&lt;DD-MM-YYYY&gt;\
                </div>
                <p className="text-[11px] text-zinc-400">
                  Auto-saved by Page and local date. No manual folder selection needed.
                </p>
              </div>
            ) : (
              <div className="mt-2 text-xs text-amber-400">
                Select a Page before running the workflow.
              </div>
            )}
          </div>

          {contentSource === 'auto' && (
            <div className="sm:col-span-2 rounded-lg border border-white/[0.08] bg-black/20 p-4">
              <label className="flex items-center justify-between gap-3 text-sm font-medium text-zinc-200" htmlFor="floword-research-enabled">
                <span>Research</span>
                <input id="floword-research-enabled" type="checkbox" checked={input.researchEnabled} onChange={(event) => change('researchEnabled', event.target.checked)} />
              </label>
              {input.researchEnabled && (
                <div className="mt-4 grid gap-4 sm:grid-cols-2">
                  <div>
                    <label className={fieldLabel} htmlFor="floword-research-platform">Research Platform</label>
                    <select id="floword-research-platform" className={control} value={input.researchPlatform} disabled={researchCapabilities.platforms.length === 0} onChange={(event) => change('researchPlatform', event.target.value as WorkflowInput['researchPlatform'])}>
                      {researchCapabilities.platforms.map((platform) => <option key={platform.value} value={platform.value}>{platform.label}</option>)}
                    </select>
                    <p className="mt-1.5 text-[11px] text-zinc-600">TikTok global is not supported by the bundled crawler and is not mapped to Douyin.</p>
                  </div>
                  <div>
                    <label className={fieldLabel} htmlFor="floword-research-mode">Research Mode</label>
                    <select id="floword-research-mode" className={control} value={input.researchMode} disabled={researchCapabilities.modes.length === 0} onChange={(event) => change('researchMode', event.target.value as WorkflowInput['researchMode'])}>
                      {researchCapabilities.modes.map((mode) => <option key={mode.value} value={mode.value}>{mode.label}</option>)}
                    </select>
                  </div>
                  {input.researchPlatform === 'xhs' && (
                    <div className="sm:col-span-2">
                      <label className={fieldLabel} htmlFor="floword-xhs-variant">XHS Variant</label>
                      <select
                        id="floword-xhs-variant"
                        className={control}
                        value={input.xhsVariant || 'mainland'}
                        onChange={(event) => change('xhsVariant', event.target.value as WorkflowInput['xhsVariant'])}
                      >
                        <option value="mainland">Xiaohongshu Mainland</option>
                        <option value="international">RedNote International</option>
                      </select>
                    </div>
                  )}
                  {researchCapabilitiesUnavailable && (
                    <p className="sm:col-span-2 text-[11px] text-amber-400">MediaCrawler capability catalog is unavailable. Start the unified backend to choose a research platform and mode.</p>
                  )}
                  <div className="sm:col-span-2">
                    <label className={fieldLabel} htmlFor="floword-research-query">{input.researchMode === 'search' ? 'Research Query' : input.researchMode === 'detail' ? 'Post / Video IDs' : 'Creator IDs'}</label>
                    <input id="floword-research-query" className={control} value={input.researchQuery} onChange={(event) => change('researchQuery', event.target.value)} placeholder={input.researchMode === 'search' ? 'Keywords to research' : 'Comma-separated IDs or supported URLs'} />
                  </div>
                </div>
              )}
            </div>
          )}

          <div className="sm:col-span-2">
            <button
              type="button"
              onClick={() => {
                void open({ multiple: false, filters: [{ name: 'Audio', extensions: ['mp3', 'wav', 'm4a', 'aac', 'flac'] }] }).then((selected) => {
                  if (typeof selected === 'string') change('musicPath', selected);
                });
              }}
              className="floword-button floword-button-secondary w-full justify-between text-zinc-300"
            >
              <span className="flex min-w-0 items-center gap-2"><FolderOpen className="h-4 w-4 shrink-0" /><span className="truncate">{input.musicPath || 'Choose local audio'}</span></span>
              <span className="text-xs text-zinc-500">Browse</span>
            </button>
          </div>
        </div>
      </div>
    </section>
  );
};
