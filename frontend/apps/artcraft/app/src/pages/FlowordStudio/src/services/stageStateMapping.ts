import type { FlowordStageState, GetFlowordWorkflowResponse } from '../api/flowordClient';
import type { StepRun, StepStatus } from './workflowEngine';

const BUSINESS_STAGE_TO_STEP: Record<string, string> = {
  research: 'step-1',
  story_script: 'step-2',
  ingest_analyze: 'step-3',
  voice: 'step-4',
  media_timeline: 'step-5',
  capcut: 'step-6',
};

function toStepStatus(status: FlowordStageState['status']): StepStatus {
  switch (status) {
    case 'completed': return 'succeeded';
    case 'skipped': return 'skipped';
    case 'failed': return 'failed';
    case 'cancelled': return 'cancelled';
    case 'waiting_input': return 'waiting_input';
    case 'running':
    case 'retrying': return 'running';
    case 'pending': return 'ready';
  }
}

function businessStageFromWorkerStage(stage: string): string | undefined {
  if (stage.includes('ingest_analyze')) return 'ingest_analyze';
  if (stage.includes('research')) return 'research';
  if (stage.includes('script')) return 'story_script';
  if (stage.includes('voice')) return 'voice';
  if (stage.includes('media_timeline')) return 'media_timeline';
  if (stage.includes('draft') || stage.includes('caption') || stage.includes('render') || stage.includes('capcut')) return 'capcut';
  return undefined;
}

function legacyErrorCode(message: string): string {
  return message.match(/\b([A-Z][A-Z0-9_]{2,})\b/)?.[1] ?? 'STEP_RUN_ERROR';
}

/** Merge canonical persisted business-stage state into the existing module rows. */
export function mergeBackendStageStates(stepRuns: StepRun[], response: GetFlowordWorkflowResponse): StepRun[] {
  const byStep = new Map<string, FlowordStageState>();
  for (const stage of response.stage_states ?? []) {
    const stepId = BUSINESS_STAGE_TO_STEP[stage.stage_id];
    if (stepId) byStep.set(stepId, stage);
  }

  // A legacy job may only have job.error. Use it solely when the persisted
  // worker stage identifies the owning business stage; never attach it broadly.
  const legacyBusinessStage = businessStageFromWorkerStage(response.current_stage ?? '');
  const legacyStepId = legacyBusinessStage ? BUSINESS_STAGE_TO_STEP[legacyBusinessStage] : undefined;
  const legacyFailure = response.status === 'complete_failure' && response.failure_message && legacyStepId
    ? { stepId: legacyStepId, message: response.failure_message }
    : undefined;

  return stepRuns.map((step) => {
    const stage = byStep.get(step.id);
    if (stage) {
      const error = stage.error ? {
        code: stage.error.code,
        message: stage.error.message,
        service: stage.error.service ?? stage.service ?? undefined,
        stageId: stage.error.stage_id ?? stage.stage_id,
        retryable: stage.error.retryable,
        timestamp: stage.error.timestamp ?? stage.finished_at ?? undefined,
      } : undefined;
      return {
        ...step,
        status: toStepStatus(stage.status),
        progress: stage.status === 'completed' || stage.status === 'skipped' ? 100 : step.progress,
        startedAt: stage.started_at ?? step.startedAt,
        completedAt: stage.finished_at ?? step.completedAt,
        retryCount: Math.max(0, stage.attempt - 1),
        error,
        errorCode: error?.code,
        errorMessage: error?.message,
      };
    }
    if (legacyFailure?.stepId === step.id) {
      const error = {
        code: legacyErrorCode(legacyFailure.message),
        message: legacyFailure.message,
        stageId: legacyBusinessStage,
        retryable: false,
      };
      return { ...step, status: 'failed', error, errorCode: error.code, errorMessage: error.message };
    }
    return step;
  });
}
