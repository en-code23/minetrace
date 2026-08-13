export type Confidence = "verified" | "high" | "partial" | "unknown";

export type ExitKind = "clean" | "crash" | "forced" | "unknown";

export type SessionKind = "server" | "world" | "mixed" | "menu";

export interface SessionContext {
  kind: "server" | "world";
  name: string;
}

export interface ObservedDailyActivity {
  date: string;
  minutes: number;
  sessions: number;
  confidence: Confidence;
  coverage: "observed";
}

export interface MissingDailyActivity {
  date: string;
  minutes: null;
  sessions: null;
  confidence: "unknown";
  coverage: "missing";
}

export type DailyActivity = ObservedDailyActivity | MissingDailyActivity;

export interface ObservedMonthlyActivity {
  month: string;
  label: string;
  minutes: number;
  sessions: number;
  estimatedShare: number;
  confidence: Confidence;
  coverage: "observed";
}

export interface MissingMonthlyActivity {
  month: string;
  label: string;
  minutes: null;
  sessions: null;
  estimatedShare: null;
  confidence: "unknown";
  coverage: "missing";
}

export type MonthlyActivity = ObservedMonthlyActivity | MissingMonthlyActivity;

export interface Session {
  id: string;
  startedAt: string;
  endedAt: string | null;
  durationMinutes: number | null;
  launcher: string;
  instance: string;
  version: string;
  loader: string | null;
  kind: SessionKind;
  destination: string | null;
  contexts: SessionContext[];
  exitKind: ExitKind;
  confidence: Confidence;
  source: string;
  note?: string;
}

export interface BoundedCollection<T> {
  items: T[];
  total: number;
  truncated: boolean;
}

export type SessionCollection = BoundedCollection<Session>;

export interface InstanceSummary {
  id: string;
  name: string;
  launcher: string;
  version: string | null;
  loader: string | null;
  totalMinutes: number;
  sessions: number;
  lastPlayedAt: string | null;
  modCount: number | null;
  worldCount: number;
  crashCount: number;
  confidence: Confidence;
  accent: "moss" | "copper" | "quartz" | "slate";
}

export interface WorldSummary {
  id: string;
  name: string;
  instance: string;
  mode: string | null;
  version: string | null;
  totalMinutes: number;
  lastPlayedAt: string | null;
  sizeLabel: string | null;
  confidence: Confidence;
  runtimeBasis: "sessionLinked";
}

export interface ServerSummary {
  id: string;
  name: string;
  address: string;
  sessions: number;
  totalMinutes: number;
  lastPlayedAt: string | null;
  favoriteVersion: string | null;
  confidence: Confidence;
  runtimeBasis: "sessionLinked";
}

export interface VersionSummary {
  id: string;
  name: string;
  type: "release" | "snapshot" | "other";
  totalMinutes: number;
  sessions: number;
  firstPlayedAt: string;
  lastPlayedAt: string;
  loaders: string[];
  confidence: Confidence;
}

export interface DashboardData {
  archiveState: "unscanned" | "scannedNoEvidence" | "ready";
  generatedAt: string;
  coverage: {
    firstDetectedAt: string;
    lastDetectedAt: string;
    quality: "verified" | "partial" | "limited" | "unknown";
    score: number;
    verifiedShare: number;
    warning: string;
    observedMonths: number;
    gapMonths: number;
  };
  totals: {
    playtimeMinutes: number;
    uniquePlaytimeMinutes: number;
    sessions: number;
    activeDays: number;
    longestSessionMinutes: number | null;
    averageSessionMinutes: number | null;
  };
  top: {
    launcher: { name: string; minutes: number };
    instance: { name: string; minutes: number };
    version: { name: string; minutes: number };
    server: { name: string; minutes: number };
    world: { name: string; minutes: number };
  };
  monthly: MonthlyActivity[];
  daily: DailyActivity[];
  recentSessions: Session[];
}

export interface DiscoveredLocation {
  id: string;
  name: string;
  kind: string;
  path: string;
  instances: number;
  confidence: Confidence;
  enabled: boolean;
  platform: "windows" | "macos" | "linux";
}

export type ScanMode = "quick" | "standard" | "deep";

export type ScanPhase =
  | "idle"
  | "discovering"
  | "indexing"
  | "parsing"
  | "aggregating"
  | "complete"
  | "cancelled"
  | "interrupted"
  | "failed";

export type ScanState = "queued" | "running" | "completed" | "cancelled" | "failed" | "interrupted";

export interface ScanIssue {
  severity: "warning" | "error";
  code: string;
  entityLabel: string | null;
  message: string;
}

export interface ScanProgress {
  id: string;
  mode: ScanMode;
  state: ScanState;
  phase: ScanPhase;
  current: number;
  total: number;
  currentPath: string | null;
  warnings: number;
  errors: number;
  message: string | null;
  issues: ScanIssue[];
  startedAt: string | null;
  finishedAt: string | null;
  datasetRevision: number | null;
}

export type StatisticUnit = "count" | "ticks" | "centimeters" | "tenths";
export type StatisticsBasis = "uuidMatched" | "inferredLocalPlayer" | "singleLocalPlayer" | "none";
export type WorldAvailability = "available" | "missing" | "backupOnly";

export interface ProfileIdentity {
  name: string;
  uuid: string | null;
  source: string;
  active: boolean;
}

export interface SkinAsset {
  id: string;
  dataUrl: string;
  observedAt: string | null;
  source: string;
  width: number;
  height: number;
}

export interface ProfileAvatar {
  dataUrl: string;
  source: string;
}

export interface ProfileStatistic {
  id: string;
  label: string;
  value: number;
  unit: StatisticUnit;
  sourceWorlds: number;
}

export interface ProfileStatisticSection {
  id: string;
  label: string;
  items: ProfileStatistic[];
}

export interface LauncherUsage {
  id: string;
  name: string;
  instances: number;
  sessions: number;
  totalMinutes: number;
  firstObservedAt: string | null;
  lastObservedAt: string | null;
}

export interface ProfileWorld {
  id: string;
  name: string;
  folderName: string | null;
  instance: string;
  launcher: string;
  availability: WorldAvailability;
  totalMinutes: number | null;
  lastObservedAt: string | null;
  statsAvailable: boolean;
  statsBasis: StatisticsBasis;
  backupCount: number;
}

export interface WorldBackup {
  id: string;
  name: string;
  instance: string;
  sizeBytes: number;
  modifiedAt: string | null;
}

export interface ProfileData {
  generatedAt: string;
  identity: ProfileIdentity | null;
  identities: ProfileIdentity[];
  currentSkin: SkinAsset | null;
  avatar: ProfileAvatar | null;
  previousSkins: SkinAsset[];
  summary: {
    totalPlaytimeMinutes: number;
    sessions: number;
    activeDays: number;
    mostPlayedVersion: string | null;
    mostPlayedWorld: string | null;
    launcherCount: number;
    availableWorlds: number;
    missingWorlds: number;
    backupCount: number;
    statisticsWorlds: number;
    trackedStatistics: number;
  };
  randomStats: ProfileStatistic[];
  statisticSections: ProfileStatisticSection[];
  statisticsBasis: StatisticsBasis;
  launchers: LauncherUsage[];
  worlds: ProfileWorld[];
  backups: WorldBackup[];
  limitations: string[];
}
