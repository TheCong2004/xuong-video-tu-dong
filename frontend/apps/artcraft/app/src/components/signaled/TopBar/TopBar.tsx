import {
  faDash,
  faSquare,
  faWindowRestore,
  faXmark,
} from "@fortawesome/pro-regular-svg-icons";
import {
  faGear,
  faHouse,
  faImages,
} from "@fortawesome/pro-solid-svg-icons";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import { signal } from "@preact/signals-react";
import { useSignals } from "@preact/signals-react/runtime";
import { getCreatorIcon, ModelCreator } from "@storyteller/model-list";
import { gtagEvent } from "@storyteller/google-analytics";
import { ProviderBillingModal } from "@storyteller/provider-billing-modal";
import { ProviderSetupModal } from "@storyteller/provider-setup-modal";
import {
  useTauriPlatform,
  useTauriWindowControls,
} from "@storyteller/tauri-utils";
import { Button } from "@storyteller/ui-button";
import {
  GalleryModal,
  galleryModalLightboxVisible,
  galleryModalVisibleDuringDrag,
  galleryModalVisibleViewMode,
} from "@storyteller/ui-gallery-modal";
import {
  MenuIconItem,
  MenuIconSelector,
} from "@storyteller/ui-menu-icon-selector";
import { SettingsModal } from "@storyteller/ui-settings-modal";
import { Tooltip } from "@storyteller/ui-tooltip";
import { useEffect, useRef, useState } from "react";
import { APP_DESCRIPTORS } from "~/config/appMenu";
import {
  downloadMediaFileToDisk,
} from "~/components/generation-feed/desktopMediaActions";
import { TabId, useTabStore } from "~/pages/Stores/TabState";
import {
  galleryModalDeleteMedia,
  galleryModalSubscribeToMediaEvents,
} from "~/Helpers/galleryModalTauriBindings";
import { AppsQuickMenu } from "./AppsQuickMenu";
import { TaskQueue } from "./TaskQueue";
import { UploadImagesButton } from "./UploadImagesButton";

interface Props {
  pageName: string;
  loginSignUpPressed: () => void;
}

type SettingsSection =
  | "general"
  | "accounts"
  | "alerts"
  | "about"
  | "provider_priority"
  | "billing";

const SWITCHER_THROTTLE_TIME = 500;

const appMenuTabs: MenuIconItem[] = [
  {
    id: "APPS",
    label: "Home",
    icon: <FontAwesomeIcon icon={faHouse} />,
    description: "Explore all apps and miniapps",
    large: true,
    tooltipContent: <AppsQuickMenu />,
    tooltipInteractive: true,
    tooltipPosition: "bottom",
  },
  ...APP_DESCRIPTORS.map((d) => ({
    id: d.id,
    label: d.label,
    icon: <FontAwesomeIcon icon={d.icon} />,
    imageSrc: d.imageSrc,
    description: d.description,
    large: d.large,
  })),
];

export const topNavMediaId = signal<string>("");
export const topNavMediaUrl = signal<string>("");

export const TopBar = ({ pageName }: Props) => {
  useSignals();

  const [isSettingsModalOpen, setIsSettingsModalOpen] = useState(false);
  const [settingsSection, setSettingsSection] =
    useState<SettingsSection>("general");

  const { isDesktop, isMaximized, minimize, toggleMaximize, close } =
    useTauriWindowControls();
  const platform = useTauriPlatform();

  const handleOpenGalleryModal = () => {
    galleryModalVisibleViewMode.value = true;
    galleryModalVisibleDuringDrag.value = true;
    gtagEvent("open_gallery_modal", { tab: tabStore.activeTabId });
  };

  const tabStore = useTabStore();
  const [disableSwitcher, setDisableSwitcher] = useState(false);
  const switcherThrottle = useRef(false);

  const disableTabSwitcher = () => disableSwitcher;

  const downloadFile = downloadMediaFileToDisk;

  const getPageTitle = (tabId: string) => {
    switch (tabId) {
      case "FLOWORD_STUDIO":
        return "Floword Studio";
      case "CAPCUT_AUTOMATION":
        return "CapCut Studio Automation";
      case "OMNI_ROUTE":
        return "OmniRoute AI Router";
      case "YOUWEE":
        return "Youwee Video Processor";
      case "MEDIA_CRAWLER":
        return "MediaCrawler";
      case "INKOS":
        return "InkOS Story Studio";
      case "APPS":
        return "Workspace Tools";
      default:
        return pageName;
    }
  };

  const pageTitle = getPageTitle(tabStore.activeTabId);

  return (
    <>
      <header
        className="fixed top-0 z-40 flex h-[56px] w-screen items-center bg-ui-background p-2 text-base-fg select-none"
        data-tauri-drag-region
      >
        <nav
          className="relative grid h-[40px] w-full grid-cols-[1fr_auto_1fr] items-center"
          data-tauri-drag-region
        >
          <div className="flex items-center gap-2" data-tauri-drag-region>
            <MenuIconSelector
              menuItems={appMenuTabs}
              items={appMenuTabs}
              activeMenu={tabStore.activeTabId}
              activeId={tabStore.activeTabId}
              disabled={disableTabSwitcher()}
              onMenuChange={(tabId) => {
                if (switcherThrottle.current) return;
                switcherThrottle.current = true;
                setDisableSwitcher(true);

                useTabStore.getState().setActiveTab(tabId as TabId);
                setTimeout(() => {
                  switcherThrottle.current = false;
                  setDisableSwitcher(false);
                }, SWITCHER_THROTTLE_TIME);
              }}
              onChange={(tabId) => {
                if (switcherThrottle.current) return;
                switcherThrottle.current = true;
                setDisableSwitcher(true);

                useTabStore.getState().setActiveTab(tabId as TabId);
                setTimeout(() => {
                  switcherThrottle.current = false;
                  setDisableSwitcher(false);
                }, SWITCHER_THROTTLE_TIME);
              }}
              className="no-drag w-fit"
            />
          </div>

          <div
            className="flex items-center justify-center gap-2 font-medium"
            data-tauri-drag-region
          >
            <h1
              className="flex items-center gap-2.5 text-base-fg font-semibold"
              data-tauri-drag-region
            >
              {getCreatorIcon(
                ModelCreator.ArtCraft,
                "h-5 w-5 icon-auto-contrast opacity-70",
              )}
              {pageTitle}
            </h1>
          </div>

          <div className="flex justify-end gap-2" data-tauri-drag-region>
            <div className="no-drag flex items-center gap-1.5">
              <UploadImagesButton className="h-[34px] w-[34px]" />

              <Tooltip content="Settings" position="bottom" delay={300}>
                <Button
                  variant="secondary"
                  icon={faGear}
                  className="h-[34px] w-[34px]"
                  onClick={() => {
                    setSettingsSection("general");
                    setIsSettingsModalOpen(true);
                    gtagEvent("open_settings_modal");
                  }}
                />
              </Tooltip>

              <Button
                variant="secondary"
                icon={faImages}
                onClick={handleOpenGalleryModal}
              >
                <span className="hidden whitespace-nowrap text-base-fg xl:block">
                  My Library
                </span>
              </Button>

              <TaskQueue />
            </div>

            {isDesktop && platform !== "macos" && (
              <div className="no-drag flex items-center">
                <Button
                  variant="secondary"
                  className="h-[32px] w-[44px] rounded-none border-0 bg-transparent text-base-fg opacity-70 shadow-none hover:bg-ui-controls/20 hover:opacity-100"
                  onClick={minimize}
                >
                  <FontAwesomeIcon icon={faDash} className="text-xs" />
                </Button>
                <Button
                  variant="secondary"
                  className="h-[32px] w-[44px] rounded-none border-0 bg-transparent text-base-fg opacity-70 shadow-none hover:bg-ui-controls/20 hover:opacity-100"
                  onClick={toggleMaximize}
                >
                  <FontAwesomeIcon
                    icon={isMaximized ? faWindowRestore : faSquare}
                    className="text-xs"
                  />
                </Button>
                <Button
                  variant="secondary"
                  className="h-[32px] w-[44px] rounded-none border-0 bg-transparent text-base-fg opacity-70 shadow-none hover:bg-red/10 hover:text-red"
                  onClick={close}
                >
                  <FontAwesomeIcon icon={faXmark} className="text-lg" />
                </Button>
              </div>
            )}
          </div>
        </nav>
      </header>

      <SettingsModal
        isOpen={isSettingsModalOpen}
        onClose={() => setIsSettingsModalOpen(false)}
        globalAccountLogoutCallback={() => {
          setIsSettingsModalOpen(false);
        }}
        initialSection={settingsSection}
      />

      <GalleryModal
        mode="view"
        onDownloadClicked={downloadFile}
        onDeleteMedia={galleryModalDeleteMedia}
        subscribeToMediaEvents={galleryModalSubscribeToMediaEvents}
      />

      <ProviderSetupModal />
      <ProviderBillingModal isVideoPage={false} />
    </>
  );
};
