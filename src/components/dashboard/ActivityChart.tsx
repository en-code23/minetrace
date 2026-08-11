import { BarChart3, Table2 } from "lucide-react";
import { useState } from "react";
import { formatDuration, formatMonth } from "../../lib/format";
import type { MonthlyActivity, ObservedMonthlyActivity } from "../../types/domain";

function isObservedMonth(month: MonthlyActivity): month is ObservedMonthlyActivity {
  return month.coverage === "observed";
}

export function ActivityChart({ data }: { data: MonthlyActivity[] }) {
  const [tableVisible, setTableVisible] = useState(false);
  if (data.length === 0) {
    return (
      <section className="panel activity-panel" aria-labelledby="activity-heading">
        <header className="panel__header">
          <div>
            <p className="eyebrow">Rhythm</p>
            <h2 id="activity-heading">Activity by month</h2>
            <p>Run a scan to build the first observed month.</p>
          </div>
        </header>
        <div className="chart-empty-state">No source-backed activity yet</div>
      </section>
    );
  }
  const observedMonths = data.filter(isObservedMonth);
  const maxMinutes = Math.max(1, ...observedMonths.map((month) => month.minutes));
  const total = observedMonths.reduce((sum, month) => sum + month.minutes, 0);
  const average = Math.round(total / Math.max(observedMonths.length, 1));
  const hasMissingMonths = data.some((month) => month.coverage === "missing");

  return (
    <section className="panel activity-panel" aria-labelledby="activity-heading">
      <header className="panel__header">
        <div>
          <p className="eyebrow">Rhythm</p>
          <h2 id="activity-heading">Activity by month</h2>
          <p>
            {formatDuration(average)} average across {observedMonths.length}{" "}
            observed {observedMonths.length === 1 ? "month" : "months"}
          </p>
        </div>
        <div className="panel__header-actions">
          <span className="legend-key">
            <i className="legend-key__verified" /> Verified
          </span>
          <span className="legend-key">
            <i className="legend-key__estimated" /> Estimated
          </span>
          {hasMissingMonths && (
            <span className="legend-key">
              <i className="legend-key__missing" /> No evidence
            </span>
          )}
          <button
            className="icon-action"
            type="button"
            onClick={() => setTableVisible((visible) => !visible)}
            aria-expanded={tableVisible}
            aria-controls="activity-table"
            title={tableVisible ? "Hide data table" : "View data table"}
          >
            {tableVisible ? <BarChart3 aria-hidden="true" /> : <Table2 aria-hidden="true" />}
          </button>
        </div>
      </header>

      <div className="activity-chart" aria-hidden={tableVisible}>
        <div className="activity-chart__scale">
          <span>{Math.round(maxMinutes / 60)}h</span>
          <span>{Math.round(maxMinutes / 120)}h</span>
          <span>0</span>
        </div>
        <div
          className="activity-chart__plot"
          style={{ gridTemplateColumns: `repeat(${Math.max(data.length, 1)}, minmax(14px, 1fr))` }}
        >
          <span className="activity-chart__grid activity-chart__grid--top" />
          <span className="activity-chart__grid activity-chart__grid--middle" />
          <span className="activity-chart__grid activity-chart__grid--base" />
          {data.map((month) => {
            const missing = month.coverage === "missing";
            const totalHeight = missing ? 0 : (month.minutes / maxMinutes) * 100;
            const estimatedHeight = missing ? 0 : totalHeight * month.estimatedShare;
            return (
              <div className="activity-bar" key={month.month}>
                <div
                  className="activity-bar__track"
                  title={missing
                    ? `${formatMonth(month.month)}: no evidence available`
                    : `${formatMonth(month.month)}: ${formatDuration(month.minutes)}, ${month.confidence} confidence`}
                >
                  {missing ? (
                    <div className="activity-bar__missing" aria-hidden="true" />
                  ) : (
                    <div
                      className="activity-bar__verified"
                      style={{ height: `${totalHeight}%` }}
                    >
                      <span
                        className="activity-bar__estimated"
                        style={{ height: `${estimatedHeight}%` }}
                      />
                    </div>
                  )}
                </div>
                <span>{month.label}</span>
              </div>
            );
          })}
        </div>
      </div>

      {tableVisible && (
        <div className="chart-table-wrap" id="activity-table">
          <table className="data-table data-table--compact">
            <caption>Monthly detected client runtime across a contiguous evidence window</caption>
            <thead>
              <tr>
                <th scope="col">Month</th>
                <th scope="col">Runtime</th>
                <th scope="col">Sessions</th>
                <th scope="col">Estimated share</th>
              </tr>
            </thead>
            <tbody>
              {data.map((month) => (
                <tr key={month.month}>
                  <th scope="row">{formatMonth(month.month)}</th>
                  <td>{month.coverage === "missing" ? "Not available" : formatDuration(month.minutes)}</td>
                  <td>{month.coverage === "missing" ? "Not available" : month.sessions}</td>
                  <td>{month.coverage === "missing" ? "Not available" : `${Math.round(month.estimatedShare * 100)}%`}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
