//! Deterministic session reconstruction from parsed Minecraft log evidence.
//!
//! Callers must group logs by Minecraft instance before invoking this module and
//! assign `source_order` from oldest rotation to newest. Disconnects close an
//! activity destination, not the whole game process. Only client stopping,
//! clean-shutdown, crash, a later startup, or a conservative source tail closes
//! a play session.

use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, FixedOffset, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::domain::Confidence;

use super::minecraft_log::{
    EvidenceProvenance, EvidenceRule, EvidenceTag, EvidenceTimestamp, LogEvidence,
    MinecraftLogEvent, ParsedLog,
};

pub const RECONSTRUCTION_REVISION: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStartBoundary {
    ExplicitStartup,
    InferredFromActivity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEndBoundary {
    CleanShutdown,
    ClientStopping,
    Crash,
    LastEvidenceBeforeNextStart,
    SourceEndHint,
    TruncatedAtLastEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconstructedExitKind {
    Clean,
    Crash,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionDestination {
    Server { address: String },
    LocalWorld { world_name: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionEvidenceRole {
    Start,
    End,
    Version,
    Destination,
    Exit,
    Supporting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEvidenceLink {
    pub evidence_tag: EvidenceTag,
    pub role: SessionEvidenceRole,
    pub confidence_score: u8,
    pub provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructedSession {
    pub started_at: EvidenceTimestamp,
    pub ended_at: EvidenceTimestamp,
    pub duration_seconds: u64,
    pub start_boundary: SessionStartBoundary,
    pub end_boundary: SessionEndBoundary,
    pub exit_kind: ReconstructedExitKind,
    pub confidence_score: u8,
    pub confidence_label: Confidence,
    pub reconstruction_revision: u16,
    pub versions: Vec<String>,
    pub destinations: Vec<SessionDestination>,
    pub source_ids: Vec<String>,
    pub evidence: Vec<SessionEvidenceLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructionPolicy {
    /// Multiple startup markers are common during bootstrap and must not split
    /// one process into several sessions.
    pub startup_merge_window_seconds: i64,
    /// A file mtime/end hint is accepted only this far after the last evidence.
    pub source_end_tail_limit_seconds: i64,
}

impl Default for ReconstructionPolicy {
    fn default() -> Self {
        Self {
            startup_merge_window_seconds: 120,
            source_end_tail_limit_seconds: 15 * 60,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructionDiagnostics {
    pub duplicate_evidence: u64,
    pub orphan_end_evidence: u64,
    pub orphan_activity_without_timestamp: u64,
    pub startup_without_timestamp: u64,
    pub non_monotonic_evidence: u64,
    pub zero_duration_sessions: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconstructionResult {
    pub sessions: Vec<ReconstructedSession>,
    pub diagnostics: ReconstructionDiagnostics,
}

#[derive(Debug, Clone)]
struct EndCandidate {
    timestamp: EvidenceTimestamp,
    boundary: SessionEndBoundary,
    exit_kind: ReconstructedExitKind,
}

#[derive(Debug)]
struct SessionBuilder {
    started_at: EvidenceTimestamp,
    start_boundary: SessionStartBoundary,
    last_timestamp: EvidenceTimestamp,
    last_source_end_hint: Option<DateTime<FixedOffset>>,
    pending_stop: Option<EndCandidate>,
    versions: Vec<String>,
    version_index: HashSet<String>,
    destinations: Vec<SessionDestination>,
    destination_index: HashSet<SessionDestination>,
    has_named_local_world: bool,
    evidence: Vec<SessionEvidenceLink>,
    evidence_index: HashSet<EvidenceLinkKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EvidenceLinkKey {
    evidence_tag: EvidenceTag,
    role: SessionEvidenceRole,
    confidence_score: u8,
    source_id: String,
    source_order: u32,
    line_number: u64,
    byte_start: u64,
    byte_end: u64,
    rule: EvidenceRule,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CrossSourceEvidenceKey {
    event: MinecraftLogEvent,
    moment: EvidenceMomentKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EvidenceKeyOccurrence {
    key: CrossSourceEvidenceKey,
    line_number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EvidenceMomentKey {
    Utc(i64),
    Local(NaiveDateTime),
    Sequence(i64),
}

const MIN_PARTIAL_OVERLAP_EVIDENCE: usize = 2;

impl SessionBuilder {
    fn new(
        evidence: &LogEvidence,
        start_boundary: SessionStartBoundary,
        source_end_hint: Option<DateTime<FixedOffset>>,
    ) -> Self {
        let mut builder = Self {
            started_at: evidence.timestamp.clone(),
            start_boundary,
            last_timestamp: evidence.timestamp.clone(),
            last_source_end_hint: source_end_hint,
            pending_stop: None,
            versions: Vec::new(),
            version_index: HashSet::new(),
            destinations: Vec::new(),
            destination_index: HashSet::new(),
            has_named_local_world: false,
            evidence: Vec::new(),
            evidence_index: HashSet::new(),
        };
        builder.link(evidence, SessionEvidenceRole::Start);
        builder
    }

    fn link(&mut self, evidence: &LogEvidence, role: SessionEvidenceRole) {
        let key = EvidenceLinkKey {
            evidence_tag: evidence.event.tag(),
            role,
            confidence_score: evidence.confidence_score,
            source_id: evidence.provenance.source_id.clone(),
            source_order: evidence.provenance.source_order,
            line_number: evidence.provenance.line_number,
            byte_start: evidence.provenance.byte_start,
            byte_end: evidence.provenance.byte_end,
            rule: evidence.provenance.rule,
        };
        if !self.evidence_index.insert(key) {
            return;
        }
        let link = SessionEvidenceLink {
            evidence_tag: evidence.event.tag(),
            role,
            confidence_score: evidence.confidence_score,
            provenance: evidence.provenance.clone(),
        };
        self.evidence.push(link);
    }

    fn observe_timestamp(
        &mut self,
        timestamp: &EvidenceTimestamp,
        diagnostics: &mut ReconstructionDiagnostics,
    ) {
        let Some(candidate) = timestamp.comparable_millis() else {
            return;
        };
        let Some(last) = self.last_timestamp.comparable_millis() else {
            self.last_timestamp = timestamp.clone();
            return;
        };

        if candidate >= last {
            self.last_timestamp = timestamp.clone();
        } else {
            diagnostics.non_monotonic_evidence += 1;
        }
    }

    fn observe_source_end_hint(&mut self, source_end_hint: Option<DateTime<FixedOffset>>) {
        let Some(candidate) = source_end_hint else {
            return;
        };
        if self
            .last_source_end_hint
            .is_none_or(|current| candidate > current)
        {
            self.last_source_end_hint = Some(candidate);
        }
    }

    fn add_version(&mut self, version: &str) {
        if self.version_index.insert(version.to_owned()) {
            self.versions.push(version.to_owned());
        }
    }

    fn add_destination(&mut self, destination: SessionDestination) {
        match &destination {
            SessionDestination::LocalWorld {
                world_name: Some(_),
            } => {
                if !self.has_named_local_world {
                    let unnamed = SessionDestination::LocalWorld { world_name: None };
                    if self.destination_index.remove(&unnamed) {
                        self.destinations.retain(|existing| existing != &unnamed);
                    }
                    self.has_named_local_world = true;
                }
                if self.destination_index.contains(&destination) {
                    return;
                }
            }
            SessionDestination::LocalWorld { world_name: None } if self.has_named_local_world => {
                return;
            }
            _ => {}
        }

        if self.destination_index.insert(destination.clone()) {
            self.destinations.push(destination);
        }
    }
}

/// Reconstruct sessions for one instance using the default conservative policy.
pub fn reconstruct_sessions(logs: &[ParsedLog]) -> ReconstructionResult {
    reconstruct_sessions_with_policy(logs, &ReconstructionPolicy::default())
}

/// Reconstruct sessions for one instance from oldest-to-newest rotated evidence.
pub fn reconstruct_sessions_with_policy(
    logs: &[ParsedLog],
    policy: &ReconstructionPolicy,
) -> ReconstructionResult {
    let mut ordered_logs: Vec<&ParsedLog> = logs.iter().collect();
    ordered_logs.sort_by(|left, right| {
        left.context
            .source_order
            .cmp(&right.context.source_order)
            .then_with(|| left.context.source_id.cmp(&right.context.source_id))
    });

    let mut result = ReconstructionResult::default();
    let mut active: Option<SessionBuilder> = None;
    let mut seen_source_hashes = BTreeSet::new();
    let mut previous_source_keys: Option<Vec<EvidenceKeyOccurrence>> = None;

    for log in ordered_logs {
        if let Some(source_hash) = log.context.source_content_hash
            && !seen_source_hashes.insert(source_hash)
        {
            result.diagnostics.duplicate_evidence = result
                .diagnostics
                .duplicate_evidence
                .saturating_add(log.evidence.len() as u64);
            continue;
        }
        let source_end_hint = log.context.source_end_hint;
        let current_source_keys = log
            .evidence
            .iter()
            .filter_map(cross_source_evidence_key)
            .collect::<Vec<_>>();
        let duplicate_prefix = previous_source_keys
            .as_deref()
            .map(|previous| suffix_prefix_overlap(previous, &current_source_keys))
            .filter(|overlap| {
                *overlap >= MIN_PARTIAL_OVERLAP_EVIDENCE
                    && overlap_spans_multiple_lines(
                        previous_source_keys
                            .as_deref()
                            .expect("checked previous keys"),
                        &current_source_keys,
                        *overlap,
                    )
            })
            .unwrap_or_default();
        let mut keyed_evidence_index = 0_usize;

        for evidence in &log.evidence {
            if cross_source_evidence_key(evidence).is_some() {
                let duplicate = keyed_evidence_index < duplicate_prefix;
                keyed_evidence_index = keyed_evidence_index.saturating_add(1);
                if duplicate {
                    result.diagnostics.duplicate_evidence =
                        result.diagnostics.duplicate_evidence.saturating_add(1);
                    continue;
                }
            }
            match &evidence.event {
                MinecraftLogEvent::GameStarted => {
                    if evidence.timestamp.comparable_millis().is_none() {
                        result.diagnostics.startup_without_timestamp += 1;
                        if let Some(builder) = active.as_mut() {
                            builder.link(evidence, SessionEvidenceRole::Supporting);
                        }
                        continue;
                    }

                    let is_bootstrap_marker = active.as_ref().is_some_and(|builder| {
                        let same_source = builder.evidence.first().is_some_and(|link| {
                            link.provenance.source_id == evidence.provenance.source_id
                        });
                        builder.pending_stop.is_none()
                            && same_source
                            && elapsed_seconds(&builder.started_at, &evidence.timestamp)
                                .is_some_and(|elapsed| {
                                    (0..=policy.startup_merge_window_seconds).contains(&elapsed)
                                })
                    });

                    if is_bootstrap_marker {
                        let builder = active.as_mut().expect("checked active builder");
                        builder.link(evidence, SessionEvidenceRole::Supporting);
                        builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                        builder.observe_source_end_hint(source_end_hint);
                        continue;
                    }

                    if let Some(builder) = active.take() {
                        let candidate =
                            builder
                                .pending_stop
                                .clone()
                                .unwrap_or_else(|| EndCandidate {
                                    timestamp: builder.last_timestamp.clone(),
                                    boundary: SessionEndBoundary::LastEvidenceBeforeNextStart,
                                    exit_kind: ReconstructedExitKind::Unknown,
                                });
                        finish_session(builder, candidate, &mut result);
                    }

                    active = Some(SessionBuilder::new(
                        evidence,
                        SessionStartBoundary::ExplicitStartup,
                        source_end_hint,
                    ));
                }
                MinecraftLogEvent::VersionObserved { version } => {
                    if let Some(builder) = active.as_mut() {
                        builder.add_version(version);
                        builder.link(evidence, SessionEvidenceRole::Version);
                        builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                        builder.observe_source_end_hint(source_end_hint);
                    }
                }
                MinecraftLogEvent::ServerJoined { address } => {
                    prepare_for_activity(&mut active, evidence, source_end_hint, &mut result);
                    if let Some(builder) = active.as_mut() {
                        builder.add_destination(SessionDestination::Server {
                            address: address.clone(),
                        });
                        builder.link(evidence, SessionEvidenceRole::Destination);
                        builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                    }
                }
                MinecraftLogEvent::IntegratedServerStarted { version } => {
                    prepare_for_activity(&mut active, evidence, source_end_hint, &mut result);
                    if let Some(builder) = active.as_mut() {
                        if let Some(version) = version {
                            builder.add_version(version);
                            builder.link(evidence, SessionEvidenceRole::Version);
                        }
                        builder
                            .add_destination(SessionDestination::LocalWorld { world_name: None });
                        builder.link(evidence, SessionEvidenceRole::Destination);
                        builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                    }
                }
                MinecraftLogEvent::WorldLoaded { world_name } => {
                    prepare_for_activity(&mut active, evidence, source_end_hint, &mut result);
                    if let Some(builder) = active.as_mut() {
                        builder.add_destination(SessionDestination::LocalWorld {
                            world_name: world_name.clone(),
                        });
                        builder.link(evidence, SessionEvidenceRole::Destination);
                        builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                    }
                }
                MinecraftLogEvent::Disconnected { .. } => {
                    if let Some(builder) = active.as_mut() {
                        builder.link(evidence, SessionEvidenceRole::Supporting);
                        builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                        builder.observe_source_end_hint(source_end_hint);
                    } else {
                        result.diagnostics.orphan_end_evidence += 1;
                    }
                }
                MinecraftLogEvent::Stopping => {
                    if let Some(builder) = active.as_mut() {
                        builder.link(evidence, SessionEvidenceRole::End);
                        builder.link(evidence, SessionEvidenceRole::Exit);
                        builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                        builder.observe_source_end_hint(source_end_hint);
                        if evidence.timestamp.comparable_millis().is_some() {
                            builder.pending_stop.get_or_insert_with(|| EndCandidate {
                                timestamp: evidence.timestamp.clone(),
                                boundary: SessionEndBoundary::ClientStopping,
                                exit_kind: ReconstructedExitKind::Clean,
                            });
                        }
                    } else {
                        result.diagnostics.orphan_end_evidence += 1;
                    }
                }
                MinecraftLogEvent::CleanShutdown => {
                    if let Some(mut builder) = active.take() {
                        if evidence.timestamp.comparable_millis().is_some() {
                            builder.link(evidence, SessionEvidenceRole::End);
                            builder.link(evidence, SessionEvidenceRole::Exit);
                            builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                            finish_session(
                                builder,
                                EndCandidate {
                                    timestamp: evidence.timestamp.clone(),
                                    boundary: SessionEndBoundary::CleanShutdown,
                                    exit_kind: ReconstructedExitKind::Clean,
                                },
                                &mut result,
                            );
                        } else {
                            active = Some(builder);
                        }
                    } else {
                        result.diagnostics.orphan_end_evidence += 1;
                    }
                }
                MinecraftLogEvent::Crash { .. } => {
                    if let Some(mut builder) = active.take() {
                        if evidence.timestamp.comparable_millis().is_some() {
                            builder.link(evidence, SessionEvidenceRole::End);
                            builder.link(evidence, SessionEvidenceRole::Exit);
                            builder.observe_timestamp(&evidence.timestamp, &mut result.diagnostics);
                            finish_session(
                                builder,
                                EndCandidate {
                                    timestamp: evidence.timestamp.clone(),
                                    boundary: SessionEndBoundary::Crash,
                                    exit_kind: ReconstructedExitKind::Crash,
                                },
                                &mut result,
                            );
                        } else {
                            active = Some(builder);
                        }
                    } else {
                        result.diagnostics.orphan_end_evidence += 1;
                    }
                }
            }
        }
        if !current_source_keys.is_empty() {
            previous_source_keys = Some(current_source_keys);
        }
    }

    if let Some(builder) = active.take() {
        let candidate = choose_tail_boundary(&builder, policy).unwrap_or_else(|| EndCandidate {
            timestamp: builder.last_timestamp.clone(),
            boundary: SessionEndBoundary::TruncatedAtLastEvidence,
            exit_kind: ReconstructedExitKind::Unknown,
        });
        finish_session(builder, candidate, &mut result);
    }

    result
}

fn cross_source_evidence_key(evidence: &LogEvidence) -> Option<EvidenceKeyOccurrence> {
    let moment = if let Some(utc) = evidence.timestamp.occurred_at_utc_ms {
        EvidenceMomentKey::Utc(utc)
    } else if let Some(local) = evidence.timestamp.observed_local {
        EvidenceMomentKey::Local(local)
    } else {
        EvidenceMomentKey::Sequence(evidence.timestamp.sequence_millis?)
    };
    Some(EvidenceKeyOccurrence {
        key: CrossSourceEvidenceKey {
            event: evidence.event.clone(),
            moment,
        },
        line_number: evidence.provenance.line_number,
    })
}

/// Returns the exact longest suffix/prefix match in linear time. Requiring a
/// multi-event boundary match avoids treating an isolated same-second marker
/// from a legitimate concurrent launch as copied rotation evidence.
fn suffix_prefix_overlap(
    previous: &[EvidenceKeyOccurrence],
    current: &[EvidenceKeyOccurrence],
) -> usize {
    if previous.is_empty() || current.is_empty() {
        return 0;
    }
    let mut prefix = vec![0_usize; current.len()];
    for index in 1..current.len() {
        let mut matched = prefix[index - 1];
        while matched > 0 && current[index].key != current[matched].key {
            matched = prefix[matched - 1];
        }
        if current[index].key == current[matched].key {
            matched += 1;
        }
        prefix[index] = matched;
    }

    let mut matched = 0_usize;
    for (index, key) in previous.iter().enumerate() {
        while matched > 0 && key.key != current[matched].key {
            matched = prefix[matched - 1];
        }
        if key.key == current[matched].key {
            matched += 1;
        }
        if matched == current.len() && index + 1 < previous.len() {
            matched = prefix[matched - 1];
        }
    }
    matched
}

fn overlap_spans_multiple_lines(
    previous: &[EvidenceKeyOccurrence],
    current: &[EvidenceKeyOccurrence],
    overlap: usize,
) -> bool {
    let previous_start = previous.len().saturating_sub(overlap);
    previous[previous_start..]
        .windows(2)
        .any(|pair| pair[0].line_number != pair[1].line_number)
        && current[..overlap]
            .windows(2)
            .any(|pair| pair[0].line_number != pair[1].line_number)
}

fn prepare_for_activity(
    active: &mut Option<SessionBuilder>,
    evidence: &LogEvidence,
    source_end_hint: Option<DateTime<FixedOffset>>,
    result: &mut ReconstructionResult,
) {
    if active
        .as_ref()
        .is_some_and(|builder| builder.pending_stop.is_some())
    {
        let builder = active.take().expect("checked active builder");
        let candidate = builder.pending_stop.clone().expect("checked pending stop");
        finish_session(builder, candidate, result);
    }

    if active.is_none() {
        if evidence.timestamp.comparable_millis().is_some() {
            *active = Some(SessionBuilder::new(
                evidence,
                SessionStartBoundary::InferredFromActivity,
                source_end_hint,
            ));
        } else {
            result.diagnostics.orphan_activity_without_timestamp += 1;
        }
    } else if let Some(builder) = active.as_mut() {
        builder.observe_source_end_hint(source_end_hint);
    }
}

fn choose_tail_boundary(
    builder: &SessionBuilder,
    policy: &ReconstructionPolicy,
) -> Option<EndCandidate> {
    if let Some(candidate) = builder.pending_stop.clone() {
        return Some(candidate);
    }

    let source_end = builder.last_source_end_hint?;
    let source_end_timestamp = EvidenceTimestamp::from_datetime(source_end);
    let tail_seconds = elapsed_seconds(&builder.last_timestamp, &source_end_timestamp)?;
    if (0..=policy.source_end_tail_limit_seconds).contains(&tail_seconds) {
        return Some(EndCandidate {
            timestamp: source_end_timestamp,
            boundary: SessionEndBoundary::SourceEndHint,
            exit_kind: ReconstructedExitKind::Unknown,
        });
    }

    None
}

fn finish_session(
    builder: SessionBuilder,
    mut candidate: EndCandidate,
    result: &mut ReconstructionResult,
) {
    let duration_millis = match (
        builder.started_at.comparable_millis(),
        candidate.timestamp.comparable_millis(),
    ) {
        (Some(start), Some(end)) if end >= start => end - start,
        (Some(_), Some(_)) => {
            result.diagnostics.non_monotonic_evidence += 1;
            candidate.timestamp = builder.last_timestamp.clone();
            elapsed_millis(&builder.started_at, &candidate.timestamp)
                .unwrap_or(0)
                .max(0)
        }
        _ => 0,
    };
    let duration_seconds = (duration_millis / 1_000).max(0) as u64;
    if duration_seconds == 0 {
        result.diagnostics.zero_duration_sessions += 1;
    }

    let confidence_score = session_confidence(&builder, &candidate, duration_seconds);
    let source_ids: Vec<String> = builder
        .evidence
        .iter()
        .map(|link| link.provenance.source_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    result.sessions.push(ReconstructedSession {
        started_at: builder.started_at,
        ended_at: candidate.timestamp,
        duration_seconds,
        start_boundary: builder.start_boundary,
        end_boundary: candidate.boundary,
        exit_kind: candidate.exit_kind,
        confidence_score,
        confidence_label: Confidence::from_score(confidence_score),
        reconstruction_revision: RECONSTRUCTION_REVISION,
        versions: builder.versions,
        destinations: builder.destinations,
        source_ids,
        evidence: builder.evidence,
    });
}

fn session_confidence(
    builder: &SessionBuilder,
    candidate: &EndCandidate,
    duration_seconds: u64,
) -> u8 {
    let mut score: i16 = match builder.start_boundary {
        SessionStartBoundary::ExplicitStartup => 72,
        SessionStartBoundary::InferredFromActivity => 52,
    };

    score += match candidate.boundary {
        SessionEndBoundary::CleanShutdown => 20,
        SessionEndBoundary::ClientStopping => 16,
        SessionEndBoundary::Crash => 18,
        SessionEndBoundary::SourceEndHint => 5,
        SessionEndBoundary::LastEvidenceBeforeNextStart => 0,
        SessionEndBoundary::TruncatedAtLastEvidence => -4,
    };

    if !builder.versions.is_empty() {
        score += 4;
    }
    if !builder.destinations.is_empty() {
        score += 4;
    }
    if duration_seconds == 0 {
        score -= 20;
    }

    if builder.start_boundary == SessionStartBoundary::InferredFromActivity {
        score = score.min(79);
    }
    score = match candidate.boundary {
        SessionEndBoundary::SourceEndHint => score.min(69),
        SessionEndBoundary::LastEvidenceBeforeNextStart
        | SessionEndBoundary::TruncatedAtLastEvidence => score.min(54),
        SessionEndBoundary::CleanShutdown
        | SessionEndBoundary::ClientStopping
        | SessionEndBoundary::Crash => score,
    };
    if duration_seconds == 0 {
        score = score.min(54);
    }

    score.clamp(0, 100) as u8
}

fn elapsed_seconds(start: &EvidenceTimestamp, end: &EvidenceTimestamp) -> Option<i64> {
    elapsed_millis(start, end).map(|millis| millis / 1_000)
}

fn elapsed_millis(start: &EvidenceTimestamp, end: &EvidenceTimestamp) -> Option<i64> {
    Some(end.comparable_millis()? - start.comparable_millis()?)
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Cursor};

    use chrono::{FixedOffset, NaiveDate, TimeZone};

    use crate::domain::Confidence;

    use super::{
        ReconstructedExitKind, SessionDestination, SessionEndBoundary, SessionEvidenceRole,
        SessionStartBoundary, reconstruct_sessions,
    };
    use crate::parser::minecraft_log::{LogParseContext, parse_minecraft_log};

    const CLEAN_LOG: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/logs/vanilla-clean.log"
    ));
    const CRASH_LOG: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/logs/forge-integrated-crash.log"
    ));
    const ROTATED_OLDER: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/logs/rotated-older.log"
    ));
    const ROTATED_NEWER: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/logs/rotated-newer.log"
    ));

    fn context(source_id: &str, source_order: u32) -> LogParseContext {
        LogParseContext::new(source_id, source_order)
            .with_date_hint(NaiveDate::from_ymd_opt(2026, 8, 6).expect("valid test date"))
            .with_utc_offset(FixedOffset::east_opt(2 * 60 * 60).expect("valid offset"))
    }

    #[test]
    fn reconstructs_clean_session_with_provenance() {
        let parsed = parse_minecraft_log(BufReader::new(CLEAN_LOG), context("clean", 1))
            .expect("fixture parses");
        let result = reconstruct_sessions(&[parsed]);

        assert_eq!(result.sessions.len(), 1);
        let session = &result.sessions[0];
        assert_eq!(session.duration_seconds, 3_538);
        assert_eq!(
            session.start_boundary,
            SessionStartBoundary::ExplicitStartup
        );
        assert_eq!(session.end_boundary, SessionEndBoundary::CleanShutdown);
        assert_eq!(session.exit_kind, ReconstructedExitKind::Clean);
        assert_eq!(session.confidence_label, Confidence::Verified);
        assert_eq!(session.versions, ["1.20.1"]);
        assert!(session.destinations.contains(&SessionDestination::Server {
            address: "play.example.net:25565".to_owned()
        }));
        assert!(session.evidence.iter().any(|link| {
            link.role == SessionEvidenceRole::Start && link.provenance.line_number == 1
        }));
        assert_eq!(session.source_ids, ["clean"]);
    }

    #[test]
    fn reconstructs_crash_session_and_named_world() {
        let parsed = parse_minecraft_log(BufReader::new(CRASH_LOG), context("crash", 1))
            .expect("fixture parses");
        let result = reconstruct_sessions(&[parsed]);

        assert_eq!(result.sessions.len(), 1);
        let session = &result.sessions[0];
        assert_eq!(session.exit_kind, ReconstructedExitKind::Crash);
        assert_eq!(session.end_boundary, SessionEndBoundary::Crash);
        assert!(
            session
                .destinations
                .contains(&SessionDestination::LocalWorld {
                    world_name: Some("Redstone Lab".to_owned())
                })
        );
        assert_eq!(result.diagnostics.orphan_end_evidence, 1);
    }

    #[test]
    fn rotated_input_order_does_not_change_reconstruction() {
        let older = parse_minecraft_log(BufReader::new(ROTATED_OLDER), context("older", 1))
            .expect("older fixture parses");
        let newer = parse_minecraft_log(BufReader::new(ROTATED_NEWER), context("newer", 2))
            .expect("newer fixture parses");

        let chronological = reconstruct_sessions(&[older.clone(), newer.clone()]);
        let reversed = reconstruct_sessions(&[newer, older]);

        assert_eq!(chronological, reversed);
        assert_eq!(chronological.sessions.len(), 2);
        assert_eq!(
            chronological.sessions[0].end_boundary,
            SessionEndBoundary::LastEvidenceBeforeNextStart
        );
        assert_eq!(chronological.sessions[0].duration_seconds, 47 * 60);
        assert_eq!(
            chronological.sessions[1].exit_kind,
            ReconstructedExitKind::Clean
        );
    }

    #[test]
    fn truncated_tail_uses_nearby_source_end_hint() {
        let offset = FixedOffset::east_opt(2 * 60 * 60).expect("valid offset");
        let end_hint = offset
            .with_ymd_and_hms(2026, 8, 6, 10, 30, 30)
            .single()
            .expect("valid end hint");
        let bytes = b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[10:30:00] [Render thread/INFO]: Connecting to example.net, 25565";
        let parsed = parse_minecraft_log(
            Cursor::new(bytes),
            context("truncated", 1).with_source_end_hint(end_hint),
        )
        .expect("truncated stream parses");

        let result = reconstruct_sessions(&[parsed]);
        assert_eq!(result.sessions.len(), 1);
        assert_eq!(
            result.sessions[0].end_boundary,
            SessionEndBoundary::SourceEndHint
        );
        assert_eq!(result.sessions[0].duration_seconds, 30 * 60 + 30);
        assert_eq!(result.sessions[0].exit_kind, ReconstructedExitKind::Unknown);
    }

    #[test]
    fn duplicate_rotations_do_not_duplicate_sessions() {
        let content_hash = *blake3::hash(CLEAN_LOG).as_bytes();
        let first = parse_minecraft_log(
            BufReader::new(CLEAN_LOG),
            context("copy-a", 1).with_source_content_hash(content_hash),
        )
        .expect("first fixture parses");
        let second = parse_minecraft_log(
            BufReader::new(CLEAN_LOG),
            context("copy-b", 2).with_source_content_hash(content_hash),
        )
        .expect("second fixture parses");

        let result = reconstruct_sessions(&[first, second]);
        assert_eq!(result.sessions.len(), 1);
        assert!(result.diagnostics.duplicate_evidence > 0);
    }

    #[test]
    fn same_second_events_from_distinct_source_content_are_not_deduplicated() {
        let first_bytes = b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[10:00:01] [Render thread/INFO]: Connecting to first.example.net, 25565\n\
[10:05:00] [Render thread/INFO]: Stopping!\n";
        let second_bytes = b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[10:00:01] [Render thread/INFO]: Connecting to second.example.net, 25565\n\
[10:06:00] [Render thread/INFO]: Stopping!\n";
        let first = parse_minecraft_log(
            Cursor::new(first_bytes),
            context("concurrent-a", 1)
                .with_source_content_hash(*blake3::hash(first_bytes).as_bytes()),
        )
        .expect("first concurrent source");
        let second = parse_minecraft_log(
            Cursor::new(second_bytes),
            context("concurrent-b", 2)
                .with_source_content_hash(*blake3::hash(second_bytes).as_bytes()),
        )
        .expect("second concurrent source");

        let result = reconstruct_sessions(&[first, second]);
        assert_eq!(result.sessions.len(), 2);
        assert_eq!(result.diagnostics.duplicate_evidence, 0);
    }

    #[test]
    fn partial_rotation_overlap_deduplicates_a_multi_line_suffix_prefix() {
        let older_bytes = b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[10:05:00] [Render thread/INFO]: Connecting to overlap.example.net, 25565\n\
[10:06:00] [Render thread/INFO]: Disconnected from server\n";
        let newer_bytes =
            b"[10:05:00] [Render thread/INFO]: Connecting to overlap.example.net, 25565\n\
[10:06:00] [Render thread/INFO]: Disconnected from server\n\
[10:30:00] [Render thread/INFO]: Stopping!\n\
[10:30:01] [Render thread/INFO]: Stopping worker threads\n";
        let older = parse_minecraft_log(
            Cursor::new(older_bytes),
            context("overlap-older", 1)
                .with_source_content_hash(*blake3::hash(older_bytes).as_bytes()),
        )
        .expect("older overlap source");
        let newer = parse_minecraft_log(
            Cursor::new(newer_bytes),
            context("overlap-newer", 2)
                .with_source_content_hash(*blake3::hash(newer_bytes).as_bytes()),
        )
        .expect("newer overlap source");

        let result = reconstruct_sessions(&[older, newer]);
        assert_eq!(result.sessions.len(), 1);
        assert_eq!(result.diagnostics.duplicate_evidence, 2);
        assert_eq!(
            result.sessions[0].destinations,
            [SessionDestination::Server {
                address: "overlap.example.net:25565".to_owned()
            }]
        );
        assert_eq!(
            result.sessions[0].source_ids,
            ["overlap-newer", "overlap-older"]
        );
    }

    #[test]
    fn isolated_identical_same_second_launch_markers_remain_distinct() {
        let bytes = b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n";
        let first = parse_minecraft_log(
            Cursor::new(bytes),
            context("launch-a", 1).with_source_content_hash(*blake3::hash(b"source-a").as_bytes()),
        )
        .expect("first launch");
        let second = parse_minecraft_log(
            Cursor::new(bytes),
            context("launch-b", 2).with_source_content_hash(*blake3::hash(b"source-b").as_bytes()),
        )
        .expect("second launch");

        let result = reconstruct_sessions(&[first, second]);
        assert_eq!(result.sessions.len(), 2);
        assert_eq!(result.diagnostics.duplicate_evidence, 0);
    }

    #[test]
    fn single_start_is_still_a_bounded_zero_duration_partial_session() {
        let parsed = parse_minecraft_log(
            Cursor::new(b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader"),
            context("one-line", 1),
        )
        .expect("one-line stream parses");

        let result = reconstruct_sessions(&[parsed]);
        assert_eq!(result.sessions.len(), 1);
        assert_eq!(result.sessions[0].duration_seconds, 0);
        assert_eq!(
            result.sessions[0].end_boundary,
            SessionEndBoundary::TruncatedAtLastEvidence
        );
        assert_eq!(result.sessions[0].confidence_label, Confidence::Partial);
        assert_eq!(result.diagnostics.zero_duration_sessions, 1);
    }

    #[test]
    fn quick_restart_after_stopping_is_not_merged_into_bootstrap() {
        let bytes = b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[10:00:20] [Render thread/INFO]: Stopping!\n\
[10:00:40] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[10:01:00] [Render thread/INFO]: Stopping!\n";
        let parsed = parse_minecraft_log(Cursor::new(bytes), context("restart", 1))
            .expect("restart stream parses");

        let result = reconstruct_sessions(&[parsed]);
        assert_eq!(result.sessions.len(), 2);
        assert!(
            result
                .sessions
                .iter()
                .all(|session| session.end_boundary == SessionEndBoundary::ClientStopping)
        );
    }
}
