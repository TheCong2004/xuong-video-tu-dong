import { create } from "zustand";

export type TabId =
  | "APPS"
  | "FLOWORD_STUDIO"
  | "CAPCUT_AUTOMATION"
  | "OMNI_ROUTE"
  | "YOUWEE"
  | "MEDIA_CRAWLER"
  | "INKOS";

const DEFAULT_TAB: TabId = "FLOWORD_STUDIO";

interface TabState {
  // Current active tab
  activeTabId: TabId;
  // Tab data stored as stringified JSON
  tabData: {
    [K in TabId]?: string;
  };
  // Actions
  setActiveTab: (tabId: TabId) => Promise<boolean>;
  updateTabData: (tabId: TabId, data: unknown) => void;
  getTabData: <T>(tabId: TabId) => T | null;
  clearTabData: (tabId: TabId) => void;
}

export const useTabStore = create<TabState>((set, get) => ({
  activeTabId: DEFAULT_TAB,
  tabData: {},

  setActiveTab: async (newTabId) => {
    const currentTabId = get().activeTabId;
    if (currentTabId === newTabId) return true;

    set({ activeTabId: newTabId });
    return true;
  },

  updateTabData: (tabId, data) => {
    set((state) => ({
      tabData: {
        ...state.tabData,
        [tabId]: JSON.stringify(data),
      },
    }));
  },

  getTabData: <T>(tabId: TabId): T | null => {
    const state = get();
    const data = state.tabData[tabId];
    if (!data) return null;
    try {
      return JSON.parse(data) as T;
    } catch (e) {
      console.error(`Error parsing tab data for ${tabId}:`, e);
      return null;
    }
  },

  clearTabData: (tabId) => {
    set((state) => {
      const newTabData = { ...state.tabData };
      delete newTabData[tabId];
      return { tabData: newTabData };
    });
  },
}));
