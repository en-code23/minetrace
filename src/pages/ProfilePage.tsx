import { useQuery } from "@tanstack/react-query";
import {
  Archive,
  BarChart3,
  Clock3,
  FolderSearch,
  Gamepad2,
  HardDrive,
  Image as ImageIcon,
  Layers3,
  PackageOpen,
  RefreshCw,
  Search,
  Share2,
  ShieldQuestion,
  Sparkles,
  Trophy,
  UserRound,
} from "lucide-react";
import { useState } from "react";
import { SkinPreview } from "../components/profile/SkinPreview";
import { ShareProfileDialog } from "../components/profile/ShareProfileDialog";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";
import { PageHeader } from "../components/ui/PageHeader";
import { formatDuration, formatRelativeDate, pluralize } from "../lib/format";
import { getProfile } from "../lib/runtime";
import type { ProfileData, ProfileStatistic, ProfileWorld, WorldAvailability } from "../types/domain";

type ProfileTab = "overview" | "statistics" | "worlds" | "clients";
type WorldFilter = "all" | WorldAvailability;

export function ProfilePage() {
  const [tab, setTab] = useState<ProfileTab>("overview");
  const [sharing, setSharing] = useState(false);
  const profile = useQuery({ queryKey: ["profile"], queryFn: getProfile });

  if (profile.isPending) {
    return <ProfileLoading />;
  }
  if (profile.isError || !profile.data) {
    return (
      <div className="page page--profile">
        <PageHeader eyebrow="Player dossier" title="Profile" />
        <EmptyState
          icon={UserRound}
          title="Profile data is unavailable"
          description="MineTrace could not read the approved local account, world, and statistics files."
          action={<Button leadingIcon={<RefreshCw aria-hidden="true" />} onClick={() => void profile.refetch()}>Try again</Button>}
        />
      </div>
    );
  }

  const data = profile.data;
  return (
    <div className="page page--profile">
      <PageHeader
        eyebrow="Player dossier · Local files"
        title="Profile"
        actions={<div className="profile-header-actions">
          <Button leadingIcon={<RefreshCw aria-hidden="true" />} loading={profile.isFetching} onClick={() => void profile.refetch()}>Refresh local data</Button>
          <Button variant="primary" leadingIcon={<Share2 aria-hidden="true" />} onClick={() => setSharing(true)}>Share on social</Button>
        </div>}
      />

      <ProfileHero profile={data} />

      <div className="profile-tabs" role="tablist" aria-label="Profile sections">
        <TabButton id="overview" current={tab} onChange={setTab} icon={Sparkles} label="Overview" />
        <TabButton id="statistics" current={tab} onChange={setTab} icon={BarChart3} label="Statistics" />
        <TabButton id="worlds" current={tab} onChange={setTab} icon={FolderSearch} label="Worlds & backups" />
        <TabButton id="clients" current={tab} onChange={setTab} icon={Layers3} label="Clients & launchers" />
      </div>

      {tab === "overview" && <OverviewTab profile={data} />}
      {tab === "statistics" && <StatisticsTab profile={data} />}
      {tab === "worlds" && <WorldsTab profile={data} />}
      {tab === "clients" && <ClientsTab profile={data} />}

      {sharing && <ShareProfileDialog profile={data} onClose={() => setSharing(false)} />}
    </div>
  );
}

function ProfileHero({ profile }: { profile: ProfileData }) {
  const identity = profile.identity;
  return (
    <section className="profile-hero">
      <div className="profile-hero__figure">
        <SkinPreview dataUrl={profile.currentSkin?.dataUrl ?? null} mode="body" label={identity ? `${identity.name}'s most recent local skin` : "No locally cached full skin"} />
        {!profile.currentSkin && <span className="profile-hero__no-skin"><ImageIcon aria-hidden="true" />Full skin not found</span>}
      </div>
      <div className="profile-hero__identity">
        <span className="profile-avatar">
          {profile.currentSkin
            ? <SkinPreview dataUrl={profile.currentSkin.dataUrl} mode="head" label="Player head" />
            : profile.avatar
              ? <img src={profile.avatar.dataUrl} alt="Player avatar" />
              : <UserRound aria-hidden="true" />}
        </span>
        <div>
          <p>{identity?.active ? "Active local account" : identity ? "Local account" : "Local archive"}</p>
          <h2>{identity?.name ?? "Minecraft player"}</h2>
          <span>{identity?.source ?? "No launcher account profile detected"}</span>
        </div>
      </div>
      <div className="profile-hero__metrics">
        <HeroMetric icon={Clock3} label="Playtime" value={formatDuration(profile.summary.totalPlaytimeMinutes, true)} />
        <HeroMetric icon={Gamepad2} label="Sessions" value={profile.summary.sessions.toLocaleString()} />
        <HeroMetric icon={Trophy} label="Top version" value={profile.summary.mostPlayedVersion ?? "Not observed"} />
        <HeroMetric icon={Archive} label="Top world" value={profile.summary.mostPlayedWorld ?? "Not observed"} />
      </div>
    </section>
  );
}

function HeroMetric({ icon: Icon, label, value }: { icon: typeof Clock3; label: string; value: string }) {
  return <div className="profile-hero-metric"><Icon aria-hidden="true" /><span><small>{label}</small><strong>{value}</strong></span></div>;
}

function TabButton({ id, current, onChange, icon: Icon, label }: { id: ProfileTab; current: ProfileTab; onChange: (tab: ProfileTab) => void; icon: typeof Sparkles; label: string }) {
  return (
    <button type="button" role="tab" aria-selected={current === id} className={current === id ? "profile-tab profile-tab--active" : "profile-tab"} onClick={() => onChange(id)}>
      <Icon aria-hidden="true" /><span>{label}</span>
    </button>
  );
}

function OverviewTab({ profile }: { profile: ProfileData }) {
  return (
    <div className="profile-overview" role="tabpanel">
      <section className="profile-stat-spotlight" aria-labelledby="spotlight-title">
        <header><div><p className="eyebrow">From local world statistics</p><h2 id="spotlight-title">Stat spotlights</h2></div><span>{basisLabel(profile.statisticsBasis)}</span></header>
        {profile.randomStats.length > 0 ? (
          <div className="profile-stat-spotlight__grid">
            {profile.randomStats.map((stat, index) => <StatTile stat={stat} key={stat.id} index={index} />)}
          </div>
        ) : (
          <div className="profile-inline-empty"><BarChart3 aria-hidden="true" /><span><strong>No local in-game statistics found</strong><small>Single-player world stats appear here after a scan finds an attributable stats file.</small></span></div>
        )}
      </section>

      <div className="profile-overview__split">
        <section className="profile-section" aria-labelledby="worlds-title">
          <header><div><p className="eyebrow">World folders & archive links</p><h2 id="worlds-title">Most-played worlds</h2></div><span>{profile.worlds.length.toLocaleString()} known</span></header>
          <div className="profile-world-list">
            {profile.worlds.slice(0, 8).map((world) => <WorldRow world={world} key={world.id} />)}
            {profile.worlds.length === 0 && <div className="profile-inline-empty"><Archive aria-hidden="true" /><span><strong>No worlds observed</strong><small>Run a scan after playing a local world.</small></span></div>}
          </div>
          {profile.summary.missingWorlds > 0 && <p className="profile-caveat"><ShieldQuestion aria-hidden="true" /> {pluralize(profile.summary.missingWorlds, "save folder")} not found. It may be deleted, moved, renamed, or outside approved locations.</p>}
        </section>

        <section className="profile-section" aria-labelledby="skins-title">
          <header><div><p className="eyebrow">Launcher skin cache</p><h2 id="skins-title">Earlier local skins</h2></div><span>{profile.previousSkins.length.toLocaleString()} retained</span></header>
          {profile.previousSkins.length > 0 ? (
            <div className="skin-history">
              {profile.previousSkins.map((skin, index) => (
                <article key={skin.id}><SkinPreview dataUrl={skin.dataUrl} mode="head" label={`Previous skin ${index + 1}`} /><span><strong>Skin {index + 1}</strong><small>{skin.observedAt ? formatRelativeDate(skin.observedAt) : "Date unavailable"}</small></span></article>
              ))}
            </div>
          ) : <div className="profile-inline-empty"><ImageIcon aria-hidden="true" /><span><strong>No earlier skin retained</strong><small>MineTrace only shows full textures still present in a local launcher cache.</small></span></div>}

          <div className="backup-summary">
            <PackageOpen aria-hidden="true" /><span><strong>{pluralize(profile.summary.backupCount, "backup")}</strong><small>{profile.summary.backupCount > 0 ? "ZIP archives found in approved instance backup folders" : "No world backup archives found"}</small></span>
          </div>
        </section>
      </div>
    </div>
  );
}

function StatisticsTab({ profile }: { profile: ProfileData }) {
  const [query, setQuery] = useState("");
  const normalizedQuery = query.trim().toLowerCase();
  const visibleSections = profile.statisticSections
    .map((section) => ({
      ...section,
      items: section.items.filter((stat) => stat.label.toLowerCase().includes(normalizedQuery)),
    }))
    .filter((section) => section.items.length > 0);
  return (
    <div className="profile-statistics" role="tabpanel">
      <section className="profile-statistics__intro">
        <div><p className="eyebrow">Statistics basis</p><h2>{basisLabel(profile.statisticsBasis)}</h2><p>Totals combine {pluralize(profile.summary.statisticsWorlds, "local world")} with a selected local player statistics file.</p></div>
        <div className="profile-statistics__coverage">
          <span><strong>{profile.summary.statisticsWorlds.toLocaleString()}</strong><small>Worlds matched</small></span>
          <span><strong>{profile.summary.trackedStatistics.toLocaleString()}</strong><small>Measures tracked</small></span>
          <HardDrive aria-label="Local save data only" />
        </div>
      </section>
      {profile.statisticSections.length > 0 && <label className="profile-search">
        <Search aria-hidden="true" />
        <span className="sr-only">Search in-game statistics</span>
        <input type="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Find a block, item, mob, or measure" />
        <kbd>{visibleSections.reduce((total, section) => total + section.items.length, 0).toLocaleString()} found</kbd>
      </label>}
      {visibleSections.length > 0 ? visibleSections.map((section) => (
        <section className="profile-stat-group" key={section.id}>
          <header><h2>{section.label}</h2><span>{pluralize(section.items.length, "measure")}</span></header>
          <div className="profile-stat-table">
            {section.items.map((stat, index) => <StatRow stat={stat} index={index} key={stat.id} />)}
          </div>
        </section>
      )) : profile.statisticSections.length === 0 ? (
        <EmptyState icon={BarChart3} title="No attributable in-game statistics" description="Minecraft stores these per local world and player. Multiplayer servers usually keep them server-side." />
      ) : <EmptyState icon={Search} title="No statistics match this search" description="Try a block, item, mob, or broader term." />}
      <ul className="profile-limitations">{profile.limitations.map((item) => <li key={item}>{item}</li>)}</ul>
    </div>
  );
}

function WorldsTab({ profile }: { profile: ProfileData }) {
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<WorldFilter>("all");
  const normalizedQuery = query.trim().toLowerCase();
  const matching = profile.worlds.filter((world) =>
    (filter === "all" || world.availability === filter)
    && [world.name, world.folderName, world.instance, world.launcher]
      .filter(Boolean)
      .some((value) => value!.toLowerCase().includes(normalizedQuery)),
  );
  const visible = matching.slice(0, 100);
  return (
    <div className="profile-worlds" role="tabpanel">
      <section className="profile-world-summary" aria-label="World library summary">
        <WorldSummaryFact label="Save available" value={profile.summary.availableWorlds} tone="available" />
        <WorldSummaryFact label="Save not found" value={profile.summary.missingWorlds} tone="missing" />
        <WorldSummaryFact label="Backups found" value={profile.summary.backupCount} tone="backupOnly" />
        <WorldSummaryFact label="Stats matched" value={profile.summary.statisticsWorlds} tone="stats" />
      </section>

      <section className="profile-section">
        <header className="profile-world-controls">
          <label className="profile-search">
            <Search aria-hidden="true" />
            <span className="sr-only">Search worlds and backups</span>
            <input type="search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Find a world, instance, or launcher" />
          </label>
          <div className="profile-world-filters" aria-label="Filter worlds">
            {(["all", "available", "missing", "backupOnly"] as const).map((value) => (
              <button type="button" aria-pressed={filter === value} onClick={() => setFilter(value)} key={value}>
                {value === "all" ? "All" : availabilityLabel(value)}
              </button>
            ))}
          </div>
        </header>
        <div className="profile-world-list">
          {visible.map((world) => <WorldRow world={world} key={world.id} />)}
          {matching.length === 0 && <div className="profile-inline-empty"><FolderSearch aria-hidden="true" /><span><strong>No worlds match these filters</strong><small>Change the availability filter or search text.</small></span></div>}
        </div>
        {matching.length > visible.length && <p className="profile-caveat">Showing the first {visible.length.toLocaleString()} of {matching.length.toLocaleString()} matches. Refine the search to find a specific world.</p>}
      </section>
    </div>
  );
}

function WorldSummaryFact({ label, value, tone }: { label: string; value: number; tone: "available" | "missing" | "backupOnly" | "stats" }) {
  return <div className={`profile-world-summary__fact profile-world-summary__fact--${tone}`}><span /><strong>{value.toLocaleString()}</strong><small>{label}</small></div>;
}

function ClientsTab({ profile }: { profile: ProfileData }) {
  return (
    <div className="profile-clients" role="tabpanel">
      <section className="profile-section">
        <header><div><p className="eyebrow">Session evidence</p><h2>Clients & launchers</h2></div><span>{pluralize(profile.launchers.length, "launcher")}</span></header>
        <div className="launcher-ledger">
          {profile.launchers.map((launcher, index) => (
            <article key={launcher.id}>
              <span className="launcher-ledger__index">{String(index + 1).padStart(2, "0")}</span>
              <div><strong>{launcher.name}</strong><small>{pluralize(launcher.instances, "instance")} · {launcher.lastObservedAt ? `last seen ${formatRelativeDate(launcher.lastObservedAt)}` : "no dated session"}</small></div>
              <span><small>Sessions</small><strong>{launcher.sessions.toLocaleString()}</strong></span>
              <span><small>Playtime</small><strong>{formatDuration(launcher.totalMinutes, true)}</strong></span>
            </article>
          ))}
          {profile.launchers.length === 0 && <div className="profile-inline-empty"><Layers3 aria-hidden="true" /><span><strong>No launcher usage yet</strong><small>Launcher rows appear after session evidence has been scanned.</small></span></div>}
        </div>
      </section>

      <section className="profile-section">
        <header><div><p className="eyebrow">Local launcher profiles</p><h2>Detected identities</h2></div><span>{pluralize(profile.identities.length, "identity", "identities")}</span></header>
        <div className="identity-ledger">
          {profile.identities.map((identity) => (
            <article key={`${identity.uuid ?? identity.name}-${identity.source}`}>
              <UserRound aria-hidden="true" /><div><strong>{identity.name}</strong><small>{identity.source}</small></div>
              {identity.active && <em>Active</em>}
              <code>{identity.uuid ?? "UUID not available"}</code>
            </article>
          ))}
          {profile.identities.length === 0 && <div className="profile-inline-empty"><UserRound aria-hidden="true" /><span><strong>No local account profile found</strong><small>MineTrace does not sign in or send an online profile request.</small></span></div>}
        </div>
      </section>
    </div>
  );
}

function WorldRow({ world }: { world: ProfileWorld }) {
  return (
    <article className="profile-world-row">
      <span className={`world-availability world-availability--${world.availability}`} aria-label={availabilityLabel(world.availability)} />
      <div><strong>{world.name}</strong><small>{world.instance} · {availabilityLabel(world.availability)}</small></div>
      {world.statsAvailable && <span className="world-stat-mark"><BarChart3 aria-hidden="true" /> Stats</span>}
      {world.backupCount > 0 && <span className="world-backup-mark"><PackageOpen aria-hidden="true" /> {world.backupCount}</span>}
      <strong>{world.totalMinutes === null ? "—" : formatDuration(world.totalMinutes, true)}</strong>
    </article>
  );
}

function StatTile({ stat, index }: { stat: ProfileStatistic; index: number }) {
  return <article className={`profile-stat-tile profile-stat-tile--${index % 4}`}><span>{String(index + 1).padStart(2, "0")}</span><strong>{formatStatistic(stat)}</strong><small>{stat.label}</small><em>{pluralize(stat.sourceWorlds, "world")}</em></article>;
}

function StatRow({ stat, index }: { stat: ProfileStatistic; index: number }) {
  return <div className="profile-stat-row"><span>{String(index + 1).padStart(2, "0")}</span><strong>{stat.label}</strong><small>{pluralize(stat.sourceWorlds, "world")}</small><b>{formatStatistic(stat)}</b></div>;
}

function formatStatistic(stat: ProfileStatistic): string {
  if (stat.unit === "ticks") return formatDuration(Math.round(stat.value / 1_200));
  if (stat.unit === "centimeters") return stat.value >= 100_000
    ? `${(stat.value / 100_000).toLocaleString(undefined, { maximumFractionDigits: 1 })} km`
    : `${Math.round(stat.value / 100).toLocaleString()} m`;
  if (stat.unit === "tenths") return (stat.value / 10).toLocaleString(undefined, { maximumFractionDigits: 1 });
  return stat.value.toLocaleString();
}

function availabilityLabel(value: WorldAvailability): string {
  if (value === "available") return "Save available";
  if (value === "backupOnly") return "Backup only";
  return "Save not found";
}

function basisLabel(value: ProfileData["statisticsBasis"]): string {
  if (value === "uuidMatched") return "Matched to this player UUID";
  if (value === "inferredLocalPlayer") return "Most frequent local player file";
  if (value === "singleLocalPlayer") return "Single local player file";
  return "No attributable statistics";
}

function ProfileLoading() {
  return (
    <div className="page page--profile" aria-busy="true">
      <PageHeader eyebrow="Player dossier" title="Profile" />
      <div className="profile-loading"><span /><span /><span /></div>
    </div>
  );
}
