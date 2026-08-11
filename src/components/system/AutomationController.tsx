import { useEffect } from "react";
import { getScanStatus, startScan } from "../../lib/runtime";
import { scanIsRunning } from "../../lib/scanStatus";
import { useUiStore } from "../../stores/ui-store";
import { useUpdateStore } from "../../stores/update-store";

const AUTO_SCAN_INTERVAL_MS = 30 * 60 * 1_000;
let startupUpdateChecked = false;
let lastAutomatedScanAt = 0;

export function AutomationController({ enabled }: { enabled: boolean }) {
  const setupComplete = useUiStore((state) => state.setupComplete);
  const autoScan = useUiStore((state) => state.autoScan);
  const autoUpdate = useUiStore((state) => state.autoUpdate);
  const checkForUpdates = useUpdateStore((state) => state.checkForUpdates);

  useEffect(() => {
    if (!enabled || !setupComplete || !autoUpdate || startupUpdateChecked) return;
    startupUpdateChecked = true;
    void checkForUpdates(true);
  }, [autoUpdate, checkForUpdates, enabled, setupComplete]);

  useEffect(() => {
    if (!enabled || !setupComplete || !autoScan) return;
    let disposed = false;

    async function refreshArchive() {
      if (disposed || document.visibilityState !== "visible") return;
      const now = Date.now();
      if (now - lastAutomatedScanAt < AUTO_SCAN_INTERVAL_MS - 5_000) return;
      lastAutomatedScanAt = now;
      try {
        const status = await getScanStatus();
        if (!disposed && !scanIsRunning(status)) await startScan("standard");
      } catch {
        // Scan Center exposes durable diagnostics. Background refreshes stay quiet.
      }
    }

    void refreshArchive();
    const timer = window.setInterval(() => void refreshArchive(), AUTO_SCAN_INTERVAL_MS);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [autoScan, enabled, setupComplete]);

  return null;
}
