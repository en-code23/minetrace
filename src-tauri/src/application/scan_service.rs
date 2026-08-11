use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    io::{self, BufReader, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use chrono::{FixedOffset, Local, NaiveDate, TimeZone, Utc};
use flate2::read::GzDecoder;

use super::{
    DiscoveryService,
    scan_models::{
        MAX_SCAN_ISSUES, ScanIssue, ScanIssueSeverity, ScanMode, ScanPhase, ScanState, ScanStatus,
    },
};
use crate::{
    domain::{Confidence, location::DiscoveredInstallation},
    error::BackendError,
    parser::{
        EvidenceProvenance, EvidenceTag, LogParseContext, LogParseLimits, MinecraftLogEvent,
        ParsedLog, ReconstructedExitKind, ReconstructedSession, SessionDestination,
        SessionEndBoundary, SessionEvidenceRole, TimestampOrigin, is_log_parse_limit_error,
        parse_minecraft_log_with_control, reconstruct_sessions,
    },
    platform::native_path_key,
    scan::{
        FileDecision, FingerprintOptions, FingerprintedLog, InventoryOptions, InventoryWarningKind,
        LogCandidate, LogFileKind, ParserStamp, RollbackKind,
        ScanMessageSeverity as StoredScanMessageSeverity, ScanMode as StorageScanMode,
        SourceParseStatus, StagedSource, create_verified_file_snapshot_with_control,
        fingerprint_log_with_previous_size_and_control, inventory_logs_with_control,
        open_log_read_only_no_follow,
    },
    storage::{
        Database, ReconstructionPayload, StagedActivity, StagedEvidence, StagedEvidenceLink,
        StagedSession, StoredInstance,
    },
};

const PARSER_NAME: &str = "minecraft-java-log";
const PARSER_REVISION: u32 = 3;
const MAX_INSTANCE_LOG_FILES: usize = 4_096;
const MAX_INSTANCE_DECOMPRESSED_BYTES: u64 = 512 * 1024 * 1024;
const MAX_INSTANCE_EVIDENCE_EVENTS: u64 = 500_000;
const MAX_INSTANCE_CANONICAL_SESSIONS: usize = 100_000;
const MAX_SESSION_DESTINATIONS: usize = 64;
const MAX_SESSION_VERSIONS: usize = 64;
const MAX_SESSION_EVIDENCE_LINKS: usize = 100_000;
const MAX_DESTINATION_UTF8_BYTES: usize = 512;
const MAX_VERSION_UTF8_BYTES: usize = 256;
const MAX_SCAN_LOG_FILES: usize = 16_384;
const MAX_SCAN_DECOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_SCAN_EVIDENCE_EVENTS: u64 = 1_000_000;
const MAX_SCAN_RECONSTRUCTED_SESSIONS: usize = 200_000;
const MAX_SCAN_CONTEXTS: usize = 500_000;
const MAX_GLOBAL_CANONICAL_SESSIONS: usize = 250_000;
const MAX_GLOBAL_CANONICAL_CONTEXTS: usize = 1_000_000;

#[derive(Debug, Clone, Copy)]
struct InstanceParseLimits {
    max_log_files: usize,
    max_decompressed_bytes: u64,
    max_evidence_events: u64,
    max_reconstructed_sessions: usize,
    max_destinations_per_session: usize,
    max_versions_per_session: usize,
    max_evidence_links_per_session: usize,
    max_destination_utf8_bytes: usize,
    max_version_utf8_bytes: usize,
}

impl Default for InstanceParseLimits {
    fn default() -> Self {
        Self {
            max_log_files: MAX_INSTANCE_LOG_FILES,
            max_decompressed_bytes: MAX_INSTANCE_DECOMPRESSED_BYTES,
            max_evidence_events: MAX_INSTANCE_EVIDENCE_EVENTS,
            max_reconstructed_sessions: MAX_INSTANCE_CANONICAL_SESSIONS,
            max_destinations_per_session: MAX_SESSION_DESTINATIONS,
            max_versions_per_session: MAX_SESSION_VERSIONS,
            max_evidence_links_per_session: MAX_SESSION_EVIDENCE_LINKS,
            max_destination_utf8_bytes: MAX_DESTINATION_UTF8_BYTES,
            max_version_utf8_bytes: MAX_VERSION_UTF8_BYTES,
        }
    }
}

#[derive(Debug, Default)]
struct InstanceParseBudget {
    retained_logs: usize,
    decompressed_bytes: u64,
    evidence_events: u64,
}

#[derive(Debug, Clone, Copy)]
struct ScanParseLimits {
    max_log_files: usize,
    max_decompressed_bytes: u64,
    max_evidence_events: u64,
    max_reconstructed_sessions: usize,
    max_contexts: usize,
}

impl Default for ScanParseLimits {
    fn default() -> Self {
        Self {
            max_log_files: MAX_SCAN_LOG_FILES,
            max_decompressed_bytes: MAX_SCAN_DECOMPRESSED_BYTES,
            max_evidence_events: MAX_SCAN_EVIDENCE_EVENTS,
            max_reconstructed_sessions: MAX_SCAN_RECONSTRUCTED_SESSIONS,
            max_contexts: MAX_SCAN_CONTEXTS,
        }
    }
}

#[derive(Debug, Default)]
struct ScanParseBudget {
    log_files: usize,
    decompressed_bytes: u64,
    evidence_events: u64,
    reconstructed_sessions: usize,
    contexts: usize,
}

impl ScanParseBudget {
    fn retain_log(
        &mut self,
        decompressed_bytes: u64,
        evidence_events: u64,
        limits: &ScanParseLimits,
    ) -> Result<(), &'static str> {
        let log_files = self.log_files.checked_add(1).ok_or("scan log file count")?;
        let decompressed_bytes = self
            .decompressed_bytes
            .checked_add(decompressed_bytes)
            .ok_or("scan decompressed byte count")?;
        let evidence_events = self
            .evidence_events
            .checked_add(evidence_events)
            .ok_or("scan evidence event count")?;
        if log_files > limits.max_log_files {
            return Err("scan log file count");
        }
        if decompressed_bytes > limits.max_decompressed_bytes {
            return Err("scan decompressed byte count");
        }
        if evidence_events > limits.max_evidence_events {
            return Err("scan evidence event count");
        }
        self.log_files = log_files;
        self.decompressed_bytes = decompressed_bytes;
        self.evidence_events = evidence_events;
        Ok(())
    }

    fn charge_failed_parse(&mut self, limits: &ScanParseLimits) -> Result<(), &'static str> {
        self.retain_log(
            LogParseLimits::default().max_decompressed_bytes,
            LogParseLimits::default().max_evidence_events,
            limits,
        )
    }

    fn retain_reconstruction(
        &mut self,
        sessions: usize,
        contexts: usize,
        limits: &ScanParseLimits,
    ) -> Result<(), &'static str> {
        let sessions = self
            .reconstructed_sessions
            .checked_add(sessions)
            .ok_or("scan reconstructed session count")?;
        let contexts = self
            .contexts
            .checked_add(contexts)
            .ok_or("scan context count")?;
        if sessions > limits.max_reconstructed_sessions {
            return Err("scan reconstructed session count");
        }
        if contexts > limits.max_contexts {
            return Err("scan context count");
        }
        self.reconstructed_sessions = sessions;
        self.contexts = contexts;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct CanonicalArchiveLimits {
    max_sessions: usize,
    max_contexts: usize,
}

impl Default for CanonicalArchiveLimits {
    fn default() -> Self {
        Self {
            max_sessions: MAX_GLOBAL_CANONICAL_SESSIONS,
            max_contexts: MAX_GLOBAL_CANONICAL_CONTEXTS,
        }
    }
}

#[derive(Debug, Default)]
struct CanonicalArchiveBudget {
    session_ids: HashSet<String>,
    session_ids_by_instance: HashMap<String, HashSet<String>>,
    contexts: usize,
    contexts_by_instance: HashMap<String, usize>,
    already_over_limit: bool,
}

impl CanonicalArchiveBudget {
    fn load(database: &Database, limits: &CanonicalArchiveLimits) -> Result<Self, BackendError> {
        let (session_count, context_count) = database.read(|connection| {
            Ok((
                connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| {
                    row.get::<_, i64>(0)
                })?,
                connection.query_row("SELECT COUNT(*) FROM activity_segments", [], |row| {
                    row.get::<_, i64>(0)
                })?,
            ))
        })?;
        let session_count = usize::try_from(session_count.max(0)).unwrap_or(usize::MAX);
        let context_count = usize::try_from(context_count.max(0)).unwrap_or(usize::MAX);
        if session_count > limits.max_sessions || context_count > limits.max_contexts {
            return Ok(Self {
                contexts: context_count,
                already_over_limit: true,
                ..Self::default()
            });
        }

        let (session_rows, context_rows) = database.read(|connection| {
            let session_rows = {
                let mut statement =
                    connection.prepare("SELECT id, instance_id FROM sessions ORDER BY id")?;
                statement
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let context_rows = {
                let mut statement = connection.prepare(
                    "SELECT session.instance_id, COUNT(activity.id)
                     FROM sessions session
                     LEFT JOIN activity_segments activity ON activity.session_id = session.id
                     GROUP BY session.instance_id",
                )?;
                statement
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            Ok((session_rows, context_rows))
        })?;

        let mut budget = Self {
            contexts: context_count,
            ..Self::default()
        };
        for (session_id, instance_id) in session_rows {
            budget.session_ids.insert(session_id.clone());
            budget
                .session_ids_by_instance
                .entry(instance_id)
                .or_default()
                .insert(session_id);
        }
        for (instance_id, contexts) in context_rows {
            budget.contexts_by_instance.insert(
                instance_id,
                usize::try_from(contexts.max(0)).unwrap_or(usize::MAX),
            );
        }
        Ok(budget)
    }

    fn retain_payload(
        &mut self,
        payload: &ReconstructionPayload,
        limits: &CanonicalArchiveLimits,
    ) -> Result<(), &'static str> {
        if self.already_over_limit {
            return Err("global canonical archive usage");
        }
        let incoming_ids = payload
            .sessions
            .iter()
            .map(|session| session.id.as_str())
            .collect::<HashSet<_>>();
        let removed_ids = if payload.replace_all_instance_evidence {
            self.session_ids_by_instance
                .get(&payload.instance_id)
                .map_or(0, HashSet::len)
        } else {
            0
        };
        let retained_sessions = self.session_ids.len().saturating_sub(removed_ids);
        let new_sessions = incoming_ids
            .iter()
            .filter(|session_id| {
                !self.session_ids.contains(**session_id)
                    || (payload.replace_all_instance_evidence
                        && self
                            .session_ids_by_instance
                            .get(&payload.instance_id)
                            .is_some_and(|ids| ids.contains(**session_id)))
            })
            .count();
        let sessions_after = retained_sessions
            .checked_add(new_sessions)
            .ok_or("global canonical session count")?;

        let removed_contexts = if payload.replace_all_instance_evidence {
            self.contexts_by_instance
                .get(&payload.instance_id)
                .copied()
                .unwrap_or_default()
        } else {
            0
        };
        let incoming_contexts = payload
            .sessions
            .iter()
            .try_fold(0_usize, |count, session| {
                count.checked_add(session.activities.len())
            })
            .ok_or("global canonical context count")?;
        let contexts_after = self
            .contexts
            .saturating_sub(removed_contexts)
            .checked_add(incoming_contexts)
            .ok_or("global canonical context count")?;
        if sessions_after > limits.max_sessions {
            return Err("global canonical session count");
        }
        if contexts_after > limits.max_contexts {
            return Err("global canonical context count");
        }

        if payload.replace_all_instance_evidence
            && let Some(removed) = self.session_ids_by_instance.remove(&payload.instance_id)
        {
            for session_id in removed {
                self.session_ids.remove(&session_id);
            }
        }
        let instance_ids = self
            .session_ids_by_instance
            .entry(payload.instance_id.clone())
            .or_default();
        for session_id in incoming_ids {
            let session_id = session_id.to_owned();
            self.session_ids.insert(session_id.clone());
            instance_ids.insert(session_id);
        }
        self.contexts = contexts_after;
        self.contexts_by_instance.insert(
            payload.instance_id.clone(),
            if payload.replace_all_instance_evidence {
                incoming_contexts
            } else {
                self.contexts_by_instance
                    .get(&payload.instance_id)
                    .copied()
                    .unwrap_or_default()
                    .saturating_add(incoming_contexts)
            },
        );
        Ok(())
    }
}

impl InstanceParseBudget {
    fn validate_log_count(count: usize, limits: &InstanceParseLimits) -> Result<(), &'static str> {
        if count > limits.max_log_files {
            Err("log file count")
        } else {
            Ok(())
        }
    }

    fn retain(
        &mut self,
        decoded_bytes: u64,
        evidence_events: u64,
        limits: &InstanceParseLimits,
    ) -> Result<(), &'static str> {
        let retained_logs = self
            .retained_logs
            .checked_add(1)
            .ok_or("retained log count")?;
        let decompressed_bytes = self
            .decompressed_bytes
            .checked_add(decoded_bytes)
            .ok_or("decompressed byte count")?;
        let evidence_events = self
            .evidence_events
            .checked_add(evidence_events)
            .ok_or("evidence event count")?;
        if retained_logs > limits.max_log_files {
            return Err("retained log count");
        }
        if decompressed_bytes > limits.max_decompressed_bytes {
            return Err("decompressed byte count");
        }
        if evidence_events > limits.max_evidence_events {
            return Err("evidence event count");
        }
        self.retained_logs = retained_logs;
        self.decompressed_bytes = decompressed_bytes;
        self.evidence_events = evidence_events;
        Ok(())
    }

    fn validate_reconstruction(
        sessions: &[ReconstructedSession],
        limits: &InstanceParseLimits,
    ) -> Result<(), &'static str> {
        if sessions.len() > limits.max_reconstructed_sessions {
            return Err("reconstructed session count");
        }
        for session in sessions {
            if session.destinations.len() > limits.max_destinations_per_session {
                return Err("destinations per session");
            }
            if session.versions.len() > limits.max_versions_per_session {
                return Err("versions per session");
            }
            if session.evidence.len() > limits.max_evidence_links_per_session {
                return Err("evidence links per session");
            }
            if session
                .destinations
                .iter()
                .any(|destination| match destination {
                    SessionDestination::Server { address } => {
                        address.len() > limits.max_destination_utf8_bytes
                    }
                    SessionDestination::LocalWorld { world_name } => world_name
                        .as_ref()
                        .is_some_and(|name| name.len() > limits.max_destination_utf8_bytes),
                })
            {
                return Err("destination string bytes");
            }
            if session
                .versions
                .iter()
                .any(|version| version.len() > limits.max_version_utf8_bytes)
            {
                return Err("version string bytes");
            }
        }
        Ok(())
    }

    fn validate_parsed_payload(
        log: &ParsedLog,
        limits: &InstanceParseLimits,
    ) -> Result<(), &'static str> {
        for evidence in &log.evidence {
            match &evidence.event {
                MinecraftLogEvent::ServerJoined { address }
                    if address.len() > limits.max_destination_utf8_bytes =>
                {
                    return Err("destination string bytes");
                }
                MinecraftLogEvent::WorldLoaded {
                    world_name: Some(world_name),
                } if world_name.len() > limits.max_destination_utf8_bytes => {
                    return Err("destination string bytes");
                }
                MinecraftLogEvent::VersionObserved { version }
                    if version.len() > limits.max_version_utf8_bytes =>
                {
                    return Err("version string bytes");
                }
                MinecraftLogEvent::IntegratedServerStarted {
                    version: Some(version),
                } if version.len() > limits.max_version_utf8_bytes => {
                    return Err("version string bytes");
                }
                _ => {}
            }
        }
        Ok(())
    }
}
const MAX_PLAUSIBLE_SESSION_SECONDS: u64 = 31 * 24 * 60 * 60;

#[derive(Clone)]
pub struct ScanService {
    database: Database,
    discovery: Arc<DiscoveryService>,
    runtime: Arc<ScanRuntime>,
}

struct ScanRuntime {
    status: Mutex<ScanStatus>,
    cancellation: Mutex<Option<Arc<AtomicBool>>>,
}

struct InstanceGroup {
    relative_root: PathBuf,
    name: String,
    candidates: Vec<LogCandidate>,
}

struct PreparedInstance {
    stored: StoredInstance,
    files: Vec<FingerprintedLog>,
    staged: Vec<StagedSource>,
    replace_all: bool,
    should_reconstruct: bool,
}

#[derive(Debug)]
enum WorkerError {
    Cancelled,
    Failed(BackendError),
}

#[derive(Debug)]
enum SourceParseError {
    Cancelled,
    SnapshotChanged(String),
    Io(io::Error),
}

impl From<BackendError> for WorkerError {
    fn from(value: BackendError) -> Self {
        Self::Failed(value)
    }
}

impl ScanService {
    pub fn new(database: Database, discovery: Arc<DiscoveryService>) -> Self {
        Self {
            database,
            discovery,
            runtime: Arc::new(ScanRuntime {
                status: Mutex::new(ScanStatus::idle()),
                cancellation: Mutex::new(None),
            }),
        }
    }

    pub fn start(&self, mode: ScanMode) -> Result<ScanStatus, BackendError> {
        {
            let current = self.lock_status()?;
            if !current.state.is_terminal() {
                return Ok(current.clone());
            }
        }

        let run = self.database.begin_scan(storage_mode(mode))?;
        let queued = ScanStatus::queued(run.id.clone(), mode);
        let cancellation = Arc::new(AtomicBool::new(false));
        *self.lock_status()? = queued.clone();
        *self.lock_cancellation()? = Some(cancellation.clone());

        let service = self.clone();
        let scan_id = run.id.clone();
        let spawn_result = thread::Builder::new()
            .name(format!(
                "minetrace-scan-{}",
                &scan_id[scan_id.len().saturating_sub(8)..]
            ))
            .spawn(move || service.finish_worker(scan_id, mode, cancellation));
        if let Err(error) = spawn_result {
            let _ = self.record_issue(
                &run.id,
                ScanIssueSeverity::Error,
                "worker_spawn_failed",
                None,
                "The local scan worker could not start.",
            );
            let _ = self.persist_status_snapshot(&run.id);
            let _ = self.database.rollback_scan(
                &run.id,
                RollbackKind::Failed {
                    error_code: "worker_spawn_failed".to_owned(),
                },
            );
            *self.lock_cancellation()? = None;
            let mut status = self.lock_status()?;
            status.state = ScanState::Failed;
            status.phase = ScanPhase::Failed;
            status.errors = status.errors.max(1);
            status.message = Some("The local scan worker could not start.".to_owned());
            status.finished_at = Some(Utc::now().to_rfc3339());
            return Err(BackendError::BackgroundTask(format!(
                "start scan worker: {error}"
            )));
        }

        Ok(queued)
    }

    pub fn status(&self) -> Result<ScanStatus, BackendError> {
        let current = { self.lock_status()?.clone() };
        if current.phase != ScanPhase::Idle || !current.id.is_empty() {
            return Ok(current);
        }

        let Some(snapshot) = self.database.latest_terminal_scan()? else {
            return Ok(current);
        };
        let restored = ScanStatus::from_durable(snapshot);
        *self.lock_status()? = restored.clone();
        Ok(restored)
    }

    pub fn cancel(&self) -> Result<ScanStatus, BackendError> {
        if let Some(cancellation) = self.lock_cancellation()?.as_ref() {
            cancellation.store(true, Ordering::Release);
        } else {
            return self.status();
        }

        for _ in 0..100 {
            let status = self.status()?;
            if status.state.is_terminal() {
                return Ok(status);
            }
            thread::sleep(Duration::from_millis(20));
        }

        let mut status = self.lock_status()?;
        status.message =
            Some("Cancellation requested; waiting for the current read to stop safely.".to_owned());
        Ok(status.clone())
    }

    fn finish_worker(&self, scan_id: String, mode: ScanMode, cancellation: Arc<AtomicBool>) {
        let outcome = self.run_worker(&scan_id, mode, &cancellation);
        match outcome {
            Ok(dataset_revision) => {
                if let Ok(mut status) = self.lock_status() {
                    status.state = ScanState::Completed;
                    status.phase = ScanPhase::Complete;
                    status.current = status.total;
                    status.current_path = None;
                    status.message = Some(
                        "The local archive was updated from source-backed session evidence."
                            .to_owned(),
                    );
                    status.finished_at = Some(Utc::now().to_rfc3339());
                    status.dataset_revision = Some(dataset_revision.max(0) as u64);
                }
            }
            Err(WorkerError::Cancelled) => {
                let _ = self.persist_status_snapshot(&scan_id);
                let _ = self
                    .database
                    .rollback_scan(&scan_id, RollbackKind::Cancelled);
                if let Ok(mut status) = self.lock_status() {
                    status.state = ScanState::Cancelled;
                    status.phase = ScanPhase::Cancelled;
                    status.current_path = None;
                    status.message = Some(
                        "Cancellation confirmed before promotion; the prior archive remains unchanged."
                            .to_owned(),
                    );
                    status.finished_at = Some(Utc::now().to_rfc3339());
                }
            }
            Err(WorkerError::Failed(error)) => {
                let message = user_facing_scan_error(&error);
                let _ = self.record_issue(
                    &scan_id,
                    ScanIssueSeverity::Error,
                    "scan_failed",
                    None,
                    &message,
                );
                let _ = self.persist_status_snapshot(&scan_id);
                let _ = self.database.rollback_scan(
                    &scan_id,
                    RollbackKind::Failed {
                        error_code: "scan_failed".to_owned(),
                    },
                );
                if let Ok(mut status) = self.lock_status() {
                    status.state = ScanState::Failed;
                    status.phase = ScanPhase::Failed;
                    status.current_path = None;
                    status.message = Some(message);
                    status.finished_at = Some(Utc::now().to_rfc3339());
                }
            }
        }
        if let Ok(mut slot) = self.lock_cancellation() {
            *slot = None;
        }
    }

    fn run_worker(
        &self,
        scan_id: &str,
        mode: ScanMode,
        cancellation: &AtomicBool,
    ) -> Result<i64, WorkerError> {
        self.update_phase(
            scan_id,
            ScanPhase::Discovering,
            0,
            0,
            None,
            "Resolving approved launcher and instance roots.",
        )?;
        check_cancelled(cancellation)?;

        let discovered = self
            .discovery
            .discover_with_control(|| cancellation.load(Ordering::Acquire));
        check_cancelled(cancellation)?;
        let locations = discovered?;
        let enabled = locations
            .into_iter()
            .filter(|location| location.enabled)
            .collect::<Vec<_>>();
        if enabled.is_empty() {
            return Err(WorkerError::Failed(BackendError::BackgroundTask(
                "No enabled Minecraft locations are available to scan.".to_owned(),
            )));
        }
        self.database.upsert_scan_locations(&enabled)?;

        self.update_phase(
            scan_id,
            ScanPhase::Indexing,
            0,
            0,
            None,
            "Inventorying log files and comparing full-file fingerprints.",
        )?;

        let parser_stamp = ParserStamp::new(PARSER_NAME, PARSER_REVISION);
        let mut prepared = Vec::new();
        let mut indexed = 0_u64;
        let mut total = 0_u64;
        let mut readable_locations = 0_usize;
        for location in &enabled {
            check_cancelled(cancellation)?;
            let report = match inventory_logs_with_control(
                &location.path,
                &InventoryOptions::default(),
                || cancellation.load(Ordering::Acquire),
            ) {
                Ok(report) => report,
                Err(crate::scan::ScanError::Cancelled) => {
                    return Err(WorkerError::Cancelled);
                }
                Err(error) => {
                    let message =
                        "An approved location became unavailable and was skipped during inventory.";
                    self.record_issue(
                        scan_id,
                        ScanIssueSeverity::Error,
                        "location_inventory_failed",
                        Some(&location.name),
                        message,
                    )?;
                    self.set_message(Some(message.to_owned()))?;
                    let _ = error;
                    continue;
                }
            };
            readable_locations += 1;
            for warning in &report.warnings {
                let (code, message) = match warning.kind {
                    InventoryWarningKind::SymlinkSkipped => (
                        "inventory_symlink_skipped",
                        "A symbolic link was skipped because MineTrace does not follow links.",
                    ),
                    InventoryWarningKind::DepthLimitReached => (
                        "inventory_depth_limit_reached",
                        "A nested folder was skipped after the safe inventory depth was reached.",
                    ),
                    InventoryWarningKind::FileTooLarge => (
                        "inventory_file_too_large",
                        "A log was skipped because it exceeds the safe source-file size limit.",
                    ),
                    InventoryWarningKind::UnreadableEntry => (
                        "inventory_entry_unreadable",
                        "A folder entry could not be read and was skipped.",
                    ),
                };
                let label = redacted_path_label(&warning.path);
                self.record_issue(
                    scan_id,
                    ScanIssueSeverity::Warning,
                    code,
                    label.as_deref(),
                    message,
                )?;
            }
            let mut groups = group_candidates(location, report.candidates, mode);
            if mode != ScanMode::Quick {
                let observed_roots = groups
                    .iter()
                    .map(|group| native_path_key(&group.relative_root))
                    .collect::<BTreeSet<_>>();
                for stored in self.database.list_instances_for_location(&location.id)? {
                    if !observed_roots.contains(&native_path_key(&stored.relative_root)) {
                        groups.push(InstanceGroup {
                            relative_root: stored.relative_root,
                            name: stored.name,
                            candidates: Vec::new(),
                        });
                    }
                }
            }
            total = total.saturating_add(
                groups
                    .iter()
                    .map(|group| group.candidates.len() as u64)
                    .sum::<u64>(),
            );
            self.set_total(total)?;

            for group in groups {
                check_cancelled(cancellation)?;
                let stored =
                    self.database
                        .upsert_instance(location, &group.relative_root, &group.name)?;
                let mut files = Vec::with_capacity(group.candidates.len());
                for candidate in group.candidates {
                    check_cancelled(cancellation)?;
                    self.set_current_path(Some(display_relative(&candidate.relative_path)))?;
                    let previous_size = self
                        .database
                        .current_source_size(&location.id, &candidate.relative_path_key)?;
                    match fingerprint_log_with_previous_size_and_control(
                        &candidate,
                        &FingerprintOptions::default(),
                        previous_size,
                        || cancellation.load(Ordering::Acquire),
                    ) {
                        Ok(fingerprint) => files.push(FingerprintedLog {
                            candidate,
                            fingerprint,
                        }),
                        Err(crate::scan::ScanError::Cancelled) => {
                            return Err(WorkerError::Cancelled);
                        }
                        Err(error) => {
                            let message = "A log changed or became unreadable while it was indexed and was skipped.";
                            let label = display_relative(&candidate.relative_path);
                            self.record_issue(
                                scan_id,
                                ScanIssueSeverity::Error,
                                "log_fingerprint_failed",
                                Some(&label),
                                message,
                            )?;
                            self.set_message(Some(message.to_owned()))?;
                            let _ = error;
                        }
                    }
                    indexed = indexed.saturating_add(1);
                    self.set_progress(indexed, total)?;
                }

                let complete_scope = mode != ScanMode::Quick;
                let scope_key = format!(
                    "{}:{}",
                    if complete_scope { "complete" } else { "quick" },
                    stored.id
                );
                let staged = self.database.stage_inventory(
                    scan_id,
                    &location.id,
                    Some(&stored.id),
                    &scope_key,
                    &files,
                    &parser_stamp,
                )?;
                let changed = staged
                    .sources
                    .iter()
                    .any(|source| source.decision.requires_parse());
                let has_missing_sources = !staged.missing_source_instance_ids.is_empty();
                let has_replaced_source = staged
                    .sources
                    .iter()
                    .any(|source| source.decision == FileDecision::Replaced);
                let has_replacement_history = self
                    .database
                    .instance_has_unreconciled_replacement_history(&stored.id)?;
                let should_reconstruct = match mode {
                    ScanMode::Quick | ScanMode::Standard => changed,
                    ScanMode::Deep => !files.is_empty(),
                };
                prepared.push(PreparedInstance {
                    stored,
                    files,
                    staged: staged.sources,
                    replace_all: complete_scope
                        && !has_missing_sources
                        && !has_replaced_source
                        && !has_replacement_history,
                    should_reconstruct,
                });
            }
        }

        if readable_locations == 0 {
            return Err(WorkerError::Failed(BackendError::BackgroundTask(
                "No approved Minecraft location remained readable when scanning began.".to_owned(),
            )));
        }

        self.update_phase(
            scan_id,
            ScanPhase::Parsing,
            0,
            prepared
                .iter()
                .filter(|instance| instance.should_reconstruct)
                .map(|instance| instance.files.len() as u64)
                .sum(),
            None,
            "Streaming log evidence and reconstructing session boundaries.",
        )?;

        let mut parsed_count = 0_u64;
        let parse_total = self.status()?.total;
        let scan_limits = ScanParseLimits::default();
        let mut scan_budget = ScanParseBudget::default();
        let archive_limits = CanonicalArchiveLimits::default();
        let mut archive_budget = CanonicalArchiveBudget::load(&self.database, &archive_limits)?;
        let mut scan_exhaustion: Option<(&'static str, &'static str)> = None;
        for instance in prepared {
            check_cancelled(cancellation)?;
            for source in &instance.staged {
                if !instance.should_reconstruct || source.decision == FileDecision::Unchanged {
                    self.database.mark_source_parse(
                        scan_id,
                        &source.source_revision_id,
                        SourceParseStatus::Parsed,
                        None,
                    )?;
                }
            }
            if !instance.should_reconstruct {
                continue;
            }
            if let Some((resource, error_code)) = scan_exhaustion {
                self.defer_changed_sources(scan_id, &instance.staged, error_code)?;
                self.set_message(Some(format!(
                    "The remaining instances were deferred after the safe {resource} limit was reached; their previous sessions were preserved."
                )))?;
                continue;
            }

            let limits = InstanceParseLimits::default();
            if let Err(resource) =
                InstanceParseBudget::validate_log_count(instance.files.len(), &limits)
            {
                let message = format!(
                    "This instance exceeded the safe {resource} limit ({} logs); its previous sessions were preserved.",
                    limits.max_log_files
                );
                self.record_issue(
                    scan_id,
                    ScanIssueSeverity::Error,
                    "instance_resource_limit_exceeded",
                    Some(&instance.stored.name),
                    &message,
                )?;
                for source in instance
                    .staged
                    .iter()
                    .filter(|source| source.decision.requires_parse())
                {
                    self.database.mark_source_parse(
                        scan_id,
                        &source.source_revision_id,
                        SourceParseStatus::Failed,
                        Some("instance_resource_limit_exceeded"),
                    )?;
                }
                self.set_message(Some(message))?;
                continue;
            }

            let source_by_path = instance
                .staged
                .iter()
                .map(|source| (native_path_key(&source.relative_path), source))
                .collect::<BTreeMap<_, _>>();
            let mut parsed_logs = Vec::new();
            let mut failed = false;
            let mut failed_source_ids = BTreeSet::new();
            let mut budget = InstanceParseBudget::default();
            let mut chronological = instance.files;
            sort_logs_chronologically(&mut chronological);
            for (order, file) in chronological.iter().enumerate() {
                check_cancelled(cancellation)?;
                let Some(source) = source_by_path.get(&file.candidate.relative_path_key) else {
                    continue;
                };
                self.set_current_path(Some(display_relative(&file.candidate.relative_path)))?;
                let context = log_context(source, order as u32, file);
                match parse_file(file, context, cancellation) {
                    Ok(parsed) => {
                        let evidence_events =
                            u64::try_from(parsed.evidence.len()).unwrap_or(u64::MAX);
                        if let Err(resource) = scan_budget.retain_log(
                            parsed.diagnostics.decoded_bytes,
                            evidence_events,
                            &scan_limits,
                        ) {
                            failed = true;
                            failed_source_ids.insert(source.source_revision_id.clone());
                            scan_exhaustion = Some((resource, "scan_resource_limit_exceeded"));
                            let message = format!(
                                "The scan reached its safe {resource} limit; this instance and all remaining instances were deferred."
                            );
                            self.record_issue(
                                scan_id,
                                ScanIssueSeverity::Error,
                                "scan_resource_limit_exceeded",
                                Some(&instance.stored.name),
                                &message,
                            )?;
                            self.database.mark_source_parse(
                                scan_id,
                                &source.source_revision_id,
                                SourceParseStatus::Failed,
                                Some("scan_resource_limit_exceeded"),
                            )?;
                            self.set_message(Some(message))?;
                            parsed_count = parsed_count.saturating_add(1);
                            self.set_progress(parsed_count, parse_total)?;
                            break;
                        }
                        let resource_error =
                            InstanceParseBudget::validate_parsed_payload(&parsed, &limits)
                                .err()
                                .or_else(|| {
                                    budget
                                        .retain(
                                            parsed.diagnostics.decoded_bytes,
                                            evidence_events,
                                            &limits,
                                        )
                                        .err()
                                });
                        if let Some(resource) = resource_error {
                            failed = true;
                            failed_source_ids.insert(source.source_revision_id.clone());
                            let message = format!(
                                "This instance exceeded the aggregate {resource} limit; its previous sessions were preserved."
                            );
                            self.record_issue(
                                scan_id,
                                ScanIssueSeverity::Error,
                                "instance_resource_limit_exceeded",
                                Some(&instance.stored.name),
                                &message,
                            )?;
                            self.database.mark_source_parse(
                                scan_id,
                                &source.source_revision_id,
                                SourceParseStatus::Failed,
                                Some("instance_resource_limit_exceeded"),
                            )?;
                            self.set_message(Some(message))?;
                            parsed_count = parsed_count.saturating_add(1);
                            self.set_progress(parsed_count, parse_total)?;
                            break;
                        }
                        let warning = parser_has_warnings(&parsed);
                        if warning {
                            let label = display_relative(&file.candidate.relative_path);
                            self.record_issue(
                                scan_id,
                                ScanIssueSeverity::Warning,
                                "log_parse_warning",
                                Some(&label),
                                "A log contained malformed or incomplete lines; usable evidence was retained.",
                            )?;
                        }
                        self.database.mark_source_parse(
                            scan_id,
                            &source.source_revision_id,
                            if warning {
                                SourceParseStatus::Warning
                            } else {
                                SourceParseStatus::Parsed
                            },
                            None,
                        )?;
                        parsed_logs.push(parsed);
                    }
                    Err(SourceParseError::Cancelled) => {
                        return Err(WorkerError::Cancelled);
                    }
                    Err(SourceParseError::SnapshotChanged(error)) => {
                        failed = true;
                        failed_source_ids.insert(source.source_revision_id.clone());
                        let message = "A log changed after indexing, so its parsed evidence was discarded and previous sessions were preserved.";
                        let label = display_relative(&file.candidate.relative_path);
                        self.record_issue(
                            scan_id,
                            ScanIssueSeverity::Error,
                            "source_changed_during_parse",
                            Some(&label),
                            message,
                        )?;
                        self.database.mark_source_parse(
                            scan_id,
                            &source.source_revision_id,
                            SourceParseStatus::Failed,
                            Some("source_changed_during_parse"),
                        )?;
                        self.set_message(Some(message.to_owned()))?;
                        let _ = error;
                        if let Err(resource) = scan_budget.charge_failed_parse(&scan_limits) {
                            scan_exhaustion = Some((resource, "scan_resource_limit_exceeded"));
                        }
                    }
                    Err(SourceParseError::Io(error)) => {
                        failed = true;
                        failed_source_ids.insert(source.source_revision_id.clone());
                        let resource_limited = is_log_parse_limit_error(&error);
                        let (code, message) = if resource_limited {
                            (
                                "log_resource_limit_exceeded",
                                "A log exceeded MineTrace's safe parsing limits; its previous sessions were preserved.",
                            )
                        } else {
                            (
                                "log_read_failed",
                                "A log could not be decoded; its previous sessions were preserved.",
                            )
                        };
                        let label = display_relative(&file.candidate.relative_path);
                        self.record_issue(
                            scan_id,
                            ScanIssueSeverity::Error,
                            code,
                            Some(&label),
                            message,
                        )?;
                        self.database.mark_source_parse(
                            scan_id,
                            &source.source_revision_id,
                            SourceParseStatus::Failed,
                            Some(if resource_limited {
                                "log_resource_limit_exceeded"
                            } else {
                                "log_read_failed"
                            }),
                        )?;
                        self.set_message(Some(message.to_owned()))?;
                        if let Err(resource) = scan_budget.charge_failed_parse(&scan_limits) {
                            scan_exhaustion = Some((resource, "scan_resource_limit_exceeded"));
                        }
                    }
                }
                parsed_count = parsed_count.saturating_add(1);
                self.set_progress(parsed_count, parse_total)?;
            }

            // Reconstruction is instance-wide and sessions may span rotations.
            // If any present source failed, promoting a payload built from the
            // remaining sources could delete or rewrite a canonical cross-file
            // session. Preserve the whole instance and retry it on a later scan.
            if failed {
                for source in instance.staged.iter().filter(|source| {
                    source.decision.requires_parse()
                        && !failed_source_ids.contains(&source.source_revision_id)
                }) {
                    self.database.mark_source_parse(
                        scan_id,
                        &source.source_revision_id,
                        SourceParseStatus::Failed,
                        Some("instance_reconstruction_deferred"),
                    )?;
                }
                continue;
            }
            let reconstruction = reconstruct_sessions(&parsed_logs);
            if let Err(resource) =
                InstanceParseBudget::validate_reconstruction(&reconstruction.sessions, &limits)
            {
                let message = format!(
                    "This instance exceeded the safe {resource} limit; its previous sessions were preserved."
                );
                self.record_issue(
                    scan_id,
                    ScanIssueSeverity::Error,
                    "instance_resource_limit_exceeded",
                    Some(&instance.stored.name),
                    &message,
                )?;
                for source in instance
                    .staged
                    .iter()
                    .filter(|source| source.decision.requires_parse())
                {
                    self.database.mark_source_parse(
                        scan_id,
                        &source.source_revision_id,
                        SourceParseStatus::Failed,
                        Some("instance_resource_limit_exceeded"),
                    )?;
                }
                self.set_message(Some(message))?;
                continue;
            }
            let contexts = reconstruction
                .sessions
                .iter()
                .try_fold(0_usize, |count, session| {
                    count.checked_add(session.destinations.len())
                })
                .unwrap_or(usize::MAX);
            if let Err(resource) = scan_budget.retain_reconstruction(
                reconstruction.sessions.len(),
                contexts,
                &scan_limits,
            ) {
                scan_exhaustion = Some((resource, "scan_resource_limit_exceeded"));
                let message = format!(
                    "The scan reached its safe {resource} limit; this instance and all remaining instances were deferred."
                );
                self.record_issue(
                    scan_id,
                    ScanIssueSeverity::Error,
                    "scan_resource_limit_exceeded",
                    Some(&instance.stored.name),
                    &message,
                )?;
                self.defer_changed_sources(
                    scan_id,
                    &instance.staged,
                    "scan_resource_limit_exceeded",
                )?;
                self.set_message(Some(message))?;
                continue;
            }
            let payload = build_payload(
                &instance.stored,
                instance.replace_all,
                &instance.staged,
                &parsed_logs,
                &reconstruction.sessions,
            )?;
            if !self.database.instance_session_archive_within_limit(
                &payload,
                limits.max_reconstructed_sessions,
            )? {
                let message = format!(
                    "This instance would exceed the safe canonical session archive limit ({} sessions); its previous sessions were preserved.",
                    limits.max_reconstructed_sessions
                );
                self.record_issue(
                    scan_id,
                    ScanIssueSeverity::Error,
                    "instance_session_archive_limit_exceeded",
                    Some(&instance.stored.name),
                    &message,
                )?;
                for source in instance
                    .staged
                    .iter()
                    .filter(|source| source.decision.requires_parse())
                {
                    self.database.mark_source_parse(
                        scan_id,
                        &source.source_revision_id,
                        SourceParseStatus::Failed,
                        Some("instance_session_archive_limit_exceeded"),
                    )?;
                }
                self.set_message(Some(message))?;
                continue;
            }
            if let Err(resource) = archive_budget.retain_payload(&payload, &archive_limits) {
                scan_exhaustion = Some((resource, "global_archive_resource_limit_exceeded"));
                let message = format!(
                    "The global {resource} limit would be exceeded; this instance and all remaining instances were deferred."
                );
                self.record_issue(
                    scan_id,
                    ScanIssueSeverity::Error,
                    "global_archive_resource_limit_exceeded",
                    Some(&instance.stored.name),
                    &message,
                )?;
                self.defer_changed_sources(
                    scan_id,
                    &instance.staged,
                    "global_archive_resource_limit_exceeded",
                )?;
                self.set_message(Some(message))?;
                continue;
            }
            self.database.stage_reconstruction(scan_id, &payload)?;
        }

        check_cancelled(cancellation)?;
        self.update_phase(
            scan_id,
            ScanPhase::Aggregating,
            total,
            total,
            None,
            "Promoting the completed reconstruction in one database transaction.",
        )?;
        check_cancelled(cancellation)?;
        let summary = self.database.promote_scan(scan_id)?;
        Ok(summary.dataset_revision)
    }

    fn update_phase(
        &self,
        scan_id: &str,
        phase: ScanPhase,
        current: u64,
        total: u64,
        current_path: Option<String>,
        message: &str,
    ) -> Result<(), WorkerError> {
        {
            let mut status = self.lock_status()?;
            status.state = ScanState::Running;
            status.phase = phase;
            status.current = current;
            status.total = total;
            status.current_path = current_path;
            status.message = Some(message.to_owned());
            status
                .started_at
                .get_or_insert_with(|| Utc::now().to_rfc3339());
        }
        let counters = self.status()?;
        let counters_json = serde_json::json!({
            "current": counters.current,
            "total": counters.total,
            "warnings": counters.warnings,
            "errors": counters.errors,
        })
        .to_string();
        self.database
            .update_scan_phase(scan_id, phase.as_str(), &counters_json)?;
        Ok(())
    }

    fn lock_status(&self) -> Result<std::sync::MutexGuard<'_, ScanStatus>, BackendError> {
        self.runtime
            .status
            .lock()
            .map_err(|_| BackendError::BackgroundTask("scan status lock was poisoned".to_owned()))
    }

    fn lock_cancellation(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<Arc<AtomicBool>>>, BackendError> {
        self.runtime.cancellation.lock().map_err(|_| {
            BackendError::BackgroundTask("scan cancellation lock was poisoned".to_owned())
        })
    }

    fn set_progress(&self, current: u64, total: u64) -> Result<(), BackendError> {
        let mut status = self.lock_status()?;
        status.current = current;
        status.total = total;
        Ok(())
    }

    fn set_total(&self, total: u64) -> Result<(), BackendError> {
        self.lock_status()?.total = total;
        Ok(())
    }

    fn set_current_path(&self, path: Option<String>) -> Result<(), BackendError> {
        self.lock_status()?.current_path = path;
        Ok(())
    }

    fn set_message(&self, message: Option<String>) -> Result<(), BackendError> {
        self.lock_status()?.message = message;
        Ok(())
    }

    fn persist_status_snapshot(&self, scan_id: &str) -> Result<(), BackendError> {
        let status = { self.lock_status()?.clone() };
        let counters_json = serde_json::json!({
            "current": status.current,
            "total": status.total,
            "warnings": status.warnings,
            "errors": status.errors,
        })
        .to_string();
        self.database
            .update_scan_phase(scan_id, status.phase.as_str(), &counters_json)
    }

    fn record_issue(
        &self,
        scan_id: &str,
        severity: ScanIssueSeverity,
        code: &str,
        entity_label: Option<&str>,
        message: &str,
    ) -> Result<(), BackendError> {
        self.database.record_scan_message(
            scan_id,
            match severity {
                ScanIssueSeverity::Warning => StoredScanMessageSeverity::Warning,
                ScanIssueSeverity::Error => StoredScanMessageSeverity::Error,
            },
            code,
            entity_label,
            message,
        )?;

        let mut status = self.lock_status()?;
        match severity {
            ScanIssueSeverity::Warning => {
                status.warnings = status.warnings.saturating_add(1);
            }
            ScanIssueSeverity::Error => {
                status.errors = status.errors.saturating_add(1);
            }
        }
        if status.issues.len() == MAX_SCAN_ISSUES {
            status.issues.remove(0);
        }
        status.issues.push(ScanIssue {
            severity,
            code: code.to_owned(),
            entity_label: entity_label.map(ToOwned::to_owned),
            message: message.to_owned(),
        });
        Ok(())
    }

    fn defer_changed_sources(
        &self,
        scan_id: &str,
        sources: &[StagedSource],
        error_code: &str,
    ) -> Result<(), WorkerError> {
        for source in sources
            .iter()
            .filter(|source| source.decision.requires_parse())
        {
            self.database.mark_source_parse(
                scan_id,
                &source.source_revision_id,
                SourceParseStatus::Failed,
                Some(error_code),
            )?;
        }
        Ok(())
    }
}

fn group_candidates(
    installation: &DiscoveredInstallation,
    candidates: Vec<LogCandidate>,
    mode: ScanMode,
) -> Vec<InstanceGroup> {
    let mut groups = BTreeMap::<Vec<u8>, InstanceGroup>::new();
    for candidate in candidates {
        if mode == ScanMode::Quick
            && (candidate.kind != LogFileKind::Log
                || !candidate
                    .relative_path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("latest.log")))
        {
            continue;
        }
        let components = candidate
            .relative_path
            .components()
            .map(|component| component.as_os_str().to_owned())
            .collect::<Vec<_>>();
        let Some(log_index) = components
            .iter()
            .position(|component| component.to_string_lossy().eq_ignore_ascii_case("logs"))
        else {
            continue;
        };
        let instance_index = components[..log_index].iter().position(|component| {
            component
                .to_string_lossy()
                .eq_ignore_ascii_case("instances")
        });
        if instance_index.is_none()
            && (installation.adapter_kind == crate::domain::location::AdapterKind::Prism
                || installation.instances > 1)
        {
            continue;
        }
        let (relative_root, name) = if let Some(index) = instance_index {
            let Some(name) = components.get(index + 1) else {
                continue;
            };
            let mut root = PathBuf::new();
            for component in &components[..log_index] {
                root.push(component);
            }
            (root, name.to_string_lossy().into_owned())
        } else {
            (PathBuf::new(), installation.name.clone())
        };
        let key = native_path_key(&relative_root);
        groups
            .entry(key)
            .or_insert_with(|| InstanceGroup {
                relative_root,
                name,
                candidates: Vec::new(),
            })
            .candidates
            .push(candidate);
    }

    if groups.is_empty()
        && mode != ScanMode::Quick
        && installation.adapter_kind != crate::domain::location::AdapterKind::Prism
    {
        groups.insert(
            Vec::new(),
            InstanceGroup {
                relative_root: PathBuf::new(),
                name: installation.name.clone(),
                candidates: Vec::new(),
            },
        );
    }
    groups.into_values().collect()
}

struct CancellableReader<'a, 'b, R, H> {
    inner: R,
    cancellation: &'a AtomicBool,
    on_read: &'b mut H,
    bytes_read: u64,
}

impl<R, H> Read for CancellableReader<'_, '_, R, H>
where
    R: Read,
    H: FnMut(u64),
{
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.cancellation.load(Ordering::Acquire) {
            // Do not use `Interrupted` here: decoder internals are allowed to
            // retry that kind indefinitely. The scan loop maps this error to
            // cancellation whenever the token is set.
            Err(io::Error::other("Minecraft log parsing cancelled"))
        } else {
            let read = self.inner.read(buffer)?;
            if read > 0 {
                self.bytes_read = self.bytes_read.saturating_add(read as u64);
                (self.on_read)(self.bytes_read);
            }
            Ok(read)
        }
    }
}

fn parse_file(
    file: &FingerprintedLog,
    context: LogParseContext,
    cancellation: &AtomicBool,
) -> Result<ParsedLog, SourceParseError> {
    parse_file_with_read_hook(file, context, cancellation, |_| {})
}

fn parse_file_with_read_hook<H>(
    file: &FingerprintedLog,
    context: LogParseContext,
    cancellation: &AtomicBool,
    mut on_read: H,
) -> Result<ParsedLog, SourceParseError>
where
    H: FnMut(u64),
{
    let path = &file.candidate.absolute_path;
    let options = FingerprintOptions::default();
    let mut live_input = open_log_read_only_no_follow(&file.candidate)
        .map_err(|error| SourceParseError::SnapshotChanged(error.to_string()))?;
    let mut input = match create_verified_file_snapshot_with_control(
        &mut live_input,
        path,
        &file.fingerprint,
        &options,
        || cancellation.load(Ordering::Acquire),
    ) {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return Err(SourceParseError::SnapshotChanged(
                "the opened source no longer matches its staged fingerprint".to_owned(),
            ));
        }
        Err(crate::scan::ScanError::Cancelled) => return Err(SourceParseError::Cancelled),
        Err(error) => return Err(SourceParseError::SnapshotChanged(error.to_string())),
    };

    let parsed = match file.candidate.kind {
        LogFileKind::Log => {
            let input = CancellableReader {
                inner: &mut input,
                cancellation,
                on_read: &mut on_read,
                bytes_read: 0,
            };
            parse_minecraft_log_with_control(
                BufReader::new(input),
                context,
                LogParseLimits::default(),
                || cancellation.load(Ordering::Acquire),
            )
        }
        LogFileKind::CompressedLog => {
            let input = CancellableReader {
                inner: &mut input,
                cancellation,
                on_read: &mut on_read,
                bytes_read: 0,
            };
            parse_minecraft_log_with_control(
                BufReader::new(GzDecoder::new(input)),
                context,
                LogParseLimits::default(),
                || cancellation.load(Ordering::Acquire),
            )
        }
    };
    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(_error) if cancellation.load(Ordering::Acquire) => {
            return Err(SourceParseError::Cancelled);
        }
        Err(error) => return Err(SourceParseError::Io(error)),
    };

    let observed =
        fingerprint_log_with_previous_size_and_control(&file.candidate, &options, None, || {
            cancellation.load(Ordering::Acquire)
        });
    match observed {
        Ok(observed)
            if observed.size_bytes == file.fingerprint.size_bytes
                && observed.modified_at_ms == file.fingerprint.modified_at_ms
                && observed.birthtime_ms == file.fingerprint.birthtime_ms
                && observed.full_hash == file.fingerprint.full_hash =>
        {
            Ok(parsed)
        }
        Ok(_) => Err(SourceParseError::SnapshotChanged(
            "the source path changed before snapshot validation completed".to_owned(),
        )),
        Err(crate::scan::ScanError::Cancelled) => Err(SourceParseError::Cancelled),
        Err(error) => Err(SourceParseError::SnapshotChanged(error.to_string())),
    }
}

fn log_context(source: &StagedSource, order: u32, file: &FingerprintedLog) -> LogParseContext {
    let modified = Local
        .timestamp_millis_opt(file.fingerprint.modified_at_ms)
        .single()
        .unwrap_or_else(Local::now);
    let fixed_end = modified.fixed_offset();
    let filename_date = filename_date(&file.candidate.relative_path);
    let date_hint = filename_date.unwrap_or_else(|| modified.date_naive());
    let offset = local_offset_for_date(date_hint).unwrap_or(*fixed_end.offset());
    let context = LogParseContext::new(source.source_revision_id.clone(), order);
    let context = if filename_date.is_some() {
        context.with_date_hint(date_hint)
    } else {
        // An undated latest.log may begin before midnight and end on its mtime
        // day. Treating the mtime as the first line's day shifts the whole
        // session forward after rollover, so anchor it as the final day.
        context.with_final_date_hint(date_hint)
    };
    context
        .with_source_content_hash(file.fingerprint.full_hash)
        .with_utc_offset(offset)
        .with_source_end_hint(fixed_end)
}

fn filename_date(path: &Path) -> Option<NaiveDate> {
    let name = path.file_name()?.to_string_lossy();
    let prefix = name.get(..10)?;
    NaiveDate::parse_from_str(prefix, "%Y-%m-%d").ok()
}

fn sort_logs_chronologically(logs: &mut [FingerprintedLog]) {
    logs.sort_by(|left, right| {
        let left_latest = is_latest_log(&left.candidate.relative_path);
        let right_latest = is_latest_log(&right.candidate.relative_path);
        match (left_latest, right_latest) {
            (true, false) => return std::cmp::Ordering::Greater,
            (false, true) => return std::cmp::Ordering::Less,
            _ => {}
        }

        match (
            dated_rotation_key(&left.candidate.relative_path),
            dated_rotation_key(&right.candidate.relative_path),
        ) {
            (Some(left_key), Some(right_key)) => left_key.cmp(&right_key).then_with(|| {
                left.candidate
                    .relative_path_key
                    .cmp(&right.candidate.relative_path_key)
            }),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left
                .fingerprint
                .modified_at_ms
                .cmp(&right.fingerprint.modified_at_ms)
                .then_with(|| {
                    left.candidate
                        .relative_path_key
                        .cmp(&right.candidate.relative_path_key)
                }),
        }
    });
}

fn is_latest_log(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("latest.log"))
}

fn dated_rotation_key(path: &Path) -> Option<(NaiveDate, u32)> {
    let name = path.file_name()?.to_string_lossy();
    let stem = name
        .strip_suffix(".log.gz")
        .or_else(|| name.strip_suffix(".log"))?;
    let (date, sequence) = stem.rsplit_once('-')?;
    Some((
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?,
        sequence.parse().ok()?,
    ))
}

fn local_offset_for_date(date: NaiveDate) -> Option<FixedOffset> {
    let noon = date.and_hms_opt(12, 0, 0)?;
    Local
        .from_local_datetime(&noon)
        .single()
        .map(|value| *value.offset())
}

fn parser_has_warnings(parsed: &ParsedLog) -> bool {
    let diagnostics = &parsed.diagnostics;
    diagnostics.malformed_utf8_lines > 0
        || diagnostics.unparsed_timestamp_prefixes > 0
        || diagnostics.non_monotonic_timestamps > 0
        || diagnostics.evidence_without_timestamp > 0
}

fn build_payload(
    instance: &StoredInstance,
    replace_all: bool,
    sources: &[StagedSource],
    logs: &[ParsedLog],
    sessions: &[ReconstructedSession],
) -> Result<ReconstructionPayload, WorkerError> {
    let mut evidence_rows = Vec::new();
    for log in logs {
        for (event_order, evidence) in log.evidence.iter().enumerate() {
            let id = evidence_id(&evidence.provenance, evidence.event.tag());
            evidence_rows.push(StagedEvidence {
                id,
                source_revision_id: evidence.provenance.source_id.clone(),
                event_order: event_order as u64,
                line_number: evidence.provenance.line_number,
                byte_start: evidence.provenance.byte_start,
                byte_end: evidence.provenance.byte_end,
                kind: evidence_tag(evidence.event.tag()).to_owned(),
                observed_local: evidence
                    .timestamp
                    .observed_local
                    .map(|value| value.format("%Y-%m-%dT%H:%M:%S%.3f").to_string()),
                occurred_at_utc_ms: evidence.timestamp.occurred_at_utc_ms,
                utc_offset_minutes: evidence.timestamp.utc_offset_minutes,
                timestamp_origin: timestamp_origin(evidence.timestamp.origin).to_owned(),
                confidence_score: evidence.confidence_score,
                payload_json: serde_json::to_string(&evidence.event).map_err(|error| {
                    WorkerError::Failed(BackendError::BackgroundTask(format!(
                        "serialize evidence event: {error}"
                    )))
                })?,
                event_key: stable_digest(&[
                    evidence.provenance.source_id.as_bytes(),
                    &evidence.provenance.line_number.to_le_bytes(),
                    &evidence.provenance.byte_start.to_le_bytes(),
                    format!("{:?}", evidence.provenance.rule).as_bytes(),
                    evidence_tag(evidence.event.tag()).as_bytes(),
                ]),
            });
        }
    }

    let mut staged_sessions = Vec::new();
    for session in sessions {
        let Some(started_at) = session.started_at.occurred_at_utc_ms else {
            continue;
        };
        // A source that simply stops after its last evidence marker does not
        // prove when the client process ended. Keep that boundary unknown in
        // canonical storage instead of turning the final marker into an
        // observed zero-length (or otherwise falsely bounded) session.
        let implausible_duration = session.duration_seconds > MAX_PLAUSIBLE_SESSION_SECONDS;
        let has_bounded_end = !implausible_duration
            && !matches!(
                session.end_boundary,
                SessionEndBoundary::TruncatedAtLastEvidence
            );
        let ended_at = has_bounded_end
            .then_some(session.ended_at.occurred_at_utc_ms)
            .flatten();
        let duration_seconds = has_bounded_end.then_some(session.duration_seconds);
        let confidence_score = if implausible_duration {
            session.confidence_score.min(54)
        } else {
            session.confidence_score
        };
        let confidence = if implausible_duration && session.confidence_label != Confidence::Unknown
        {
            Confidence::Partial
        } else {
            session.confidence_label
        };
        let exit_kind = if implausible_duration {
            ReconstructedExitKind::Unknown
        } else {
            session.exit_kind
        };
        let canonical_key = stable_digest(&[
            instance.id.as_bytes(),
            &started_at.to_le_bytes(),
            &ended_at.unwrap_or(started_at).to_le_bytes(),
            serde_json::to_string(&session.versions)
                .unwrap_or_default()
                .as_bytes(),
            serde_json::to_string(&session.destinations)
                .unwrap_or_default()
                .as_bytes(),
        ]);
        let id = format!("session_{}", hex_prefix(&canonical_key, 24));
        let activities = session
            .destinations
            .iter()
            .map(|destination| match destination {
                SessionDestination::Server { address } => {
                    let canonical = normalize_server_address(address);
                    StagedActivity::Server {
                        id: format!(
                            "server_{}",
                            hex_prefix(&stable_digest(&[canonical.as_bytes()]), 24)
                        ),
                        canonical_address: canonical,
                        original_address: address.clone(),
                        started_at_utc_ms: Some(started_at),
                        ended_at_utc_ms: ended_at,
                        confidence_score,
                    }
                }
                SessionDestination::LocalWorld { world_name } => StagedActivity::World {
                    world_name: world_name
                        .clone()
                        .unwrap_or_else(|| "Unnamed local world".to_owned()),
                    started_at_utc_ms: Some(started_at),
                    ended_at_utc_ms: ended_at,
                    confidence_score,
                },
            })
            .collect();
        staged_sessions.push(StagedSession {
            id,
            started_at_utc_ms: started_at,
            ended_at_utc_ms: ended_at,
            duration_seconds,
            exit_kind: reconstructed_exit(exit_kind).to_owned(),
            confidence_score,
            confidence_label: confidence_label(confidence).to_owned(),
            reconstruction_revision: session.reconstruction_revision,
            canonical_key,
            timezone_id: session.started_at.utc_offset_minutes.map(format_utc_offset),
            minecraft_version: session.versions.last().cloned(),
            loader: None,
            utc_offset_minutes: session.started_at.utc_offset_minutes,
            evidence_links: session
                .evidence
                .iter()
                .map(|link| StagedEvidenceLink {
                    evidence_event_id: evidence_id(&link.provenance, link.evidence_tag),
                    role: evidence_role(link.role).to_owned(),
                })
                .collect(),
            source_revision_ids: session.source_ids.clone(),
            activities,
        });
    }

    let minecraft_version = sessions
        .iter()
        .flat_map(|session| session.versions.iter())
        .next_back()
        .cloned();
    let parsed_source_revision_ids = logs
        .iter()
        .map(|log| log.context.source_id.as_str())
        .collect::<BTreeSet<_>>();
    Ok(ReconstructionPayload {
        instance_id: instance.id.clone(),
        replace_all_instance_evidence: replace_all,
        source_path_ids: sources
            .iter()
            .filter(|source| {
                parsed_source_revision_ids.contains(source.source_revision_id.as_str())
            })
            .map(|source| source.source_path_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        minecraft_version,
        loader: None,
        evidence: evidence_rows,
        sessions: staged_sessions,
    })
}

fn evidence_id(provenance: &EvidenceProvenance, tag: EvidenceTag) -> String {
    let digest = stable_digest(&[
        provenance.source_id.as_bytes(),
        &provenance.line_number.to_le_bytes(),
        &provenance.byte_start.to_le_bytes(),
        format!("{:?}", provenance.rule).as_bytes(),
        evidence_tag(tag).as_bytes(),
    ]);
    format!("evidence_{}", hex_prefix(&digest, 24))
}

fn stable_digest(pieces: &[&[u8]]) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    for piece in pieces {
        hasher.update(&(piece.len() as u64).to_le_bytes());
        hasher.update(piece);
    }
    hasher.finalize().as_bytes().to_vec()
}

fn hex_prefix(bytes: &[u8], length: usize) -> String {
    let text = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    text[..length.min(text.len())].to_owned()
}

fn normalize_server_address(address: &str) -> String {
    let normalized = address.trim().to_ascii_lowercase();

    if normalized.starts_with('[') {
        let Some(closing_bracket) = normalized.find(']') else {
            return normalized;
        };
        let host = &normalized[..=closing_bracket];
        let suffix = normalized[closing_bracket + 1..].trim();
        return match suffix {
            "" | ":25565" => host.to_owned(),
            _ => format!("{host}{suffix}"),
        };
    }

    // Multiple colons denote an unbracketed IPv6 literal. Its final hextet is
    // never interpreted as a port, even when it happens to be `25565`.
    if normalized.bytes().filter(|byte| *byte == b':').count() > 1 {
        return normalized;
    }

    if let Some((host, port)) = normalized.split_once(':') {
        let host = host.trim_end_matches('.');
        if port == "25565" {
            host.to_owned()
        } else {
            format!("{host}:{port}")
        }
    } else {
        normalized.trim_end_matches('.').to_owned()
    }
}

fn evidence_tag(value: EvidenceTag) -> &'static str {
    match value {
        EvidenceTag::GameStarted => "game_started",
        EvidenceTag::VersionObserved => "version_observed",
        EvidenceTag::ServerJoined => "server_joined",
        EvidenceTag::IntegratedServerStarted => "integrated_server_started",
        EvidenceTag::WorldLoaded => "world_loaded",
        EvidenceTag::Disconnected => "disconnected",
        EvidenceTag::Stopping => "stopping",
        EvidenceTag::CleanShutdown => "clean_shutdown",
        EvidenceTag::Crash => "crash",
    }
}

fn timestamp_origin(value: TimestampOrigin) -> &'static str {
    match value {
        TimestampOrigin::LineDateTime => "line_datetime",
        TimestampOrigin::LineTimeWithDateHint => "line_time_with_date_hint",
        TimestampOrigin::LineTimeOnly => "line_time_only",
        TimestampOrigin::Missing => "missing",
    }
}

fn evidence_role(value: SessionEvidenceRole) -> &'static str {
    match value {
        SessionEvidenceRole::Start => "start",
        SessionEvidenceRole::End => "end",
        SessionEvidenceRole::Version => "version",
        SessionEvidenceRole::Destination => "destination",
        SessionEvidenceRole::Exit => "exit",
        SessionEvidenceRole::Supporting => "supporting",
    }
}

fn reconstructed_exit(value: ReconstructedExitKind) -> &'static str {
    match value {
        ReconstructedExitKind::Clean => "clean",
        ReconstructedExitKind::Crash => "crash",
        ReconstructedExitKind::Unknown => "unknown",
    }
}

fn confidence_label(value: Confidence) -> &'static str {
    match value {
        Confidence::Verified => "verified",
        Confidence::High => "high",
        Confidence::Partial => "partial",
        Confidence::Unknown => "unknown",
    }
}

fn format_utc_offset(minutes: i32) -> String {
    let sign = if minutes < 0 { '-' } else { '+' };
    let minutes = minutes.unsigned_abs();
    format!("UTC{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

fn storage_mode(mode: ScanMode) -> StorageScanMode {
    match mode {
        ScanMode::Quick => StorageScanMode::Quick,
        ScanMode::Standard => StorageScanMode::Standard,
        ScanMode::Deep => StorageScanMode::Deep,
    }
}

fn check_cancelled(cancellation: &AtomicBool) -> Result<(), WorkerError> {
    if cancellation.load(Ordering::Acquire) {
        Err(WorkerError::Cancelled)
    } else {
        Ok(())
    }
}

fn user_facing_scan_error(error: &BackendError) -> String {
    match error {
        BackendError::InvalidLocation { .. } => {
            "An approved location is no longer a supported Minecraft directory.".to_owned()
        }
        BackendError::Io { .. } => {
            "A local file could not be read. The prior archive remains unchanged.".to_owned()
        }
        BackendError::Database(_) => {
            "The local archive database could not complete the scan. The prior archive remains unchanged."
                .to_owned()
        }
        BackendError::MigrationChecksum { .. } => {
            "The local archive schema could not be verified. The prior archive remains unchanged."
                .to_owned()
        }
        BackendError::BackgroundTask(_) => {
            "The scan stopped before promotion. The prior archive remains unchanged.".to_owned()
        }
    }
}

fn display_relative(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn redacted_path_label(path: &Path) -> Option<String> {
    if path.is_absolute() {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    } else {
        Some(display_relative(path))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        fs,
        io::{Cursor, Write},
        path::{Path, PathBuf},
        sync::{Arc, atomic::AtomicBool},
    };

    use chrono::{Days, FixedOffset, Local, NaiveDate, TimeZone};
    use flate2::{Compression, write::GzEncoder};
    use tempfile::tempdir;

    use super::{
        CanonicalArchiveBudget, CanonicalArchiveLimits, InstanceParseBudget, InstanceParseLimits,
        PARSER_NAME, PARSER_REVISION, ScanMode, ScanParseBudget, ScanParseLimits, ScanPhase,
        ScanService, SourceParseError, build_payload, group_candidates, log_context,
        normalize_server_address, parse_file_with_read_hook, sort_logs_chronologically,
    };
    use crate::{
        application::DiscoveryService,
        discovery::AdapterRegistry,
        domain::{
            Confidence, PlatformKind,
            location::{AdapterKind, DiscoveredInstallation},
        },
        parser::{
            LogParseContext, LogParseLimits, SessionEndBoundary, parse_minecraft_log,
            reconstruct_sessions,
        },
        platform::PlatformPaths,
        scan::{
            FileDecision, FileFingerprint, FingerprintOptions, FingerprintedLog, InventoryOptions,
            LogCandidate, LogFileKind, ParserStamp, RollbackKind, ScanMessageSeverity,
            ScanMode as StoredScanMode, SourceParseStatus, StagedSource, fingerprint_log,
            inventory_logs,
        },
        storage::{Database, StoredInstance},
    };

    #[test]
    fn service_restores_the_latest_durable_terminal_scan() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let run = database
            .begin_scan(StoredScanMode::Standard)
            .expect("begin scan");
        database
            .update_scan_phase(
                &run.id,
                "parsing",
                r#"{"current":4,"total":9,"warnings":1,"errors":0}"#,
            )
            .expect("phase");
        database
            .record_scan_message(
                &run.id,
                ScanMessageSeverity::Warning,
                "log_parse_warning",
                Some("logs/latest.log"),
                "A log contained incomplete lines; usable evidence was retained.",
            )
            .expect("message");
        database
            .rollback_scan(
                &run.id,
                RollbackKind::Failed {
                    error_code: "scan_failed".to_owned(),
                },
            )
            .expect("rollback");

        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let status = ScanService::new(database, discovery)
            .status()
            .expect("restored status");

        assert_eq!(status.id, run.id);
        assert_eq!(status.phase, ScanPhase::Failed);
        assert_eq!(status.current, 4);
        assert_eq!(status.total, 9);
        assert_eq!(status.warnings, 1);
        assert_eq!(status.errors, 1);
        assert_eq!(status.issues.len(), 2);
        assert_eq!(status.issues[0].code, "log_parse_warning");
        assert_eq!(
            status.issues[0].entity_label.as_deref(),
            Some("logs/latest.log")
        );
        assert_eq!(status.issues[1].code, "scan_failed");
    }

    #[test]
    fn server_normalization_is_host_and_port_aware_without_damaging_ipv6() {
        assert_eq!(
            normalize_server_address(" PLAY.EXAMPLE.NET.:25565 "),
            "play.example.net"
        );
        assert_eq!(
            normalize_server_address("play.example.net."),
            "play.example.net"
        );
        assert_eq!(
            normalize_server_address("PLAY.EXAMPLE.NET.:25566"),
            "play.example.net:25566"
        );
        assert_eq!(
            normalize_server_address("[2001:DB8::1]:25565"),
            "[2001:db8::1]"
        );
        assert_eq!(
            normalize_server_address("[2001:DB8::1]:25566"),
            "[2001:db8::1]:25566"
        );
        assert_eq!(
            normalize_server_address("2001:DB8::25565"),
            "2001:db8::25565"
        );
    }

    #[test]
    fn instance_budget_bounds_log_count_decompressed_bytes_and_retained_evidence() {
        let limits = InstanceParseLimits {
            max_log_files: 2,
            max_decompressed_bytes: 100,
            max_evidence_events: 3,
            max_reconstructed_sessions: 2,
            max_destinations_per_session: 2,
            max_versions_per_session: 2,
            max_evidence_links_per_session: 3,
            max_destination_utf8_bytes: 512,
            max_version_utf8_bytes: 256,
        };
        assert_eq!(
            InstanceParseBudget::validate_log_count(3, &limits),
            Err("log file count")
        );

        let mut budget = InstanceParseBudget::default();
        budget.retain(60, 2, &limits).expect("first log fits");
        assert_eq!(budget.retain(1, 2, &limits), Err("evidence event count"));
        assert_eq!(
            budget.retain(41, 1, &limits),
            Err("decompressed byte count")
        );
        budget
            .retain(40, 1, &limits)
            .expect("exact aggregate limits fit");
        assert_eq!(budget.retained_logs, 2);
        assert_eq!(budget.decompressed_bytes, 100);
        assert_eq!(budget.evidence_events, 3);

        let parsed = parse_minecraft_log(
            Cursor::new(
                b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[10:01:00] [Render thread/INFO]: Connecting to bounded.example.net, 25565\n\
[10:02:00] [Render thread/INFO]: Stopping!\n",
            ),
            LogParseContext::new("budget", 0),
        )
        .expect("budget fixture");
        let mut payload_parsed = parsed.clone();
        if let crate::parser::MinecraftLogEvent::ServerJoined { address } = &mut payload_parsed
            .evidence
            .iter_mut()
            .find(|evidence| {
                matches!(
                    &evidence.event,
                    crate::parser::MinecraftLogEvent::ServerJoined { .. }
                )
            })
            .expect("server evidence")
            .event
        {
            *address = "x".repeat(512);
        }
        let payload_limits = InstanceParseLimits::default();
        assert!(
            InstanceParseBudget::validate_parsed_payload(&payload_parsed, &payload_limits).is_ok()
        );
        if let crate::parser::MinecraftLogEvent::ServerJoined { address } = &mut payload_parsed
            .evidence
            .iter_mut()
            .find(|evidence| {
                matches!(
                    &evidence.event,
                    crate::parser::MinecraftLogEvent::ServerJoined { .. }
                )
            })
            .expect("server evidence")
            .event
        {
            address.push('x');
        }
        assert_eq!(
            InstanceParseBudget::validate_parsed_payload(&payload_parsed, &payload_limits),
            Err("destination string bytes")
        );

        let reconstruction = reconstruct_sessions(&[parsed]);
        let mut reconstruction_limits = InstanceParseLimits {
            max_reconstructed_sessions: 0,
            ..InstanceParseLimits::default()
        };
        assert_eq!(
            InstanceParseBudget::validate_reconstruction(
                &reconstruction.sessions,
                &reconstruction_limits
            ),
            Err("reconstructed session count")
        );
        reconstruction_limits.max_reconstructed_sessions = 1;
        reconstruction_limits.max_destinations_per_session = 0;
        assert_eq!(
            InstanceParseBudget::validate_reconstruction(
                &reconstruction.sessions,
                &reconstruction_limits
            ),
            Err("destinations per session")
        );
        reconstruction_limits.max_destinations_per_session = 1;
        let destination_bytes = match &reconstruction.sessions[0].destinations[0] {
            crate::parser::SessionDestination::Server { address } => address.len(),
            crate::parser::SessionDestination::LocalWorld { world_name } => {
                world_name.as_ref().map_or(0, String::len)
            }
        };
        reconstruction_limits.max_destination_utf8_bytes = destination_bytes;
        reconstruction_limits.max_version_utf8_bytes = reconstruction.sessions[0].versions[0].len();
        assert!(
            InstanceParseBudget::validate_reconstruction(
                &reconstruction.sessions,
                &reconstruction_limits
            )
            .is_ok(),
            "exact string byte boundaries must remain valid"
        );
        reconstruction_limits.max_destination_utf8_bytes = destination_bytes - 1;
        assert_eq!(
            InstanceParseBudget::validate_reconstruction(
                &reconstruction.sessions,
                &reconstruction_limits
            ),
            Err("destination string bytes")
        );
        reconstruction_limits.max_destination_utf8_bytes = destination_bytes;
        reconstruction_limits.max_version_utf8_bytes =
            reconstruction.sessions[0].versions[0].len() - 1;
        assert_eq!(
            InstanceParseBudget::validate_reconstruction(
                &reconstruction.sessions,
                &reconstruction_limits
            ),
            Err("version string bytes")
        );
    }

    #[test]
    fn scan_and_global_archive_budgets_are_cumulative_and_atomic() {
        let scan_limits = ScanParseLimits {
            max_log_files: 1,
            max_decompressed_bytes: 100,
            max_evidence_events: 2,
            max_reconstructed_sessions: 1,
            max_contexts: 1,
        };
        let mut scan_budget = ScanParseBudget::default();
        scan_budget
            .retain_log(100, 2, &scan_limits)
            .expect("exact scan log budget");
        assert_eq!(
            scan_budget.retain_log(0, 0, &scan_limits),
            Err("scan log file count")
        );
        scan_budget
            .retain_reconstruction(1, 1, &scan_limits)
            .expect("exact scan reconstruction budget");
        assert_eq!(
            scan_budget.retain_reconstruction(1, 0, &scan_limits),
            Err("scan reconstructed session count")
        );

        let parsed = parse_minecraft_log(
            Cursor::new(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/logs/vanilla-clean.log"
            ))),
            LogParseContext::new("archive-budget", 0)
                .with_date_hint(NaiveDate::from_ymd_opt(2026, 8, 11).expect("valid date"))
                .with_utc_offset(FixedOffset::east_opt(2 * 60 * 60).expect("valid offset")),
        )
        .expect("archive fixture");
        let reconstruction = reconstruct_sessions(std::slice::from_ref(&parsed));
        let instance = StoredInstance {
            id: "archive-instance-a".to_owned(),
            relative_root: PathBuf::new(),
            name: "Archive A".to_owned(),
        };
        let payload = build_payload(
            &instance,
            false,
            &[],
            std::slice::from_ref(&parsed),
            &reconstruction.sessions,
        )
        .expect("archive payload");
        let archive_limits = CanonicalArchiveLimits {
            max_sessions: 1,
            max_contexts: 1,
        };
        let mut archive_budget = CanonicalArchiveBudget::default();
        archive_budget
            .retain_payload(&payload, &archive_limits)
            .expect("first archive payload fits");
        let mut second_payload = payload.clone();
        second_payload.instance_id = "archive-instance-b".to_owned();
        second_payload.sessions[0].id = "archive-session-b".to_owned();
        assert_eq!(
            archive_budget.retain_payload(&second_payload, &archive_limits),
            Err("global canonical session count")
        );

        let mut context_budget = CanonicalArchiveBudget::default();
        assert_eq!(
            context_budget.retain_payload(
                &payload,
                &CanonicalArchiveLimits {
                    max_sessions: 1,
                    max_contexts: 0,
                }
            ),
            Err("global canonical context count")
        );
    }

    #[test]
    fn dated_rotations_ignore_inverted_mtimes_and_latest_log_is_always_last() {
        let mut logs = vec![
            fingerprinted_log("logs/latest.log", -100),
            fingerprinted_log("logs/2026-08-10-2.log.gz", 10),
            fingerprinted_log("logs/2026-08-09-9.log.gz", 900),
            fingerprinted_log("logs/2026-08-10-1.log.gz", 800),
            fingerprinted_log("logs/custom-b.log", 50),
            fingerprinted_log("logs/custom-a.log", 50),
        ];

        sort_logs_chronologically(&mut logs);

        let ordered = logs
            .iter()
            .map(|log| log.candidate.relative_path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            ordered,
            vec![
                "logs/2026-08-09-9.log.gz",
                "logs/2026-08-10-1.log.gz",
                "logs/2026-08-10-2.log.gz",
                "logs/custom-a.log",
                "logs/custom-b.log",
                "logs/latest.log",
            ]
        );
    }

    #[test]
    fn append_during_parse_is_rejected_by_snapshot_revalidation() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        let file = write_snapshot_log(&game, &generated_long_log());
        let appended = Cell::new(false);
        let cancellation = AtomicBool::new(false);

        let error = parse_file_with_read_hook(
            &file,
            LogParseContext::new("append-snapshot", 0),
            &cancellation,
            |_| {
                if !appended.replace(true) {
                    let mut output = fs::OpenOptions::new()
                        .append(true)
                        .open(&file.candidate.absolute_path)
                        .expect("open live log for append");
                    output
                        .write_all(
                            b"[13:00:00] [main/INFO]: Loading Minecraft 1.21.4 with Fabric Loader\n",
                        )
                        .expect("append during parse");
                }
            },
        )
        .expect_err("a live append must invalidate the staged snapshot");

        assert!(appended.get());
        assert!(matches!(error, SourceParseError::SnapshotChanged(_)));
    }

    #[test]
    fn path_replacement_during_parse_is_rejected_by_snapshot_revalidation() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        let file = write_snapshot_log(&game, &generated_long_log());
        let replacement = game.join("logs/replacement.tmp");
        fs::write(
            &replacement,
            b"[15:00:00] [main/INFO]: Loading Minecraft 9.9.9 with Fabric Loader\n",
        )
        .expect("replacement source");
        let replaced = Cell::new(false);
        let cancellation = AtomicBool::new(false);

        let error = parse_file_with_read_hook(
            &file,
            LogParseContext::new("rotation-snapshot", 0),
            &cancellation,
            |_| {
                if !replaced.replace(true) {
                    fs::remove_file(&file.candidate.absolute_path).expect("remove old path");
                    fs::rename(&replacement, &file.candidate.absolute_path)
                        .expect("rotate replacement into place");
                }
            },
        )
        .expect_err("a path replacement must invalidate the staged snapshot");

        assert!(replaced.get());
        assert!(matches!(error, SourceParseError::SnapshotChanged(_)));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_swap_after_fingerprinting_is_rejected_before_parse() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        let file = write_snapshot_log(&game, &generated_long_log());
        let outside = temp.path().join("outside-secret.log");
        fs::write(
            &outside,
            b"[20:00:00] [main/INFO]: Loading Minecraft outside-approved-root\n",
        )
        .expect("outside file");
        fs::remove_file(&file.candidate.absolute_path).expect("remove indexed source");
        symlink(&outside, &file.candidate.absolute_path).expect("swap source for symlink");

        let error = parse_file_with_read_hook(
            &file,
            LogParseContext::new("symlink-snapshot", 0),
            &AtomicBool::new(false),
            |_| panic!("the symlink target must not be read"),
        )
        .expect_err("no-follow open must reject a swapped symlink");

        assert!(matches!(error, SourceParseError::SnapshotChanged(_)));
    }

    #[cfg(unix)]
    #[test]
    fn intermediate_directory_symlink_swap_cannot_escape_the_approved_root() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        let file = write_snapshot_log(&game, &generated_long_log());
        let outside_logs = temp.path().join("outside-logs");
        fs::create_dir_all(&outside_logs).expect("outside logs");
        fs::write(
            outside_logs.join("latest.log"),
            b"[20:00:00] [main/INFO]: Loading Minecraft outside-approved-root\n",
        )
        .expect("outside file");
        fs::rename(game.join("logs"), game.join("indexed-logs")).expect("move indexed logs");
        symlink(&outside_logs, game.join("logs")).expect("swap logs directory for symlink");

        let error = parse_file_with_read_hook(
            &file,
            LogParseContext::new("intermediate-symlink-snapshot", 0),
            &AtomicBool::new(false),
            |_| panic!("the intermediate symlink target must not be read"),
        )
        .expect_err("root-relative no-follow traversal must reject an intermediate symlink");

        assert!(matches!(error, SourceParseError::SnapshotChanged(_)));
    }

    #[test]
    fn payload_keeps_a_truncated_session_end_and_duration_unknown() {
        let parsed = parse_minecraft_log(
            Cursor::new(b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader"),
            LogParseContext::new("one-line-source", 0)
                .with_date_hint(NaiveDate::from_ymd_opt(2026, 8, 10).expect("valid date"))
                .with_utc_offset(FixedOffset::east_opt(0).expect("UTC offset")),
        )
        .expect("one-line log");
        let reconstruction = reconstruct_sessions(std::slice::from_ref(&parsed));
        assert_eq!(reconstruction.sessions.len(), 1);
        assert_eq!(
            reconstruction.sessions[0].end_boundary,
            SessionEndBoundary::TruncatedAtLastEvidence
        );

        let instance = StoredInstance {
            id: "instance-one-line".to_owned(),
            relative_root: PathBuf::new(),
            name: "One line".to_owned(),
        };
        let payload = build_payload(
            &instance,
            true,
            &[],
            std::slice::from_ref(&parsed),
            &reconstruction.sessions,
        )
        .expect("payload");
        let session = payload.sessions.first().expect("staged session");
        assert_eq!(session.ended_at_utc_ms, None);
        assert_eq!(session.duration_seconds, None);
    }

    #[test]
    fn payload_rejects_implausible_multi_year_runtime_without_inventing_a_cap() {
        let parsed = parse_minecraft_log(
            Cursor::new(
                b"[0001-01-01 00:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[9999-12-31 23:59:59] [Render thread/INFO]: Stopping!\n",
            ),
            LogParseContext::new("pathological-duration", 0)
                .with_utc_offset(FixedOffset::east_opt(0).expect("UTC offset")),
        )
        .expect("pathological log parses");
        let reconstruction = reconstruct_sessions(std::slice::from_ref(&parsed));
        assert_eq!(reconstruction.sessions.len(), 1);
        assert!(reconstruction.sessions[0].duration_seconds > super::MAX_PLAUSIBLE_SESSION_SECONDS);

        let instance = StoredInstance {
            id: "instance-pathological-duration".to_owned(),
            relative_root: PathBuf::new(),
            name: "Pathological duration".to_owned(),
        };
        let payload = build_payload(
            &instance,
            true,
            &[],
            std::slice::from_ref(&parsed),
            &reconstruction.sessions,
        )
        .expect("payload");
        let session = payload.sessions.first().expect("staged session");
        assert_eq!(session.ended_at_utc_ms, None);
        assert_eq!(session.duration_seconds, None);
        assert_eq!(session.exit_kind, "unknown");
        assert_eq!(session.confidence_label, "partial");
        assert!(session.confidence_score <= 54);
    }

    #[test]
    fn prism_logs_are_grouped_by_instance_and_quick_keeps_only_latest_plain_logs() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        fs::create_dir_all(root.join("instances/A/.minecraft/logs")).expect("a");
        fs::create_dir_all(root.join("instances/B/.minecraft/logs")).expect("b");
        fs::write(root.join("instances/A/.minecraft/logs/latest.log"), "a").expect("latest");
        fs::write(root.join("instances/A/.minecraft/logs/old.log.gz"), "a").expect("old");
        fs::write(root.join("instances/B/.minecraft/logs/latest.log"), "b").expect("latest");
        let location = test_location(root.to_path_buf(), AdapterKind::Prism);
        let report = inventory_logs(root, &InventoryOptions::default()).expect("inventory");

        let quick = group_candidates(&location, report.candidates, ScanMode::Quick);
        assert_eq!(quick.len(), 2);
        assert!(quick.iter().all(|group| group.candidates.len() == 1));
    }

    #[test]
    fn latest_log_context_anchors_an_overnight_session_to_its_mtime_day() {
        let source_end = Local
            .with_ymd_and_hms(2026, 8, 10, 0, 0, 5)
            .single()
            .expect("unambiguous local source end");
        let candidate = LogCandidate {
            approved_root: PathBuf::from("/not-read"),
            absolute_path: PathBuf::from("/not-read/logs/latest.log"),
            relative_path: PathBuf::from("logs/latest.log"),
            relative_path_key: b"logs/latest.log".to_vec(),
            kind: LogFileKind::Log,
            observed_size_bytes: 128,
        };
        let fingerprint = FileFingerprint {
            size_bytes: 128,
            modified_at_ms: source_end.timestamp_millis(),
            birthtime_ms: None,
            prefix_hash: [1; 32],
            full_hash: [2; 32],
            comparison_prefix_len: None,
            comparison_prefix_hash: None,
        };
        let file = FingerprintedLog {
            candidate,
            fingerprint: fingerprint.clone(),
        };
        let source = StagedSource {
            instance_id: Some("instance".to_owned()),
            source_path_id: "source".to_owned(),
            source_revision_id: "revision".to_owned(),
            relative_path: PathBuf::from("logs/latest.log"),
            kind: LogFileKind::Log,
            decision: FileDecision::New,
            generation: 1,
            fingerprint,
        };
        let bytes = b"[23:59:58] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n\
[00:00:03] [Render thread/INFO]: Stopping!\n";

        let parsed = parse_minecraft_log(Cursor::new(bytes), log_context(&source, 0, &file))
            .expect("overnight latest.log");
        let start = parsed.evidence.first().expect("start evidence");
        let end = parsed.evidence.last().expect("end evidence");
        assert_eq!(
            start.timestamp.observed_local.expect("start local").date(),
            source_end
                .date_naive()
                .checked_sub_days(Days::new(1))
                .expect("previous day")
        );
        assert_eq!(
            end.timestamp.observed_local.expect("end local").date(),
            source_end.date_naive()
        );
    }

    #[test]
    fn end_to_end_scan_promotes_plain_and_gzip_sessions_without_duplicates() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let clean = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logs/vanilla-clean.log"
        ));
        fs::write(game.join("logs/latest.log"), clean).expect("plain");
        let rotation_path = game
            .join("logs")
            .join(format!("{}-1.log.gz", chrono::Local::now().date_naive()));
        let gzip_file = fs::File::create(&rotation_path).expect("gzip");
        let mut encoder = GzEncoder::new(gzip_file, Compression::default());
        encoder.write_all(clean).expect("gzip body");
        encoder.finish().expect("finish gzip");
        let plain_before =
            blake3::hash(&fs::read(game.join("logs/latest.log")).expect("read plain before scan"));
        let gzip_before = blake3::hash(&fs::read(&rotation_path).expect("read gzip before scan"));

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game, AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("start");
        for _ in 0..200 {
            let status = service.status().expect("status");
            if status.state.is_terminal() {
                assert_eq!(
                    status.phase,
                    crate::application::scan_models::ScanPhase::Complete,
                    "terminal scan status: {status:?}"
                );
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let dashboard = crate::application::DashboardService
            .load(&database)
            .expect("dashboard");
        assert_eq!(dashboard.totals.sessions, 1);
        assert_eq!(dashboard.recent_sessions[0].version, "1.20.1");
        assert!(dashboard.recent_sessions[0].loader.is_none());
        assert_eq!(
            plain_before,
            blake3::hash(
                &fs::read(temp.path().join("game/logs/latest.log")).expect("read plain after scan")
            )
        );
        assert_eq!(
            gzip_before,
            blake3::hash(&fs::read(&rotation_path).expect("read gzip after scan"))
        );

        service.start(ScanMode::Standard).expect("second start");
        for _ in 0..200 {
            if service.status().expect("status").state.is_terminal() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let dashboard = crate::application::DashboardService
            .load(&database)
            .expect("dashboard second");
        assert_eq!(dashboard.totals.sessions, 1);

        fs::remove_file(temp.path().join("game/logs/latest.log")).expect("remove plain");
        fs::remove_file(&rotation_path).expect("remove gzip");
        service
            .start(ScanMode::Standard)
            .expect("missing-file scan");
        for _ in 0..200 {
            if service.status().expect("status").state.is_terminal() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let dashboard = crate::application::DashboardService
            .load(&database)
            .expect("dashboard after removal");
        assert_eq!(dashboard.totals.sessions, 1);
        let missing_sources = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*) FROM source_paths WHERE presence = 'missing'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("missing source count");
        assert_eq!(missing_sources, 2);
    }

    #[test]
    fn standard_promotes_new_sessions_while_preserving_history_from_a_missing_rotation() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let rotated_start =
            b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader 0.16.10\n\
[10:04:00] [Render thread/INFO]: Connecting to cross-file.example.net, 25565\n";
        let rotation_path = game
            .join("logs")
            .join(format!("{}-1.log", chrono::Local::now().date_naive()));
        fs::write(&rotation_path, rotated_start).expect("rotated start");
        let mut latest_end = b"[11:00:00] [Render thread/INFO]: Disconnected from server\n\
[11:01:00] [Render thread/INFO]: Stopping!\n\
[11:01:01] [Render thread/INFO]: Stopping worker threads\n"
            .to_vec();
        let filler = b"[11:02:00] [Render thread/DEBUG]: harmless test padding\n";
        while latest_end.len() < 70 * 1024 {
            latest_end.extend_from_slice(filler);
        }
        fs::write(game.join("logs/latest.log"), &latest_end).expect("latest end");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        let before = archive_counts(&database);
        assert_eq!(before.0, 1);
        let revision_before = dataset_revision(&database);
        let preserved_session_id = database
            .read(|connection| {
                connection.query_row("SELECT id FROM sessions LIMIT 1", [], |row| {
                    row.get::<_, String>(0)
                })
            })
            .expect("initial session id");
        let linked_sources = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(DISTINCT source_revision_id) FROM session_sources",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("linked source count");
        assert_eq!(linked_sources, 2);

        fs::remove_file(&rotation_path).expect("remove rotation");
        let mut latest = fs::OpenOptions::new()
            .append(true)
            .open(game.join("logs/latest.log"))
            .expect("open latest for append");
        latest
            .write_all(
                b"[12:00:00] [main/INFO]: Loading Minecraft 1.21.4 with Fabric Loader 0.16.14\n\
[12:04:00] [Render thread/INFO]: Connecting to new-session.example.net, 25565\n\
[12:30:00] [Render thread/INFO]: Disconnected from server\n\
[12:31:00] [Render thread/INFO]: Stopping!\n\
[12:31:01] [Render thread/INFO]: Stopping worker threads\n",
            )
            .expect("append latest");
        drop(latest);

        service
            .start(ScanMode::Standard)
            .expect("incomplete standard scan");
        wait_for_scan(&service);
        let after = archive_counts(&database);
        assert_eq!(after.0, 2, "new present evidence must be promoted");
        assert!(
            after.1 > before.1,
            "new evidence must reach the canonical dataset"
        );
        assert!(dataset_revision(&database) > revision_before);
        let sessions = crate::application::DashboardService
            .sessions(&database)
            .expect("sessions after incomplete scan");
        assert!(
            sessions
                .iter()
                .any(|session| session.id == preserved_session_id),
            "the cross-source session must survive its missing rotation"
        );
        assert!(sessions.iter().any(|session| {
            session.version == "1.21.4"
                && session.destination.as_deref() == Some("new-session.example.net:25565")
        }));
        let missing_sources = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*) FROM source_paths WHERE presence = 'missing'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("missing source count");
        assert_eq!(missing_sources, 1);
        let preserved_linked_sources = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(DISTINCT source_revision_id)
                     FROM session_sources WHERE session_id = ?1",
                    [&preserved_session_id],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("preserved source links");
        assert_eq!(preserved_linked_sources, 2);

        service
            .start(ScanMode::Standard)
            .expect("unchanged incomplete standard scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database), after);
    }

    #[test]
    fn resource_limited_present_rotation_preserves_instance_with_another_rotation_missing() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        let logs = game.join("logs");
        fs::create_dir_all(&logs).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");

        let date = Local::now().date_naive();
        let missing_rotation = logs.join(format!("{date}-1.log"));
        let failed_rotation = logs.join(format!("{date}-2.log"));
        fs::write(
            &missing_rotation,
            b"[09:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader 0.16.10\n\
[09:05:00] [Render thread/INFO]: Connecting to historical.example.net, 25565\n\
[09:30:00] [Render thread/INFO]: Disconnected from server\n\
[09:31:00] [Render thread/INFO]: Stopping!\n\
[09:31:01] [Render thread/INFO]: Stopping worker threads\n",
        )
        .expect("historical rotation");
        fs::write(
            &failed_rotation,
            b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader 0.16.10\n\
[10:05:00] [Render thread/INFO]: Connecting to cross-file.example.net, 25565\n",
        )
        .expect("cross-file start");
        fs::write(
            logs.join("latest.log"),
            b"[11:00:00] [Render thread/INFO]: Disconnected from server\n\
[11:01:00] [Render thread/INFO]: Stopping!\n\
[11:01:01] [Render thread/INFO]: Stopping worker threads\n\
[12:00:00] [main/INFO]: Loading Minecraft 1.21.4 with Fabric Loader 0.16.14\n\
[12:05:00] [Render thread/INFO]: Connecting to current.example.net, 25565\n\
[12:30:00] [Render thread/INFO]: Disconnected from server\n\
[12:31:00] [Render thread/INFO]: Stopping!\n\
[12:31:01] [Render thread/INFO]: Stopping worker threads\n",
        )
        .expect("latest sessions");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        let before_counts = archive_counts(&database);
        assert_eq!(before_counts.0, 3);
        let before_sessions = crate::application::DashboardService
            .sessions(&database)
            .expect("initial sessions");
        let cross_file_session = before_sessions
            .iter()
            .find(|session| session.destination.as_deref() == Some("cross-file.example.net:25565"))
            .expect("cross-file session")
            .id
            .clone();
        let cross_file_source_count = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(DISTINCT source_revision_id)
                     FROM session_sources WHERE session_id = ?1",
                    [&cross_file_session],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("cross-file source count");
        assert_eq!(cross_file_source_count, 2);

        fs::remove_file(&missing_rotation).expect("remove unrelated rotation");
        let oversized_line = vec![b'x'; LogParseLimits::default().max_line_bytes + 1];
        fs::write(&failed_rotation, oversized_line).expect("oversized present rotation");

        service
            .start(ScanMode::Standard)
            .expect("resource-limited scan");
        wait_for_scan(&service);

        assert_eq!(archive_counts(&database), before_counts);
        let after_sessions = crate::application::DashboardService
            .sessions(&database)
            .expect("sessions after resource limit");
        assert!(
            after_sessions
                .iter()
                .any(|session| session.id == cross_file_session),
            "a cross-file session linked to the failed present source must remain canonical"
        );
        let resource_limit_results = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*) FROM scan_file_results
                     WHERE error_code = 'log_resource_limit_exceeded'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("resource-limit result count");
        assert_eq!(resource_limit_results, 1);
        let missing_sources = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*) FROM source_paths WHERE presence = 'missing'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("missing source count");
        assert_eq!(missing_sources, 1);
    }

    #[test]
    fn failed_quick_reparse_preserves_history_and_is_retried_by_standard() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let clean = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logs/vanilla-clean.log"
        ));
        fs::write(game.join("logs/latest.log"), clean).expect("plain");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        let before = archive_counts(&database);
        assert_eq!(before.0, 1);
        assert!(before.1 > 0);

        let mut changed = clean.to_vec();
        changed.extend_from_slice(b"\n[23:59:59] [Render thread/INFO]: changed after import\n");
        fs::write(game.join("logs/latest.log"), changed).expect("changed log");

        let report = inventory_logs(&game, &InventoryOptions::default()).expect("inventory");
        let mut groups = group_candidates(&location, report.candidates, ScanMode::Quick);
        assert_eq!(groups.len(), 1);
        let group = groups.pop().expect("quick group");
        let stored = database
            .upsert_instance(&location, &group.relative_root, &group.name)
            .expect("instance");
        let files = group
            .candidates
            .into_iter()
            .map(|candidate| {
                let fingerprint = fingerprint_log(&candidate, &FingerprintOptions::default())
                    .expect("fingerprint");
                FingerprintedLog {
                    candidate,
                    fingerprint,
                }
            })
            .collect::<Vec<_>>();
        let run = database
            .begin_scan(crate::scan::ScanMode::Quick)
            .expect("quick scan");
        let staged = database
            .stage_inventory(
                &run.id,
                &location.id,
                Some(&stored.id),
                &format!("quick:{}", stored.id),
                &files,
                &ParserStamp::new(PARSER_NAME, PARSER_REVISION),
            )
            .expect("stage quick inventory");
        assert!(
            staged
                .sources
                .iter()
                .any(|source| source.decision.requires_parse())
        );
        for source in &staged.sources {
            database
                .mark_source_parse(
                    &run.id,
                    &source.source_revision_id,
                    SourceParseStatus::Failed,
                    Some("log_read_failed"),
                )
                .expect("mark failed");
        }

        let mut payload = build_payload(&stored, false, &staged.sources, &[], &[])
            .expect("partial reconstruction payload");
        assert!(
            payload.source_path_ids.is_empty(),
            "a failed source must not enter the partial replacement set"
        );
        // Promotion must still reject a failed path if a stale or malformed
        // caller includes it in a partial replacement payload.
        payload.source_path_ids = staged
            .sources
            .iter()
            .map(|source| source.source_path_id.clone())
            .collect();
        database
            .stage_reconstruction(&run.id, &payload)
            .expect("stage reconstruction");
        database.promote_scan(&run.id).expect("promote quick scan");

        assert_eq!(archive_counts(&database), before);
        let failed_revisions = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*) FROM source_revisions WHERE parse_status = 'failed'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("failed revision count");
        assert_eq!(failed_revisions, 1);

        service
            .start(ScanMode::Standard)
            .expect("recovery standard scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database), before);
        let current_revision = database
            .read(|connection| {
                connection.query_row(
                    "SELECT revision.generation, revision.parse_status
                     FROM source_paths source
                     JOIN source_revisions revision
                       ON revision.id = source.current_revision_id",
                    [],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )
            })
            .expect("current source revision");
        assert_eq!(current_revision.0, 3);
        assert!(matches!(current_revision.1.as_str(), "parsed" | "warning"));
    }

    #[test]
    fn quick_replacement_preserves_prior_generation_history_and_is_idempotent() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let old_session = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logs/vanilla-clean.log"
        ));
        fs::write(game.join("logs/latest.log"), old_session).expect("old latest");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        let initial_counts = archive_counts(&database);
        assert_eq!(initial_counts.0, 1);

        let later_session =
            b"[20:02:03] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.10\n\
[21:01:00] [Render thread/INFO]: Stopping!\n\
[21:01:01] [Render thread/INFO]: Stopping worker threads\n";
        assert!(later_session.len() < old_session.len());
        fs::write(game.join("logs/latest.log"), later_session).expect("replacement latest");

        service
            .start(ScanMode::Quick)
            .expect("replacement quick scan");
        wait_for_scan(&service);
        let replacement_counts = archive_counts(&database);
        assert_eq!(replacement_counts.0, 2);
        assert!(replacement_counts.1 > initial_counts.1);
        assert_eq!(source_revision_count(&database), 2);
        let replacement_dataset_revision = dataset_revision(&database);

        service
            .start(ScanMode::Quick)
            .expect("idempotent quick scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database), replacement_counts);
        assert_eq!(source_revision_count(&database), 2);
        assert_eq!(dataset_revision(&database), replacement_dataset_revision);
    }

    #[test]
    fn standard_replacement_preserves_prior_generation_history() {
        assert_complete_scan_replacement_preserves_history(ScanMode::Standard);
    }

    #[test]
    fn deep_replacement_preserves_prior_generation_history() {
        assert_complete_scan_replacement_preserves_history(ScanMode::Deep);
    }

    #[test]
    fn dated_exact_copy_reconciles_a_replaced_latest_generation() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let old_session = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logs/vanilla-clean.log"
        ));
        fs::write(game.join("logs/latest.log"), old_session).expect("old latest");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        let initial_counts = archive_counts(&database);
        assert_eq!(initial_counts.0, 1);
        let (session_id, initial_links) = database
            .read(|connection| {
                let session_id =
                    connection.query_row("SELECT id FROM sessions LIMIT 1", [], |row| {
                        row.get::<_, String>(0)
                    })?;
                let links = connection.query_row(
                    "SELECT COUNT(*) FROM session_evidence WHERE session_id = ?1",
                    [&session_id],
                    |row| row.get::<_, i64>(0),
                )?;
                Ok((session_id, links))
            })
            .expect("initial canonical links");

        fs::write(
            game.join("logs/latest.log"),
            b"[20:00:00] [Render thread/DEBUG]: replacement generation without evidence\n",
        )
        .expect("replacement latest");
        service.start(ScanMode::Standard).expect("replacement scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database), initial_counts);

        let rotation = game
            .join("logs")
            .join(format!("{}-1.log.gz", Local::now().date_naive()));
        let gzip_file = fs::File::create(rotation).expect("dated gzip rotation");
        let mut encoder = GzEncoder::new(gzip_file, Compression::default());
        encoder
            .write_all(old_session)
            .expect("reappearing decoded rotation");
        encoder.finish().expect("finish dated gzip rotation");
        service
            .start(ScanMode::Standard)
            .expect("reconciliation scan");
        wait_for_scan(&service);

        assert_eq!(archive_counts(&database), initial_counts);
        let (sessions, sources, links, old_latest_links): (i64, i64, i64, i64) = database
            .read(|connection| {
                Ok((
                    connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?,
                    connection.query_row(
                        "SELECT COUNT(*) FROM session_sources WHERE session_id = ?1",
                        [&session_id],
                        |row| row.get(0),
                    )?,
                    connection.query_row(
                        "SELECT COUNT(*) FROM session_evidence WHERE session_id = ?1",
                        [&session_id],
                        |row| row.get(0),
                    )?,
                    connection.query_row(
                        "SELECT COUNT(*)
                         FROM session_sources link
                         JOIN source_revisions revision ON revision.id = link.source_revision_id
                         JOIN source_paths source ON source.id = revision.source_path_id
                         WHERE link.session_id = ?1
                           AND source.relative_path_display = 'logs/latest.log'
                           AND revision.generation = 1",
                        [&session_id],
                        |row| row.get(0),
                    )?,
                ))
            })
            .expect("reconciled canonical links");
        assert_eq!(sessions, 1);
        assert_eq!(sources, 1);
        assert_eq!(links, initial_links);
        assert_eq!(old_latest_links, 0);

        let instance_id: String = database
            .read(|connection| {
                connection.query_row("SELECT id FROM instances LIMIT 1", [], |row| {
                    row.get::<_, String>(0)
                })
            })
            .expect("instance id");
        assert!(
            !database
                .instance_has_unreconciled_replacement_history(&instance_id)
                .expect("replacement lineage state")
        );

        service.start(ScanMode::Deep).expect("cleanup deep scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database), initial_counts);
    }

    #[test]
    fn quick_append_refreshes_prior_generation_without_duplicates() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let clean = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logs/vanilla-clean.log"
        ));
        let mut original = clean.to_vec();
        let filler = b"[15:02:00] [Render thread/DEBUG]: harmless test padding\n";
        while original.len() < 70 * 1024 {
            original.extend_from_slice(filler);
        }
        fs::write(game.join("logs/latest.log"), &original).expect("original latest");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database).0, 1);

        let appended_session =
            b"[20:02:03] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.10\n\
[21:01:00] [Render thread/INFO]: Stopping!\n\
[21:01:01] [Render thread/INFO]: Stopping worker threads\n";
        let mut latest = fs::OpenOptions::new()
            .append(true)
            .open(game.join("logs/latest.log"))
            .expect("open latest for append");
        latest
            .write_all(appended_session)
            .expect("append later session");
        drop(latest);

        service.start(ScanMode::Quick).expect("append quick scan");
        wait_for_scan(&service);
        let appended_counts = archive_counts(&database);
        assert_eq!(appended_counts.0, 2);
        assert_eq!(source_revision_count(&database), 2);
        let appended_dataset_revision = dataset_revision(&database);

        service
            .start(ScanMode::Quick)
            .expect("idempotent quick scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database), appended_counts);
        assert_eq!(source_revision_count(&database), 2);
        assert_eq!(dataset_revision(&database), appended_dataset_revision);
    }

    #[test]
    fn quick_short_append_is_detected_and_remains_idempotent() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let clean = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logs/vanilla-clean.log"
        ));
        assert!(clean.len() < 64 * 1024);
        fs::write(game.join("logs/latest.log"), clean).expect("original latest");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database).0, 1);

        let appended_session =
            b"[20:02:03] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.10\n\
[21:01:00] [Render thread/INFO]: Stopping!\n\
[21:01:01] [Render thread/INFO]: Stopping worker threads\n";
        let mut latest = fs::OpenOptions::new()
            .append(true)
            .open(game.join("logs/latest.log"))
            .expect("open short latest for append");
        latest
            .write_all(appended_session)
            .expect("append later session");
        drop(latest);
        assert!(
            fs::metadata(game.join("logs/latest.log"))
                .expect("short latest metadata")
                .len()
                < 64 * 1024
        );

        service
            .start(ScanMode::Quick)
            .expect("short append quick scan");
        wait_for_scan(&service);
        let appended_counts = archive_counts(&database);
        assert_eq!(appended_counts.0, 2);
        assert_eq!(source_revision_count(&database), 2);
        let maximum_session_sources = database
            .read(|connection| {
                connection.query_row(
                    "SELECT MAX(link_count) FROM (
                        SELECT COUNT(*) AS link_count
                        FROM session_sources GROUP BY session_id
                     )",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("maximum session source links");
        assert_eq!(maximum_session_sources, 1);
        let appended_dataset_revision = dataset_revision(&database);

        service
            .start(ScanMode::Quick)
            .expect("idempotent short append quick scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database), appended_counts);
        assert_eq!(source_revision_count(&database), 2);
        assert_eq!(dataset_revision(&database), appended_dataset_revision);
    }

    #[test]
    fn quick_short_append_replaces_an_open_partial_session_with_its_clean_completion() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let open_session =
            b"[14:02:03] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader 0.16.10\n\
[14:04:00] [Render thread/INFO]: Connecting to play.example.net, 25565\n";
        fs::write(game.join("logs/latest.log"), open_session).expect("open latest");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database).0, 1);

        let mut latest = fs::OpenOptions::new()
            .append(true)
            .open(game.join("logs/latest.log"))
            .expect("open latest for completion");
        latest
            .write_all(
                b"[15:00:00] [Render thread/INFO]: Disconnected from server\n\
[15:01:00] [Render thread/INFO]: Stopping!\n\
[15:01:01] [Render thread/INFO]: Stopping worker threads\n",
            )
            .expect("append clean completion");
        drop(latest);

        service
            .start(ScanMode::Quick)
            .expect("completion quick scan");
        wait_for_scan(&service);

        assert_eq!(archive_counts(&database).0, 1);
        let exit_kind = database
            .read(|connection| {
                connection.query_row("SELECT exit_kind FROM sessions", [], |row| {
                    row.get::<_, String>(0)
                })
            })
            .expect("completed exit kind");
        assert_eq!(exit_kind, "clean");
        assert_eq!(source_revision_count(&database), 2);
    }

    fn wait_for_scan(service: &ScanService) {
        for _ in 0..200 {
            let status = service.status().expect("status");
            if status.state.is_terminal() {
                assert_eq!(
                    status.phase,
                    crate::application::scan_models::ScanPhase::Complete,
                    "terminal scan status: {status:?}"
                );
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("scan did not finish");
    }

    fn assert_complete_scan_replacement_preserves_history(mode: ScanMode) {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");
        let old_session = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/logs/vanilla-clean.log"
        ));
        fs::write(game.join("logs/latest.log"), old_session).expect("old latest");

        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = test_location(game.clone(), AdapterKind::Manual);
        database.upsert_scan_location(&location).expect("location");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("empty-home"),
            PathBuf::from("/unused"),
        );
        let discovery = Arc::new(DiscoveryService::new(
            database.clone(),
            paths,
            AdapterRegistry::standard(),
        ));
        let service = ScanService::new(database.clone(), discovery);

        service.start(ScanMode::Standard).expect("initial scan");
        wait_for_scan(&service);
        let old_session_id: String = database
            .read(|connection| {
                connection.query_row("SELECT id FROM sessions LIMIT 1", [], |row| {
                    row.get::<_, String>(0)
                })
            })
            .expect("old session id");

        let later_session =
            b"[20:02:03] [main/INFO]: Loading Minecraft 1.21.1 with Fabric Loader 0.16.10\n\
[21:01:00] [Render thread/INFO]: Stopping!\n\
[21:01:01] [Render thread/INFO]: Stopping worker threads\n";
        assert!(later_session.len() < old_session.len());
        fs::write(game.join("logs/latest.log"), later_session).expect("replacement latest");

        service.start(mode).expect("replacement complete scan");
        wait_for_scan(&service);
        let replacement_counts = archive_counts(&database);
        assert_eq!(replacement_counts.0, 2);
        assert!(
            crate::application::DashboardService
                .sessions(&database)
                .expect("replacement sessions")
                .iter()
                .any(|session| session.id == old_session_id),
            "complete scans must preserve an unreconciled prior source generation"
        );
        let replacement_revisions = database
            .read(|connection| {
                connection.query_row(
                    "SELECT COUNT(*) FROM source_revisions WHERE change_kind = 'replaced'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .expect("replacement revision count");
        assert_eq!(replacement_revisions, 1);

        service.start(mode).expect("idempotent complete scan");
        wait_for_scan(&service);
        assert_eq!(archive_counts(&database), replacement_counts);
        assert!(
            crate::application::DashboardService
                .sessions(&database)
                .expect("idempotent replacement sessions")
                .iter()
                .any(|session| session.id == old_session_id)
        );
    }

    fn archive_counts(database: &Database) -> (i64, i64) {
        database
            .read(|connection| {
                let sessions =
                    connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
                let evidence =
                    connection
                        .query_row("SELECT COUNT(*) FROM evidence_events", [], |row| row.get(0))?;
                Ok((sessions, evidence))
            })
            .expect("archive counts")
    }

    fn source_revision_count(database: &Database) -> i64 {
        database
            .read(|connection| {
                connection.query_row("SELECT COUNT(*) FROM source_revisions", [], |row| {
                    row.get(0)
                })
            })
            .expect("source revision count")
    }

    fn dataset_revision(database: &Database) -> i64 {
        database
            .read(|connection| {
                connection.query_row(
                    "SELECT revision FROM dataset_state WHERE id = 1",
                    [],
                    |row| row.get(0),
                )
            })
            .expect("dataset revision")
    }

    fn test_location(path: PathBuf, adapter_kind: AdapterKind) -> DiscoveredInstallation {
        DiscoveredInstallation {
            id: crate::platform::stable_location_id(&crate::platform::native_path_key(&path)),
            name: "Test Game".to_owned(),
            kind_label: adapter_kind.label().to_owned(),
            adapter_kind,
            path,
            instances: 1,
            confidence: Confidence::Verified,
            validation_score: 95,
            enabled: true,
            platform: PlatformKind::Linux,
            origin: "custom",
        }
    }

    fn generated_long_log() -> Vec<u8> {
        let mut body =
            b"[10:00:00] [main/INFO]: Loading Minecraft 1.20.1 with Fabric Loader\n".to_vec();
        let filler = b"[10:00:01] [Render thread/DEBUG]: bounded snapshot test padding\n";
        while body.len() < 32 * 1024 {
            body.extend_from_slice(filler);
        }
        body.extend_from_slice(
            b"[11:00:00] [Render thread/INFO]: Stopping!\n\
[11:00:01] [Render thread/INFO]: Stopping worker threads\n",
        );
        body
    }

    fn write_snapshot_log(game: &Path, body: &[u8]) -> FingerprintedLog {
        fs::create_dir_all(game.join("logs")).expect("snapshot logs");
        fs::write(game.join("logs/latest.log"), body).expect("snapshot log");
        let report = inventory_logs(game, &InventoryOptions::default()).expect("inventory");
        let candidate = report
            .candidates
            .into_iter()
            .find(|candidate| candidate.relative_path.ends_with("latest.log"))
            .expect("latest candidate");
        let fingerprint = fingerprint_log(&candidate, &FingerprintOptions::default())
            .expect("snapshot fingerprint");
        FingerprintedLog {
            candidate,
            fingerprint,
        }
    }

    fn fingerprinted_log(relative_path: &str, modified_at_ms: i64) -> FingerprintedLog {
        let relative_path = PathBuf::from(relative_path);
        let relative_path_key = relative_path.to_string_lossy().as_bytes().to_vec();
        FingerprintedLog {
            candidate: LogCandidate {
                approved_root: PathBuf::from("/not-read"),
                absolute_path: PathBuf::from("/not-read").join(&relative_path),
                relative_path,
                relative_path_key,
                kind: LogFileKind::Log,
                observed_size_bytes: 0,
            },
            fingerprint: FileFingerprint {
                size_bytes: 0,
                modified_at_ms,
                birthtime_ms: None,
                prefix_hash: [0; 32],
                full_hash: [0; 32],
                comparison_prefix_len: None,
                comparison_prefix_hash: None,
            },
        }
    }
}
