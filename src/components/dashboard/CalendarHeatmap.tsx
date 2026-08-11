import { ListTree } from "lucide-react";
import { useMemo, useState } from "react";
import { format, parseISO } from "date-fns";
import { formatDuration } from "../../lib/format";
import type { DailyActivity, ObservedDailyActivity } from "../../types/domain";

const missingEvidenceStyle = {
  backgroundColor: "transparent",
  backgroundImage: "repeating-linear-gradient(45deg, var(--quartz-muted) 0 1px, transparent 1px 3px)",
  outline: "1px solid var(--border)",
};

function activityLevel(minutes: number): 0 | 1 | 2 | 3 | 4 {
  if (minutes === 0) return 0;
  if (minutes < 45) return 1;
  if (minutes < 100) return 2;
  if (minutes < 180) return 3;
  return 4;
}

function hasActivity(day: DailyActivity): day is ObservedDailyActivity {
  return day.coverage !== "missing" && day.minutes > 0;
}

function dayDescription(day: DailyActivity): string {
  const date = format(parseISO(day.date), "MMMM d, yyyy");

  if (day.coverage === "missing") {
    return `${date}: no evidence available`;
  }

  if (day.minutes === 0) {
    return `${date}: observed, no playtime detected, ${day.confidence} evidence confidence`;
  }

  return `${date}: ${formatDuration(day.minutes)}, ${day.sessions} ${day.sessions === 1 ? "session" : "sessions"}, ${day.confidence} evidence confidence`;
}

export function CalendarHeatmap({ data }: { data: DailyActivity[] }) {
  const [listVisible, setListVisible] = useState(false);
  const activeDays = useMemo(() => data.filter(hasActivity), [data]);
  const missingDays = useMemo(() => data.filter((day) => day.coverage === "missing"), [data]);
  const observedZeroDays = useMemo(
    () => data.filter((day) => day.coverage === "observed" && day.minutes === 0),
    [data],
  );
  const peak = useMemo(
    () =>
      activeDays.reduce<ObservedDailyActivity | null>(
        (best, day) => (!best || day.minutes > best.minutes ? day : best),
        null,
      ),
    [activeDays],
  );

  return (
    <section className="panel heatmap-panel" aria-labelledby="heatmap-heading">
      <header className="panel__header">
        <div>
          <p className="eyebrow">Consistency</p>
          <h2 id="heatmap-heading">A year in traces</h2>
          <p>
            {activeDays.length} active days
            {peak ? ` · Peak ${format(parseISO(peak.date), "MMM d")}` : ""}
            {missingDays.length > 0 ? ` · ${missingDays.length} days without evidence` : ""}
          </p>
        </div>
        <button
          className="icon-action"
          type="button"
          onClick={() => setListVisible((visible) => !visible)}
          aria-expanded={listVisible}
          aria-controls="heatmap-list"
          aria-label={listVisible ? "Show activity heatmap" : "Show all activity days as a table"}
          title={listVisible ? "Show heatmap" : "View accessible activity list"}
        >
          <ListTree aria-hidden="true" />
        </button>
      </header>

      {!listVisible ? (
        <>
          <div
            className="heatmap"
            role="img"
            aria-label={`Activity heatmap with ${activeDays.length} active days, ${observedZeroDays.length} observed days without detected playtime, and ${missingDays.length} days with no evidence. Use the activity table button for every day's details.`}
          >
            {data.map((day) => {
              const missing = day.coverage === "missing";
              const level = missing ? 0 : activityLevel(day.minutes);

              return (
                <span
                  key={day.date}
                  aria-hidden="true"
                  className={`heatmap__day heatmap__day--level-${level} heatmap__day--${day.confidence}${
                    missing ? " heatmap__day--partial" : ""
                  }`}
                  style={missing ? missingEvidenceStyle : undefined}
                  title={dayDescription(day)}
                />
              );
            })}
          </div>
          <div className="heatmap-legend" aria-hidden="true">
            <span>Observed zero</span>
            {[0, 1, 2, 3, 4].map((level) => (
              <i key={level} className={`heatmap__day heatmap__day--level-${level}`} />
            ))}
            <span>4h+</span>
            <i className="heatmap__day heatmap__day--partial" />
            <span>Incomplete evidence</span>
            <i className="heatmap__day" style={missingEvidenceStyle} />
            <span>No evidence</span>
          </div>
        </>
      ) : (
        <div className="heatmap-list" id="heatmap-list">
          <table className="data-table data-table--compact">
            <caption>All {data.length} days in the selected year, newest first</caption>
            <thead>
              <tr>
                <th scope="col">Date</th>
                <th scope="col">Runtime</th>
                <th scope="col">Sessions</th>
                <th scope="col">Coverage</th>
                <th scope="col">Evidence confidence</th>
              </tr>
            </thead>
            <tbody>
              {[...data]
                .sort((a, b) => b.date.localeCompare(a.date))
                .map((day) => (
                  <tr key={day.date}>
                    <th scope="row">{format(parseISO(day.date), "MMM d, yyyy")}</th>
                    <td>
                      {day.coverage === "missing"
                        ? "Not available"
                        : day.minutes === 0
                          ? "No detected playtime"
                          : formatDuration(day.minutes)}
                    </td>
                    <td>{day.coverage === "missing" ? "Not available" : day.sessions}</td>
                    <td>{day.coverage === "missing" ? "No evidence" : "Observed"}</td>
                    <td className="text-capitalize">{day.confidence}</td>
                  </tr>
                ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
