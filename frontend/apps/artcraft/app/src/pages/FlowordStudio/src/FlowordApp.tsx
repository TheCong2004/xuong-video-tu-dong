import React, { useState, useEffect, useRef, useCallback } from 'react';
import toast, { Toaster } from 'react-hot-toast';
import { FlowordSidebar, FlowordView } from './components/FlowordSidebar';
import { FlowordHeader } from './components/FlowordHeader';
import { DashboardView } from './components/views/DashboardView';
import { JobsView } from './components/views/JobsView';
import { PagesView } from './components/views/PagesView';
import { StudioView } from './components/views/StudioView';
import { PublishView } from './components/views/PublishView';
import { SettingsDevView } from './components/views/SettingsDevView';
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
  listFlowordWorkflows,
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

const TERMINAL_STAGES = new Set(['completed', 'draft_ready', 'failed', 'cancelled']);
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
        metadata: artifact.metadata && typeof artifact.metadata === 'object' ? (artifact.metadata as Record<string, unknown>) : undefined,
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
  // Navigation & Shell State
  const [activeView, setActiveView] = useState<FlowordView>('dashboard');
  const [sidebarCollapsed, setSidebarCollapsed] = useState<boolean>(false);
  const [mobileNavOpen, setMobileNavOpen] = useState<boolean>(false);
  const [configureOpen, setConfigureOpen] = useState<boolean>(false);
  const [selectedJobId, setSelectedJobId] = useState<string | undefined>(undefined);

  // Workflow State
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
    '🟢 [NEODONUT ENGINE] Floword Unified Task System initialized.',
    '💡 Enqueue commands dispatch directly to the Rust Worker Thread & SQLite database.',
  ]);
  const [detailModalStepId, setDetailModalStepId] = useState<string | null>(null);
  const [activeWorkflowRun, setActiveWorkflowRun] = useState<WorkflowRun | null>(null);
  const [activeJobId, setActiveJobId] = useState<string | null>(null);
  const [allRuns, setAllRuns] = useState<WorkflowRun[]>([]);

  // ContentPages Domain State
  const [pages, setPages] = useState<ContentPage[]>([]);
  const [activePageId, setActivePageId] = useState<string | null>(() => {
    migrateLegacyLocalStorageKeys();
    return localStorage.getItem(ACTIVE_PAGE_ID_KEY);
  });
  const [isPageModalOpen, setIsPageModalOpen] = useState<boolean>(false);
  const [pageToEdit, setPageToEdit] = useState<ContentPage | null>(null);

  // Readiness State
  const [readiness, setReadiness] = useState<DetailedReadinessStatus>(DEFAULT_READINESS);
  const pollTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const appendLog = (msg: string) => {
    setLogs((prev) => [...prev, `[${new Date().toLocaleTimeString()}] ${msg}`]);
  };

  const refreshJobs = useCallback(async () => {
    try {
      const workflows = await listFlowordWorkflows();
      const hydratedRuns: WorkflowRun[] = workflows.map((wf) => {
        const outputs = parseWorkflowOutputs(wf.stage_outputs);
        const percent = backendProgress(wf.current_stage, wf.status);
        const isSuccess = wf.status === 'complete_success';
        const isFailure = wf.status === 'complete_failure';
        const isCancelled = wf.status.includes('cancelled');
        const statusStr: WorkflowRun['status'] = isSuccess
          ? 'completed'
          : isFailure
          ? 'failed'
          : isCancelled
          ? 'cancelled'
          : 'running';

        return {
          id: wf.job_id,
          pageId: activePageId ?? 'general',
          input: workflowInput,
          currentStage: wf.current_stage,
          status: statusStr,
          progressPercent: percent,
          artifacts: outputs.artifacts,
          finalDraftUrl: outputs.draftUrl,
        };
      });
      setAllRuns(hydratedRuns);
    } catch (err) {
      console.error('[Floword] Failed to hydrate jobs from SQLite DB:', err);
    }
  }, [activePageId, workflowInput]);

  const refreshPages = useCallback(async () => {
    try {
      const list = await listContentPages();
      setPages(list);
      if (!activePageId && list.length > 0) {
        setActivePageId(list[0].id);
        localStorage.setItem(ACTIVE_PAGE_ID_KEY, list[0].id);
      }
    } catch (err) {
      console.error('[Floword] Failed to load ContentPages from database:', err);
      setPages([]);
    }
  }, [activePageId]);

  useEffect(() => {
    refreshPages();
    refreshJobs();
    fetchDetailedReadiness().then(setReadiness).catch(() => {});
  }, [refreshPages, refreshJobs]);

  // Polling loop for active workflow
  const startWorkflowPolling = useCallback((jobId: string) => {
    if (pollTimerRef.current) clearInterval(pollTimerRef.current);

    pollTimerRef.current = setInterval(async () => {
      try {
        const res: GetFlowordWorkflowResponse = await getFlowordWorkflow(jobId);
        const outputs = parseWorkflowOutputs(res.stage_outputs);
        const percent = backendProgress(res.current_stage, res.status);
        const isSuccess = res.status === 'complete_success';
        const isFailure = res.status === 'complete_failure';
        const isCancelled = res.status.includes('cancelled');
        const statusStr: WorkflowRun['status'] = isSuccess
          ? 'completed'
          : isFailure
          ? 'failed'
          : isCancelled
          ? 'cancelled'
          : 'running';

        const updatedRun: WorkflowRun = {
          id: jobId,
          pageId: activePageId ?? 'general',
          input: workflowInput,
          currentStage: res.current_stage,
          status: statusStr,
          progressPercent: percent,
          artifacts: outputs.artifacts,
          finalDraftUrl: outputs.draftUrl,
        };

        setActiveWorkflowRun(updatedRun);
        setProgress(percent);

        // Update list of all runs
        setAllRuns((prev) => {
          const idx = prev.findIndex((r) => r.id === jobId);
          if (idx >= 0) {
            const next = [...prev];
            next[idx] = updatedRun;
            return next;
          }
          return [updatedRun, ...prev];
        });

        if (TERMINAL_STATUSES.has(res.status) || TERMINAL_STAGES.has(res.current_stage)) {
          if (pollTimerRef.current) clearInterval(pollTimerRef.current);
          setRunning(false);
          if (res.status === 'complete_success') {
            toast.success('🎉 Video pipeline hoàn thành xuất sắc!');
          }
        }
      } catch (err) {
        console.error('Polling error:', err);
      }
    }, POLL_INTERVAL_MS);
  }, [activePageId, workflowInput]);

  const handleExecuteWorkflow = async (customInput?: WorkflowInput) => {
    const inputToUse = customInput || workflowInput;
    setRunning(true);
    setProgress(5);
    appendLog('🚀 Khởi chạy Floword Production Pipeline...');

    try {
      const res = await enqueueFlowordWorkflow({
        page_id: activePageId ?? undefined,
        workflow_name: inputToUse.workflowName || 'floword_video_pipeline',
        prompt: inputToUse.prompt || inputToUse.customPrompt || '',
        topic: inputToUse.topic,
        source_urls: inputToUse.sourceUrls,
        source_files: inputToUse.sourceFiles,
        research_enabled: inputToUse.researchEnabled,
        target_platform: inputToUse.targetPlatform,
        aspect_ratio: inputToUse.aspectRatio,
        target_duration_seconds: inputToUse.targetDurationSeconds,
        output_mode: inputToUse.outputMode,
      });

      const jobId = res.job_id;
      setActiveJobId(jobId);
      localStorage.setItem(ACTIVE_JOB_ID_KEY, jobId);
      appendLog(`✅ Pipeline Enqueued. Job ID: ${jobId}`);
      toast.success(`Đã khởi chạy Job ${jobId}`);

      const initialRun: WorkflowRun = {
        id: jobId,
        pageId: activePageId ?? 'general',
        input: inputToUse,
        currentStage: 'image',
        status: 'running',
        progressPercent: 5,
        artifacts: [],
      };
      setActiveWorkflowRun(initialRun);
      setAllRuns((prev) => [initialRun, ...prev.filter((r) => r.id !== jobId)]);

      startWorkflowPolling(jobId);
    } catch (err) {
      setRunning(false);
      const msg = err instanceof Error ? err.message : String(err);
      appendLog(`❌ Lỗi khởi chạy: ${msg}`);
      toast.error(`Không thể khởi chạy pipeline: ${msg}`);
    }
  };

  const handleCancelWorkflow = async () => {
    if (pollTimerRef.current) clearInterval(pollTimerRef.current);
    setRunning(false);
    if (activeJobId) {
      try {
        await cancelFlowordWorkflow(activeJobId);
        appendLog(`🛑 Đã gửi lệnh dừng Job ${activeJobId}`);
        toast('Đã dừng tiến trình Job.');
        await refreshJobs();
      } catch (err) {
        console.error(err);
        toast.error('Lỗi khi gửi lệnh dừng Job.');
      }
    }
  };

  const handleCancelJob = async (jobId: string) => {
    try {
      await cancelFlowordWorkflow(jobId);
      toast('Đã gửi lệnh dừng Job.');
      await refreshJobs();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Lỗi khi dừng Job: ${msg}`);
    }
  };

  const handleRetryStep = async (runId: string, stepId: string) => {
    try {
      await retryFlowordStep(runId, stepId);
      toast.success(`Đang retry stage: ${stepId}`);
      setRunning(true);
      startWorkflowPolling(runId);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Retry thất bại: ${msg}`);
    }
  };

  const handleApprovePublish = async (runId: string) => {
    toast('Chức năng Publishing Engine tự động đang chờ cấu hình profile mạng xã hội.', {
      icon: '⏳',
    });
  };

  const handleRejectPublish = async (runId: string) => {
    toast('Đã từ chối đăng video. Bạn có thể tinh chỉnh lại trong Studio.', { icon: '↩️' });
  };

  const handleCreatePage = async (req: CreateContentPageRequest) => {
    try {
      await createContentPage(req);
      await refreshPages();
      toast.success(`Đã tạo Page: ${req.name}`);
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Không thể lưu Page vào database: ${msg}`);
      throw err;
    }
  };

  const handleUpdatePage = async (pageId: string, req: Partial<UpdateContentPageRequest> & { name?: string }) => {
    try {
      const current = pages.find((p) => p.id === pageId);
      await updateContentPage({
        id: pageId,
        name: req.name || current?.name || '',
        output_root: req.output_root || current?.output_root || 'D:\\',
        ...req,
      });
      await refreshPages();
      toast.success('Cập nhật Page thành công!');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Không thể cập nhật Page: ${msg}`);
      throw err;
    }
  };

  const handleArchivePage = async (pageId: string) => {
    try {
      await archiveContentPage(pageId);
      await refreshPages();
      toast.success('Đã lưu trữ Page.');
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      toast.error(`Không thể lưu trữ Page: ${msg}`);
      throw err;
    }
  };

  const activeDraftUrl = activeWorkflowRun?.finalDraftUrl ?? '';

  return (
    <div className="floword-shell relative flex h-[calc(100vh-56px)] w-full overflow-hidden bg-[#0d1017]">
      <Toaster
        position="top-right"
        toastOptions={{
          style: {
            background: '#161b22',
            color: '#e6e6ef',
            border: '1px solid rgba(255,255,255,.08)',
          },
        }}
      />

      {/* 1. Left Fixed Clean Sidebar */}
      <FlowordSidebar
        activeView={activeView}
        collapsed={sidebarCollapsed}
        mobileOpen={mobileNavOpen}
        activeJobsCount={allRuns.filter((r) => r.status === 'running').length}
        pendingPublishCount={allRuns.filter((r) => r.status === 'completed' && !r.isPublished).length}
        onChange={(v) => setActiveView(v)}
        onToggleCollapse={() => setSidebarCollapsed(!sidebarCollapsed)}
        onCloseMobile={() => setMobileNavOpen(false)}
        onOpenMobile={() => setMobileNavOpen(true)}
      />

      {/* 2. Main Content View Area */}
      <div className="flex min-w-0 flex-1 flex-col overflow-hidden">
        {/* Simplified Header */}
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
          onSelectPage={(id) => {
            setActivePageId(id);
            localStorage.setItem(ACTIVE_PAGE_ID_KEY, id);
          }}
          onOpenCreatePage={() => setIsPageModalOpen(true)}
          onOpenEditPage={() => {
            const current = pages.find((p) => p.id === activePageId);
            if (current) {
              setPageToEdit(current);
              setIsPageModalOpen(true);
            }
          }}
          onRunWorkflow={running ? handleCancelWorkflow : () => handleExecuteWorkflow()}
          onSaveWorkflow={() => toast.success('Đã lưu cấu hình pipeline.')}
          onConfigure={() => setConfigureOpen(true)}
        />

        {/* Dynamic 6 Views Router */}
        <main className="flex-1 overflow-y-auto">
          {activeView === 'dashboard' && (
            <DashboardView
              readiness={readiness}
              activeRuns={allRuns}
              pages={pages}
              onNewJob={() => setActiveView('studio')}
              onNavigateTab={(tab) => setActiveView(tab)}
              onSelectJob={(id) => {
                setSelectedJobId(id);
                setActiveView('jobs');
              }}
              onRefresh={() => fetchDetailedReadiness().then(setReadiness)}
            />
          )}

          {activeView === 'jobs' && (
            <JobsView
              runs={allRuns}
              pages={pages}
              selectedJobId={selectedJobId}
              onSelectJob={(id) => setSelectedJobId(id)}
              onNewJob={() => setActiveView('studio')}
              onRetryStep={handleRetryStep}
              onCancelJob={handleCancelJob}
            />
          )}

          {activeView === 'pages' && (
            <PagesView
              pages={pages}
              activePageId={activePageId || undefined}
              onSelectPage={(id) => {
                setActivePageId(id);
                localStorage.setItem(ACTIVE_PAGE_ID_KEY, id);
              }}
              onCreatePage={handleCreatePage}
              onUpdatePage={handleUpdatePage}
              onArchivePage={handleArchivePage}
            />
          )}

          {activeView === 'studio' && (
            <StudioView
              pages={pages}
              activePageId={activePageId || undefined}
              onSelectPage={(id) => setActivePageId(id)}
              activeRun={activeWorkflowRun}
              isRunning={running}
              onRunWorkflow={handleExecuteWorkflow}
              onCancelWorkflow={handleCancelWorkflow}
            />
          )}

          {activeView === 'publish' && (
            <PublishView
              runs={allRuns}
              pages={pages}
              onApprovePublish={handleApprovePublish}
              onRejectPublish={handleRejectPublish}
            />
          )}

          {activeView === 'settings' && (
            <SettingsDevView
              readiness={readiness}
              onRefreshReadiness={() => fetchDetailedReadiness().then(setReadiness)}
            />
          )}
        </main>
      </div>

      {/* Drawers & Modals */}
      <PageManagementModal
        isOpen={isPageModalOpen}
        onClose={() => {
          setIsPageModalOpen(false);
          setPageToEdit(null);
        }}
        pageToEdit={pageToEdit}
        onSavePage={async (req) => {
          if (pageToEdit) {
            await handleUpdatePage(pageToEdit.id, req);
          } else {
            await handleCreatePage(req);
          }
          setIsPageModalOpen(false);
          setPageToEdit(null);
        }}
        onArchivePage={async (id) => {
          await handleArchivePage(id);
          setIsPageModalOpen(false);
          setPageToEdit(null);
        }}
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
        }}
        onChangeInput={(key, value) => {
          setWorkflowInput((current) => ({ ...current, [key]: value }));
        }}
        onClose={() => setConfigureOpen(false)}
        onSave={() => toast.success('Đã lưu cấu hình.')}
        onOpenCapCutAutomation={onOpenCapCutAutomation}
      />
    </div>
  );
};
