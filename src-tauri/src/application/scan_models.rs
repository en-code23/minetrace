use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::scan::{
    DurableScanSnapshot, ScanMessageSeverity as StoredSeverity, ScanMode as StoredMode,
    ScanState as StoredState,
};

pub(crate) const MAX_SCAN_ISSUES: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanMode {
    Quick,
    Standard,
    Deep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanState {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
}

impl ScanState {
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Failed | Self::Interrupted
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanPhase {
    Idle,
    Discovering,
    Indexing,
    Parsing,
    Aggregating,
    Complete,
    Cancelled,
    Failed,
    Interrupted,
}

impl ScanPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Discovering => "discovering",
            Self::Indexing => "indexing",
            Self::Parsing => "parsing",
            Self::Aggregating => "aggregating",
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanIssueSeverity {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanIssue {
    pub severity: ScanIssueSeverity,
    pub code: String,
    pub entity_label: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub id: String,
    pub mode: ScanMode,
    pub state: ScanState,
    pub phase: ScanPhase,
    pub current: u64,
    pub total: u64,
    pub current_path: Option<String>,
    pub warnings: u64,
    pub errors: u64,
    pub issues: Vec<ScanIssue>,
    pub message: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub dataset_revision: Option<u64>,
}

impl ScanStatus {
    pub fn idle() -> Self {
        Self {
            id: String::new(),
            mode: ScanMode::Standard,
            state: ScanState::Completed,
            phase: ScanPhase::Idle,
            current: 0,
            total: 0,
            current_path: None,
            warnings: 0,
            errors: 0,
            issues: Vec::new(),
            message: None,
            started_at: None,
            finished_at: None,
            dataset_revision: None,
        }
    }

    pub fn queued(id: String, mode: ScanMode) -> Self {
        Self {
            id,
            mode,
            state: ScanState::Queued,
            phase: ScanPhase::Discovering,
            current: 0,
            total: 0,
            current_path: None,
            warnings: 0,
            errors: 0,
            issues: Vec::new(),
            message: Some("Preparing approved locations".to_owned()),
            started_at: None,
            finished_at: None,
            dataset_revision: None,
        }
    }

    pub(crate) fn from_durable(snapshot: DurableScanSnapshot) -> Self {
        let counters =
            serde_json::from_str::<DurableCounters>(&snapshot.counters_json).unwrap_or_default();
        let mode = match snapshot.mode {
            StoredMode::Quick => ScanMode::Quick,
            StoredMode::Standard => ScanMode::Standard,
            StoredMode::Deep => ScanMode::Deep,
        };
        let (state, phase, message) = match snapshot.state {
            StoredState::Completed => (
                ScanState::Completed,
                ScanPhase::Complete,
                "The scan completed and the local archive reflects the available source evidence.",
            ),
            StoredState::Cancelled => (
                ScanState::Cancelled,
                ScanPhase::Cancelled,
                "The scan was cancelled before promotion; the prior archive remains unchanged.",
            ),
            StoredState::Failed => (
                ScanState::Failed,
                ScanPhase::Failed,
                "The scan failed before promotion; the prior archive remains unchanged.",
            ),
            StoredState::Interrupted
            | StoredState::Queued
            | StoredState::Running
            | StoredState::Paused => (
                ScanState::Interrupted,
                ScanPhase::Interrupted,
                "The previous scan was interrupted before promotion; the prior archive remains unchanged.",
            ),
        };
        let mut issues = snapshot
            .messages
            .into_iter()
            .map(|issue| ScanIssue {
                severity: match issue.severity {
                    StoredSeverity::Warning => ScanIssueSeverity::Warning,
                    StoredSeverity::Error => ScanIssueSeverity::Error,
                },
                code: issue.code,
                entity_label: issue.entity_ref,
                message: issue.redacted_message,
            })
            .collect::<Vec<_>>();
        if state == ScanState::Failed
            && !issues
                .iter()
                .any(|issue| issue.severity == ScanIssueSeverity::Error)
        {
            if issues.len() == MAX_SCAN_ISSUES {
                issues.remove(0);
            }
            issues.push(ScanIssue {
                severity: ScanIssueSeverity::Error,
                code: snapshot
                    .error_code
                    .unwrap_or_else(|| "scan_failed".to_owned()),
                entity_label: None,
                message: "The scan stopped before its reconstruction could be promoted.".to_owned(),
            });
        }
        if issues.len() > MAX_SCAN_ISSUES {
            issues.drain(..issues.len() - MAX_SCAN_ISSUES);
        }
        let issue_warnings = issues
            .iter()
            .filter(|issue| issue.severity == ScanIssueSeverity::Warning)
            .count() as u64;
        let issue_errors = issues
            .iter()
            .filter(|issue| issue.severity == ScanIssueSeverity::Error)
            .count() as u64;

        Self {
            id: snapshot.id,
            mode,
            state,
            phase,
            current: counters.current,
            total: counters.total,
            current_path: None,
            warnings: counters
                .warnings
                .max(snapshot.warning_count)
                .max(issue_warnings),
            errors: counters.errors.max(snapshot.error_count).max(issue_errors),
            issues,
            message: Some(message.to_owned()),
            started_at: snapshot.started_at_ms.and_then(rfc3339_millis),
            finished_at: snapshot.finished_at_ms.and_then(rfc3339_millis),
            dataset_revision: snapshot
                .dataset_revision_after
                .unwrap_or(snapshot.dataset_revision_before)
                .max(0)
                .try_into()
                .ok(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DurableCounters {
    current: u64,
    total: u64,
    warnings: u64,
    errors: u64,
}

fn rfc3339_millis(value: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(value).map(|value| value.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::{ScanIssueSeverity, ScanPhase, ScanState, ScanStatus};
    use crate::scan::{
        DurableScanSnapshot, ScanMessageSeverity, ScanMode, ScanState as StoredState,
        StoredScanMessage,
    };

    #[test]
    fn durable_interruption_restores_terminal_status_and_redacted_issues() {
        let status = ScanStatus::from_durable(DurableScanSnapshot {
            id: "scan-recovered".to_owned(),
            mode: ScanMode::Standard,
            state: StoredState::Interrupted,
            counters_json: r#"{"current":3,"total":7,"warnings":1,"errors":0}"#.to_owned(),
            error_code: None,
            started_at_ms: Some(1_700_000_000_000),
            finished_at_ms: Some(1_700_000_001_000),
            dataset_revision_before: 4,
            dataset_revision_after: Some(4),
            warning_count: 1,
            error_count: 0,
            messages: vec![StoredScanMessage {
                severity: ScanMessageSeverity::Warning,
                code: "scan_interrupted".to_owned(),
                entity_ref: None,
                redacted_message: "The scan was interrupted before promotion.".to_owned(),
            }],
        });

        assert_eq!(status.state, ScanState::Interrupted);
        assert_eq!(status.phase, ScanPhase::Interrupted);
        assert_eq!(status.current, 3);
        assert_eq!(status.total, 7);
        assert_eq!(status.warnings, 1);
        assert_eq!(status.dataset_revision, Some(4));
        assert_eq!(status.issues.len(), 1);
        assert_eq!(status.issues[0].severity, ScanIssueSeverity::Warning);
        assert_eq!(status.issues[0].code, "scan_interrupted");
    }
}
