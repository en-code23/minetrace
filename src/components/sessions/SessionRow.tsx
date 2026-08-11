import {
  ChevronDown,
  CircleDotDashed,
  FileCode2,
  MonitorPlay,
  RadioTower,
  Split,
  TreePine,
} from "lucide-react";
import { useState } from "react";
import { clsx } from "clsx";
import { formatDuration, formatSessionTime } from "../../lib/format";
import { useUiStore } from "../../stores/ui-store";
import type { Session } from "../../types/domain";
import { ConfidenceBadge } from "../ui/ConfidenceBadge";

export function SessionRow({ session, isLast = false }: { session: Session; isLast?: boolean }) {
  const [expanded, setExpanded] = useState(false);
  const privacyMask = useUiStore((state) => state.privacyMask);
  const DestinationIcon = session.kind === "server" ? RadioTower : session.kind === "world" ? TreePine : session.kind === "mixed" ? Split : MonitorPlay;
  const destination = privacyMask && session.kind === "server" ? "Masked multiplayer server" : session.destination;

  return (
    <article className={clsx("session-entry", expanded && "session-entry--expanded", isLast && "session-entry--last")}>
      <div className="session-entry__rail" aria-hidden="true">
        <span className={`session-entry__node session-entry__node--${session.confidence}`} />
      </div>
      <button
        className="session-entry__summary"
        type="button"
        onClick={() => setExpanded((value) => !value)}
        aria-expanded={expanded}
        aria-controls={`session-detail-${session.id}`}
      >
        <span className="session-entry__time">
          <strong>{formatSessionTime(session.startedAt)}</strong>
          <small>{session.endedAt ? formatSessionTime(session.endedAt) : "Open end"}</small>
        </span>
        <span className="session-entry__destination">
          <span className="session-entry__destination-icon" aria-hidden="true">
            <DestinationIcon />
          </span>
          <span>
            <strong>{destination ?? "No destination detected"}</strong>
            <small>{session.kind === "server" ? "Multiplayer" : session.kind === "world" ? "Local world" : session.kind === "mixed" ? "Multiple destinations" : "Menu only"}</small>
          </span>
        </span>
        <span className="session-entry__instance">
          <strong>{session.instance}</strong>
          <small>{session.launcher}</small>
        </span>
        <span className="session-entry__version">
          <strong>{session.version}</strong>
          <small>{session.loader ?? "Loader not detected"}</small>
        </span>
        <span className="session-entry__duration">
          <strong>{formatDuration(session.durationMinutes, true)}</strong>
          <small className={`exit-label exit-label--${session.exitKind}`}>{session.exitKind}</small>
        </span>
        <ConfidenceBadge confidence={session.confidence} compact />
        <ChevronDown className="session-entry__chevron" aria-hidden="true" />
      </button>

      {expanded && (
        <div className="session-detail" id={`session-detail-${session.id}`}>
          <div className="session-detail__evidence">
            <p className="eyebrow">Why this session exists</p>
            <div className="evidence-line">
              <span className="evidence-line__icon" aria-hidden="true"><FileCode2 /></span>
              <div>
                <strong>Source log</strong>
                <code>{session.source}</code>
              </div>
            </div>
            <div className="evidence-line">
              <span className="evidence-line__icon" aria-hidden="true"><CircleDotDashed /></span>
              <div>
                <strong>Boundary assessment</strong>
                <span>
                  {session.confidence === "verified"
                    ? "Session boundaries are strongly supported by observed lifecycle markers."
                    : session.confidence === "high"
                      ? "Most boundary evidence was observed; at least one detail was inferred."
                      : session.confidence === "partial"
                        ? "Important boundary evidence is missing or inferred in the available log."
                        : "The file is too incomplete to calculate a reliable duration."}
                </span>
              </div>
            </div>
          </div>
        </div>
      )}
    </article>
  );
}
