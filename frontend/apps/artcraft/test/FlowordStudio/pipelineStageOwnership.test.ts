import { INITIAL_STEP_CONFIGS, StepRun } from '../../app/src/pages/FlowordStudio/src/services/workflowEngine';
import {
  buildPipelineProgressStages,
  PIPELINE_STAGE_OWNERSHIP,
} from '../../app/src/pages/FlowordStudio/src/services/pipelineStageOwnership';

function runs(): StepRun[] {
  return INITIAL_STEP_CONFIGS.map((step) => ({
    ...step,
    status: step.id === 'step-3' ? 'running' : 'ready',
    progress: 0,
    logs: [],
    artifacts: [],
    retryCount: 0,
  }));
}

describe('canonical Floword pipeline ownership', () => {
  test('uses business stages and canonical owners in pipeline order', () => {
    expect(PIPELINE_STAGE_OWNERSHIP.map(({ id, owner }) => ({ id, owner }))).toEqual([
      { id: 'input', owner: 'Project Brief' },
      { id: 'ingest_analyze', owner: 'Youwee + Vynaro' },
      { id: 'research', owner: 'MediaCrawler' },
      { id: 'story_script', owner: 'Story Studio + OmniRoute' },
      { id: 'voice', owner: 'TTS / OmniRoute' },
      { id: 'media_timeline', owner: 'OpenMontage' },
      { id: 'capcut', owner: 'CapCut Backend + CapCut Mate' },
    ]);
  });

  test('reads status by canonical stage mapping rather than array position', () => {
    const shuffled = runs().reverse();
    const stages = buildPipelineProgressStages(shuffled, 'ready');
    expect(stages.find((stage) => stage.id === 'ingest_analyze')).toMatchObject({
      owner: 'Youwee + Vynaro',
      sourceStepId: 'step-3',
      status: 'running',
    });
  });
});
