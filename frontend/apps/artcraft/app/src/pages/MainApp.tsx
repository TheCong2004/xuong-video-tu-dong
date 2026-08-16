// Top-level shell for the artcraft app. Always-mounted chrome
// (TopBar, login + pricing modals, toaster, Tauri event listeners,
// background refresh hooks) lives here, and a single tab-driven
// switch picks the active page below it.

import React, { Component, useEffect, useState } from "react";
import * as gpu from "detect-gpu";
import { useSignals } from "@preact/signals-react/runtime";

import { TopBar } from "~/components";
import { ErrorDialog } from "~/components";
import { LoginModal, useLoginModalStore } from "@storyteller/ui-login-modal";
import { toast, Toaster } from "@storyteller/ui-toaster";
import {
  GalleryDragComponent,
  GalleryItem,
  onImageDrop,
  removeImageDropListener,
} from "@storyteller/ui-gallery-modal";
import {
  PricingModal,
  CreditsModal,
  useCreditsModalStore,
} from "@storyteller/ui-pricing-modal";
import {
  isActionReminderOpen,
  actionReminderProps,
  ActionReminderModal,
} from "@storyteller/ui-action-reminder-modal";
import {
  useGenerationEnqueueSuccessEvent,
} from "@storyteller/tauri-events";
import { useStoryboardPageEnabled } from "@storyteller/ui-settings-modal";
import { DomLevels, usePageSceneStore } from "@storyteller/ui-pagescene";

import { useActiveJobs } from "~/hooks/useActiveJobs";
import { useBackgroundLoadingMedia } from "~/hooks/useBackgroundLoadingMedia";
import { UsersApi } from "~/Classes/ApiManager";
import { authentication } from "~/signals";
import { AUTH_STATUS } from "~/enums";
import { useTabStore } from "./Stores/TabState";

import { AppsIndexPage } from "./PageApps/AppsIndexPage";
const PageDraw = React.lazy(() => import("./PageDraw/PageDraw"));
const TextToImage = React.lazy(() => import("./PageImage/TextToImage"));
const ImageToVideo = React.lazy(() => import("./PageVideo/ImageToVideo"));
const CreateAudio = React.lazy(() => import("./PageAudio/CreateAudio"));
const VideoFrameExtractor = React.lazy(() => import("./PageVideoFrameExtractor").then(m => ({ default: m.VideoFrameExtractor })));
const VideoWatermarkRemover = React.lazy(() => import("./PageVideoWatermarkRemover").then(m => ({ default: m.VideoWatermarkRemover })));
const ImageWatermarkRemover = React.lazy(() => import("./PageImageWatermarkRemover").then(m => ({ default: m.ImageWatermarkRemover })));
const ImageTo3DObject = React.lazy(() => import("./PageImageTo3DObject").then(m => ({ default: m.ImageTo3DObject })));
const ImageTo3DWorld = React.lazy(() => import("./PageImageTo3DWorld").then(m => ({ default: m.ImageTo3DWorld })));
const RemoveBackground = React.lazy(() => import("./PageRemoveBackground").then(m => ({ default: m.RemoveBackground })));
const Angles = React.lazy(() => import("./PageAngles").then(m => ({ default: m.Angles })));
const Storyboard = React.lazy(() => import("./PageStoryboard").then(m => ({ default: m.Storyboard })));
const PageBackgroundChange = React.lazy(() => import("./PageBackgroundChange").then(m => ({ default: m.PageBackgroundChange })));
const PageScene = React.lazy(() => import("./PageScene").then(m => ({ default: m.PageScene })));
const PageVideoEditor = React.lazy(() => import("./PageVideoEditor").then(m => ({ default: m.PageVideoEditor })));
const PageMoodboard = React.lazy(() => import("./PageMoodboard").then(m => ({ default: m.PageMoodboard })));
const CapCutAutomation = React.lazy(() => import("./PageCapCutAutomation").then(m => ({ default: m.CapCutAutomation })));
const Youwee = React.lazy(() => import("./PageYouwee").then(m => ({ default: m.Youwee })));
const PageMediaCrawler = React.lazy(() => import("./PageMediaCrawler").then(m => ({ default: m.PageMediaCrawler })));
const PageOpenMontage = React.lazy(() => import("./PageOpenMontage").then(m => ({ default: m.PageOpenMontage })));
const PageFreeLLMApi = React.lazy(() => import("./freellmapi").then(m => ({ default: m.PageFreeLLMApi })));
const PageOmniRoute = React.lazy(() => import("./OmniRoute/index").then(m => ({ default: m.PageOmniRoute })));
const PageFlowordStudio = React.lazy(() => import("./FlowordStudio").then(m => ({ default: m.PageFlowordStudio })));
const PageInkOS = React.lazy(() => import("./PageInkOS").then(m => ({ default: m.PageInkOS })));
const PageVynaro = React.lazy(() => import("./PageVynaro").then(m => ({ default: m.PageVynaro })));
import {
  topNavMediaId,
  topNavMediaUrl,
} from "~/components/signaled/TopBar/TopBar";

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
    console.error(`[TabErrorBoundary] Error in ${this.props.tabName}:`, error, errorInfo);
  }
  render() {
    if (this.state.hasError) {
      return (
        <div className="flex h-[calc(100vh-56px)] w-full flex-col items-center justify-center bg-[#121318] p-8 text-center text-slate-200">
          <div className="rounded-2xl border border-red-500/20 bg-[#1c1e26] p-8 max-w-md space-y-4 shadow-xl">
            <h3 className="text-xl font-bold text-white">
              Ứng dụng {this.props.tabName} gặp sự cố
            </h3>
            <p className="text-xs text-red-400 font-mono bg-[#0e0f14] p-3 rounded-xl overflow-x-auto text-left">
              {this.state.error?.message || "Lỗi không xác định"}
            </p>
            <button
              onClick={() => this.setState({ hasError: false, error: null })}
              className="px-5 py-2.5 rounded-xl bg-blue-600 hover:bg-blue-500 text-white font-medium text-sm transition"
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
  useGenerationEnqueueSuccessEvent();

  useEffect(() => {
    const usersApi = new UsersApi();
    usersApi.GetSession().then((result) => {
      console.log(
        `User Info | Username: ${result.data?.user?.username}, Token: ${result.data?.user?.user_token}`,
      );
    });
  }, []);

  const [, setValidGpu] = useState("unknown");
  useEffect(() => {
    const { getGPUTier } = gpu;
    getGPUTier().then((gpuTier) => {
      console.log("GPU tier", gpuTier);
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
      // Preload high-priority apps so they are ready when clicked
      console.log("[MainApp] Preloading FlowordStudio and OmniRoute in background...");
      import("./FlowordStudio");
      import("./OmniRoute/index");
    };

    if ("requestIdleCallback" in window) {
      window.requestIdleCallback(preloadAppChunks, { timeout: 5000 });
    } else {
      setTimeout(preloadAppChunks, 2000);
    }
  }, []);

  const { triggerRecheck } = useLoginModalStore();
  const { isOpen: isCreditsOpen, closeModal: closeCreditsModal } =
    useCreditsModalStore();
  const disableHotkeyInput = usePageSceneStore((s) => s.disableHotkeyInput);
  const enableHotkeyInput = usePageSceneStore((s) => s.enableHotkeyInput);

  const currentReminderModalProps = actionReminderProps.value;

  return (
    <div className="w-screen">
      <TopBar
        loginSignUpPressed={() => {
          console.log("PRESSED");
          triggerRecheck();
        }}
        pageName="Edit Scene"
      />
      <LoginModal
        videoSrc2D="/resources/videos/artcraft-canvas-demo.mp4"
        videoSrc3D="/resources/videos/artcraft-3d-demo.mp4"
        onOpenChange={(isOpen: boolean) => {
          if (isOpen) {
            disableHotkeyInput(DomLevels.DIALOGUE);
          } else {
            enableHotkeyInput(DomLevels.DIALOGUE);
          }
        }}
        onArtCraftAuthSuccess={(userInfo: any) => {
          authentication.status.value = AUTH_STATUS.LOGGED_IN;
          authentication.userInfo.value = userInfo;
        }}
      />

      <TabBody sceneToken={sceneToken} />

      <GalleryDragComponent />
      <ErrorDialog />
      <Toaster offsetTop={70} offsetRight={12} zIndex={9999} />
      {currentReminderModalProps && (
        <ActionReminderModal
          isOpen={isActionReminderOpen.value}
          onClose={currentReminderModalProps.onClose}
          reminderType={currentReminderModalProps.reminderType}
          onPrimaryAction={currentReminderModalProps.onPrimaryAction}
          title={currentReminderModalProps.title}
          message={currentReminderModalProps.message}
          primaryActionText={currentReminderModalProps.primaryActionText}
          secondaryActionText={currentReminderModalProps.secondaryActionText}
          onSecondaryAction={currentReminderModalProps.onSecondaryAction}
          isLoading={currentReminderModalProps.isLoading}
          openAiLogo={currentReminderModalProps.openAiLogo}
          primaryActionIcon={currentReminderModalProps.primaryActionIcon}
          primaryActionBtnClassName={
            currentReminderModalProps.primaryActionBtnClassName
          }
        />
      )}
      <PricingModal />
      <CreditsModal isOpen={isCreditsOpen} onClose={closeCreditsModal} />
    </div>
  );
};

const TabFallback = () => (
  <div className="flex h-[calc(100vh-56px)] w-full items-center justify-center bg-[#0f1015]">
    <div className="flex flex-col items-center gap-3">
      <div className="h-8 w-8 animate-spin rounded-full border-2 border-indigo-500/20 border-t-indigo-500" />
      <div className="text-xs text-slate-500">Loading module...</div>
    </div>
  </div>
);

const TabBody = ({ sceneToken }: { sceneToken?: string }) => {
  const tabStore = useTabStore();
  const storyboardPageEnabled = useStoryboardPageEnabled();

  const tabId = tabStore.activeTabId;

  // OmniRoute mounts on first activation instead of on app boot: mounting at
  // boot raced the OmniRoute dev server on :20128, so the iframe requested
  // /omniroute/ before Next was listening and never recovered. Once mounted it
  // stays warm — visibility is still toggled via CSS on tab switch.
  const [omniRouteMounted, setOmniRouteMounted] = useState(false);
  useEffect(() => {
    if (tabId === "OMNI_ROUTE") setOmniRouteMounted(true);
  }, [tabId]);

  return (
    <>
      {/* Persistent OmniRoute container — mounts on first visit, then stays warm */}
      <div
        data-testid="omniroute-persistent-container"
        className={tabId === "OMNI_ROUTE" ? "h-[calc(100vh-56px)] w-full overflow-hidden block" : "hidden"}
      >
        {omniRouteMounted && (
          <TabErrorBoundary tabName="OMNI_ROUTE">
            <React.Suspense fallback={<TabFallback />}>
              <PageOmniRoute />
            </React.Suspense>
          </TabErrorBoundary>
        )}
      </div>

      {tabId !== "OMNI_ROUTE" && (
        <TabErrorBoundary tabName={tabId} key={tabId}>
          <React.Suspense fallback={<TabFallback />}>
            {(() => {
              switch (tabId) {
                case "3D":
                  return <PageScene sceneToken={sceneToken} />;
                case "APPS":
                  return (
                    <div>
                      <AppsIndexPage />
                    </div>
                  );
              case "2D":
                return (
                  <div>
                    <PageDrawWithGalleryDrop />
                  </div>
                );
              case "IMAGE":
                return (
                  <div>
                    <TextToImage
                      imageMediaId={topNavMediaId.value}
                      imageUrl={topNavMediaUrl.value}
                    />
                  </div>
                );
              case "VIDEO":
                return (
                  <div>
                    <ImageToVideo />
                  </div>
                );
              case "AUDIO":
                return (
                  <div>
                    <CreateAudio />
                  </div>
                );
              case "VIDEO_FRAME_EXTRACTOR":
                return (
                  <div>
                    <VideoFrameExtractor />
                  </div>
                );
              case "VIDEO_WATERMARK_REMOVAL":
                return (
                  <div>
                    <VideoWatermarkRemover />
                  </div>
                );
              case "IMAGE_WATERMARK_REMOVAL":
                return (
                  <div>
                    <ImageWatermarkRemover />
                  </div>
                );
              case "IMAGE_TO_3D_OBJECT":
                return (
                  <div>
                    <ImageTo3DObject />
                  </div>
                );
              case "IMAGE_TO_3D_WORLD":
                return (
                  <div>
                    <ImageTo3DWorld />
                  </div>
                );
              case "REMOVE_BACKGROUND":
                return (
                  <div>
                    <RemoveBackground />
                  </div>
                );
              case "ANGLES":
                return (
                  <div>
                    <Angles />
                  </div>
                );
              case "STORYBOARD":
                return storyboardPageEnabled ? (
                  <div>
                    <Storyboard />
                  </div>
                ) : null;
              case "BACKGROUND_CHANGE":
                return (
                  <div>
                    <PageBackgroundChange />
                  </div>
                );
              case "VIDEO_EDITOR":
                return (
                  <div className="h-[calc(100vh-3rem)] w-full">
                    <PageVideoEditor />
                  </div>
                );
              case "MOODBOARD":
                return (
                  <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                    <PageMoodboard />
                  </div>
                );
              case "CAPCUT_AUTOMATION":
                return (
                  <div>
                    <CapCutAutomation />
                  </div>
                );
              case "YOUWEE":
                return (
                  <div>
                    <Youwee />
                  </div>
                );
              case "MEDIA_CRAWLER":
                return (
                  <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                    <PageMediaCrawler />
                  </div>
                );
              case "OPEN_MONTAGE":
                return (
                  <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                    <PageOpenMontage />
                  </div>
                );
              case "FREE_LLM_API":
                return (
                  <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                    <PageFreeLLMApi />
                  </div>
                );
              case "OMNI_ROUTE":
                return null;
              case "FLOWORD_STUDIO":
                return (
                  <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                    <PageFlowordStudio />
                  </div>
                );
              case "INKOS":
                return (
                  <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                    <PageInkOS />
                  </div>
                );
              case "VYNARO":
                return (
                  <div className="h-[calc(100vh-56px)] w-full overflow-hidden">
                    <PageVynaro />
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

const PageDrawWithGalleryDrop = () => {
  useEffect(() => {
    const handler = onImageDrop(
      (item: GalleryItem, position: { x: number; y: number }) => {
        const canvasElement = document.querySelectorAll("canvas")[0];
        if (!canvasElement) return;
        const rect = canvasElement.getBoundingClientRect();
        if (
          position.x >= rect.left &&
          position.x <= rect.right &&
          position.y >= rect.top &&
          position.y <= rect.bottom
        ) {
          const dropEvent = new CustomEvent("gallery-2d-drop", {
            detail: { item, position: { x: position.x - rect.left, y: position.y - rect.top } },
          });
          window.dispatchEvent(dropEvent);
        }
      },
    );

    return () => {
      if (handler) {
        removeImageDropListener(handler);
      }
    };
  }, []);

  return <PageDraw />;
};
