import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { ProfileData } from "../types/domain";

vi.mock("../lib/runtime", () => ({
  getProfile: vi.fn(),
  saveShareImage: vi.fn(),
}));

import { ProfilePage } from "./ProfilePage";

const profile: ProfileData = {
  generatedAt: "2026-08-11T12:00:00Z",
  identity: { name: "CaveBuilder", uuid: "12345678-1234-1234-1234-1234567890ab", source: "Prism Launcher", active: true },
  identities: [{ name: "CaveBuilder", uuid: "12345678-1234-1234-1234-1234567890ab", source: "Prism Launcher", active: true }],
  currentSkin: null,
  previousSkins: [],
  summary: {
    totalPlaytimeMinutes: 4_620,
    sessions: 91,
    activeDays: 32,
    mostPlayedVersion: "1.21.8",
    mostPlayedWorld: "Workshop",
    launcherCount: 1,
    availableWorlds: 1,
    missingWorlds: 1,
    backupCount: 2,
    statisticsWorlds: 1,
  },
  randomStats: [{ id: "minecraft:jump", label: "Jumps", value: 882, unit: "count", sourceWorlds: 1 }],
  statisticSections: [{ id: "general", label: "General", items: [{ id: "minecraft:jump", label: "Jumps", value: 882, unit: "count", sourceWorlds: 1 }] }],
  statisticsBasis: "uuidMatched",
  launchers: [{ id: "prism", name: "Prism Launcher", instances: 2, sessions: 91, totalMinutes: 4_620, firstObservedAt: "2026-01-01T10:00:00Z", lastObservedAt: "2026-08-11T10:00:00Z" }],
  worlds: [
    { id: "workshop", name: "Workshop", folderName: "Workshop", instance: "Builder Pack", launcher: "Prism Launcher", availability: "available", totalMinutes: 2_800, lastObservedAt: "2026-08-11T10:00:00Z", statsAvailable: true, statsBasis: "uuidMatched", backupCount: 2 },
    { id: "old-home", name: "Old Home", folderName: null, instance: "Builder Pack", launcher: "Observed archive", availability: "missing", totalMinutes: 220, lastObservedAt: "2026-03-11T10:00:00Z", statsAvailable: false, statsBasis: "none", backupCount: 0 },
  ],
  backups: [],
  limitations: ["A missing save may have been deleted, moved, renamed, or excluded from the approved scan locations."],
};

function renderProfile(data: ProfileData): string {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
  queryClient.setQueryData(["profile"], data);
  return renderToStaticMarkup(<QueryClientProvider client={queryClient}><ProfilePage /></QueryClientProvider>);
}

describe("Profile page", () => {
  it("renders locally attributable player and archive facts", () => {
    const html = renderProfile(profile);
    expect(html).toContain("CaveBuilder");
    expect(html).toContain("Prism Launcher");
    expect(html).toContain("1.21.8");
    expect(html).toContain("Jumps");
    expect(html).toContain("Share on social");
  });

  it("does not claim a missing save was definitely deleted", () => {
    const html = renderProfile(profile);
    expect(html).toContain("Save not found");
    expect(html).toContain("deleted, moved, renamed, or outside approved locations");
    expect(html).not.toContain("Deleted world");
  });
});
