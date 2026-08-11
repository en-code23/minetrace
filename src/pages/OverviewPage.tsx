import { useQuery } from "@tanstack/react-query";
import { CalendarRange, RefreshCw, TriangleAlert } from "lucide-react";
import { ActivityChart } from "../components/dashboard/ActivityChart";
import { CalendarHeatmap } from "../components/dashboard/CalendarHeatmap";
import { CoverageNote } from "../components/dashboard/CoverageNote";
import { HeroLedger } from "../components/dashboard/HeroLedger";
import { RecentSessions } from "../components/dashboard/RecentSessions";
import { TopContexts } from "../components/dashboard/TopContexts";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";
import { PageHeader } from "../components/ui/PageHeader";
import { formatShortMonth } from "../lib/format";
import { getDashboard } from "../lib/runtime";

export function OverviewPage() {
  const dashboard = useQuery({ queryKey: ["dashboard", "all-time"], queryFn: getDashboard });

  if (dashboard.isPending) {
    return <OverviewSkeleton />;
  }

  if (dashboard.isError) {
    return (
      <EmptyState
        icon={TriangleAlert}
        title="The archive could not be loaded"
        description="Your source files were not changed. Retry the local database query or open Scan center to run a fresh scan."
        action={
          <Button leadingIcon={<RefreshCw aria-hidden="true" />} onClick={() => void dashboard.refetch()}>
            Retry
          </Button>
        }
      />
    );
  }

  const data = dashboard.data;
  const hasEvidence = data.archiveState === "ready" && data.totals.sessions > 0;
  const hasCompletedEmptyScan = data.archiveState === "scannedNoEvidence";
  const rangeLabel = hasEvidence
    ? `${formatRangeMonth(data.coverage.firstDetectedAt)} – ${formatRangeMonth(data.coverage.lastDetectedAt)}`
    : hasCompletedEmptyScan
      ? "Completed scan · No session evidence"
      : "Run the first scan";

  return (
    <div className="page page--overview">
      <PageHeader
        eyebrow={hasEvidence
          ? "Observed archive · All detected history"
          : hasCompletedEmptyScan
            ? "Local archive · Scan completed without session evidence"
            : "Local archive · Awaiting first scan"}
        title="Overview"
        actions={
          <div className="range-control" aria-label={hasEvidence ? `Displayed detected range: ${rangeLabel}` : "No detected date range yet"}>
            <CalendarRange aria-hidden="true" />
            {hasEvidence ? "All detected history" : "No detected range"}
            <span>{rangeLabel}</span>
          </div>
        }
      />

      <HeroLedger data={data} />

      <div className="overview-primary-grid">
        <ActivityChart data={data.monthly} />
        <TopContexts top={data.top} totalMinutes={data.totals.playtimeMinutes} />
      </div>

      <div className="overview-secondary-grid">
        <CalendarHeatmap data={data.daily} />
        <RecentSessions sessions={data.recentSessions} />
      </div>

      <CoverageNote coverage={data.coverage} archiveState={data.archiveState} />
    </div>
  );
}

function formatRangeMonth(value: string): string {
  return formatShortMonth(value);
}

function OverviewSkeleton() {
  return (
    <div className="page page--overview" aria-label="Loading overview" aria-busy="true">
      <div className="skeleton skeleton--header" />
      <div className="skeleton skeleton--hero" />
      <div className="overview-primary-grid">
        <div className="skeleton skeleton--panel" />
        <div className="skeleton skeleton--panel" />
      </div>
    </div>
  );
}
