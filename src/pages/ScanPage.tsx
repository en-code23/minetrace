import { open } from "@tauri-apps/plugin-dialog";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  Check,
  ChevronRight,
  CircleStop,
  FileSearch,
  FolderInput,
  FolderOpen,
  HardDrive,
  Laptop,
  Play,
  Plus,
  RotateCw,
  ScanLine,
  ShieldCheck,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "../components/ui/Button";
import { ConfidenceBadge } from "../components/ui/ConfidenceBadge";
import { EmptyState } from "../components/ui/EmptyState";
import { EvidenceSeam, type EvidenceSegment } from "../components/ui/EvidenceSeam";
import { PageHeader } from "../components/ui/PageHeader";
import {
  addCustomLocation,
  cancelScan as cancelRuntimeScan,
  detectPlatform,
  discoverInstallations,
  getScanStatus,
  isTauriRuntime,
  startScan as startRuntimeScan,
} from "../lib/runtime";
import type { ScanMode, ScanProgress } from "../types/domain";

const idleProgress: ScanProgress = {
  id: "",
  mode: "standard",
  state: "completed",
  phase: "idle",
  current: 0,
  total: 0,
  currentPath: null,
  warnings: 0,
  errors: 0,
  message: null,
  issues: [],
  startedAt: null,
  finishedAt: null,
  datasetRevision: null,
};

const runningPhases = new Set<ScanProgress["phase"]>(["discovering", "indexing", "parsing", "aggregating"]);

const nativePhaseCopy: Record<ScanProgress["phase"], { label: string; description: string }> = {
  idle: { label: "Ready to scan", description: "Choose a depth, review the approved roots, and start the local scanner." },
  discovering: { label: "Discovering evidence", description: "The local scanner is resolving approved launcher and instance roots." },
  indexing: { label: "Indexing changed files", description: "File fingerprints are being compared with the local archive." },
  parsing: { label: "Reconstructing sessions", description: "Local evidence is being parsed into canonical play sessions." },
  aggregating: { label: "Updating the local archive", description: "The completed reconstruction is being promoted atomically." },
  complete: { label: "Scan complete", description: "The local archive and dashboard now reflect the completed scan." },
  cancelled: { label: "Scan cancelled", description: "The scanner confirmed cancellation. Completed source reads remain unchanged." },
  interrupted: { label: "Previous scan was interrupted", description: "MineTrace closed before that scan could promote changes. The prior archive remains intact." },
  failed: { label: "Scan failed", description: "The local scanner stopped before completing the archive update." },
};

function errorText(error: unknown): string {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error === "string" && error) return error;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  return "The scanner returned an unknown error.";
}

function progressRatio(progress: ScanProgress): number {
  if (progress.phase === "complete") return 1;
  if (progress.total <= 0) return 0;
  return Math.min(1, Math.max(0, progress.current / progress.total));
}

export function ScanPage() {
  const queryClient = useQueryClient();
  const native = isTauriRuntime();
  const discovery = useQuery({
    queryKey: ["locations", "discover"],
    queryFn: discoverInstallations,
    enabled: native,
  });
  const platform = detectPlatform();
  const [progress, setProgress] = useState<ScanProgress>(idleProgress);
  const [mode, setMode] = useState<ScanMode>("standard");
  const [statusLoading, setStatusLoading] = useState(native);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [actionPending, setActionPending] = useState<"start" | "cancel" | null>(null);
  const previousPhase = useRef<ScanProgress["phase"]>("idle");

  const locations = discovery.data ?? [];
  const isRunning = runningPhases.has(progress.phase);
  const ratio = progressRatio(progress);
  const percent = Math.round(ratio * 100);
  const currentCopy = nativePhaseCopy[progress.phase];
  const issues = progress.issues ?? [];

  useEffect(() => {
    if (!native) return;
    let disposed = false;

    void getScanStatus()
      .then((status) => {
        if (disposed) return;
        setProgress(status);
        setStatusError(null);
      })
      .catch((error: unknown) => {
        if (disposed) return;
        setStatusError(`Unable to read scan status. ${errorText(error)}`);
      })
      .finally(() => {
        if (!disposed) setStatusLoading(false);
      });

    return () => {
      disposed = true;
    };
  }, [native]);

  useEffect(() => {
    if (!native || !isRunning || actionPending === "cancel") return;

    let disposed = false;
    let inFlight = false;

    async function poll() {
      if (disposed || inFlight) return;
      inFlight = true;
      try {
        const status = await getScanStatus();
        if (disposed) return;
        setProgress(status);
        setStatusError(null);
      } catch (error: unknown) {
        if (!disposed) {
          setStatusError(`Live scan status is temporarily unavailable; retrying. ${errorText(error)}`);
        }
      } finally {
        inFlight = false;
      }
    }

    void poll();
    const timer = window.setInterval(() => void poll(), 750);

    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [actionPending, isRunning, native]);

  useEffect(() => {
    if (progress.phase === "complete" && previousPhase.current !== "complete") {
      void queryClient.invalidateQueries({ queryKey: ["dashboard"] });
      void queryClient.invalidateQueries({ queryKey: ["sessions"] });
      void queryClient.invalidateQueries({ queryKey: ["instances"] });
      void queryClient.invalidateQueries({ queryKey: ["worlds"] });
      void queryClient.invalidateQueries({ queryKey: ["servers"] });
      void queryClient.invalidateQueries({ queryKey: ["versions"] });
    }
    previousPhase.current = progress.phase;
  }, [progress.phase, queryClient]);

  const seam: EvidenceSegment[] = Array.from({ length: 18 }, (_, index) => {
    const processed = (index + 1) / 18 <= ratio;
    return {
      id: `scan-${index}`,
      weight: 1,
      intensity: processed ? ((index % 4) + 1) as 1 | 2 | 3 | 4 : 0,
      confidence: processed ? (progress.errors > 0 ? "partial" : "verified") : "unknown",
      label: processed ? "Processed" : "Waiting",
    };
  });

  const statusAnnouncement = statusLoading
    ? "Checking scan status."
    : progress.phase === "complete"
      ? `Scan complete. ${progress.current.toLocaleString()} files processed with ${progress.warnings} warnings and ${progress.errors} errors.`
      : progress.phase === "cancelled" || progress.phase === "interrupted" || progress.phase === "failed"
        ? `${currentCopy.label}. ${progress.message ?? currentCopy.description}`
        : `${currentCopy.label}.`;

  async function chooseFolder() {
    if (!native) return;
    setStatusError(null);
    try {
      const selected = await open({ directory: true, multiple: false, title: "Choose a Minecraft or launcher folder" });
      if (!selected || Array.isArray(selected)) return;
      const custom = await addCustomLocation(selected);
      queryClient.setQueryData<Awaited<ReturnType<typeof discoverInstallations>>>(
        ["locations", "discover"],
        (current) => {
          const existing = current ?? [];
          return [...existing.filter((location) => location.id !== custom.id), custom];
        },
      );
      const refreshed = await discovery.refetch();
      if (refreshed.error) {
        setStatusError(
          `The folder was added, but launcher discovery could not be refreshed. ${errorText(refreshed.error)}`,
        );
      }
    } catch (error: unknown) {
      setStatusError(`The folder could not be added. ${errorText(error)}`);
    }
  }

  async function handleStartScan() {
    setActionPending("start");
    setStatusError(null);
    try {
      setProgress(await startRuntimeScan(mode));
    } catch (error: unknown) {
      const message = `The scan could not start. ${errorText(error)}`;
      setProgress({ ...idleProgress, phase: "failed", errors: 1, message });
    } finally {
      setActionPending(null);
    }
  }

  async function handleCancelScan() {
    setActionPending("cancel");
    setStatusError(null);
    try {
      setProgress(await cancelRuntimeScan());
    } catch (error: unknown) {
      setStatusError(`Cancellation could not be confirmed; status polling will continue. ${errorText(error)}`);
    } finally {
      setActionPending(null);
    }
  }

  const enabledRoots = locations.filter((location) => location.enabled).length;
  const startLabel = actionPending === "start"
    ? "Starting…"
    : progress.phase === "complete" ? "Scan again" : progress.phase === "failed" ? "Retry scan" : "Start scan";

  if (!native) {
    return (
      <div className="page page--scan">
        <PageHeader
          eyebrow="Desktop application"
          title="Scan center"
          description="MineTrace reads local Minecraft evidence through its protected native backend."
        />
        <EmptyState
          icon={Laptop}
          title="Open MineTrace on your desktop"
          description="The browser build cannot access launcher folders, run scans, or display archive records. No demo data is substituted."
        />
      </div>
    );
  }

  return (
    <div className="page page--scan">
      <PageHeader
        eyebrow={`Your data · ${platform === "macos" ? "macOS" : platform}`}
        title="Update your stats"
        description="MineTrace finds Minecraft automatically. Press the button and keep the app open until it finishes."
      />

      <section className="scan-simple" aria-labelledby="scan-simple-title">
        <span className="scan-simple__icon" aria-hidden="true"><ScanLine /></span>
        <div>
          <p className="eyebrow">{isRunning ? `${percent}% complete` : "One-click refresh"}</p>
          <h2 id="scan-simple-title">{isRunning ? currentCopy.label : "Update my stats"}</h2>
          <p>{isRunning ? (progress.message ?? currentCopy.description) : `${enabledRoots} ${enabledRoots === 1 ? "Minecraft location is" : "Minecraft locations are"} ready. Files are read only and stay on this PC.`}</p>
        </div>
        {isRunning ? (
          <Button variant="ghost" leadingIcon={<CircleStop aria-hidden="true" />} onClick={() => void handleCancelScan()} disabled={actionPending === "cancel"}>{actionPending === "cancel" ? "Stopping…" : "Stop"}</Button>
        ) : (
          <Button variant="primary" leadingIcon={<Play aria-hidden="true" />} onClick={() => void handleStartScan()} disabled={statusLoading || actionPending !== null || enabledRoots === 0}>{startLabel}</Button>
        )}
        {isRunning && <progress value={progress.current} max={Math.max(progress.total, 1)} aria-label="Scan progress" />}
      </section>

      <section className="scan-trust-strip" aria-label="Scanning guarantees">
        <span><ShieldCheck aria-hidden="true" /><strong>Read-only</strong><small>Source files are never changed</small></span>
        <span><HardDrive aria-hidden="true" /><strong>Local</strong><small>No account or upload</small></span>
        <span>
          <ScanLine aria-hidden="true" />
          <strong>Live status</strong>
          <small>Counts from the local scanner</small>
        </span>
      </section>

      <details className="scan-advanced">
        <summary><span>Advanced options</span><small>Folders, scan depth, and source details</small><ChevronRight aria-hidden="true" /></summary>
        <div className="scan-grid">
        <section className="panel source-panel" aria-labelledby="source-heading">
          <header className="panel__header">
            <div>
              <p className="eyebrow">Approved roots</p>
              <h2 id="source-heading">Detected launchers</h2>
              <p>{locations.reduce((sum, location) => sum + location.instances, 0)} instances across {locations.length} locations</p>
            </div>
            <button
              className="icon-action"
              type="button"
              onClick={() => void discovery.refetch()}
              aria-label="Discover launcher locations again"
              title="Discover again"
              disabled={isRunning}
            >
              <RotateCw aria-hidden="true" />
            </button>
            <Button size="small" variant="secondary" leadingIcon={<FolderInput aria-hidden="true" />} onClick={() => void chooseFolder()} disabled={isRunning}>Add folder</Button>
          </header>

          {discovery.isPending ? (
            <div className="source-loading" aria-busy="true">
              <ScanLine aria-hidden="true" />
              <span>Checking exact launcher locations…</span>
            </div>
          ) : discovery.isError ? (
            <EmptyState
              icon={AlertTriangle}
              title="Launcher discovery failed"
              description="MineTrace could not read the approved launcher locations. No source files were changed."
              action={<Button onClick={() => void discovery.refetch()}>Try again</Button>}
            />
          ) : locations.length > 0 ? (
            <div className="source-list">
              {locations.map((location) => (
                <div className="source-row" key={location.id}>
                  <span className="source-row__icon" aria-hidden="true"><FolderOpen /></span>
                  <div className="source-row__copy">
                    <div>
                      <strong>{location.name}</strong>
                      <ConfidenceBadge confidence={location.confidence} />
                    </div>
                    <code>{location.path}</code>
                    <small>{location.instances} {location.instances === 1 ? "instance" : "instances"} · {location.kind} adapter</small>
                  </div>
                  <span className={`source-row__status ${location.enabled ? "source-row__status--included" : ""}`}>
                    {location.enabled ? "Included" : "Unavailable"}
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <EmptyState
              icon={FileSearch}
              title="No Minecraft location found"
              description="Choose a folder containing at least two markers such as logs, saves, versions, or options.txt."
              action={<Button leadingIcon={<Plus aria-hidden="true" />} onClick={() => void chooseFolder()}>Choose folder</Button>}
            />
          )}
        </section>

        <section className="panel scan-plan" aria-labelledby="scan-plan-heading">
          <header className="panel__header">
            <div>
              <p className="eyebrow">Preflight</p>
              <h2 id="scan-plan-heading">Scan plan</h2>
              <p>Choose how thoroughly the local scanner should inspect approved roots.</p>
            </div>
          </header>
          <div className="scan-modes" role="radiogroup" aria-label="Scan depth">
            <ScanMode name="quick" label="Quick" detail="Current plain logs and basic sessions" mode={mode} setMode={setMode} meta="Fastest" disabled={isRunning} />
            <ScanMode name="standard" label="Standard" detail="Changed plain and compressed log history" mode={mode} setMode={setMode} meta="Balanced" recommended disabled={isRunning} />
            <ScanMode name="deep" label="Deep" detail="Rebuild all logs with full-file fingerprints" mode={mode} setMode={setMode} meta="Thorough" disabled={isRunning} />
          </div>
          <div className="scan-estimate">
            <div><span>Enabled roots</span><strong>{enabledRoots}</strong></div>
            <div><span>Depth</span><strong className="text-capitalize">{mode}</strong></div>
            <div><span>Status source</span><strong>Local scanner</strong></div>
          </div>
        </section>
        </div>
      </details>

      <section
        className={`scan-console scan-console--${progress.phase}`}
        aria-labelledby="scan-state-heading"
        aria-busy={isRunning}
      >
        <div className="scan-console__state-icon" aria-hidden="true">
          {progress.phase === "complete"
            ? <Check />
            : progress.phase === "failed" || progress.phase === "interrupted"
              ? <AlertTriangle style={{ animation: "none" }} />
              : progress.phase === "cancelled"
                ? <CircleStop style={{ animation: "none" }} />
                : <ScanLine />}
        </div>
        <div className="scan-console__main">
          <header>
            <div>
              <p className="eyebrow">
                {statusLoading
                  ? "Checking status"
                  : progress.phase === "idle"
                    ? "Next run"
                    : isRunning
                      ? `${percent}% complete`
                      : progress.phase === "cancelled"
                        ? "Stopped safely"
                        : progress.phase === "failed" || progress.phase === "interrupted"
                          ? "Needs attention"
                          : "Finished"}
              </p>
              <h2 id="scan-state-heading">{currentCopy.label}</h2>
              <p>{progress.message ?? currentCopy.description}</p>
              {statusError && <p role="alert">{statusError}</p>}
            </div>
            <div className="scan-console__actions">
              {isRunning ? (
                <Button
                  variant="ghost"
                  leadingIcon={<CircleStop aria-hidden="true" />}
                  onClick={() => void handleCancelScan()}
                  disabled={actionPending === "cancel"}
                >
                  {actionPending === "cancel" ? "Cancelling…" : "Cancel"}
                </Button>
              ) : (
                <Button
                  variant="primary"
                  leadingIcon={<Play aria-hidden="true" />}
                  onClick={() => void handleStartScan()}
                  disabled={statusLoading || actionPending !== null || enabledRoots === 0}
                >
                  {startLabel}
                </Button>
              )}
            </div>
          </header>

          <EvidenceSeam segments={seam} label={`Scan ${percent}% complete`} />
          <progress
            value={progress.current}
            max={Math.max(progress.total, 1)}
            aria-label="Local scan progress"
            aria-valuetext={progress.total > 0 ? `${percent}% complete` : "Not started"}
          />

          <p className="sr-only" role="status" aria-live="polite" aria-atomic="true">
            {statusAnnouncement}
          </p>
          {issues.length > 0 && (
            <section className="scan-issues" aria-labelledby="scan-issues-heading">
              <header>
                <div>
                  <p className="eyebrow">Recorded diagnostics</p>
                  <h3 id="scan-issues-heading">Issues from this scan</h3>
                </div>
                <span>{issues.length} shown</span>
              </header>
              <ul>
                {issues.map((issue, index) => (
                  <li className={`scan-issue scan-issue--${issue.severity}`} key={`${issue.code}-${issue.entityLabel ?? "scan"}-${index}`}>
                    <AlertTriangle aria-hidden="true" />
                    <div>
                      <strong>{issue.message}</strong>
                      <small>{issue.code.replaceAll("_", " ")}</small>
                    </div>
                    {issue.entityLabel && <code>{issue.entityLabel}</code>}
                  </li>
                ))}
              </ul>
            </section>
          )}
          <footer className="scan-console__footer">
            <span>
              {progress.total > 0
                ? `${progress.current.toLocaleString()} / ${progress.total.toLocaleString()} files`
                : "Waiting for scanner totals"}
            </span>
            {progress.currentPath && <code>{progress.currentPath}</code>}
            <span>
              {progress.warnings > 0 && <><AlertTriangle aria-hidden="true" /> {progress.warnings} warnings</>}
              {progress.warnings > 0 && progress.errors > 0 ? " · " : null}
              {progress.errors > 0 && <><AlertTriangle aria-hidden="true" /> {progress.errors} errors</>}
              {progress.warnings === 0 && progress.errors === 0 ? "No reported issues" : null}
            </span>
          </footer>
        </div>
        <ChevronRight className="scan-console__edge" aria-hidden="true" />
      </section>
    </div>
  );
}

function ScanMode({
  name,
  label,
  detail,
  meta,
  mode,
  setMode,
  recommended = false,
  disabled = false,
}: {
  name: ScanMode;
  label: string;
  detail: string;
  meta: string;
  mode: ScanMode;
  setMode: (mode: ScanMode) => void;
  recommended?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={mode === name}
      className={`scan-mode ${mode === name ? "scan-mode--selected" : ""}`}
      onClick={() => setMode(name)}
      disabled={disabled}
    >
      <span className="scan-mode__radio"><i /></span>
      <span className="scan-mode__copy"><strong>{label}</strong><small>{detail}</small></span>
      <span className="scan-mode__meta">{recommended && <em>Recommended</em>}<small>{meta}</small></span>
    </button>
  );
}
