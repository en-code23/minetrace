import { ArrowRight, MonitorPlay, RadioTower, Split, TreePine } from "lucide-react";
import { Link } from "react-router-dom";
import { formatDuration, formatRelativeDate, formatSessionTime } from "../../lib/format";
import { useUiStore } from "../../stores/ui-store";
import type { Session } from "../../types/domain";
import { ConfidenceBadge } from "../ui/ConfidenceBadge";

export function RecentSessions({ sessions }: { sessions: Session[] }) {
  const privacyMask = useUiStore((state) => state.privacyMask);

  return (
    <section className="panel recent-panel" aria-labelledby="recent-heading">
      <header className="panel__header">
        <div>
          <p className="eyebrow">Latest evidence</p>
          <h2 id="recent-heading">Recent sessions</h2>
        </div>
        <Link className="text-link" to="/sessions">
          All sessions <ArrowRight aria-hidden="true" />
        </Link>
      </header>
      <div className="recent-list">
        {sessions.length === 0 ? (
          <div className="recent-list__empty">
            <MonitorPlay aria-hidden="true" />
            <strong>No source-backed sessions yet</strong>
            <span>Run Scan center to reconstruct the first entries.</span>
            <Link className="text-link" to="/scan">Open Scan center <ArrowRight aria-hidden="true" /></Link>
          </div>
        ) : sessions.map((session) => {
          const DestinationIcon = session.kind === "server" ? RadioTower : session.kind === "world" ? TreePine : session.kind === "mixed" ? Split : MonitorPlay;
          return (
            <Link className="recent-row" to="/sessions" key={session.id}>
              <span className="recent-row__time">
                <strong>{formatSessionTime(session.startedAt)}</strong>
                <small>{formatRelativeDate(session.startedAt)}</small>
              </span>
              <span className="recent-row__trace" aria-hidden="true">
                <i className={`recent-row__node recent-row__node--${session.confidence}`} />
              </span>
              <span className="recent-row__destination">
                <DestinationIcon aria-hidden="true" />
                <span>
                  <strong>{privacyMask && session.kind === "server" ? "Masked multiplayer server" : session.destination ?? "No destination detected"}</strong>
                  <small>{session.instance} · {session.version}</small>
                </span>
              </span>
              <span className="recent-row__duration">{formatDuration(session.durationMinutes, true)}</span>
              <ConfidenceBadge confidence={session.confidence} compact />
            </Link>
          );
        })}
      </div>
    </section>
  );
}
