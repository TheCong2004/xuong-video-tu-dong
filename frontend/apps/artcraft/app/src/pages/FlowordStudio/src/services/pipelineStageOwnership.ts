import type { StepRun, StepStatus } from './workflowEngine';

export interface PipelineProgressStage {
  id: string;
  title: string;
  owner: string;
  status: StepStatus;
  sourceStepId?: string;
}

interface CanonicalStageDefinition {
  id: string;
  title: string;
  owner: string;
  sourceStepId?: string;
}

/** Canonical business-stage ownership; legacy workflow module labels are not authoritative. */
export const PIPELINE_STAGE_OWNERSHIP: readonly CanonicalStageDefinition[] = [
  { id: 'input', title: 'Input', owner: 'Project Brief' },
  { id: 'ingest_analyze', title: 'Ingest / Analyze', owner: 'Youwee + Vynaro', sourceStepId: 'step-3' },
  { id: 'research', title: 'Research', owner: 'MediaCrawler', sourceStepId: 'step-1' },
  { id: 'story_script', title: 'Story / Script', owner: 'Story Studio + OmniRoute', sourceStepId: 'step-2' },
  { id: 'voice', title: 'Voice', owner: 'TTS / OmniRoute', sourceStepId: 'step-4' },
  { id: 'media_timeline', title: 'Media / Timeline', owner: 'OpenMontage', sourceStepId: 'step-5' },
  { id: 'capcut', title: 'CapCut', owner: 'CapCut Backend + CapCut Mate', sourceStepId: 'step-6' },
] as const;

export function buildPipelineProgressStages(stepRuns: StepRun[], inputStatus: StepStatus): PipelineProgressStage[] {
  const byId = new Map(stepRuns.map((step) => [step.id, step]));
  return PIPELINE_STAGE_OWNERSHIP.map((stage) => {
    if (!stage.sourceStepId) return { ...stage, status: inputStatus };
    const source = byId.get(stage.sourceStepId);
    return {
      ...stage,
      status: source?.status ?? 'not_ready',
      sourceStepId: source?.id,
    };
  });
}
