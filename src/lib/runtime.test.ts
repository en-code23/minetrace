import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cancelScan,
  discoverInstallations,
  getDashboard,
  getInstances,
  getScanStatus,
  getServers,
  getSessions,
  getVersions,
  getWorlds,
  startScan,
} from "./runtime";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

afterEach(() => {
  vi.clearAllMocks();
  vi.unstubAllGlobals();
});

describe("desktop-only runtime boundary", () => {
  it("never substitutes browser demo data for native records", async () => {
    await expect(getDashboard()).rejects.toThrow("desktop app only");
    await expect(getSessions()).rejects.toThrow("desktop app only");
    await expect(getInstances()).rejects.toThrow("desktop app only");
    await expect(getWorlds()).rejects.toThrow("desktop app only");
    await expect(getServers()).rejects.toThrow("desktop app only");
    await expect(getVersions()).rejects.toThrow("desktop app only");
    await expect(discoverInstallations()).rejects.toThrow("desktop app only");
    await expect(startScan("standard")).rejects.toThrow("desktop app only");
    await expect(getScanStatus()).rejects.toThrow("desktop app only");
    await expect(cancelScan()).rejects.toThrow("desktop app only");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("maps every read command to the native boundary", async () => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockResolvedValue([]);

    await getSessions();
    await getInstances();
    await getWorlds();
    await getServers();
    await getVersions();

    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual([
      "get_sessions",
      "get_instances",
      "get_worlds",
      "get_servers",
      "get_versions",
    ]);
  });
});
