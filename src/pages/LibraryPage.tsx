import { useQuery } from "@tanstack/react-query";
import {
  ArrowUpRight,
  Boxes,
  Globe2,
  History,
  RadioTower,
  RefreshCw,
  Search,
  Server,
  Shield,
  TreePine,
  TriangleAlert,
} from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "../components/ui/Button";
import { ArchiveWindowNote } from "../components/ui/ArchiveWindowNote";
import { ConfidenceBadge } from "../components/ui/ConfidenceBadge";
import { EmptyState } from "../components/ui/EmptyState";
import { PageHeader } from "../components/ui/PageHeader";
import { formatDuration, formatRelativeDate } from "../lib/format";
import { scopedSessionRoute, serverSessionRoute, visibleServerSearchQuery } from "../lib/navigation";
import { getServers, getVersions, getWorlds } from "../lib/runtime";
import { useUiStore } from "../stores/ui-store";
import type { BoundedCollection, ServerSummary, VersionSummary, WorldSummary } from "../types/domain";

type LibraryKind = "worlds" | "servers" | "versions";

const emptyWorlds: WorldSummary[] = [];
const emptyServers: ServerSummary[] = [];
const emptyVersions: VersionSummary[] = [];
const emptyWorldCollection: BoundedCollection<WorldSummary> = { items: emptyWorlds, total: 0, truncated: false };
const emptyServerCollection: BoundedCollection<ServerSummary> = { items: emptyServers, total: 0, truncated: false };
const emptyVersionCollection: BoundedCollection<VersionSummary> = { items: emptyVersions, total: 0, truncated: false };

const metadata: Record<LibraryKind, { eyebrow: string; title: string; fact: string }> = {
  worlds: {
    eyebrow: "Observed archive · Session-linked worlds",
    title: "Worlds",
    fact: "Session-linked runtime",
  },
  servers: {
    eyebrow: "Observed archive · Multiplayer destinations",
    title: "Servers",
    fact: "Session-linked runtime",
  },
  versions: {
    eyebrow: "Observed archive · Version timeline",
    title: "Versions",
    fact: "Reconstructed sessions",
  },
};

function matchesQuery(query: string, values: Array<string | null>): boolean {
  const normalized = query.trim().toLowerCase();
  return !normalized || values.some((value) => value?.toLowerCase().includes(normalized));
}

function relativeObservation(value: string | null): string {
  return value ? formatRelativeDate(value) : "Unknown";
}

export function LibraryPage({ kind }: { kind: LibraryKind }) {
  const [query, setQuery] = useState("");
  const info = metadata[kind];
  const privacyMask = useUiStore((state) => state.privacyMask);
  const serverSearchMasked = kind === "servers" && privacyMask;
  const activeQuery = visibleServerSearchQuery(query, serverSearchMasked);

  return (
    <div className={`page page--library page--${kind}`}>
      <PageHeader eyebrow={info.eyebrow} title={info.title} />

      <div className="library-toolbar">
        <label className="search-input">
          <Search aria-hidden="true" />
          <span className="sr-only">Search detected {kind}</span>
          <input
            value={activeQuery}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={serverSearchMasked ? "Server search hidden while masking" : `Find ${kind}…`}
            disabled={serverSearchMasked}
          />
        </label>
        <div className="library-toolbar__facts">
          <span><Shield aria-hidden="true" /> Read-only local archive</span>
          <span><History aria-hidden="true" /> {info.fact}</span>
        </div>
      </div>

      {kind === "worlds" && <WorldLibrary query={activeQuery} onClear={() => setQuery("")} />}
      {kind === "servers" && <ServerLibrary query={activeQuery} privacyMask={privacyMask} onClear={() => setQuery("")} />}
      {kind === "versions" && <VersionLibrary query={activeQuery} onClear={() => setQuery("")} />}
    </div>
  );
}

function WorldLibrary({ query, onClear }: { query: string; onClear: () => void }) {
  const worldsQuery = useQuery({ queryKey: ["worlds", "all"], queryFn: getWorlds });
  const navigate = useNavigate();
  const worldCollection = worldsQuery.data ?? emptyWorldCollection;
  const worldItems = worldCollection.items;
  const items = worldItems.filter((world) =>
    matchesQuery(query, [world.name, world.instance, world.version, world.mode, world.sizeLabel]),
  );

  if (worldsQuery.isPending) return <LibraryLoading label="Loading observed worlds" />;
  if (worldsQuery.isError) {
    return (
      <LibraryError
        noun="world archive"
        onRetry={() => void worldsQuery.refetch()}
      />
    );
  }
  if (worldItems.length === 0) {
    return (
      <LibraryEmpty
        icon={Globe2}
        title="No worlds in the archive"
        description="No world names are currently linked to reconstructed sessions. MineTrace does not infer worlds from unrelated folders."
      />
    );
  }
  if (items.length === 0) {
    return <FilterEmpty noun="worlds" onClear={onClear} />;
  }

  const maxMinutes = Math.max(1, ...worldItems.map((world) => world.totalMinutes));

  return (
    <>
      {worldCollection.truncated && (
        <ArchiveWindowNote
          loaded={worldItems.length}
          total={worldCollection.total}
          noun="worlds"
          detail="Search applies to the loaded aggregate window."
        />
      )}
      <section className="library-table" aria-label="Session-observed local worlds">
      <header className="library-table__header library-table__header--worlds" aria-hidden="true">
        <span>World</span><span>Profile</span><span>Session-linked runtime</span><span>Last observed</span><span>Evidence</span><span />
      </header>
      {items.map((world) => {
        const indexedMetadata = [world.mode, world.sizeLabel].filter(Boolean).join(" · ");
        return (
          <button
            className="library-row library-row--world"
            type="button"
            key={world.id}
            aria-label={`Inspect reconstructed sessions linked to ${world.name}`}
            onClick={() => navigate(scopedSessionRoute(world.name, "context", "world"))}
          >
            <span className="library-primary">
              <i aria-hidden="true"><TreePine /></i>
              <span><strong>{world.name}</strong><small>{indexedMetadata || "Observed in session logs"}</small></span>
            </span>
            <span className="library-cell">
              <strong>{world.instance}</strong>
              <small>{world.version ?? "Version not detected"}</small>
            </span>
            <RuntimeCell minutes={world.totalMinutes} maxMinutes={maxMinutes} />
            <span className="library-cell">
              <strong>{relativeObservation(world.lastPlayedAt)}</strong>
              <small>Last linked session</small>
            </span>
            <ConfidenceBadge confidence={world.confidence} />
            <ArrowUpRight aria-hidden="true" />
          </button>
        );
      })}
      <footer className="library-footnote">
        <History aria-hidden="true" />
        <span>Runtime is session-linked: each session associated with a world contributes its full duration. It is not an estimate of exact time spent inside that world.</span>
      </footer>
      </section>
    </>
  );
}

function ServerLibrary({ query, privacyMask, onClear }: { query: string; privacyMask: boolean; onClear: () => void }) {
  const serversQuery = useQuery({ queryKey: ["servers", "all"], queryFn: getServers });
  const navigate = useNavigate();
  const serverCollection = serversQuery.data ?? emptyServerCollection;
  const serverItems = serverCollection.items;
  const items = serverItems.filter((serverItem) =>
    matchesQuery(query, [serverItem.name, serverItem.address, serverItem.favoriteVersion]),
  );

  if (serversQuery.isPending) return <LibraryLoading label="Loading observed servers" />;
  if (serversQuery.isError) {
    return <LibraryError noun="server archive" onRetry={() => void serversQuery.refetch()} />;
  }
  if (serverItems.length === 0) {
    return (
      <LibraryEmpty
        icon={Server}
        title="No servers in the archive"
        description="No multiplayer destinations are currently linked to reconstructed sessions. Saved addresses alone are not treated as proof of a visit."
      />
    );
  }
  if (items.length === 0) {
    return <FilterEmpty noun="servers" onClear={onClear} />;
  }

  const maxMinutes = Math.max(1, ...serverItems.map((serverItem) => serverItem.totalMinutes));

  return (
    <>
      {serverCollection.truncated && (
        <ArchiveWindowNote
          loaded={serverItems.length}
          total={serverCollection.total}
          noun="servers"
          detail="Search applies to the loaded aggregate window."
        />
      )}
      <section className="library-table" aria-label="Observed multiplayer servers">
      <header className="library-table__header library-table__header--servers" aria-hidden="true">
        <span>Server</span><span>Session-linked runtime</span><span>Sessions</span><span>Last observed</span><span>Evidence</span><span />
      </header>
      {items.map((serverItem) => {
        const displayName = privacyMask ? "Masked multiplayer server" : serverItem.name;
        const addressDetail = privacyMask
          ? "Address masked"
          : serverItem.name === serverItem.address
            ? "Observed destination"
            : serverItem.address;

        return (
          <button
            className="library-row library-row--server"
            type="button"
            key={serverItem.id}
            aria-label={privacyMask
              ? "View reconstructed multiplayer sessions while server addresses are masked"
              : `Inspect reconstructed sessions linked to ${serverItem.name}`}
            onClick={() => navigate(serverSessionRoute(serverItem.name, privacyMask))}
          >
            <span className="library-primary">
              <i aria-hidden="true"><RadioTower /></i>
              <span><strong>{displayName}</strong><small>{addressDetail}</small></span>
            </span>
            <RuntimeCell minutes={serverItem.totalMinutes} maxMinutes={maxMinutes} />
            <span className="library-cell">
              <strong>{serverItem.sessions.toLocaleString()}</strong>
              <small>linked {serverItem.sessions === 1 ? "session" : "sessions"}</small>
            </span>
            <span className="library-cell">
              <strong>{relativeObservation(serverItem.lastPlayedAt)}</strong>
              <small>{serverItem.favoriteVersion ?? "Version not detected"}</small>
            </span>
            <ConfidenceBadge confidence={serverItem.confidence} />
            <ArrowUpRight aria-hidden="true" />
          </button>
        );
      })}
      <footer className="library-footnote">
        <Shield aria-hidden="true" />
        <span>Addresses are displayed from local evidence only and are never contacted. Runtime counts the full duration of linked sessions, not exact connected time.</span>
      </footer>
      </section>
    </>
  );
}

function VersionLibrary({ query, onClear }: { query: string; onClear: () => void }) {
  const versionsQuery = useQuery({ queryKey: ["versions", "all"], queryFn: getVersions });
  const navigate = useNavigate();
  const versionCollection = versionsQuery.data ?? emptyVersionCollection;
  const versionItems = versionCollection.items;
  const items = versionItems
    .filter((version) => matchesQuery(query, [version.name, version.type, ...version.loaders]))
    .sort((left, right) => Date.parse(left.firstPlayedAt) - Date.parse(right.firstPlayedAt));

  if (versionsQuery.isPending) return <LibraryLoading label="Loading observed versions" />;
  if (versionsQuery.isError) {
    return <LibraryError noun="version archive" onRetry={() => void versionsQuery.refetch()} />;
  }
  if (versionItems.length === 0) {
    return (
      <LibraryEmpty
        icon={Boxes}
        title="No versions in the archive"
        description="No sessions with explicit Minecraft version evidence are currently stored. Missing versions remain unknown."
      />
    );
  }
  if (items.length === 0) {
    return <FilterEmpty noun="versions" onClear={onClear} />;
  }

  const maxMinutes = Math.max(1, ...versionItems.map((version) => version.totalMinutes));

  return (
    <>
      {versionCollection.truncated && (
        <ArchiveWindowNote
          loaded={versionItems.length}
          total={versionCollection.total}
          noun="versions"
          detail="Search and timeline ordering apply to the loaded aggregate window."
        />
      )}
      <section className="version-ledger" aria-label="Observed Minecraft versions">
      <div className="version-ledger__axis" aria-hidden="true">
        <span>Earlier</span><i /><span>Current</span>
      </div>
      {items.map((version, index) => (
        <button
          className="version-row"
          type="button"
          key={version.id}
          aria-label={`Inspect reconstructed sessions using Minecraft ${version.name}`}
          onClick={() => navigate(scopedSessionRoute(version.name, "version"))}
        >
          <span className="version-row__marker" aria-hidden="true"><i /></span>
          <span className="version-row__name"><strong>{version.name}</strong><small>{versionTypeLabel(version)}</small></span>
          <span className="version-row__range"><small>First seen</small><strong>{formatRelativeDate(version.firstPlayedAt)}</strong></span>
          <span className="version-row__range"><small>Last seen</small><strong>{formatRelativeDate(version.lastPlayedAt)}</strong></span>
          <span className="version-row__loaders"><small>Observed with</small><strong>{version.loaders.length ? version.loaders.join(" · ") : "Loader not detected"}</strong></span>
          <RuntimeCell minutes={version.totalMinutes} maxMinutes={maxMinutes} />
          <span className="version-row__sessions">{version.sessions.toLocaleString()} {version.sessions === 1 ? "session" : "sessions"}</span>
          <ConfidenceBadge confidence={version.confidence} compact />
          {index < items.length - 1 && <span className="version-row__connector" aria-hidden="true" />}
        </button>
      ))}
      </section>
    </>
  );
}

function versionTypeLabel(version: VersionSummary): string {
  if (version.type === "release") return "Release";
  if (version.type === "snapshot") return "Snapshot";
  return "Other version";
}

function RuntimeCell({ minutes, maxMinutes }: { minutes: number; maxMinutes: number }) {
  return (
    <span className="library-runtime">
      <strong>{formatDuration(minutes, true)}</strong>
      <i aria-hidden="true"><span style={{ width: `${Math.round((minutes / maxMinutes) * 100)}%` }} /></i>
    </span>
  );
}

function LibraryLoading({ label }: { label: string }) {
  return (
    <div aria-live="polite" aria-label={label} aria-busy="true">
      <div className="skeleton skeleton--panel" />
    </div>
  );
}

function LibraryError({ noun, onRetry }: { noun: string; onRetry: () => void }) {
  return (
    <EmptyState
      icon={TriangleAlert}
      title={`The ${noun} could not be loaded`}
      description="Your Minecraft files were not changed. Retry the local database query or open Scan center to run a fresh scan."
      action={<Button leadingIcon={<RefreshCw aria-hidden="true" />} onClick={onRetry}>Retry</Button>}
    />
  );
}

function LibraryEmpty({ icon, title, description }: { icon: typeof Globe2; title: string; description: string }) {
  const navigate = useNavigate();
  return (
    <EmptyState
      icon={icon}
      title={title}
      description={description}
      action={<Button onClick={() => navigate("/scan")}>Open Scan center</Button>}
    />
  );
}

function FilterEmpty({ noun, onClear }: { noun: string; onClear: () => void }) {
  return (
    <EmptyState
      icon={Search}
      title={`No ${noun} match this search`}
      description={`The archive still contains detected ${noun}. Clear the search to return to the full list.`}
      action={<Button onClick={onClear}>Clear search</Button>}
    />
  );
}
