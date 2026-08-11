import { create } from "zustand";

export type ThemePreference = "dark" | "light" | "system";

interface UiPreferences {
  theme: ThemePreference;
  privacyMask: boolean;
  sidebarCollapsed: boolean;
  setupComplete: boolean;
  autoScan: boolean;
  autoUpdate: boolean;
}

interface UiState extends UiPreferences {
  setTheme: (theme: ThemePreference) => void;
  togglePrivacyMask: () => void;
  toggleSidebar: () => void;
  setAutoScan: (enabled: boolean) => void;
  setAutoUpdate: (enabled: boolean) => void;
  completeSetup: (preferences: { autoScan: boolean; autoUpdate: boolean }) => void;
}

const STORAGE_KEY = "minetrace.interface.v1";
const defaults: UiPreferences = {
  theme: "dark",
  privacyMask: false,
  sidebarCollapsed: false,
  setupComplete: false,
  autoScan: true,
  autoUpdate: true,
};

function readPreferences(): UiPreferences {
  if (typeof window === "undefined") return defaults;
  try {
    const value = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<UiPreferences> | null;
    return {
      theme: value?.theme === "light" || value?.theme === "system" || value?.theme === "dark" ? value.theme : defaults.theme,
      privacyMask: typeof value?.privacyMask === "boolean" ? value.privacyMask : defaults.privacyMask,
      sidebarCollapsed: typeof value?.sidebarCollapsed === "boolean" ? value.sidebarCollapsed : defaults.sidebarCollapsed,
      setupComplete: typeof value?.setupComplete === "boolean" ? value.setupComplete : defaults.setupComplete,
      autoScan: typeof value?.autoScan === "boolean" ? value.autoScan : defaults.autoScan,
      autoUpdate: typeof value?.autoUpdate === "boolean" ? value.autoUpdate : defaults.autoUpdate,
    };
  } catch {
    return defaults;
  }
}

function persistPreferences(value: UiPreferences): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  } catch {
    // A restricted webview can deny storage. The in-memory preference still works.
  }
}

function applyTheme(theme: ThemePreference) {
  const systemIsDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const resolved = theme === "system" ? (systemIsDark ? "dark" : "light") : theme;
  document.documentElement.dataset.theme = resolved;
  document.documentElement.style.colorScheme = resolved;
}

const initial = readPreferences();

function nextPreferences(state: UiState, patch: Partial<UiPreferences>): UiPreferences {
  return {
    theme: patch.theme ?? state.theme,
    privacyMask: patch.privacyMask ?? state.privacyMask,
    sidebarCollapsed: patch.sidebarCollapsed ?? state.sidebarCollapsed,
    setupComplete: patch.setupComplete ?? state.setupComplete,
    autoScan: patch.autoScan ?? state.autoScan,
    autoUpdate: patch.autoUpdate ?? state.autoUpdate,
  };
}

export const useUiStore = create<UiState>((set) => ({
  ...initial,
  setTheme: (theme) => {
    applyTheme(theme);
    set((state) => {
      const next = nextPreferences(state, { theme });
      persistPreferences(next);
      return { theme };
    });
  },
  togglePrivacyMask: () => set((state) => {
    const privacyMask = !state.privacyMask;
    persistPreferences(nextPreferences(state, { privacyMask }));
    return { privacyMask };
  }),
  toggleSidebar: () => set((state) => {
    const sidebarCollapsed = !state.sidebarCollapsed;
    persistPreferences(nextPreferences(state, { sidebarCollapsed }));
    return { sidebarCollapsed };
  }),
  setAutoScan: (autoScan) => set((state) => {
    persistPreferences(nextPreferences(state, { autoScan }));
    return { autoScan };
  }),
  setAutoUpdate: (autoUpdate) => set((state) => {
    persistPreferences(nextPreferences(state, { autoUpdate }));
    return { autoUpdate };
  }),
  completeSetup: ({ autoScan, autoUpdate }) => set((state) => {
    persistPreferences(nextPreferences(state, { setupComplete: true, autoScan, autoUpdate }));
    return { setupComplete: true, autoScan, autoUpdate };
  }),
}));

let followsSystemTheme = false;

export function initializeTheme() {
  applyTheme(useUiStore.getState().theme);
  if (followsSystemTheme) return;
  followsSystemTheme = true;
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    const { theme } = useUiStore.getState();
    if (theme === "system") applyTheme(theme);
  });
}
