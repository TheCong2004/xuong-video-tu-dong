import React, { useEffect } from 'react';
import { Save, X } from 'lucide-react';

import { DetailedReadinessStatus, ServiceHealth } from '../api/flowordClient';
import { StepConfig, WorkflowInput } from '../services/workflowEngine';
import { CapabilityToolsView } from './CapabilityToolsView';
import { FlowDesignView } from './FlowDesignView';
import { ServicesView } from './ServicesView';
import { VisualGenerationProvider } from './VisualGenerationProvider';

interface ConfigureDrawerProps {
  open: boolean;
  input: WorkflowInput;
  readiness: DetailedReadinessStatus;
  steps: StepConfig[];
  onChangeSteps: (steps: StepConfig[]) => void;
  onChangeInput?: <K extends keyof WorkflowInput>(key: K, value: WorkflowInput[K]) => void;
  onClose: () => void;
  onSave: () => void;
  onOpenCapCutAutomation?: () => void;
}

const statusStyle: Record<ServiceHealth['status'], string> = {
  READY: 'bg-green-500/10 text-green-400',
  DEGRADED: 'bg-amber-500/10 text-amber-400',
  AUTH_REQUIRED: 'bg-amber-500/10 text-amber-400',
  WAITING_INPUT: 'bg-blue-500/10 text-blue-400',
  UNAVAILABLE: 'bg-zinc-500/10 text-zinc-400',
};

function StatusRow({ label, health }: { label: string; health: ServiceHealth }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-white/[0.06] py-3 last:border-0">
      <div className="min-w-0">
        <div className="text-sm font-medium text-zinc-200">{label}</div>
        <div className="mt-1 truncate text-xs text-zinc-500">{health.message || 'No status detail reported.'}</div>
      </div>
      <span className={`shrink-0 rounded-full px-2.5 py-1 text-[11px] font-semibold ${statusStyle[health.status]}`}>
        {health.status.toLowerCase().replace('_', ' ')}
      </span>
    </div>
  );
}

export const ConfigureDrawer: React.FC<ConfigureDrawerProps> = ({
  open,
  input,
  readiness,
  steps,
  onChangeSteps,
  onChangeInput,
  onClose,
  onSave,
  onOpenCapCutAutomation,
}) => {
  useEffect(() => {
    if (!open) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', closeOnEscape);
    return () => window.removeEventListener('keydown', closeOnEscape);
  }, [onClose, open]);

  if (!open) return null;

  return (
    <div className="absolute inset-0 z-40 flex justify-end bg-black/55" role="presentation" onMouseDown={onClose}>
      <aside
        role="dialog"
        aria-modal="true"
        aria-labelledby="floword-configure-title"
        className="flex h-full w-full max-w-3xl flex-col border-l border-white/[0.08] bg-[#0f131c] shadow-2xl"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header className="flex items-center justify-between border-b border-white/[0.08] px-5 py-4">
          <div>
            <h2 id="floword-configure-title" className="text-base font-semibold text-white">Configure</h2>
            <p className="mt-1 text-xs text-zinc-500">Runtime capabilities and advanced workflow settings.</p>
          </div>
          <button type="button" onClick={onClose} aria-label="Close Configure" className="rounded-lg p-2 text-zinc-400 hover:bg-white/[0.06] hover:text-white">
            <X className="h-5 w-5" />
          </button>
        </header>

        <div className="flex-1 space-y-6 overflow-y-auto p-5">
          <section aria-labelledby="configure-ai">
            <h3 id="configure-ai" className="mb-3 text-xs font-semibold uppercase tracking-[0.16em] text-zinc-500">AI</h3>
            <CapabilityToolsView tool="providers" />
          </section>

          <VisualGenerationProvider />

          <section className="floword-card p-5" aria-labelledby="configure-voice">
            <h3 id="configure-voice" className="text-xs font-semibold uppercase tracking-[0.16em] text-zinc-500">Voice</h3>
            <div className="mt-3 grid gap-4 sm:grid-cols-2">
              <div><div className="text-xs text-zinc-500">Voice</div><div className="mt-1 text-sm capitalize text-white">{input.tone}</div></div>
              <div><div className="text-xs text-zinc-500">Language</div><div className="mt-1 text-sm text-white">{input.language}</div></div>
            </div>
          </section>

          <section className="floword-card p-5" aria-labelledby="configure-capabilities">
            <h3 id="configure-capabilities" className="text-xs font-semibold uppercase tracking-[0.16em] text-zinc-500">Media</h3>
            <StatusRow label="OpenMontage / FFmpeg" health={readiness.openMontage} />
            <h3 className="mb-1 mt-5 text-xs font-semibold uppercase tracking-[0.16em] text-zinc-500">Research</h3>
            <StatusRow label="MediaCrawler" health={readiness.mediaCrawler} />
            <h3 className="mb-1 mt-5 text-xs font-semibold uppercase tracking-[0.16em] text-zinc-500">Automation</h3>
            <StatusRow label="Playwright / CDP" health={readiness.playwrightCdp} />
            <h3 className="mb-1 mt-5 text-xs font-semibold uppercase tracking-[0.16em] text-zinc-500">Output</h3>
            <StatusRow label="CapCut Mate" health={readiness.mateAgent} />
          </section>

          <section aria-labelledby="configure-research-session">
            <h3 id="configure-research-session" className="mb-3 text-xs font-semibold uppercase tracking-[0.16em] text-zinc-500">Research Session</h3>
            <CapabilityToolsView
              tool="research"
              researchPlatform={input.researchPlatform}
              xhsVariant={input.xhsVariant}
              onVariantChange={(variant) => onChangeInput?.('xhsVariant', variant)}
            />
          </section>

          <details className="floword-card p-5">
            <summary className="cursor-pointer text-sm font-semibold text-white">Service health details</summary>
            <div className="mt-5"><ServicesView onOpenCapCutAutomation={onOpenCapCutAutomation} /></div>
          </details>

          <details className="floword-card p-5">
            <summary className="cursor-pointer text-sm font-semibold text-white">Advanced pipeline design</summary>
            <div className="mt-5"><FlowDesignView steps={steps} onChangeSteps={onChangeSteps} /></div>
          </details>
        </div>

        <footer className="flex justify-end gap-2 border-t border-white/[0.08] px-5 py-4">
          <button type="button" onClick={onClose} className="floword-button floword-button-secondary text-zinc-300">Close</button>
          <button type="button" onClick={onSave} className="floword-button floword-button-primary"><Save className="h-4 w-4" /> Save</button>
        </footer>
      </aside>
    </div>
  );
};
