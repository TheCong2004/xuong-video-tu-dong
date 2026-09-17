import { useEffect, useState } from "react";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { faCircle, faClapperboard } from "@fortawesome/pro-solid-svg-icons";
import { CapCutMateProvider } from "./api/CapCutMateContext";
import { ensureLegacyCapCutMate } from "./api/pipelineClient";
import { AllProjectPanel } from "./layout/AllProjectPanel";
import { CapCutSideNav } from "./layout/CapCutSideNav";
import { ProjectBar } from "./layout/ProjectBar";
import { AdjustmentPanel } from "./panels/adjustment/AdjustmentPanel";
import { AiGeneratePanel } from "./panels/ai-generate/AiGeneratePanel";
import { AnimationsPanel } from "./panels/animations/AnimationsPanel";
import { NativeAutomationWorkspace } from "./panels/auto-render/NativeAutomationWorkspace";
import { CaptionPanel } from "./panels/caption/CaptionPanel";
import { EffectsPanel } from "./panels/effects/EffectsPanel";
import { ExtensionPanel } from "./panels/extension/ExtensionPanel";
import { FiltersPanel } from "./panels/filters/FiltersPanel";
import { KeyframePanel } from "./panels/keyframe/KeyframePanel";
import { MaterialsPanel } from "./panels/materials/MaterialsPanel";
import { LocalDraftPanel } from "./panels/local/LocalDraftPanel";
import { MediaPanel } from "./panels/media/MediaPanel";
import { SoundsPanel } from "./panels/sounds/SoundsPanel";
import { StickersPanel } from "./panels/stickers/StickersPanel";
import { SyncPanel } from "./panels/sync/SyncPanel";
import { TransitionsPanel } from "./panels/transitions/TransitionsPanel";
import type { SideNavId } from "./types";

/** One CapCut-style workspace: native render and draft tools are panels. */
export const CapCutAutomation = () => <CapCutAutomationWorkspace />;

function CapCutAutomationWorkspace() {
  const [sideNav, setSideNav] = useState<SideNavId>("local-draft");
  const usesLegacyBackend = sideNav !== "auto-render";
  const [legacyReady, setLegacyReady] = useState(true);
  const [legacyError, setLegacyError] = useState<string | null>(null);

  useEffect(() => {
    if (!usesLegacyBackend) {
      setLegacyError(null);
      return;
    }

    let cancelled = false;
    setLegacyReady(true);
    setLegacyError(null);
    void ensureLegacyCapCutMate()
      .then(() => {
        if (!cancelled) setLegacyReady(true);
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          console.warn("CapCut Mate readiness note:", reason);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [usesLegacyBackend]);

  return (
    <CapCutMateProvider>
      <div className="floword-shell relative flex h-full w-full overflow-hidden bg-[#0d1017] text-[#e6e6ef]">
        {/* 1. Left Fixed Sidebar */}
        <CapCutSideNav activeId={sideNav} onSelect={setSideNav} />

        {/* 2. Main Area */}
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
          {/* Top Header */}
          <ProjectBar />

          {/* Body */}
          <div className="flex min-h-0 flex-1 overflow-hidden">
            <main className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto">
              {usesLegacyBackend ? (
                <LegacyPanelState
                  ready={legacyReady}
                  error={legacyError}
                  sideNav={sideNav}
                />
              ) : (
                <NativeAutomationWorkspace />
              )}
            </main>
            {usesLegacyBackend && <AllProjectPanel />}
          </div>
        </div>
      </div>
    </CapCutMateProvider>
  );
}

function EditorToolbar({ usesLegacyBackend }: { usesLegacyBackend: boolean }) {
  return (
    <div className="flex h-14 shrink-0 items-center gap-3 border-b border-white/10 bg-[#0b0e14]/95 px-4 shadow-[0_8px_30px_rgba(0,0,0,0.22)]">
      <div className="flex items-center gap-2.5">
        <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-[#e54d5e] to-[#a855f7] text-white shadow-lg shadow-rose-500/20">
          <FontAwesomeIcon icon={faClapperboard} />
        </div>
        <div className="leading-tight">
          <div className="text-sm font-bold tracking-wide text-white">
            Xưởng Sản Xuất Video
          </div>
          <div className="text-zinc-500 text-[10px] uppercase tracking-[0.18em]">
            Không gian biên tập nội bộ
          </div>
        </div>
      </div>
      <div className="ml-4 flex items-center gap-2 rounded-full border border-white/10 bg-emerald-500/10 px-2.5 py-1 text-[11px] text-emerald-200">
        <FontAwesomeIcon
          icon={faCircle}
          className="text-[7px] text-emerald-400"
        />
        {usesLegacyBackend ? "Công cụ draft" : "Kết xuất nội bộ"}
      </div>
      <div className="text-slate-400 ml-auto hidden items-center gap-2 text-[11px] sm:flex">
        <span className="border-white/10 bg-slate-900/40 rounded-md border px-2.5 py-1.5">
          Chỉ chạy trên máy
        </span>
        <span className="text-slate-500">Chọn công cụ ở thanh bên</span>
      </div>
    </div>
  );
}

function LegacyPanelState({
  ready,
  error,
  sideNav,
}: {
  ready: boolean;
  error: string | null;
  sideNav: SideNavId;
}) {
  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center p-8 text-center">
        <div className="max-w-md rounded-2xl border border-white/10 bg-rose-400/5 p-6">
          <h2 className="text-base font-semibold text-rose-200">
            Không kết nối được công cụ draft
          </h2>
          <p className="text-slate-300/70 mt-2 text-xs leading-relaxed">
            Kết xuất nội bộ vẫn hoạt động độc lập. Khởi động CapCut Mate để dùng
            các công cụ chỉnh draft cũ.
          </p>
          <p className="mt-3 break-words text-left font-mono text-[10px] text-rose-200/70">
            {error}
          </p>
        </div>
      </div>
    );
  }
  if (!ready)
    return (
      <div className="text-slate-400 flex flex-1 items-center justify-center text-sm">
        Đang kết nối công cụ draft…
      </div>
    );
  return <LegacyPanel sideNav={sideNav} />;
}

function LegacyPanel({ sideNav }: { sideNav: SideNavId }) {
  switch (sideNav) {
    case "materials":
      return <MaterialsPanel />;
    case "local-draft":
      return <LocalDraftPanel />;
    case "sync":
      return <SyncPanel />;
    case "caption":
      return <CaptionPanel />;
    case "effects":
      return <EffectsPanel />;
    case "transitions":
      return <TransitionsPanel />;
    case "filters":
      return <FiltersPanel />;
    case "stickers":
      return <StickersPanel />;
    case "animations":
      return <AnimationsPanel />;
    case "sounds":
      return <SoundsPanel />;
    case "adjustment":
      return <AdjustmentPanel />;
    case "media":
      return <MediaPanel />;
    case "keyframe":
      return <KeyframePanel />;
    case "ai-generate":
      return <AiGeneratePanel />;
    case "extension":
      return <ExtensionPanel />;
    case "auto-render":
      return <NativeAutomationWorkspace />;
  }
}
