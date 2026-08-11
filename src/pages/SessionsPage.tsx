import { useQuery } from "@tanstack/react-query";
import { FileClock, RefreshCw, TriangleAlert } from "lucide-react";
import { useMemo } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { SessionFilters, type SessionFilterState } from "../components/sessions/SessionFilters";
import { SessionRow } from "../components/sessions/SessionRow";
import { Button } from "../components/ui/Button";
import { ArchiveWindowNote } from "../components/ui/ArchiveWindowNote";
import { EmptyState } from "../components/ui/EmptyState";
import { EvidenceSeam, type EvidenceSegment } from "../components/ui/EvidenceSeam";
import { PageHeader } from "../components/ui/PageHeader";
import { formatDuration, formatSessionDate } from "../lib/format";
import { shouldMaskDetectedServerQuery, type SessionQueryScope } from "../lib/navigation";
import { getSessions } from "../lib/runtime";
import { sessionMatchesKind, sessionMatchesQuery } from "../lib/sessionFiltering";
import { useUiStore } from "../stores/ui-store";
import type { Confidence, Session, SessionKind } from "../types/domain";

const defaultFilters: SessionFilterState = { query: "", confidence: "all", kind: "all" };
const emptySessions: Session[] = [];
const emptyCollection = { items: emptySessions, total: 0, truncated: false };
const maskedServerQueryLabel = "Masked multiplayer server";

function routeQueryScope(value: string | null): SessionQueryScope | "generic" {
  return value === "context" || value === "instance" || value === "version" ? value : "generic";
}

function groupByDay(items: Session[]): Array<[string, Session[]]> {
  const grouped = new Map<string, Session[]>();
  items.forEach((session) => {
    const day = session.startedAt.slice(0, 10);
    grouped.set(day, [...(grouped.get(day) ?? []), session]);
  });
  return [...grouped.entries()];
}

export function SessionsPage() {
  const sessionsQuery = useQuery({ queryKey: ["sessions", "all"], queryFn: getSessions });
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const privacyMask = useUiStore((state) => state.privacyMask);
  const queryScope = routeQueryScope(searchParams.get("scope"));
  const routeFilters: SessionFilterState = {
    query: searchParams.get("q") ?? "",
    confidence: (searchParams.get("confidence") as Confidence | null) ?? "all",
    kind: (searchParams.get("kind") as SessionKind | null) ?? "all",
  };

  const sessionCollection = sessionsQuery.data ?? emptyCollection;
  const sessionItems = sessionCollection.items;
  const masksDetectedServerQuery = shouldMaskDetectedServerQuery(
    routeFilters.query,
    sessionItems,
    privacyMask,
  );
  const filters: SessionFilterState = {
    ...routeFilters,
    query: masksDetectedServerQuery ? maskedServerQueryLabel : routeFilters.query,
  };

  function updateFilters(next: SessionFilterState) {
    const params = new URLSearchParams();
    const queryInputUnchanged = next.query === filters.query;
    const nextQuery = masksDetectedServerQuery && next.query === maskedServerQueryLabel
      ? routeFilters.query
      : next.query;
    if (nextQuery) params.set("q", nextQuery);
    if (next.confidence !== "all") params.set("confidence", next.confidence);
    if (next.kind !== "all") params.set("kind", next.kind);
    if (queryScope !== "generic" && queryInputUnchanged && next.kind === routeFilters.kind) {
      params.set("scope", queryScope);
    }
    setSearchParams(params, { replace: true });
  }

  const filtered = useMemo(() => {
    const query = routeFilters.query.trim().toLowerCase();
    return sessionItems.filter((session) => {
      const matchesQuery = sessionMatchesQuery(session, query, queryScope, routeFilters.kind);
      const matchesConfidence = routeFilters.confidence === "all" || session.confidence === routeFilters.confidence;
      const matchesKind = sessionMatchesKind(session, routeFilters.kind);
      return matchesQuery && matchesConfidence && matchesKind;
    });
  }, [queryScope, routeFilters.confidence, routeFilters.kind, routeFilters.query, sessionItems]);

  const grouped = groupByDay(filtered);
  const knownMinutes = filtered.reduce((sum, session) => sum + (session.durationMinutes ?? 0), 0);
  const seam: EvidenceSegment[] = filtered.map((session) => ({
    id: session.id,
    weight: Math.max(1, session.durationMinutes ?? 20),
    intensity: Math.min(4, Math.max(1, Math.ceil((session.durationMinutes ?? 20) / 60))) as 1 | 2 | 3 | 4,
    confidence: session.confidence,
    label: `${session.instance}: ${formatDuration(session.durationMinutes)}`,
  }));

  const pageHeader = (
    <PageHeader
      eyebrow="Observed archive · Reconstructed launches"
      title="Session evidence"
      description="Each detected launch keeps its source, confidence, and inferred boundaries close at hand."
    />
  );

  if (sessionsQuery.isPending) {
    return (
      <div className="page page--sessions">
        {pageHeader}
        <div aria-live="polite" aria-busy="true">
          <EmptyState
            icon={FileClock}
            title="Loading detected sessions"
            description="Reading reconstructed sessions from the local archive."
          />
        </div>
      </div>
    );
  }

  if (sessionsQuery.isError) {
    return (
      <div className="page page--sessions">
        {pageHeader}
        <EmptyState
          icon={TriangleAlert}
          title="The session archive could not be loaded"
          description="Your source files were not changed. Retry the local database query or open Scan center to run a fresh scan."
          action={
            <Button leadingIcon={<RefreshCw aria-hidden="true" />} onClick={() => void sessionsQuery.refetch()}>
              Retry
            </Button>
          }
        />
      </div>
    );
  }

  if (sessionItems.length === 0) {
    return (
      <div className="page page--sessions">
        {pageHeader}
        <EmptyState
          icon={FileClock}
          title="No sessions in the archive"
          description="No reconstructable sessions are currently stored. Run a local scan after more launcher or log evidence is available."
          action={<Button onClick={() => navigate("/scan")}>Open Scan center</Button>}
        />
      </div>
    );
  }

  return (
    <div className="page page--sessions">
      {pageHeader}

      {sessionCollection.truncated && (
        <ArchiveWindowNote
          loaded={sessionItems.length}
          total={sessionCollection.total}
          noun="sessions"
          detail="The newest sessions are loaded; filters on this page apply to that window."
        />
      )}

      <section className="sessions-summary" aria-label="Filtered session summary">
        <div>
          <span>Showing</span>
          <strong>{filtered.length}</strong>
          <small>
            of {sessionItems.length.toLocaleString()} loaded
            {sessionCollection.truncated ? ` · ${sessionCollection.total.toLocaleString()} archived` : ""}
          </small>
        </div>
        <div>
          <span>Detected runtime</span>
          <strong>{formatDuration(knownMinutes, true)}</strong>
          <small>in current view</small>
        </div>
        <div className="sessions-summary__seam">
          <span>Evidence mix</span>
          <EvidenceSeam
            segments={seam.length ? seam : [{ id: "empty", weight: 1, intensity: 0, confidence: "unknown", label: "No evidence" }]}
            compact
            label={`Evidence confidence across ${filtered.length} detected sessions; solid segments are verified and hatched segments are inferred`}
          />
          <small>Solid marks are verified; hatch marks are inferred</small>
        </div>
      </section>

      <SessionFilters filters={filters} onChange={updateFilters} onReset={() => updateFilters(defaultFilters)} />

      {filtered.length > 0 ? (
        <section className="session-timeline" aria-label="Sessions grouped by day">
          <div className="session-timeline__columns" aria-hidden="true">
            <span>Time</span>
            <span>Destination</span>
            <span>Instance</span>
            <span>Version</span>
            <span>Runtime</span>
            <span>Evidence</span>
          </div>
          {grouped.map(([day, daySessions]) => (
            <section className="session-day" key={day}>
              <header className="session-day__header">
                <h2>{formatSessionDate(day)}</h2>
                <span>{daySessions.length} {daySessions.length === 1 ? "session" : "sessions"}</span>
              </header>
              <div className="session-day__entries">
                {daySessions.map((session, index) => (
                  <SessionRow key={session.id} session={session} isLast={index === daySessions.length - 1} />
                ))}
              </div>
            </section>
          ))}
        </section>
      ) : (
        <EmptyState
          icon={FileClock}
          title="No sessions match this view"
          description={sessionCollection.truncated
            ? "No loaded session matches these filters. Older archive entries are not loaded in this bounded view."
            : "The archive still has history; these filters narrowed it to zero. Reset them to return to every detected session."}
          action={<Button onClick={() => updateFilters(defaultFilters)}>Reset filters</Button>}
        />
      )}
    </div>
  );
}
