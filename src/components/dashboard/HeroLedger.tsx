import { ArrowRight, CircleHelp, Clock4, Layers3 } from "lucide-react";
import { Link } from "react-router-dom";
import { formatDuration, formatHeroDuration, formatMonth, formatShortDate } from "../../lib/format";
import type { DashboardData } from "../../types/domain";
import { EvidenceSeam, type EvidenceSegment } from "../ui/EvidenceSeam";

export function HeroLedger({ data }: { data: DashboardData }) {
  const hero = formatHeroDuration(data.totals.playtimeMinutes);
  const hasEvidence = data.archiveState === "ready" && data.totals.sessions > 0;
  const hasCompletedEmptyScan = data.archiveState === "scannedNoEvidence";
  const coverageLabel = hasCompletedEmptyScan ? "No session evidence" : {
    verified: "Strong coverage",
    partial: "Partial history",
    limited: "Limited history",
    unknown: "Awaiting evidence",
  }[data.coverage.quality];
  const seam: EvidenceSegment[] = data.monthly.map((month) => month.coverage === "missing"
    ? {
        id: month.month,
        weight: 1,
        intensity: 0,
        confidence: "unknown",
        label: `${formatMonth(month.month)}: no evidence available`,
      }
    : {
        id: month.month,
        weight: 1,
        intensity: Math.min(4, Math.max(1, Math.ceil(month.minutes / 3_000))) as 1 | 2 | 3 | 4,
        confidence: month.confidence,
        label: `${formatMonth(month.month)}: ${Math.round(month.minutes / 60)} hours, ${Math.round(month.estimatedShare * 100)}% inferred`,
      });
  const firstSeamMonth = data.monthly.at(0)?.month;
  const lastSeamMonth = data.monthly.at(-1)?.month;

  return (
    <section className="hero-ledger" aria-labelledby="detected-runtime-heading">
      <div className="hero-ledger__main">
        <div className="hero-ledger__label-row">
          <p id="detected-runtime-heading">Detected client runtime</p>
        </div>
        <div className="hero-ledger__value" aria-label={`${hero.primary} ${hero.secondary}`}>
          <strong>{hero.primary}</strong>
          <span>{hero.secondary}</span>
        </div>
        <p className="hero-ledger__explanation">
          {hasEvidence
            ? <>Reconstructed from local files observed between {formatShortDate(data.coverage.firstDetectedAt)} and{" "}
              {formatShortDate(data.coverage.lastDetectedAt)}. It is not claimed as complete lifetime playtime.</>
            : hasCompletedEmptyScan
              ? "The completed scan found no reconstructable session evidence. The source files remain unchanged; scan again after new logs are available."
              : "No scan has completed yet. Run Scan center to build the first local archive."}
        </p>
      </div>

      <aside className="hero-ledger__coverage" aria-label="Evidence coverage">
        <div className="coverage-heading">
          <span className="coverage-heading__mark" aria-hidden="true" />
          <div>
            <strong>{coverageLabel}</strong>
            <span>{hasEvidence
              ? `${data.coverage.verifiedShare.toLocaleString(undefined, { style: "percent" })} of session boundaries verified`
              : hasCompletedEmptyScan
                ? "Completed scan found no session boundaries"
                : "No session boundaries available yet"}</span>
          </div>
        </div>
        <dl className="coverage-facts">
          <div>
            <dt>Observed months</dt>
            <dd>{data.coverage.observedMonths}</dd>
          </div>
          <div>
            <dt>Month gaps</dt>
            <dd>{data.coverage.gapMonths}</dd>
          </div>
        </dl>
        <a className="text-link" href="#coverage-method">
          <CircleHelp aria-hidden="true" />
          How coverage works
        </a>
      </aside>

      <div className="hero-ledger__seam">
        <div className="hero-ledger__seam-labels">
          <span>{firstSeamMonth ? formatMonth(firstSeamMonth) : "No evidence"}</span>
          <span>Evidence seam · solid is verified, hatch marks uncertainty or gaps</span>
          <span>{lastSeamMonth ? formatMonth(lastSeamMonth) : "No evidence"}</span>
        </div>
        <EvidenceSeam segments={seam} label="Monthly evidence coverage: solid segments are verified and hatched segments are inferred" />
      </div>

      <div className="metric-rail" aria-label="Archive metrics">
        <MetricRailItem icon={Layers3} label="Sessions" value={data.totals.sessions.toLocaleString()} />
        <MetricRailItem icon={Clock4} label="Active days" value={data.totals.activeDays.toLocaleString()} />
        <MetricRailItem label="Longest session" value={formatDuration(data.totals.longestSessionMinutes, true)} />
        <MetricRailItem label="Average session" value={formatDuration(data.totals.averageSessionMinutes, true)} />
        <Link className="metric-rail__action" to="/sessions">
          Inspect sessions
          <ArrowRight aria-hidden="true" />
        </Link>
      </div>
    </section>
  );
}

function MetricRailItem({
  label,
  value,
  icon: Icon,
}: {
  label: string;
  value: string;
  icon?: typeof Clock4;
}) {
  return (
    <div className="metric-rail__item">
      <span className="metric-rail__label">
        {Icon && <Icon aria-hidden="true" />}
        {label}
      </span>
      <strong className="metric-rail__value">{value}</strong>
    </div>
  );
}
