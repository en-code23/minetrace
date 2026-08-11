import type { Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import { isTauriRuntime } from "../lib/runtime";

type UpdateStateName = "idle" | "checking" | "available" | "downloading" | "ready" | "current" | "error";

interface UpdateState {
  state: UpdateStateName;
  availableVersion: string | null;
  progress: number | null;
  message: string | null;
  update: Update | null;
  checkForUpdates: (installIfAvailable?: boolean) => Promise<void>;
  installAvailableUpdate: () => Promise<void>;
}

function readableError(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error) return error;
  return "The update service could not be reached.";
}

async function install(update: Update, set: (patch: Partial<UpdateState>) => void): Promise<void> {
  let received = 0;
  let total: number | undefined;
  set({ state: "downloading", progress: null, message: `Downloading MineTrace ${update.version}…` });
  await update.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength;
      received = 0;
    } else if (event.event === "Progress") {
      received += event.data.chunkLength;
    } else if (event.event === "Finished") {
      set({ state: "ready", progress: 1, message: "Update installed. Restarting MineTrace…" });
      return;
    }
    set({ progress: total && total > 0 ? Math.min(1, received / total) : null });
  });
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

export const useUpdateStore = create<UpdateState>((set, get) => ({
  state: "idle",
  availableVersion: null,
  progress: null,
  message: null,
  update: null,
  checkForUpdates: async (installIfAvailable = false) => {
    if (!isTauriRuntime() || get().state === "checking" || get().state === "downloading") return;
    set({ state: "checking", message: "Checking for updates…", progress: null });
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const update = await check({ timeout: 15_000 });
      if (!update) {
        set({ state: "current", availableVersion: null, update: null, message: "MineTrace is up to date." });
        return;
      }
      set({
        state: "available",
        availableVersion: update.version,
        update,
        message: `MineTrace ${update.version} is available.`,
      });
      if (installIfAvailable) await install(update, set);
    } catch (error: unknown) {
      set({ state: "error", update: null, progress: null, message: readableError(error) });
    }
  },
  installAvailableUpdate: async () => {
    const update = get().update;
    if (!update) return;
    try {
      await install(update, set);
    } catch (error: unknown) {
      set({ state: "error", progress: null, message: readableError(error) });
    }
  },
}));
