import { useQuery } from "@tanstack/react-query";
import { ArrowDownAZ, Boxes, Plus, RefreshCw, Search, TriangleAlert } from "lucide-react";
import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { InstanceWorkbench } from "../components/instances/InstanceWorkbench";
import { ArchiveWindowNote } from "../components/ui/ArchiveWindowNote";
import { Button } from "../components/ui/Button";
import { ConfidenceBadge } from "../components/ui/ConfidenceBadge";
import { EmptyState } from "../components/ui/EmptyState";
import { PageHeader } from "../components/ui/PageHeader";
import { formatDuration, formatRelativeDate } from "../lib/format";
import { getInstances } from "../lib/runtime";
import type { BoundedCollection, InstanceSummary } from "../types/domain";

const emptyInstances: InstanceSummary[] = [];
const emptyInstanceCollection: BoundedCollection<InstanceSummary> = {
  items: emptyInstances,
  total: 0,
  truncated: false,
};

function observedAt(value: string | null): string {
  return value ? formatRelativeDate(value) : "Last observation unknown";
}

function lastObservedTimestamp(instance: InstanceSummary): number {
  return instance.lastPlayedAt ? new Date(instance.lastPlayedAt).getTime() : Number.NEGATIVE_INFINITY;
}

export function InstancesPage() {
  const instancesQuery = useQuery({ queryKey: ["instances", "all"], queryFn: getInstances });
  const navigate = useNavigate();
  const [selectedId, setSelectedId] = useState("");
  const [query, setQuery] = useState("");
  const [sortAlphabetically, setSortAlphabetically] = useState(false);
  const instanceCollection = instancesQuery.data ?? emptyInstanceCollection;
  const instanceItems = instanceCollection.items;
  const filtered = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    const matches = normalized
      ? instanceItems.filter((instance) =>
          [instance.name, instance.launcher, instance.version, instance.loader]
            .some((value) => value?.toLowerCase().includes(normalized)),
        )
      : instanceItems;

    return [...matches].sort((left, right) => {
      if (sortAlphabetically) return left.name.localeCompare(right.name);
      return lastObservedTimestamp(right) - lastObservedTimestamp(left) || left.name.localeCompare(right.name);
    });
  }, [instanceItems, query, sortAlphabetically]);
  const selected = filtered.find((instance) => instance.id === selectedId) ?? filtered[0];

  const pageHeader = (
    <PageHeader
      eyebrow="Observed archive · Detected profiles"
      title="Instances"
      description="Minecraft installations and isolated profiles joined only to the session evidence found on this device."
      actions={
        <Button leadingIcon={<Plus aria-hidden="true" />} onClick={() => navigate("/scan")}>
          Add location
        </Button>
      }
    />
  );

  if (instancesQuery.isPending) {
    return (
      <div className="page page--instances" aria-label="Loading detected instances" aria-busy="true">
        {pageHeader}
        <div className="skeleton skeleton--panel" />
      </div>
    );
  }

  if (instancesQuery.isError) {
    return (
      <div className="page page--instances">
        {pageHeader}
        <EmptyState
          icon={TriangleAlert}
          title="The instance archive could not be loaded"
          description="Your Minecraft files were not changed. Retry the local database query or open Scan center to run a fresh scan."
          action={
            <Button leadingIcon={<RefreshCw aria-hidden="true" />} onClick={() => void instancesQuery.refetch()}>
              Retry
            </Button>
          }
        />
      </div>
    );
  }

  if (instanceItems.length === 0) {
    return (
      <div className="page page--instances">
        {pageHeader}
        <EmptyState
          icon={Boxes}
          title="No instances in the archive"
          description="No source-backed profiles are currently stored. Run a local scan after more launcher or log evidence is available."
          action={<Button onClick={() => navigate("/scan")}>Open Scan center</Button>}
        />
      </div>
    );
  }

  return (
    <div className="page page--instances">
      {pageHeader}

      {instanceCollection.truncated && (
        <ArchiveWindowNote
          loaded={instanceItems.length}
          total={instanceCollection.total}
          noun="profiles"
          detail="Search and sorting apply to the loaded aggregate window."
        />
      )}

      <div className="instance-layout">
        <aside className="instance-index" aria-label="Detected instance list">
          <header className="instance-index__header">
            <label className="search-input search-input--compact">
              <Search aria-hidden="true" />
              <span className="sr-only">Search detected instances</span>
              <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Find an instance" />
            </label>
            <button
              className="icon-action"
              type="button"
              aria-label={sortAlphabetically ? "Sort instances by most recent observation" : "Sort instances alphabetically"}
              aria-pressed={sortAlphabetically}
              title={sortAlphabetically ? "Sort by most recent observation" : "Sort alphabetically"}
              onClick={() => setSortAlphabetically((value) => !value)}
            >
              <ArrowDownAZ aria-hidden="true" />
            </button>
          </header>

          <div className="instance-index__summary" aria-live="polite">
            <span>
              {filtered.length} of {instanceItems.length} loaded profiles
              {instanceCollection.truncated ? ` · ${instanceCollection.total} archived` : ""}
            </span>
            <span>{formatDuration(filtered.reduce((sum, instance) => sum + instance.totalMinutes, 0), true)}</span>
          </div>

          <div className="instance-index__list">
            {filtered.map((instance) => (
              <button
                type="button"
                key={instance.id}
                className={`instance-index-row ${instance.id === selected?.id ? "instance-index-row--active" : ""}`}
                aria-pressed={instance.id === selected?.id}
                onClick={() => setSelectedId(instance.id)}
              >
                <span className={`instance-index-row__accent instance-index-row__accent--${instance.accent}`} />
                <span className="instance-index-row__copy">
                  <strong>{instance.name}</strong>
                  <small>{instance.version ?? "Version not detected"} · {instance.loader ?? "Loader not detected"}</small>
                </span>
                <span className="instance-index-row__metric">
                  <strong>{formatDuration(instance.totalMinutes, true)}</strong>
                  <small>{observedAt(instance.lastPlayedAt)}</small>
                </span>
                <ConfidenceBadge confidence={instance.confidence} compact />
              </button>
            ))}
          </div>
        </aside>

        {selected ? (
          <InstanceWorkbench instance={selected} />
        ) : (
          <section className="instance-workbench" aria-live="polite">
            <EmptyState
              icon={Search}
              title="No profiles match this search"
              description="The archive still contains detected profiles. Clear the search to return to the full list."
              action={<Button onClick={() => setQuery("")}>Clear search</Button>}
            />
          </section>
        )}
      </div>
    </div>
  );
}
