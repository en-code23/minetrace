use serde::Serialize;

use crate::domain::Confidence;

#[derive(Debug, Clone, Serialize)]
pub struct BoundedCollection<T> {
    pub items: Vec<T>,
    pub total: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardData {
    pub generated_at: String,
    pub archive_state: ArchiveState,
    pub coverage: Coverage,
    pub totals: DashboardTotals,
    pub top: TopMetrics,
    pub monthly: Vec<MonthlyActivity>,
    pub daily: Vec<DailyActivity>,
    pub recent_sessions: Vec<Session>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveState {
    Unscanned,
    ScannedNoEvidence,
    Ready,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceAccent {
    Moss,
    Copper,
    Quartz,
    Slate,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSummary {
    pub id: String,
    pub name: String,
    pub launcher: String,
    pub version: Option<String>,
    pub loader: Option<String>,
    pub total_minutes: u64,
    pub sessions: u64,
    pub last_played_at: Option<String>,
    /// Mod inventory is not part of the current log-evidence parser.
    /// `None` is deliberately serialized as `null`, never as an invented zero.
    pub mod_count: Option<u64>,
    /// Distinct local-world names observed in session evidence.
    pub world_count: u64,
    pub crash_count: u64,
    pub confidence: Confidence,
    pub accent: InstanceAccent,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeBasis {
    /// The destination was observed during a session, but its exact enter/leave
    /// boundaries are not yet strong enough for destination-only runtime.
    SessionLinked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldSummary {
    pub id: String,
    pub name: String,
    pub instance: String,
    /// Game mode requires read-only world metadata parsing, which is not active yet.
    pub mode: Option<String>,
    pub version: Option<String>,
    pub total_minutes: u64,
    pub last_played_at: Option<String>,
    /// World size requires a save inventory, which is not active yet.
    pub size_label: Option<String>,
    pub confidence: Confidence,
    pub runtime_basis: RuntimeBasis,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    pub id: String,
    pub name: String,
    pub address: String,
    pub sessions: u64,
    pub total_minutes: u64,
    pub last_played_at: Option<String>,
    pub favorite_version: Option<String>,
    pub confidence: Confidence,
    pub runtime_basis: RuntimeBasis,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VersionKind {
    Release,
    Snapshot,
    Other,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionSummary {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: VersionKind,
    pub total_minutes: u64,
    pub sessions: u64,
    pub first_played_at: String,
    pub last_played_at: String,
    pub loaders: Vec<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coverage {
    pub first_detected_at: String,
    pub last_detected_at: String,
    pub quality: CoverageQuality,
    pub score: u8,
    pub verified_share: f64,
    pub warning: String,
    pub observed_months: u32,
    pub gap_months: u32,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum CoverageQuality {
    Verified,
    Partial,
    Limited,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardTotals {
    pub playtime_minutes: u64,
    pub unique_playtime_minutes: u64,
    pub sessions: u64,
    pub active_days: u64,
    pub longest_session_minutes: Option<u64>,
    pub average_session_minutes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopMetrics {
    pub launcher: NamedMinutes,
    pub instance: NamedMinutes,
    pub version: NamedMinutes,
    pub server: NamedMinutes,
    pub world: NamedMinutes,
}

#[derive(Debug, Clone, Serialize)]
pub struct NamedMinutes {
    pub name: String,
    pub minutes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyActivity {
    pub month: String,
    pub label: String,
    pub minutes: Option<u64>,
    pub sessions: Option<u64>,
    pub estimated_share: Option<f64>,
    pub confidence: Confidence,
    pub coverage: MonthlyCoverage,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MonthlyCoverage {
    Observed,
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailyActivity {
    pub date: String,
    pub minutes: Option<u64>,
    pub sessions: Option<u64>,
    pub confidence: Confidence,
    pub coverage: DailyCoverage,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DailyCoverage {
    Observed,
    Missing,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum ExitKind {
    Clean,
    Crash,
    Forced,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum SessionKind {
    Server,
    World,
    Mixed,
    Menu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionContextKind {
    Server,
    World,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionContext {
    pub kind: SessionContextKind,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_minutes: Option<u64>,
    pub launcher: String,
    pub instance: String,
    pub version: String,
    pub loader: Option<String>,
    pub kind: SessionKind,
    pub destination: Option<String>,
    pub contexts: Vec<SessionContext>,
    pub exit_kind: ExitKind,
    pub confidence: Confidence,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub type SessionPage = BoundedCollection<Session>;
