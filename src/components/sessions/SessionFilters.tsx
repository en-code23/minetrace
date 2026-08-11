import { CalendarRange, FilterX, Search, SlidersHorizontal } from "lucide-react";
import type { Confidence, SessionKind } from "../../types/domain";

export interface SessionFilterState {
  query: string;
  confidence: Confidence | "all";
  kind: SessionKind | "all";
}

export function SessionFilters({
  filters,
  onChange,
  onReset,
}: {
  filters: SessionFilterState;
  onChange: (filters: SessionFilterState) => void;
  onReset: () => void;
}) {
  const hasFilters = filters.query !== "" || filters.confidence !== "all" || filters.kind !== "all";

  return (
    <div className="session-filters" aria-label="Session filters">
      <label className="search-input">
        <Search aria-hidden="true" />
        <span className="sr-only">Search sessions</span>
        <input
          type="search"
          value={filters.query}
          onChange={(event) => onChange({ ...filters, query: event.target.value })}
          placeholder="Instance, world, server, version…"
        />
      </label>
      <label className="select-control">
        <SlidersHorizontal aria-hidden="true" />
        <span className="sr-only">Evidence confidence</span>
        <select
          value={filters.confidence}
          onChange={(event) => onChange({ ...filters, confidence: event.target.value as SessionFilterState["confidence"] })}
        >
          <option value="all">All evidence</option>
          <option value="verified">Verified</option>
          <option value="high">High estimate</option>
          <option value="partial">Partial</option>
          <option value="unknown">Unknown</option>
        </select>
      </label>
      <label className="select-control">
        <CalendarRange aria-hidden="true" />
        <span className="sr-only">Session destination</span>
        <select
          value={filters.kind}
          onChange={(event) => onChange({ ...filters, kind: event.target.value as SessionFilterState["kind"] })}
        >
          <option value="all">Every destination</option>
          <option value="server">Multiplayer</option>
          <option value="world">Local worlds</option>
          <option value="mixed">Multiple destinations</option>
          <option value="menu">Menu only</option>
        </select>
      </label>
      {hasFilters && (
        <button className="filter-reset" type="button" onClick={onReset}>
          <FilterX aria-hidden="true" />
          Reset
        </button>
      )}
    </div>
  );
}
