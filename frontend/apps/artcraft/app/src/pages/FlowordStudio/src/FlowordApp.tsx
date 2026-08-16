import React, { useState, useEffect, useRef, useCallback } from 'react';
import toast, { Toaster } from 'react-hot-toast';
import { FlowordHeader } from './components/FlowordHeader';
import { ExecutionPlanView } from './components/ExecutionPlanView';
import { ConfigureDrawer } from './components/ConfigureDrawer';
import { StepDetailModal } from './components/StepDetailModal';
import { PageManagementModal } from './components/PageManagementModal';
import {
  WorkflowInput,
  WorkflowRun,
  ArtifactRef,
  StepRun,
  StepConfig,
  DEFAULT_WORKFLOW_INPUT,
  INITIAL_STEP_CONFIGS,
  ACTIVE_JOB_ID_KEY,
  ACTIVE_PAGE_ID_KEY,
  migrateLegacyLocalStorageKeys,
} from './services/workflowEngine';
import {
  DetailedReadinessStatus,
  DEFAULT_READINESS,
  fetchDetailedReadiness,
  enqueueFlowordWorkflow,
  getFlowordWorkflow,
  cancelFlowordWorkflow,
  retryFlowordStep,
  skipFlowordResearch,
  getResearchSessionStatus,
  FlowordCommandError,
  GetFlowordWorkflowResponse,
  ContentPage,
  listContentPages,
  createContentPage,
  updateContentPage,
  archiveContentPage,
  CreateContentPageRequest,
  UpdateContentPageRequest,
} from './api/flowordClient';
import { buildCookieProxyInvokeOptions, loadNetworkSettings } from '../../PageYouwee/lib/network-config';
import { mergeBackendStageStates } from './services/stageStateMapping';

const POLL_INTERVAL_MS = 2000;

function backendProgress(stage: string, status: string): number {
  if (status === 'complete_success') return 100;
  if (stage.includes('ingest_analyze')) return 29;
  if (stage.includes('preflight')) return 14;
  if (stage.includes('research')) return 43;
  if (stage.includes('script')) return 57;
  if (stage.includes('voice')) return 71;
  if (stage.includes('media_timeline') || stage.includes('caption')) return 86;
  if (stage.includes('draft') || stage.includes('render') || stage.includes('capcut')) return 95;
  return 0;
}

/// Backend stages that are terminal — polling stops when one is observed.
const TERMINAL_STAGES = new Set([
  'completed',
  'draft_ready',
  'failed',
  'cancelled',
]);

/// Backend statuses that are terminal.
const TERMINAL_STATUSES = new Set([
  'complete_success',
  'complete_failure',
  'cancelled_by_user',
  'cancelled_by_provider',
  'cancelled_by_us',
  'dead',
]);

function parseWorkflowOutputs(value: string | null | undefined): {
  artifacts: ArtifactRef[];
  draftUrl?: string;
  videoUrl?: string;
} {
  if (!value) return { artifacts: [] };
  try {
    const output = JSON.parse(value) as Record<string, unknown>;
    const artifacts = ['script_artifact', 'capcut_artifact'].flatMap((key) => {
      const raw = output[key];
      if (!raw || typeof raw !== 'object') return [];
      const artifact = raw as Record<string, unknown>;
      const path = String(artifact.path ?? '');
      const type = String(artifact.artifact_type ?? 'trace') as ArtifactRef['type'];
      if (!path || !artifact.id || !artifact.workflow_id || !artifact.step_id) return [];
      return [{
        id: String(artifact.id),
        workflowId: String(artifact.workflow_id),
        stepId: String(artifact.step_id),
        type,
        name: path.split(/[\\/]/).pop() || type,
        path,
        sizeBytes: Number(artifact.size_bytes ?? 0),
        mimeType: String(artifact.mime_type ?? 'application/octet-stream'),
        sha256: artifact.sha256 ? String(artifact.sha256) : undefined,
        createdByStep: String(artifact.producer ?? artifact.step_id),
        createdAt: String(artifact.created_at ?? ''),
        metadata: artifact.metadata && typeof artifact.metadata === 'object' ? artifact.metadata as Record<string, unknown> : undefined,
      } satisfies ArtifactRef];
    });
    return {
      artifacts,
      draftUrl: typeof output.draft_url === 'string' ? output.draft_url : undefined,
      videoUrl: typeof output.video_url === 'string' ? output.video_url : undefined,
    };
  } catch {
    return { artifacts: [] };
  }
}

interface FlowordAppProps {
  onOpenCapCutAutomation?: () => void;
}

export const FlowordApp: React.FC<FlowordAppProps> = ({ onOpenCapCutAutomation }) => {
  const [configureOpen, setConfigureOpen] = useState(false);

  const [workflowInput, setWorkflowInput] = useState<WorkflowInput>(DEFAULT_WORKFLOW_INPUT);
  const [stepConfigs, setStepConfigs] = useState<StepConfig[]>(INITIAL_STEP_CONFIGS);

  const [stepRuns, setStepRuns] = useState<StepRun[]>(() =>
    INITIAL_STEP_CONFIGS.map((sc) => ({
      ...sc,
      status: 'ready',
      progress: 0,
      logs: [],
      artifacts: [],
      retryCount: 0,
    }))
  );

  const [activeStepIndex, setActiveStepIndex] = useState<number>(-1);
  const [running, setRunning] = useState<boolean>(false);
  const [progress, setProgress] = useState<number>(0);
  const [currentStepMessage, setCurrentStepMessage] = useState<string>('Ready to enqueue Rust Workflow Worker');
  const [logs, setLogs] = useState<string[]>([
    '🟢 [NEODONUT ENGINE] Rust Backend Task System initialized.',
    '💡 Enqueue commands dispatch directly to the Rust Worker Thread & SQLite database.',
  ]);
  const [detailModalStepId, setDetailModalStepId] = useState<string | null>(null);
  const [activeWorkflowRun, setActiveWorkflowRun] = useState<WorkflowRun | null>(null);
  const [activeJobId, setActiveJobId] = useState<string | null>(null);

  // ContentPages Domain State
  const [pages, setPages] = useState<ContentPage[]>([]);
  const [activePageId, setActivePageId] = useState<string | null>(() => {
    return localStorage.getItem(ACTIVE_PAGE_ID_KEY) || null;
  });
  const [isPageModalOpen, setIsPageModalOpen] = useState(false);
  const [pageToEdit, setPageToEdit] = useState<ContentPage | null>(null);

  const loadPages = useCallback(async () => {
    try {
      const pageList = await listContentPages(false);
      setPages(pageList);
      if (pageList.length > 0) {
        const storedId = localStorage.getItem(ACTIVE_PAGE_ID_KEY);
        const match = storedId ? pageList.find((p) => p.id === storedId) : null;
        if (match) {
          setActivePageId(match.id);
        } else if (!storedId || !pageList.some((p) => p.id === activePageId)) {
          setActivePageId(pageList[0].id);
          localStorage.setItem(ACTIVE_PAGE_ID_KEY, pageList[0].id);
        }
      } else {
        setActivePageId(null);
        localStorage.removeItem(ACTIVE_PAGE_ID_KEY);
      }
    } catch (err) {
      console.error('Failed to load content pages:', err);
    }
  }, [activePageId]);

  useEffect(() => {
    void loadPages();
  }, [loadPages]);

  const handleSelectPage = (pageId: string) => {
    setActivePageId(pageId);
    localStorage.setItem(ACTIVE_PAGE_ID_KEY, pageId);
    const selected = pages.find((p) => p.id === pageId);
    if (selected) {
      setWorkflowInput((prev) => ({
        ...prev,
        pageId: selected.id,
        targetPlatform: (selected.target_platform as WorkflowInput['targetPlatform']) || prev.targetPlatform,
        language: selected.default_language || prev.language,
        tone: (selected.default_tone as WorkflowInput['tone']) || prev.tone,
        aspectRatio: (selected.default_aspect_ratio as WorkflowInput['aspectRatio']) || prev.aspectRatio,
      }));
    }
  };

  const handleSavePage = async (data: CreateContentPageRequest | UpdateContentPageRequest) => {
    if ('id' in data && data.id) {
      const updated = await updateContentPage(data as UpdateContentPageRequest);
      toast.success(`Đã cập nhật Page "${updated.name}"`);
    } else {
      const created = await createContentPage(data as CreateContentPageRequest);
      toast.success(`Đã tạo Page "${created.name}"`);
      setActivePageId(created.id);
      localStorage.setItem(ACTIVE_PAGE_ID_KEY, created.id);
    }
    await loadPages();
  };

  const handleArchivePage = async (pageId: string) => {
    await archiveContentPage(pageId, true);
    toast.success('Đã lưu trữ Page');
    if (activePageId === pageId) {
      setActivePageId(null);
      localStorage.removeItem(ACTIVE_PAGE_ID_KEY);
    }
    await loadPages();
  };

  const handleOpenCreatePage = () => {
    setPageToEdit(null);
    setIsPageModalOpen(true);
  };

  const handleOpenEditPage = (page: ContentPage) => {
    setPageToEdit(page);
    setIsPageModalOpen(true);
  };

  const [readiness, setReadiness] = useState<DetailedReadinessStatus>(DEFAULT_READINESS);

  // Single polling timer + the job id it is bound to. Guards against duplicate
  // timers and stale polling of an old job after a new enqueue.
  const pollingTimerRef = useRef<number | null>(null);
  const pollingJobIdRef = useRef<string | null>(null);
  // A one-shot latch so WORKFLOW_NOT_FOUND surfaces a single toast, not one per tick.
  const notFoundNotifiedRef = useRef<boolean>(false);
  const authStatusPollInFlightRef = useRef<boolean>(false);
  const authResumeInFlightRef = useRef<boolean>(false);

  const appendLog = useCallback((msg: string) => {
    const timestamp = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev.slice(-150), `[${timestamp}] ${msg}`]);
  }, []);

  // ---- Readiness polling (backend-driven, no hard-coded READY) --------------
  useEffect(() => {
    let cancelled = false;
    const updateReadiness = async () => {
      const res = await fetchDetailedReadiness();
      if (!cancelled) setReadiness(res);
    };
    updateReadiness();
    const interval = window.setInterval(updateReadiness, 5000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, []);

  // ---- Polling lifecycle ----------------------------------------------------
  const stopWorkflowPolling = useCallback(() => {
    if (pollingTimerRef.current !== null) {
      window.clearInterval(pollingTimerRef.current);
      pollingTimerRef.current = null;
    }
    pollingJobIdRef.current = null;
  }, []);

  const applyStatusToUi = useCallback((res: GetFlowordWorkflowResponse) => {
    const stage = res.current_stage || '';
    const status = res.status || '';

    let stageIdx = 0;
    if (stage.includes('script')) stageIdx = 1;
    else if (stage.includes('media_timeline')) stageIdx = 4;
    else if (stage.includes('draft_creating') || stage.includes('draft_created')) stageIdx = 3;
    else if (stage.includes('caption')) stageIdx = 4;
    else if (stage.includes('draft_saving') || stage.includes('draft_ready')) stageIdx = 5;
    else if (stage.includes('render') || stage.includes('completed')) stageIdx = 5;

    setActiveStepIndex(stageIdx);
    const stageProgress = backendProgress(stage, status);
    const failed = status === 'complete_failure' || stage === 'failed';
    setProgress(stageProgress);
    setCurrentStepMessage(`[Rust Backend] stage=${stage} status=${status}`);
    setStepRuns((prev) => {
      if (res.stage_states && res.stage_states.length > 0) {
        return mergeBackendStageStates(prev, res);
      }
      const progressMapped = prev.map((s, idx) =>
        idx === stageIdx
          ? { ...s, status: status === 'complete_success' ? 'succeeded' : failed ? 'failed' : 'running', progress: stageProgress }
          : idx < stageIdx
          ? { ...s, status: 'succeeded', progress: 100 }
          : s
      ) as StepRun[];
      return mergeBackendStageStates(progressMapped, res);
    });
    const outputs = parseWorkflowOutputs(res.stage_outputs);
    setActiveWorkflowRun((prev) => prev ? {
      ...prev,
      artifacts: outputs.artifacts.length ? outputs.artifacts : prev.artifacts,
      finalDraftUrl: outputs.draftUrl ?? prev.finalDraftUrl,
      finalVideoUrl: outputs.videoUrl ?? prev.finalVideoUrl,
      resultType: outputs.videoUrl ? 'video' : outputs.draftUrl ? 'draft' : prev.resultType,
    } : prev);
  }, []);

  const handleWorkflowNotFound = useCallback(
    (jobId: string) => {
      stopWorkflowPolling();
      // Drop the stale id so the app boots idle next time.
      localStorage.removeItem(ACTIVE_JOB_ID_KEY);
      setActiveJobId(null);
      setRunning(false);
      setActiveStepIndex(-1);
      setActiveWorkflowRun(null);
      if (!notFoundNotifiedRef.current) {
        notFoundNotifiedRef.current = true;
        appendLog(`⚠️ [WORKFLOW_NOT_FOUND] Job ${jobId} not found in backend. Cleared active job.`);
        toast.error('Không tìm thấy workflow job trong backend. Đã xóa job cũ.');
      }
    },
    [appendLog, stopWorkflowPolling]
  );

  const pollOnce = useCallback(
    async (jobId: string) => {
      // Guard: never poll an id that is no longer the active polling target.
      if (pollingJobIdRef.current !== jobId) return;

      let res: GetFlowordWorkflowResponse;
      try {
        res = await getFlowordWorkflow(jobId);
      } catch (err) {
        if (err instanceof FlowordCommandError && err.errorCode === 'WORKFLOW_NOT_FOUND') {
          handleWorkflowNotFound(jobId);
          return;
        }
        // Transient/unexpected error: log once per tick but keep polling.
        appendLog(`❌ [POLL_ERROR] ${err instanceof Error ? err.message : String(err)}`);
        return;
      }

      applyStatusToUi(res);

      if (res.status !== 'waiting_input') {
        authResumeInFlightRef.current = false;
      } else if (!authStatusPollInFlightRef.current && !authResumeInFlightRef.current) {
        authStatusPollInFlightRef.current = true;
        try {
          const session = await getResearchSessionStatus(workflowInput.researchPlatform, workflowInput.xhsVariant);
          if (session.status === 'CONNECTED') {
            authResumeInFlightRef.current = true;
            const resumed = await retryFlowordStep(jobId, 'research');
            appendLog(`🔁 [AUTH_RESUME] ${session.platform} connected; resumed ${resumed.resumed_stage} on same job ${jobId}.`);
          } else if (session.status === 'AWAITING_LOGIN') {
            setCurrentStepMessage('Waiting for RedNote authentication...');
          }
        } catch (error) {
          appendLog(`⚠️ [AUTH_STATUS] ${error instanceof Error ? error.message : String(error)}`);
        } finally {
          authStatusPollInFlightRef.current = false;
        }
      }

      const isTerminal = TERMINAL_STATUSES.has(res.status) || TERMINAL_STAGES.has(res.current_stage);
      if (isTerminal) {
        stopWorkflowPolling();
        setRunning(false);
        setActiveStepIndex(-1);
        const isSuccess = res.status === 'complete_success';
        const terminalProgress = backendProgress(res.current_stage, res.status);
        setProgress(terminalProgress);
        setActiveWorkflowRun((prev) =>
          prev
            ? {
                ...prev,
                status: isSuccess ? (res.current_stage === 'draft_ready' ? 'draft_ready' : 'completed') : 'failed',
                progress: terminalProgress,
                completedAt: new Date().toISOString(),
                errorMessage: res.failure_message ?? undefined,
              }
            : prev
        );

        if (isSuccess) {
          appendLog(`🎉 [WORKER COMPLETE] Job ${jobId} finished (stage=${res.current_stage}).`);
          toast.success('Pipeline hoàn tất ở backend!');
        } else if (res.status.startsWith('cancelled')) {
          appendLog(`🛑 [CANCELLED] Job ${jobId} cancelled.`);
        } else {
          appendLog(`❌ [WORKER FAILED] Job ${jobId}: ${res.failure_message ?? res.status}`);
          toast.error(`Job thất bại: ${res.failure_message ?? res.status}`);
        }
      }
    },
    [appendLog, applyStatusToUi, handleWorkflowNotFound, stopWorkflowPolling, workflowInput.researchPlatform, workflowInput.xhsVariant]
  );

  const startWorkflowPolling = useCallback(
    (jobId: string) => {
      if (!jobId) return;
      // Tear down any prior timer so we never run two, and never poll an old id.
      stopWorkflowPolling();
      notFoundNotifiedRef.current = false;
      pollingJobIdRef.current = jobId;
      // Fire immediately, then on interval.
      void pollOnce(jobId);
      pollingTimerRef.current = window.setInterval(() => {
        void pollOnce(jobId);
      }, POLL_INTERVAL_MS);
    },
    [pollOnce, stopWorkflowPolling]
  );

  // Stop polling on unmount.
  useEffect(() => {
    return () => stopWorkflowPolling();
  }, [stopWorkflowPolling]);

  // ---- Restore active job from LocalStorage on mount ------------------------
  useEffect(() => {
    migrateLegacyLocalStorageKeys();
    const jobId = localStorage.getItem(ACTIVE_JOB_ID_KEY);
    if (!jobId) return;

    let cancelled = false;
    (async () => {
      try {
        const res = await getFlowordWorkflow(jobId);
        if (cancelled) return;
        setActiveJobId(jobId);
        applyStatusToUi(res);
        const isTerminal = TERMINAL_STATUSES.has(res.status) || TERMINAL_STAGES.has(res.current_stage);
        if (!isTerminal) {
          setRunning(true);
          startWorkflowPolling(jobId);
          appendLog(`♻️ [RESTORE] Resumed polling active job ${jobId}.`);
        } else {
          appendLog(`♻️ [RESTORE] Active job ${jobId} already terminal (${res.status}).`);
        }
      } catch (err) {
        if (cancelled) return;
        if (err instanceof FlowordCommandError && err.errorCode === 'WORKFLOW_NOT_FOUND') {
          localStorage.removeItem(ACTIVE_JOB_ID_KEY);
          setActiveJobId(null);
          appendLog(`♻️ [RESTORE] Stale job ${jobId} not found — cleared. Idle.`);
        } else {
          appendLog(`♻️ [RESTORE_ERROR] ${err instanceof Error ? err.message : String(err)}`);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ---- Config persistence (unchanged behavior) ------------------------------
  const handleSaveConfig = () => {
    localStorage.setItem('neodonut_project_input', JSON.stringify(workflowInput));
    localStorage.setItem('neodonut_step_configs', JSON.stringify(stepConfigs));
    appendLog('💾 [CONFIG] Saved workflow input and step configuration.');
    toast.success('Đã lưu cấu hình Workflow!');
  };

  const handleLoadConfig = () => {
    try {
      const savedInput = localStorage.getItem('neodonut_project_input');
      const savedSteps = localStorage.getItem('neodonut_step_configs');
      if (savedInput) {
        const restored = { ...DEFAULT_WORKFLOW_INPUT, ...JSON.parse(savedInput) } as WorkflowInput;
        setWorkflowInput(restored);
        setStepConfigs((current) => current.map((step) => step.module === 'media_crawler' ? { ...step, enabled: restored.researchEnabled } : step));
      }
      if (savedSteps) {
        const parsed = JSON.parse(savedSteps);
        setStepConfigs(parsed);
        setStepRuns((prev) =>
          prev.map((sr) => {
            const match = parsed.find((p: StepConfig) => p.id === sr.id);
            return match ? { ...sr, ...match } : sr;
          })
        );
      }
      appendLog('📂 [CONFIG] Loaded saved workflow configuration.');
      toast.success('Đã tải cấu hình Workflow đã lưu!');
    } catch {
      toast.error('Could not load saved workflow configuration');
    }
  };

  // ---- Execute ---------------------------------------------------------------
  const handleExecuteWorkflow = async () => {
    if (running) return;

    if (!activePageId) {
      toast.error('Select a Page before running the workflow.');
      return;
    }

    if (!workflowInput.prompt.trim() && workflowInput.sourceUrls.length === 0) {
      toast.error('Vui lòng nhập Main Prompt hoặc ít nhất 1 Source URL!');
      return;
    }
    if (workflowInput.researchEnabled && !workflowInput.researchQuery.trim()) {
      toast.error('Research Query is required when Research is enabled.');
      return;
    }

    // Clear any prior run's polling before enqueuing a fresh job.
    stopWorkflowPolling();
    setProgress(5);
    setActiveStepIndex(0);
    setCurrentStepMessage('Enqueuing workflow into Rust Task Database...');
    appendLog(`🚀 [TAURI INVOKE] enqueue_floword_workflow...`);

    let jobId: string;
    try {
      const { cookieSettings, proxySettings } = loadNetworkSettings();
      const youweeNetwork = buildCookieProxyInvokeOptions(cookieSettings, proxySettings);
      const res = await enqueueFlowordWorkflow({
        page_id: activePageId,
        workflow_name: workflowInput.workflowName,
        prompt: workflowInput.prompt,
        topic: workflowInput.topic,
        source_urls: workflowInput.sourceUrls,
        source_files: workflowInput.sourceFiles,
        workflow_mode: workflowInput.scriptMode,
        content_source: workflowInput.contentSource,
        story_url: workflowInput.storyUrl,
        target_platform: workflowInput.targetPlatform,
        aspect_ratio: workflowInput.aspectRatio,
        target_duration_seconds: workflowInput.targetDurationSeconds,
        output_mode: workflowInput.outputMode,
        model_id: workflowInput.modelId,
        language: workflowInput.language,
        research_enabled: workflowInput.researchEnabled,
        research_platform: workflowInput.researchPlatform,
        research_query: workflowInput.researchQuery,
        research_mode: workflowInput.researchMode,
        xhs_variant: workflowInput.researchPlatform === 'xhs' ? (workflowInput.xhsVariant || 'mainland') : undefined,
        cookie_mode: youweeNetwork.cookieMode,
        cookie_browser: youweeNetwork.cookieBrowser ?? undefined,
        cookie_browser_profile: youweeNetwork.cookieBrowserProfile ?? undefined,
        cookie_file_path: youweeNetwork.cookieFilePath ?? undefined,
        cookie_skip_patterns: youweeNetwork.cookieSkipPatterns,
      });
      jobId = res.job_id;
    } catch (err) {
      // Enqueue failed: show the REAL error, do NOT invent an id, do NOT poll,
      // do NOT persist, do NOT claim success.
      const code = err instanceof FlowordCommandError ? err.errorCode : undefined;
      const msg = err instanceof Error ? err.message : String(err);
      setProgress(0);
      setActiveStepIndex(-1);
      setCurrentStepMessage('Enqueue failed.');
      appendLog(`❌ [ENQUEUE_FAILED] ${code ? `[${code}] ` : ''}${msg}`);
      toast.error(`Enqueue thất bại${code ? ` (${code})` : ''}: ${msg}`);
      return;
    }

    // Success: persist ONLY the real job id and start polling it.
    localStorage.setItem(ACTIVE_JOB_ID_KEY, jobId);
    setActiveJobId(jobId);
    setRunning(true);
    appendLog(`✓ [RUST BACKEND] Enqueued. job_id=${jobId}`);

    const cleanSteps = INITIAL_STEP_CONFIGS.map((config) => ({
      ...config,
      status: 'ready' as StepRun['status'],
      progress: 0,
      logs: [],
      artifacts: [],
    }));
    setStepRuns(cleanSteps);

    const initialRun: WorkflowRun = {
      id: jobId,
      workflowName: workflowInput.workflowName || 'CapCut Campaign Run',
      input: workflowInput,
      status: 'running',
      currentStepId: 'step-1',
      progress: 10,
      createdAt: new Date().toISOString(),
      startedAt: new Date().toISOString(),
      steps: cleanSteps.map((s) => ({ ...s, status: 'queued', progress: 0, logs: [], artifacts: [] })),
      artifacts: [],
    };
    setActiveWorkflowRun(initialRun);

    startWorkflowPolling(jobId);
  };

  const handleCancelWorkflow = async () => {
    const jobId = activeJobId ?? activeWorkflowRun?.id;
    if (!jobId) {
      // Nothing running — just reset local UI.
      stopWorkflowPolling();
      setRunning(false);
      setActiveStepIndex(-1);
      return;
    }
    stopWorkflowPolling();
    setRunning(false);
    setActiveStepIndex(-1);
    try {
      await cancelFlowordWorkflow(jobId);
      appendLog(`🛑 [CANCELLED] Sent cancel_floword_workflow for ${jobId}.`);
      toast('Đã gửi lệnh hủy tới backend.', { icon: '🛑' });
    } catch (err) {
      if (err instanceof FlowordCommandError && err.errorCode === 'WORKFLOW_NOT_FOUND') {
        localStorage.removeItem(ACTIVE_JOB_ID_KEY);
        setActiveJobId(null);
        appendLog(`⚠️ [CANCEL] Job ${jobId} not found (already gone).`);
        return;
      }
      appendLog(`❌ [CANCEL_ERROR] ${err instanceof Error ? err.message : String(err)}`);
      toast.error('Lệnh hủy thất bại.');
    }
  };

  const handleRetryStep = async (stepId: string) => {
    const jobId = activeJobId ?? activeWorkflowRun?.id;
    if (!jobId) {
      toast.error('Không có job đang chạy để retry.');
      return;
    }
    try {
      const res = await retryFlowordStep(jobId, stepId);
      appendLog(`🔁 [RETRY] Step ${stepId} → resumed at ${res.resumed_stage} (retry #${res.step_retry_count}). Same job ${jobId}.`);
      toast.success(`Retry ${stepId}: tiếp tục cùng job.`);
      // Resume polling the SAME job — never enqueue a new workflow.
      setRunning(true);
      startWorkflowPolling(jobId);
    } catch (err) {
      if (err instanceof FlowordCommandError && err.errorCode === 'WORKFLOW_NOT_FOUND') {
        handleWorkflowNotFound(jobId);
        return;
      }
      appendLog(`❌ [RETRY_ERROR] ${err instanceof Error ? err.message : String(err)}`);
      toast.error('Retry thất bại.');
    }
  };

  const handleSkipResearch = async () => {
    const jobId = activeJobId ?? activeWorkflowRun?.id;
    if (!jobId) {
      toast.error('Không có job để bỏ qua Research.');
      return;
    }
    try {
      const res = await skipFlowordResearch(jobId);
      setWorkflowInput((current) => ({ ...current, researchEnabled: false }));
      setStepConfigs((current) => current.map((step) => step.module === 'media_crawler' ? { ...step, enabled: false } : step));
      appendLog(`↪ [RESEARCH_SKIPPED] Resumed the same job ${jobId} at ${res.resumed_stage}.`);
      setRunning(true);
      startWorkflowPolling(jobId);
      toast.success('Research đã được bỏ qua; pipeline tiếp tục cùng job.');
    } catch (err) {
      appendLog(`❌ [SKIP_RESEARCH_ERROR] ${err instanceof Error ? err.message : String(err)}`);
      toast.error('Không thể bỏ qua Research.');
    }
  };

  const modalStep = stepRuns.find((s) => s.id === detailModalStepId);
  const activeDraftUrl = activeWorkflowRun?.finalDraftUrl ?? activeWorkflowRun?.finalDraftPath ?? '';

  return (
    <div className="floword-shell relative flex h-full w-full overflow-hidden">
      <Toaster position="top-right" toastOptions={{ style: { background: '#161b22', color: '#e6e6ef', border: '1px solid rgba(255,255,255,.08)' } }} />
      <div className="flex min-w-0 flex-1 flex-col">
        <FlowordHeader
          status={{
            mateOnline: readiness.mateAgent.status === 'READY',
            omniOnline: readiness.omniRoute.status === 'READY',
            rustPipelineOnline: readiness.storage.status === 'READY',
          }}
          activeDraftUrl={activeDraftUrl}
          running={running}
          pages={pages}
          activePageId={activePageId}
          onSelectPage={handleSelectPage}
          onOpenCreatePage={handleOpenCreatePage}
          onOpenEditPage={handleOpenEditPage}
          onRunWorkflow={running ? handleCancelWorkflow : handleExecuteWorkflow}
          onSaveWorkflow={handleSaveConfig}
          onConfigure={() => setConfigureOpen(true)}
        />

        <main className="flex-1 overflow-y-auto px-4 py-6 md:px-7 lg:px-9">
          <div className="mx-auto w-full max-w-[1480px]">
            <header className="mb-6">
              <h1 className="text-2xl font-semibold tracking-tight text-white">Production Workspace</h1>
              <p className="mt-1 text-sm text-zinc-400">Configure one project brief, run the existing pipeline, and review its real outputs.</p>
            </header>

          <ExecutionPlanView
            input={workflowInput}
            onChangeInput={(newInput) => {
              setWorkflowInput(newInput);
              setStepConfigs((current) => current.map((step) => step.module === 'media_crawler' ? { ...step, enabled: newInput.researchEnabled } : step));
            }}
            activePage={pages.find((p) => p.id === activePageId)}
            onOpenEditPage={handleOpenEditPage}
            stepRuns={stepRuns}
            activeStepIndex={activeStepIndex}
            running={running}
            progress={progress}
            currentStepMessage={currentStepMessage}
            logs={logs}
            readiness={readiness}
            activeJobId={activeJobId}
            activeWorkflowRun={activeWorkflowRun}
            onExecuteWorkflow={handleExecuteWorkflow}
            onCancelWorkflow={handleCancelWorkflow}
            onSaveConfig={handleSaveConfig}
            onLoadConfig={handleLoadConfig}
            onClearLogs={() => setLogs([])}
            onOpenDetailModal={(id) => setDetailModalStepId(id)}
            onOpenCapCutAutomation={onOpenCapCutAutomation}
          />

          </div>
        </main>
      </div>

      <PageManagementModal
        isOpen={isPageModalOpen}
        onClose={() => setIsPageModalOpen(false)}
        pageToEdit={pageToEdit}
        onSavePage={handleSavePage}
        onArchivePage={handleArchivePage}
      />

      <ConfigureDrawer
        open={configureOpen}
        input={workflowInput}
        readiness={readiness}
        steps={stepConfigs}
        onChangeSteps={(newSteps) => {
          setStepConfigs(newSteps);
          const researchEnabled = newSteps.find((step) => step.module === 'media_crawler')?.enabled;
          if (researchEnabled !== undefined) setWorkflowInput((current) => ({ ...current, researchEnabled }));
          setStepRuns((prev) => prev.map((run) => {
            const match = newSteps.find((step) => step.id === run.id);
            return match ? { ...run, ...match } : run;
          }));
        }}
        onChangeInput={(key, value) => {
          setWorkflowInput((current) => ({ ...current, [key]: value }));
        }}
        onClose={() => setConfigureOpen(false)}
        onSave={handleSaveConfig}
        onOpenCapCutAutomation={onOpenCapCutAutomation}
      />

      {detailModalStepId && modalStep && (
        <StepDetailModal step={modalStep} onClose={() => setDetailModalStepId(null)} onRetryStep={handleRetryStep} onLoginResearch={() => setConfigureOpen(true)} onSkipResearch={handleSkipResearch} />
      )}
    </div>
  );
};
