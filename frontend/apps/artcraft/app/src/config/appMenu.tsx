import { IconDefinition } from "@fortawesome/fontawesome-svg-core";
import {
  faGlobe,
  faSparkles,
  faClapperboardPlay,
  faCloudArrowDown,
  faSpider,
} from "@fortawesome/pro-solid-svg-icons";
import { useMemo } from "react";
import { useTabStore, TabId } from "~/pages/Stores/TabState";

export type AppId =
  | "FLOWORD_STUDIO"
  | "CAPCUT_AUTOMATION"
  | "OMNI_ROUTE"
  | "YOUWEE"
  | "MEDIA_CRAWLER"
  | "INKOS";

export interface AppDescriptor {
  id: AppId;
  label: string;
  icon: IconDefinition;
  imageSrc?: string;
  description?: string;
  large?: boolean;
}

export const APP_DESCRIPTORS: AppDescriptor[] = [
  {
    id: "FLOWORD_STUDIO",
    label: "Floword Studio",
    icon: faSparkles,
    description: "STIEN Content Transformation Engine (Quy trình tự động hóa CapCut 6 tầng).",
    large: true,
  },
  {
    id: "CAPCUT_AUTOMATION",
    label: "CapCut Studio",
    icon: faClapperboardPlay,
    description: "CapCut draft management, sticker injection, and timeline automation.",
    large: true,
  },
  {
    id: "OMNI_ROUTE",
    label: "OmniRoute",
    icon: faGlobe,
    description: "Router 290+ Nhà cung cấp LLM, MCP Server & A2A Protocol.",
    large: true,
  },
  {
    id: "YOUWEE",
    label: "Youwee",
    icon: faCloudArrowDown,
    description: "Download & process video media.",
    large: false,
  },
];

export interface FullAppItem {
  id: string;
  label: string;
  description: string;
  icon: IconDefinition;
  category: "generate" | "edit";
  badge?: "NEW" | "BEST" | "SOON" | "BETA";
  action?: AppId;
  color?: string;
}

interface AppCardPalette {
  accent: string;
  iconBg: string;
  iconColor: string;
}

const APP_CARD_PALETTES: Record<string, AppCardPalette> = {
  "floword-studio": {
    accent: "from-purple-500/20 to-purple-500/0",
    iconBg: "bg-purple-500/20 border-purple-400/30",
    iconColor: "text-purple-300",
  },
  "capcut-automation": {
    accent: "from-cyan-500/20 to-cyan-500/0",
    iconBg: "bg-cyan-500/20 border-cyan-400/30",
    iconColor: "text-cyan-300",
  },
  "omni-route": {
    accent: "from-indigo-500/20 to-indigo-500/0",
    iconBg: "bg-indigo-500/20 border-indigo-400/30",
    iconColor: "text-indigo-300",
  },
  youwee: {
    accent: "from-sky-500/20 to-emerald-500/0",
    iconBg: "bg-sky-500/20 border-sky-400/30",
    iconColor: "text-sky-300",
  },
  "media-crawler": {
    accent: "from-cyan-500/20 to-cyan-500/0",
    iconBg: "bg-cyan-500/20 border-cyan-400/30",
    iconColor: "text-cyan-300",
  },
  inkos: {
    accent: "from-purple-500/20 to-purple-500/0",
    iconBg: "bg-purple-500/20 border-purple-400/30",
    iconColor: "text-purple-300",
  },
};

const FALLBACK_APP_CARD_PALETTE: AppCardPalette = {
  accent: "from-white/10 to-white/0",
  iconBg: "bg-ui-controls border-ui-controls-border",
  iconColor: "text-base-fg",
};

export const getAppCardPalette = (id: string): AppCardPalette =>
  APP_CARD_PALETTES[id] ?? FALLBACK_APP_CARD_PALETTE;

export const ALL_APPS: FullAppItem[] = [
  {
    id: "floword-studio",
    label: "Floword Studio",
    description: "STIEN Content Transformation Engine (Quy trình tự động hóa CapCut 6 tầng)",
    icon: faSparkles,
    category: "generate",
    action: "FLOWORD_STUDIO",
    color: "bg-purple-500/40",
    badge: "BEST",
  },
  {
    id: "capcut-automation",
    label: "CapCut Studio",
    description: "Auto-sync footage, audio, and subtitles directly to CapCut Drafts",
    icon: faClapperboardPlay,
    category: "edit",
    action: "CAPCUT_AUTOMATION",
    color: "bg-cyan-500/40",
    badge: "NEW",
  },
  {
    id: "omni-route",
    label: "OmniRoute",
    description: "Router 290+ Nhà cung cấp LLM, MCP Server & A2A Protocol",
    icon: faGlobe,
    category: "generate",
    action: "OMNI_ROUTE",
    color: "bg-indigo-500/40",
    badge: "NEW",
  },
  {
    id: "youwee",
    label: "Youwee",
    description: "Tải và xử lý video đa nền tảng tốc độ cao",
    icon: faCloudArrowDown,
    category: "edit",
    action: "YOUWEE",
    color: "bg-sky-500/40",
    badge: "NEW",
  },
  {
    id: "media-crawler",
    label: "MediaCrawler",
    description: "Thu thập và xuất dữ liệu từ các nền tảng mạng xã hội",
    icon: faSpider,
    category: "edit",
    action: "MEDIA_CRAWLER",
    color: "bg-cyan-500/40",
    badge: "NEW",
  },
  {
    id: "inkos",
    label: "Story Studio",
    description: "InkOS Story Creation AI Agent Workbench",
    icon: faSparkles,
    category: "generate",
    action: "INKOS",
    color: "bg-purple-600/40",
    badge: "NEW",
  },
];

export const useVisibleApps = (): FullAppItem[] => {
  return useMemo(() => ALL_APPS, []);
};

export const useGenerateApps = (): FullAppItem[] => {
  const visible = useVisibleApps();
  return useMemo(
    () => visible.filter((app) => app.category === "generate"),
    [visible],
  );
};

export const useEditApps = (): FullAppItem[] => {
  const visible = useVisibleApps();
  return useMemo(
    () => visible.filter((app) => app.category === "edit"),
    [visible],
  );
};

export const getBadgeStyles = (badge?: string) => {
  switch (badge) {
    case "NEW":
      return "bg-teal-600 text-white";
    case "BEST":
      return "bg-primary text-white";
    case "SOON":
      return "bg-gray-600 text-white";
    case "BETA":
      return "bg-amber-500 text-white";
    default:
      return "";
  }
};

export const goToApp = (action?: string) => {
  if (
    action &&
    [
      "APPS",
      "FLOWORD_STUDIO",
      "CAPCUT_AUTOMATION",
      "OMNI_ROUTE",
      "YOUWEE",
      "MEDIA_CRAWLER",
      "INKOS",
    ].includes(action)
  ) {
    useTabStore.getState().setActiveTab(action as TabId);
  }
};
