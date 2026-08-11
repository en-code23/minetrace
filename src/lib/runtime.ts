import { invoke } from "@tauri-apps/api/core";
import type {
  BoundedCollection,
  DashboardData,
  DiscoveredLocation,
  InstanceSummary,
  ProfileData,
  ScanMode,
  ScanProgress,
  ServerSummary,
  SessionCollection,
  VersionSummary,
  WorldSummary,
} from "../types/domain";

const NATIVE_REQUIRED_MESSAGE = "MineTrace data access is available in the desktop app only.";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

function requireNativeRuntime(): void {
  if (!isTauriRuntime()) throw new Error(NATIVE_REQUIRED_MESSAGE);
}

export async function getDashboard(): Promise<DashboardData> {
  requireNativeRuntime();
  return invoke<DashboardData>("get_dashboard");
}

export async function getSessions(): Promise<SessionCollection> {
  requireNativeRuntime();
  return invoke<SessionCollection>("get_sessions");
}

export async function getInstances(): Promise<BoundedCollection<InstanceSummary>> {
  requireNativeRuntime();
  return invoke<BoundedCollection<InstanceSummary>>("get_instances");
}

export async function getWorlds(): Promise<BoundedCollection<WorldSummary>> {
  requireNativeRuntime();
  return invoke<BoundedCollection<WorldSummary>>("get_worlds");
}

export async function getServers(): Promise<BoundedCollection<ServerSummary>> {
  requireNativeRuntime();
  return invoke<BoundedCollection<ServerSummary>>("get_servers");
}

export async function getVersions(): Promise<BoundedCollection<VersionSummary>> {
  requireNativeRuntime();
  return invoke<BoundedCollection<VersionSummary>>("get_versions");
}

export async function getProfile(): Promise<ProfileData> {
  requireNativeRuntime();
  return invoke<ProfileData>("get_profile");
}

export async function saveShareImage(path: string, bytes: number[]): Promise<void> {
  requireNativeRuntime();
  return invoke<void>("save_share_image", { path, bytes });
}

export async function discoverInstallations(): Promise<DiscoveredLocation[]> {
  requireNativeRuntime();
  return invoke<DiscoveredLocation[]>("discover_installations");
}

export async function addCustomLocation(path: string): Promise<DiscoveredLocation> {
  requireNativeRuntime();
  return invoke<DiscoveredLocation>("add_custom_location", { path });
}

export async function startScan(mode: ScanMode): Promise<ScanProgress> {
  requireNativeRuntime();
  return invoke<ScanProgress>("start_scan", { mode });
}

export async function getScanStatus(): Promise<ScanProgress> {
  requireNativeRuntime();
  return invoke<ScanProgress>("get_scan_status");
}

export async function cancelScan(): Promise<ScanProgress> {
  requireNativeRuntime();
  return invoke<ScanProgress>("cancel_scan");
}

export function detectPlatform(): "windows" | "macos" | "linux" {
  const platform = typeof navigator === "undefined"
    ? ""
    : `${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`.toLowerCase();
  if (platform.includes("mac")) return "macos";
  if (platform.includes("win")) return "windows";
  return "linux";
}

export function shortcutModifier(): string {
  return detectPlatform() === "macos" ? "⌘" : "Ctrl";
}
