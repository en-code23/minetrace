import { AlertTriangle, ArrowRight, FileSearch } from "lucide-react";
import { Link } from "react-router-dom";
import type { DashboardData } from "../../types/domain";

export function CoverageNote({
  coverage,
  archiveState,
}: {
  coverage: DashboardData["coverage"];
  archiveState: DashboardData["archiveState"];
}) {
  const hasObservedHistory = archiveState === "ready" && coverage.observedMonths > 0;
  const hasCompletedEmptyScan = archiveState === "scannedNoEvidence";
  const hasGaps = coverage.gapMonths > 0;

  return (
    <aside className="coverage-note" id="coverage-method">
      <span className="coverage-note__icon" aria-hidden="true">
        <FileSearch />
      </span>
      <div className="coverage-note__copy">
        <div>
          <strong>{!hasObservedHistory
            ? hasCompletedEmptyScan
              ? "Completed scan found no session evidence"
              : "Coverage awaits the first scan"
            : hasGaps
              ? "History has known gaps"
              : "No month-level gaps inside the observed range"}</strong>
          <span className="coverage-note__status">
            {hasGaps && <AlertTriangle aria-hidden="true" />}
            {!hasObservedHistory
              ? hasCompletedEmptyScan
                ? "Scan completed · No observed months"
                : "No observed months"
              : `${coverage.gapMonths} gap ${coverage.gapMonths === 1 ? "month" : "months"}`}
          </span>
        </div>
        <p>{coverage.warning}</p>
      </div>
      <Link className="text-link" to="/scan">
        {hasObservedHistory ? "Review sources" : hasCompletedEmptyScan ? "Scan again" : "Open Scan center"} <ArrowRight aria-hidden="true" />
      </Link>
    </aside>
  );
}
