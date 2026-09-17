// Top-level shell for the Floword / ArtCraft app.
// Tab-driven switch picks the active page below the TopBar.

import React, { Component, useEffect, useState } from "react";
import * as gpu from "detect-gpu";
import { useSignals } from "@preact/signals-react/runtime";

import { TopBar } from "~/components";
import { ErrorDialog } from "~/components";
import { Toaster } from "@storyteller/ui-toaster";
import { useActiveJobs } from "~/hooks/useActiveJobs";
import { useBackgroundLoadingMedia } from "~/hooks/useBackgroundLoadingMedia";
import { useTabStore } from "./Stores/TabState";

import { AppsIndexPage } from "./PageApps/AppsIndexPage";
const CapCutAutomation = React.lazy(() =>
  import("./PageCapCutAutomation").then((m) => ({
    default: m.CapCutAutomation,
  })),
);
const Youwee = React.lazy(() =>
  import("./PageYouwee").then((m) => ({ default: m.Youwee })),
);
const PageMediaCrawler = React.lazy(() =>
  import("./PageMediaCrawler").then((m) => ({ default: m.PageMediaCrawler })),
);
const PageOmniRoute = React.lazy(() =>
  import("./OmniRoute/index").then((m) => ({ default: m.PageOmniRoute })),
);
const PageFlowordStudio = React.lazy(() =>
  import("./FlowordStudio").then((m) => ({ default: m.PageFlowordStudio })),
);
const PageInkOS = React.lazy(() =>
  import("./PageInkOS").then((m) => ({ default: m.PageInkOS })),
);

interface Props {
  sceneToken?: string;
}

class TabErrorBoundary extends Component<
  { children: React.ReactNode; tabName: string },
  { hasError: boolean; error: Error | null }
> {
  constructor(props: any) {
    super(props);
    this.state = { hasError: false, error: null };
  }
  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }
  componentDidCatch(error: Error, errorInfo: any) {
    console.error(
      `[TabErrorBoundary] Error in ${this.props.tabName}:`,
      error,
      errorInfo,
    );
  }
  render() {
    if (this.state.hasError) {
      return (
        <div className="text-slate-200 flex h-[calc(100vh-56px)] w-full flex-col items-center justify-center bg-[#121318] p-8 text-center">
          <div className="max-w-md space-y-4 rounded-2xl border border-white/10 bg-[#1c1e26] p-8 shadow-xl">
            <h3 className="text-xl font-bold text-white">
              Ứng dụng {this.props.tabName} gặp sự cố
            </h3>
            <p className="overflow-x-auto rounded-xl bg-[#0e0f14] p-3 text-left font-mono text-xs text-red-400">
              {this.state.error?.message || "Lỗi không xác định"}
            </p>
            <button
              onClick={() => this.setState({ hasError: false, error: null })}
              className="rounded-xl bg-blue-600 px-5 py-2.5 text-sm font-medium text-white transition hover:bg-blue-500"
            >
              Thử lại
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

export const MainApp = ({ sceneToken }: Props) => {
  useSignals();

  useActiveJobs();
  useBackgroundLoadingMedia();

  const [, setValidGpu] = useState("unknown");
  useEffect(() => {
    const { getGPUTier } = gpu;
    getGPUTier().then((gpuTier) => {
      let isValid = false;
      const fps = gpuTier.fps || 0;
      if (gpuTier.tier > 1) isValid = true;
      if (fps > 15) isValid = true;
      if (gpuTier.gpu === "apple gpu (Apple GPU)") isValid = true;
      setValidGpu(isValid ? "valid" : "error");
    });
  }, []);

  // Idle Preloader for heavy priority chunks
  useEffect(() => {
    const preloadAppChunks = () => {
      import("./FlowordStudio");
      import("./OmniRoute/index");
    };

    if ("requestIdleCallback" in window) {
      window.requestIdleCallback(preloadAppChunks, { timeout: 5000 });
    } else {
      setTimeout(preloadAppChunks, 2000);
    }
  }, []);

  return (
    <div className="w-screen">
      <TopBar loginSignUpPressed={() => {}} pageName="Floword Studio" />

      <TabBody sceneToken={sceneToken} />

      <ErrorDialog />
      <Toaster offsetTop={70} offsetRight={12} zIndex={9999} />
    </div>
  );
};

const TabFallback = () => (
  <div className="flex h-[calc(100vh-56px)] w-full items-center justify-center bg-[#0f1015]">
    <div className="flex flex-col items-center gap-3">
      <div className="h-8 w-8 animate-spin rounded-full border-2 border-white/10 border-t-indigo-500" />
      <div className="text-slate-500 text-xs">Loading module...</div>
    </div>
  </div>
);

const TabBody = ({ sceneToken }: { sceneToken?: string }) => {
  const tabStore = useTabStore();
  const tabId = tabStore.activeTabId || "FLOWORD_STUDIO";

  const [omniRouteMounted, setOmniRouteMounted] = useState(false);
  const [capCutMounted, setCapCutMounted] = useState(false);
  const [flowordMounted, setFlowordMounted] = useState(true);

  useEffect(() => {
    if (tabId === "OMNI_ROUTE") setOmniRouteMounted(true);
    if (tabId === "CAPCUT_AUTOMATION") setCapCutMounted(true);
    if (tabId === "FLOWORD_STUDIO") setFlowordMounted(true);
  }, [tabId]);

  const isOtherTab = ["APPS", "YOUWEE", "MEDIA_CRAWLER", "INKOS"].includes(tabId);

  return (
    <>
      <div
        data-testid="omniroute-persistent-container"
        className={
          tabId === "OMNI_ROUTE"
            ? "h-[calc(100vh-56px)] w-full overflow-hidden block"
            : "hidden"
        }
      >
        {omniRouteMounted && (
          <TabErrorBoundary tabName="OMNI_ROUTE">
            <React.Suspense fallback={<TabFallback />}>
              <PageOmniRoute />
            </React.Suspense>
          </TabErrorBoundary>
        )}
      </div>

      <div
        data-testid="floword-persistent-container"
        className={
          tabId === "FLOWORD_STUDIO" || (!isOtherTab && tabId !== "OMNI_ROUTE" && tabId !== "CAPCUT_AUTOMATION")
            ? "h-[calc(100vh-56px)] w-full overflow-hidden block"
            : "hidden"
        }
      >
        {flowordMounted && (
          <TabErrorBoundary tabName="FLOWORD_STUDIO">
            <React.Suspense fallback={<TabFallback />}>
              <PageFlowordStudio />
            </React.Suspense>
          </TabErrorBoundary>
        )}
      </div>

      <div
        data-testid="capcut-persistent-container"
        className={
          tabId === "CAPCUT_AUTOMATION"
            ? "h-[calc(100vh-56px)] w-full overflow-hidden block"
            : "hidden"
        }
      >
        {capCutMounted && (
          <TabErrorBoundary tabName="CAPCUT_AUTOMATION">
            <React.Suspense fallback={<TabFallback />}>
              <CapCutAutomation />
            </React.Suspense>
          </TabErrorBoundary>
        )}
      </div>

      {isOtherTab && (
        <TabErrorBoundary tabName={tabId} key={tabId}>
          <React.Suspense fallback={<TabFallback />}>
            {(() => {
              switch (tabId) {
                case "APPS":
                  return <AppsIndexPage />;
                case "YOUWEE":
                  return <Youwee />;
                case "MEDIA_CRAWLER":
                  return (
                    <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                      <PageMediaCrawler />
                    </div>
                  );
                case "INKOS":
                  return (
                    <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                      <PageInkOS />
                    </div>
                  );
                default:
                  return null;
              }
            })()}
          </React.Suspense>
        </TabErrorBoundary>
      )}
    </>
  );
};
