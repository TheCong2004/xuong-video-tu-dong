import { FlowordStageState, GetFlowordWorkflowResponse } from '../../app/src/pages/FlowordStudio/src/api/flowordClient';
import { INITIAL_STEP_CONFIGS, StepRun } from '../../app/src/pages/FlowordStudio/src/services/workflowEngine';
import { mergeBackendStageStates } from '../../app/src/pages/FlowordStudio/src/services/stageStateMapping';

function runs(): StepRun[] {
  return INITIAL_STEP_CONFIGS.map((step) => ({ ...step, status: 'ready', progress: 0, logs: [], artifacts: [], retryCount: 0 }));
}

function response(stage: FlowordStageState): GetFlowordWorkflowResponse {
  return { job_id: 'job-1', status: stage.status === 'failed' ? 'complete_failure' : 'started', current_stage: stage.stage_id, stage_states: [stage] };
}

function failedStage(stageId: string, service: string, code: string, message: string): FlowordStageState {
  return {
    stage_id: stageId,
    status: 'failed',
    attempt: 1,
    service,
    finished_at: '2026-08-09T12:00:00Z',
    error: { code, message, service, stage_id: stageId, retryable: false, timestamp: '2026-08-09T12:00:00Z' },
  };
}

describe('Floword structured stage error mapping', () => {
  test('waiting_input belongs only to research and leaves downstream pending', () => {
    const mapped = mergeBackendStageStates(runs(), {
      job_id: 'job-1',
      status: 'waiting_input',
      current_stage: 'research',
      stage_states: [{
        stage_id: 'research',
        status: 'waiting_input',
        attempt: 1,
        service: 'mediacrawler',
        error: { code: 'RESEARCH_AUTH_REQUIRED', message: 'Waiting for RedNote login', retryable: true },
      }],
    });

    expect(mapped[0]).toMatchObject({ status: 'waiting_input', errorCode: 'RESEARCH_AUTH_REQUIRED' });
    expect(mapped.slice(1).every((step) => step.status === 'ready')).toBe(true);
  });

  test('MediaCrawler auth failure belongs to the research module', () => {
    const mapped = mergeBackendStageStates(runs(), response(failedStage('research', 'mediacrawler', 'MEDIACRAWLER_AUTH_REQUIRED', 'MediaCrawler cookie login is required')));
    expect(mapped[0]).toMatchObject({ status: 'failed', errorCode: 'MEDIACRAWLER_AUTH_REQUIRED' });
    expect(mapped[0].error).toMatchObject({ service: 'mediacrawler', stageId: 'research', retryable: false });
    expect(mapped.slice(1).every((step) => step.error === undefined)).toBe(true);
  });

  test('TikTok extractor failure belongs to ingest_analyze / Youwee', () => {
    const mapped = mergeBackendStageStates(runs(), response(failedStage('ingest_analyze', 'youwee', 'TIKTOK_EXTRACTOR_FAILED', 'TikTok extraction failed')));
    expect(mapped[2]).toMatchObject({ status: 'failed', errorCode: 'TIKTOK_EXTRACTOR_FAILED' });
  });

  test('OpenMontage 500 belongs to media_timeline', () => {
    const mapped = mergeBackendStageStates(runs(), response(failedStage('media_timeline', 'openmontage', 'OPENMONTAGE_HTTP_500', 'OpenMontage HTTP 500')));
    expect(mapped[4]).toMatchObject({ status: 'failed', errorCode: 'OPENMONTAGE_HTTP_500' });
  });

  test('successful stage has no error', () => {
    const mapped = mergeBackendStageStates(runs(), response({ stage_id: 'research', status: 'completed', attempt: 1, service: 'mediacrawler', error: null }));
    expect(mapped[0]).toMatchObject({ status: 'succeeded', progress: 100 });
    expect(mapped[0].error).toBeUndefined();
    expect(mapped[0].errorMessage).toBeUndefined();
  });

  test('failed stage without structured error stays failed without fabricating one', () => {
    const mapped = mergeBackendStageStates(runs(), response({ stage_id: 'research', status: 'failed', attempt: 1, service: 'mediacrawler', error: null }));
    expect(mapped[0].status).toBe('failed');
    expect(mapped[0].error).toBeUndefined();
  });
});
