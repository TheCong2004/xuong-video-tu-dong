import React, { useState } from 'react';
import { CheckCircle2, Play, Square } from 'lucide-react';

import { ContentPage, DetailedReadinessStatus } from '../api/flowordClient';
import { ArtifactRef, StepRun, StepStatus, WorkflowInput, WorkflowRun } from '../services/workflowEngine';
import { buildPipelineProgressStages } from '../services/pipelineStageOwnership';
import { LiveExecutionLog } from './LiveExecutionLog';
import { ProjectBriefPanel } from './ProjectBriefPanel';

type ConsoleTab = 'progress' | 'artifacts' | 'logs' | 'history';

interface ExecutionPlanViewProps {
  input: WorkflowInput;
  onChangeInput: (newInput: WorkflowInput) => void;
  activePage?: ContentPage | null;
  onOpenEditPage?: (page: ContentPage) => void;
  stepRuns: StepRun[];
  activeStepIndex: number;
  running: boolean;
  progress: number;
  currentStepMessage: string;
  logs: string[];
  readiness: DetailedReadinessStatus;
  activeJobId: string | null;
  activeWorkflowRun: WorkflowRun | null;
  onExecuteWorkflow: () => void;
  onCancelWorkflow: () => void;
  onSaveConfig: () => void;
  onLoadConfig: () => void;
  onClearLogs: () => void;
  onOpenDetailModal: (stepId: string) => void;
  onOpenCapCutAutomation?: () => void;
}

const tabLabels: Record<ConsoleTab, string> = {
  progress: 'Progress',
  artifacts: 'Artifacts',
  logs: 'Logs',
  history: 'History',
};

function stageTone(status: StepStatus): string {
  if (status === 'succeeded') return 'bg-green-500/10 text-green-400';
  if (status === 'running') return 'bg-blue-500/10 text-blue-400';
  if (status === 'failed') return 'bg-red-500/10 text-red-400';
  if (status === 'waiting_input') return 'bg-amber-500/10 text-amber-400';
  return 'bg-white/[0.05] text-zinc-500';
}

function ArtifactList({ artifacts }: { artifacts: ArtifactRef[] }) {
  if (artifacts.length === 0) {
    return <div className="p-8 text-center text-sm text-zinc-500">No artifacts have been reported for the active workflow.</div>;
  }
  return (
    <div className="divide-y divide-white/[0.06]">
      {artifacts.map((artifact) => (
        <div key={artifact.id} className="flex items-center justify-between gap-4 px-5 py-4">
          <div className="min-w-0">
            <div className="truncate text-sm font-medium text-white">{artifact.name}</div>
            <div className="mt-1 truncate font-mono text-xs text-zinc-500">{artifact.path || artifact.url}</div>
          </div>
          <span className="shrink-0 rounded-full bg-white/[0.05] px-2.5 py-1 text-xs text-zinc-400">{artifact.type}</span>
        </div>
      ))}
    </div>
  );
}

export const ExecutionPlanView: React.FC<ExecutionPlanViewProps> = ({
  input,
  onChangeInput,
  stepRuns,
  activeStepIndex,
  running,
  progress,
  currentStepMessage,
  logs,
  readiness,
  activeJobId,
  activeWorkflowRun,
  activePage,
  onOpenEditPage,
  onExecuteWorkflow,
  onCancelWorkflow,
  onSaveConfig,
  onLoadConfig,
  onClearLogs,
  onOpenDetailModal,
  onOpenCapCutAutomation,
}) => {
  const [activeConsoleTab, setActiveConsoleTab] = useState<ConsoleTab>('progress');
  const inputStatus: StepStatus = activeJobId ? 'succeeded' : 'ready';
  const businessStages = buildPipelineProgressStages(stepRuns, inputStatus);
  const isFormValid = input.prompt.trim().length > 0 || input.sourceUrls.length > 0 || input.sourceFiles.length > 0;
  const outputReady = activeWorkflowRun?.status === 'completed' || activeWorkflowRun?.status === 'draft_ready';
  const artifacts = activeWorkflowRun?.artifacts ?? [];

  return (
    <div className="flex flex-col gap-5 pb-6">
      <section className="grid items-start gap-5 xl:grid-cols-[minmax(0,2fr)_minmax(320px,0.8fr)]">
        <ProjectBriefPanel
          input={input}
          onChangeInput={onChangeInput}
          onSaveConfig={onSaveConfig}
          onLoadConfig={onLoadConfig}
          activePage={activePage}
          onOpenEditPage={onOpenEditPage}
        />

        <aside className="floword-card overflow-hidden xl:sticky xl:top-0" aria-labelledby="pipeline-progress-title">
          <div className="border-b border-white/[0.08] p-5">
            <div className="flex items-start justify-between gap-3">
              <div>
                <h2 id="pipeline-progress-title" className="text-base font-semibold text-white">Pipeline Progress</h2>
                <p className="mt-1 line-clamp-2 text-xs leading-5 text-zinc-500">{currentStepMessage}</p>
              </div>
              <span className={`rounded-full px-2.5 py-1 text-[11px] font-semibold ${readiness.isReadyForExecution ? 'bg-green-500/10 text-green-400' : 'bg-amber-500/10 text-amber-400'}`}>
                {readiness.isReadyForExecution ? 'ready' : 'degraded'}
              </span>
            </div>
            <div className="mt-4 h-1.5 overflow-hidden rounded-full bg-white/[0.06]">
              <div className="h-full rounded-full bg-[#6366f1]" style={{ width: `${progress}%` }} />
            </div>
            <div className="mt-2 text-right text-xs font-medium text-zinc-500">{progress}%</div>
          </div>

          <div className="divide-y divide-white/[0.06]">
            {businessStages.map((stage, index) => (
              <button
                key={stage.id}
                type="button"
                disabled={!stage.sourceStepId}
                onClick={() => stage.sourceStepId && onOpenDetailModal(stage.sourceStepId)}
                className="flex w-full items-center gap-3 px-5 py-3 text-left enabled:hover:bg-white/[0.03] disabled:cursor-default"
              >
                <span className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-semibold ${stageTone(stage.status)}`}>
                  {stage.status === 'succeeded' ? '✓' : index + 1}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium text-zinc-200">{stage.title}</span>
                  <span className="mt-0.5 block truncate text-xs text-zinc-600">{stage.owner} · {stage.status.replace('_', ' ')}</span>
                </span>
                {stage.status === 'running' && <span className="h-2 w-2 rounded-full bg-blue-500" />}
              </button>
            ))}
          </div>

          <div className="border-t border-white/[0.08] p-4">
            {running ? (
              <button type="button" onClick={onCancelWorkflow} className="floword-button w-full bg-red-500 text-white hover:bg-red-600">
                <Square className="h-4 w-4 fill-white" /> Cancel Run
              </button>
            ) : (
              <button type="button" onClick={onExecuteWorkflow} disabled={!isFormValid} className="floword-button floword-button-primary w-full disabled:cursor-not-allowed disabled:opacity-40">
                <Play className="h-4 w-4 fill-white" /> Run Workflow
              </button>
            )}
          </div>
        </aside>
      </section>

      {outputReady && (
        <section className="floword-card border-green-500/20 p-5">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2"><CheckCircle2 className="h-5 w-5 text-green-400" /><h3 className="text-sm font-semibold text-white">Workflow output is ready</h3></div>
            {onOpenCapCutAutomation && (
              <button type="button" onClick={onOpenCapCutAutomation} className="floword-button floword-button-primary">Open in CapCut Automation</button>
            )}
          </div>
        </section>
      )}

      <section className="floword-card overflow-hidden" aria-labelledby="run-console-title">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/[0.08] px-5 py-4">
          <div>
            <h2 id="run-console-title" className="text-base font-semibold text-white">Run Console</h2>
            <p className="mt-1 text-xs text-zinc-500">Current workflow job, artifacts, logs, and history.</p>
          </div>
          <div className="inline-flex rounded-[9px] border border-white/[0.08] bg-[#111520] p-1 text-xs" role="tablist" aria-label="Run Console">
            {(Object.keys(tabLabels) as ConsoleTab[]).map((tab) => (
              <button
                key={tab}
                type="button"
                role="tab"
                aria-selected={activeConsoleTab === tab}
                onClick={() => setActiveConsoleTab(tab)}
                className={`rounded-[7px] px-3 py-2 font-medium ${activeConsoleTab === tab ? 'bg-white/[0.08] text-white' : 'text-zinc-500 hover:text-zinc-300'}`}
              >
                {tabLabels[tab]}
              </button>
            ))}
          </div>
        </div>

        {activeConsoleTab === 'progress' && (
          <div className="grid gap-5 p-5 md:grid-cols-3">
            <div><div className="text-xs font-medium uppercase tracking-wider text-zinc-500">Execution</div><div className="mt-2 text-lg font-semibold text-white">{running ? 'Running' : activeWorkflowRun?.status ?? 'Idle'}</div></div>
            <div><div className="text-xs font-medium uppercase tracking-wider text-zinc-500">Active job</div><div className="mt-2 truncate font-mono text-sm text-zinc-200">{activeJobId || 'No active job'}</div></div>
            <div><div className="text-xs font-medium uppercase tracking-wider text-zinc-500">Progress</div><div className="mt-2 text-lg font-semibold text-white">{progress}%</div></div>
            <div className="md:col-span-3 rounded-[9px] bg-white/[0.03] p-3 font-mono text-xs text-zinc-400">{currentStepMessage}</div>
          </div>
        )}

        {activeConsoleTab === 'artifacts' && <ArtifactList artifacts={artifacts} />}

        {activeConsoleTab === 'logs' && (
          <div className="h-[420px]"><LiveExecutionLog logs={logs} running={running} progress={progress} currentStepMessage={currentStepMessage} onClearLogs={onClearLogs} /></div>
        )}

        {activeConsoleTab === 'history' && (
          activeJobId ? (
            <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 px-5 py-4">
              <div className="min-w-0"><div className="truncate font-mono text-sm text-white">{activeJobId}</div><div className="mt-1 text-xs text-zinc-500">{activeWorkflowRun?.createdAt || 'Restored backend job'}</div></div>
              <span className={`rounded-full px-2.5 py-1 text-xs font-semibold ${running ? 'bg-blue-500/10 text-blue-400' : 'bg-zinc-500/10 text-zinc-400'}`}>{running ? 'running' : activeWorkflowRun?.status || 'idle'}</span>
            </div>
          ) : <div className="p-8 text-center text-sm text-zinc-500">No workflow history has been reported in this session.</div>
        )}
      </section>
    </div>
  );
};
