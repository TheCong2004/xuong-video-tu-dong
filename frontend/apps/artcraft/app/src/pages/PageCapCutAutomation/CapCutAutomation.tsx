import { useEffect, useState } from "react";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import {
  faCircle,
  faClapperboard,
} from "@fortawesome/pro-solid-svg-icons";
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
  const [sideNav, setSideNav] = useState<SideNavId>("auto-render");
  const usesLegacyBackend = sideNav !== "auto-render";
  const [legacyReady, setLegacyReady] = useState(false);
  const [legacyError, setLegacyError] = useState<string | null>(null);

  useEffect(() => {
    if (!usesLegacyBackend) {
      setLegacyReady(false);
      setLegacyError(null);
      return;
    }

    let cancelled = false;
    setLegacyReady(false);
    setLegacyError(null);
    void ensureLegacyCapCutMate()
      .then(() => {
        if (!cancelled) setLegacyReady(true);
      })
      .catch((reason: unknown) => {
        if (!cancelled) {
          setLegacyError(
            reason instanceof Error ? reason.message : String(reason),
          );
        }
      });
    return () => {
      cancelled = true;
    };
  }, [usesLegacyBackend]);

  return (
    <div className="floword-shell capcut-workspace fixed inset-0 flex flex-col bg-[#0d1017] pt-[56px] text-white">
      <style>{`
        .capcut-workspace {
          --flow-bg: #0d1017;
          --flow-surface: #121622;
          --flow-border: rgb(51 65 85 / 0.42);
          --flow-primary: #e54d5e;
          --flow-accent: #6366f1;
          color: #e6e6ef;
          font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", system-ui, sans-serif;
          -webkit-font-smoothing: antialiased;
          background-image: linear-gradient(to right, rgba(255,255,255,0.035) 1px, transparent 1px), linear-gradient(to bottom, rgba(255,255,255,0.035) 1px, transparent 1px);
          background-size: 32px 32px;
        }
        .capcut-workspace > * { position: relative; z-index: 1; }
        .capcut-workspace [class*="border-white"] {
          border-color: var(--flow-border) !important;
        }
        .capcut-workspace [class*="border-slate-700"] {
          border-color: var(--flow-border) !important;
        }
        .capcut-workspace [class*="border-slate-800"] {
          border-color: rgb(30 41 59 / 0.8) !important;
        }
        .capcut-workspace [class~="border"]:not([class*="border-rose"]):not([class*="border-cyan"]),
        .capcut-workspace [class~="border-b"]:not([class*="border-rose"]):not([class*="border-cyan"]),
        .capcut-workspace [class~="border-t"]:not([class*="border-rose"]):not([class*="border-cyan"]),
        .capcut-workspace [class~="border-l"]:not([class*="border-rose"]):not([class*="border-cyan"]),
        .capcut-workspace [class~="border-r"]:not([class*="border-rose"]):not([class*="border-cyan"]) {
          border-color: var(--flow-border) !important;
        }
        .capcut-workspace [class~="border"][class*="bg-"]:not([class*="border-l"]):not([class*="border-r"]):not([class*="border-rose"]):not([class*="border-cyan"]) {
          border-radius: 12px;
        }
        .capcut-workspace button,
        .capcut-workspace input,
        .capcut-workspace select,
        .capcut-workspace textarea {
          border-radius: 9px;
        }
        .capcut-workspace [class*="hover:border-white"]:hover {
          border-color: rgb(103 232 249 / 0.42) !important;
        }
        .capcut-workspace [class*="bg-[#1a1b1f]"] {
          background-color: #0f171e !important;
        }
        .capcut-workspace [class*="bg-[#16171b]"] {
          background-color: #141e27 !important;
        }
        .capcut-workspace [class*="bg-[#252830]"] {
          background-color: #1a2631 !important;
        }
      `}</style>
      <EditorToolbar usesLegacyBackend={usesLegacyBackend} />
      <div className="flex min-h-0 flex-1">
        <CapCutSideNav activeId={sideNav} onSelect={setSideNav} />
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          {usesLegacyBackend ? (
            <CapCutMateProvider>
              <ProjectBar />
              <div className="flex min-h-0 flex-1">
                <main className="flex min-h-0 min-w-0 flex-1 flex-col">
                  <LegacyPanelState
                    ready={legacyReady}
                    error={legacyError}
                    sideNav={sideNav}
                  />
                </main>
                <AllProjectPanel />
              </div>
            </CapCutMateProvider>
          ) : (
            <main className="flex min-h-0 min-w-0 flex-1 flex-col">
              <NativeAutomationWorkspace />
            </main>
          )}
        </div>
      </div>
    </div>
  );
}

function EditorToolbar({ usesLegacyBackend }: { usesLegacyBackend: boolean }) {
  return (
    <div className="flex h-14 shrink-0 items-center gap-3 border-b border-white/[0.08] bg-[#0b0e14]/95 px-4 shadow-[0_8px_30px_rgba(0,0,0,0.22)]">
      <div className="flex items-center gap-2.5">
        <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-gradient-to-br from-[#e54d5e] to-[#a855f7] text-white shadow-lg shadow-rose-500/20">
          <FontAwesomeIcon icon={faClapperboard} />
        </div>
        <div className="leading-tight">
          <div className="text-sm font-bold tracking-wide text-white">CapCut Automation</div>
          <div className="text-[10px] uppercase tracking-[0.18em] text-zinc-500">Không gian biên tập nội bộ</div>
        </div>
      </div>
      <div className="ml-4 flex items-center gap-2 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2.5 py-1 text-[11px] text-emerald-200">
        <FontAwesomeIcon icon={faCircle} className="text-[7px] text-emerald-400" />
        {usesLegacyBackend ? "Công cụ draft" : "Kết xuất nội bộ"}
      </div>
      <div className="ml-auto hidden items-center gap-2 text-[11px] text-slate-400 sm:flex">
        <span className="rounded-md border border-slate-700/70 bg-slate-900/40 px-2.5 py-1.5">Chỉ chạy trên máy</span>
        <span className="text-slate-500">Chọn công cụ ở thanh bên</span>
      </div>
    </div>
  );
}

function LegacyPanelState({ ready, error, sideNav }: { ready: boolean; error: string | null; sideNav: SideNavId }) {
  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center p-8 text-center">
        <div className="max-w-md rounded-2xl border border-rose-400/20 bg-rose-400/5 p-6">
          <h2 className="text-base font-semibold text-rose-200">Không kết nối được công cụ draft</h2>
          <p className="mt-2 text-xs leading-relaxed text-slate-300/70">Kết xuất nội bộ vẫn hoạt động độc lập. Khởi động CapCut Mate để dùng các công cụ chỉnh draft cũ.</p>
          <p className="mt-3 break-words text-left font-mono text-[10px] text-rose-200/70">{error}</p>
        </div>
      </div>
    );
  }
  if (!ready) return <div className="flex flex-1 items-center justify-center text-sm text-slate-400">Đang kết nối công cụ draft…</div>;
  return <LegacyPanel sideNav={sideNav} />;
}

function LegacyPanel({ sideNav }: { sideNav: SideNavId }) {
  switch (sideNav) {
    case "materials": return <MaterialsPanel />;
    case "local-draft": return <LocalDraftPanel />;
    case "sync": return <SyncPanel />;
    case "caption": return <CaptionPanel />;
    case "effects": return <EffectsPanel />;
    case "transitions": return <TransitionsPanel />;
    case "filters": return <FiltersPanel />;
    case "stickers": return <StickersPanel />;
    case "animations": return <AnimationsPanel />;
    case "sounds": return <SoundsPanel />;
    case "adjustment": return <AdjustmentPanel />;
    case "media": return <MediaPanel />;
    case "keyframe": return <KeyframePanel />;
    case "ai-generate": return <AiGeneratePanel />;
    case "extension": return <ExtensionPanel />;
    case "auto-render": return <NativeAutomationWorkspace />;
  }
}
