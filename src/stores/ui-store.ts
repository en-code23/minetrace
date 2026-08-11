import { create } from "zustand";

export type ThemePreference = "dark" | "light" | "system";

interface UiPreferences {
  theme: ThemePreference;
  privacyMask: boolean;
  sidebarCollapsed: boolean;
}

interface UiState extends UiPreferences {
  setTheme: (theme: ThemePreference) => void;
  togglePrivacyMask: () => void;
  toggleSidebar: () => void;
}

const STORAGE_KEY = "minetrace.interface.v1";
const defaults: UiPreferences = {
  theme: "dark",
  privacyMask: false,
  sidebarCollapsed: false,
};

function readPreferences(): UiPreferences {
  if (typeof window === "undefined") return defaults;
  try {
    const value = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<UiPreferences> | null;
    return {
      theme: value?.theme === "light" || value?.theme === "system" || value?.theme === "dark" ? value.theme : defaults.theme,
      privacyMask: typeof value?.privacyMask === "boolean" ? value.privacyMask : defaults.privacyMask,
      sidebarCollapsed: typeof value?.sidebarCollapsed === "boolean" ? value.sidebarCollapsed : defaults.sidebarCollapsed,
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

export const useUiStore = create<UiState>((set) => ({
  ...initial,
  setTheme: (theme) => {
    applyTheme(theme);
    set((state) => {
      const next = { theme, privacyMask: state.privacyMask, sidebarCollapsed: state.sidebarCollapsed };
      persistPreferences(next);
      return { theme };
    });
  },
  togglePrivacyMask: () => set((state) => {
    const privacyMask = !state.privacyMask;
    persistPreferences({ theme: state.theme, privacyMask, sidebarCollapsed: state.sidebarCollapsed });
    return { privacyMask };
  }),
  toggleSidebar: () => set((state) => {
    const sidebarCollapsed = !state.sidebarCollapsed;
    persistPreferences({ theme: state.theme, privacyMask: state.privacyMask, sidebarCollapsed });
    return { sidebarCollapsed };
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
