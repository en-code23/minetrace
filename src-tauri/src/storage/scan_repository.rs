use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::Utc;
use rusqlite::{OptionalExtension, params};

use crate::{
    error::BackendError,
    scan::{
        DurableScanSnapshot, FileDecision, FingerprintedLog, ParserStamp, PromotionSummary,
        RollbackKind, ScanMessageSeverity, ScanMode, ScanRun, ScanSnapshot, ScanState,
        SourceParseStatus, StageInventoryResult, StageSummary, StagedSource, StoredScanMessage,
    },
};

use super::{Database, evidence_repository::promote_reconstruction_payloads};

static ID_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const DURABLE_SCAN_MESSAGE_LIMIT: i64 = 20;
const MAX_SCAN_MESSAGE_CODE_BYTES: usize = 96;
const MAX_SCAN_MESSAGE_LABEL_BYTES: usize = 512;
const MAX_SCAN_MESSAGE_TEXT_BYTES: usize = 1_024;

#[derive(Debug)]
struct CurrentRevision {
    id: String,
    generation: i64,
    size_bytes: u64,
    full_hash: Vec<u8>,
    parser_name: String,
    parser_revision: u32,
    parse_status: String,
}

impl Database {
    pub(crate) fn current_source_size(
        &self,
        location_id: &str,
        relative_path_key: &[u8],
    ) -> Result<Option<u64>, BackendError> {
        self.read(|connection| {
            let size = connection
                .query_row(
                    "SELECT revision.size_bytes
                     FROM source_paths source
                     JOIN source_revisions revision
                       ON revision.id = source.current_revision_id
                     WHERE source.location_id = ?1
                       AND source.relative_path_key = ?2",
                    params![location_id, relative_path_key],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            Ok(size.map(|value| value.max(0) as u64))
        })
    }

    pub(crate) fn begin_scan(&self, mode: ScanMode) -> Result<ScanRun, BackendError> {
        self.write(|transaction| {
            let revision: i64 = transaction.query_row(
                "SELECT revision FROM dataset_state WHERE id = 1",
                [],
                |row| row.get(0),
            )?;
            let id = unique_id("scan");
            transaction.execute(
                "INSERT INTO scan_runs (
                    id, mode, state, phase, requested_at_ms, counters_json,
                    dataset_revision_before
                 ) VALUES (?1, ?2, 'queued', 'discovering', ?3, '{}', ?4)",
                params![id, mode.as_str(), Utc::now().timestamp_millis(), revision],
            )?;
            Ok(ScanRun {
                id,
                state: ScanState::Queued,
                dataset_revision_before: revision,
            })
        })
    }

    pub(crate) fn update_scan_phase(
        &self,
        scan_id: &str,
        phase: &str,
        counters_json: &str,
    ) -> Result<(), BackendError> {
        self.write(|transaction| {
            transaction.execute(
                "UPDATE scan_runs
                 SET state = 'running', phase = ?2,
                     started_at_ms = COALESCE(started_at_ms, ?3), counters_json = ?4
                 WHERE id = ?1 AND state IN ('queued', 'running')",
                params![scan_id, phase, Utc::now().timestamp_millis(), counters_json],
            )?;
            Ok(())
        })
    }

    pub(crate) fn record_scan_message(
        &self,
        scan_id: &str,
        severity: ScanMessageSeverity,
        code: &str,
        entity_ref: Option<&str>,
        redacted_message: &str,
    ) -> Result<(), BackendError> {
        self.record_scan_messages(
            scan_id,
            &[StoredScanMessage {
                severity,
                code: code.to_owned(),
                entity_ref: entity_ref.map(ToOwned::to_owned),
                redacted_message: redacted_message.to_owned(),
            }],
        )
    }

    pub(crate) fn record_scan_messages(
        &self,
        scan_id: &str,
        messages: &[StoredScanMessage],
    ) -> Result<(), BackendError> {
        if messages.is_empty() {
            return Ok(());
        }
        self.write(|transaction| {
            let now = Utc::now().timestamp_millis();
            let mut statement = transaction.prepare(
                "INSERT INTO scan_messages (
                    scan_id, severity, code, entity_ref, redacted_message, created_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for message in messages {
                statement.execute(params![
                    scan_id,
                    message.severity.as_str(),
                    bounded_text(&message.code, MAX_SCAN_MESSAGE_CODE_BYTES),
                    message
                        .entity_ref
                        .as_deref()
                        .map(|value| bounded_text(value, MAX_SCAN_MESSAGE_LABEL_BYTES)),
                    bounded_text(&message.redacted_message, MAX_SCAN_MESSAGE_TEXT_BYTES),
                    now,
                ])?;
            }
            Ok(())
        })
    }

    pub(crate) fn latest_terminal_scan(&self) -> Result<Option<DurableScanSnapshot>, BackendError> {
        self.read(|connection| {
            let run = connection
                .query_row(
                    "SELECT
                        id, mode, state, counters_json, error_code,
                        started_at_ms, finished_at_ms,
                        dataset_revision_before, dataset_revision_after
                     FROM scan_runs
                     WHERE state IN ('completed', 'cancelled', 'failed', 'interrupted')
                     ORDER BY COALESCE(finished_at_ms, requested_at_ms) DESC,
                              requested_at_ms DESC,
                              id DESC
                     LIMIT 1",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<i64>>(5)?,
                            row.get::<_, Option<i64>>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, Option<i64>>(8)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                id,
                mode,
                state,
                counters_json,
                error_code,
                started_at_ms,
                finished_at_ms,
                dataset_revision_before,
                dataset_revision_after,
            )) = run
            else {
                return Ok(None);
            };

            let (warning_count, error_count) = connection.query_row(
                "SELECT
                    SUM(CASE WHEN severity = 'warning' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN severity = 'error' THEN 1 ELSE 0 END)
                 FROM scan_messages
                 WHERE scan_id = ?1",
                [&id],
                |row| {
                    Ok((
                        row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    ))
                },
            )?;
            let mut statement = connection.prepare(
                "SELECT severity, code, entity_ref, redacted_message
                 FROM scan_messages
                 WHERE scan_id = ?1
                 ORDER BY created_at_ms DESC, id DESC
                 LIMIT ?2",
            )?;
            let rows = statement.query_map(params![id, DURABLE_SCAN_MESSAGE_LIMIT], |row| {
                let severity: String = row.get(0)?;
                Ok(StoredScanMessage {
                    severity: ScanMessageSeverity::parse(&severity)
                        .unwrap_or(ScanMessageSeverity::Error),
                    code: row.get(1)?,
                    entity_ref: row.get(2)?,
                    redacted_message: row.get(3)?,
                })
            })?;
            let mut messages = rows.collect::<Result<Vec<_>, _>>()?;
            messages.reverse();

            Ok(Some(DurableScanSnapshot {
                id,
                mode: ScanMode::parse(&mode).unwrap_or(ScanMode::Standard),
                state: ScanState::parse(&state).unwrap_or(ScanState::Interrupted),
                counters_json,
                error_code,
                started_at_ms,
                finished_at_ms,
                dataset_revision_before,
                dataset_revision_after,
                warning_count: u64::try_from(warning_count.max(0)).unwrap_or(u64::MAX),
                error_count: u64::try_from(error_count.max(0)).unwrap_or(u64::MAX),
                messages,
            }))
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stage_inventory(
        &self,
        scan_id: &str,
        location_id: &str,
        instance_id: Option<&str>,
        scope_key: &str,
        files: &[FingerprintedLog],
        parser: &ParserStamp,
    ) -> Result<StageInventoryResult, BackendError> {
        self.write(|transaction| {
            let now = Utc::now().timestamp_millis();
            transaction.execute(
                "INSERT INTO scan_staged_locations (
                    scan_id, location_id, instance_id, scope_key, staged_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(scan_id, location_id, scope_key) DO UPDATE SET
                    instance_id = excluded.instance_id,
                    staged_at_ms = excluded.staged_at_ms",
                params![scan_id, location_id, instance_id, scope_key, now],
            )?;

            let mut summary = StageSummary::default();
            let mut staged = Vec::with_capacity(files.len());
            let mut observed_keys = BTreeSet::new();

            for file in files {
                let key = &file.candidate.relative_path_key;
                observed_keys.insert(key.clone());
                let existing_path: Option<(String, Option<String>)> = transaction
                    .query_row(
                        "SELECT id, current_revision_id
                         FROM source_paths
                         WHERE location_id = ?1 AND relative_path_key = ?2",
                        params![location_id, key],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()?;

                let source_path_id = existing_path
                    .as_ref()
                    .map(|value| value.0.clone())
                    .unwrap_or_else(|| stable_id("source", &[location_id.as_bytes(), key]));
                let current = existing_path
                    .and_then(|value| value.1)
                    .map(|revision_id| load_current_revision(transaction, &revision_id))
                    .transpose()?;
                let decision = decide(current.as_ref(), &file.fingerprint, parser);
                let generation = if decision == FileDecision::Unchanged {
                    current.as_ref().map_or(1, |value| value.generation)
                } else {
                    current.as_ref().map_or(1, |value| value.generation + 1)
                };
                let source_revision_id = if decision == FileDecision::Unchanged {
                    current
                        .as_ref()
                        .map(|value| value.id.clone())
                        .unwrap_or_else(|| {
                            stable_id(
                                "revision",
                                &[source_path_id.as_bytes(), &generation.to_le_bytes()],
                            )
                        })
                } else {
                    stable_id(
                        "revision",
                        &[source_path_id.as_bytes(), &generation.to_le_bytes()],
                    )
                };

                match decision {
                    FileDecision::New => summary.new_files += 1,
                    FileDecision::Appended => summary.appended_files += 1,
                    FileDecision::Replaced => summary.replaced_files += 1,
                    FileDecision::Unchanged => summary.unchanged_files += 1,
                    FileDecision::Reparse => summary.reparsed_files += 1,
                }

                transaction.execute(
                    "INSERT INTO scan_staged_sources (
                        scan_id, location_id, instance_id, source_path_id,
                        source_revision_id, relative_path_key, relative_path_display,
                        kind, size_bytes, modified_at_ms, birthtime_ms, prefix_hash,
                        full_hash, parser_name, parser_revision, decision, generation,
                        staged_at_ms
                     ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                        ?13, ?14, ?15, ?16, ?17, ?18
                     )
                     ON CONFLICT(scan_id, location_id, relative_path_key) DO UPDATE SET
                        instance_id = excluded.instance_id,
                        source_path_id = excluded.source_path_id,
                        source_revision_id = excluded.source_revision_id,
                        relative_path_display = excluded.relative_path_display,
                        kind = excluded.kind,
                        size_bytes = excluded.size_bytes,
                        modified_at_ms = excluded.modified_at_ms,
                        birthtime_ms = excluded.birthtime_ms,
                        prefix_hash = excluded.prefix_hash,
                        full_hash = excluded.full_hash,
                        parser_name = excluded.parser_name,
                        parser_revision = excluded.parser_revision,
                        decision = excluded.decision,
                        generation = excluded.generation,
                        staged_at_ms = excluded.staged_at_ms",
                    params![
                        scan_id,
                        location_id,
                        instance_id,
                        source_path_id,
                        source_revision_id,
                        key,
                        file.candidate.relative_path.to_string_lossy(),
                        file.candidate.kind.as_str(),
                        as_i64(file.fingerprint.size_bytes),
                        file.fingerprint.modified_at_ms,
                        file.fingerprint.birthtime_ms,
                        file.fingerprint.prefix_hash.as_slice(),
                        file.fingerprint.full_hash.as_slice(),
                        parser.name,
                        i64::from(parser.revision),
                        decision.as_str(),
                        generation,
                        now,
                    ],
                )?;

                staged.push(StagedSource {
                    instance_id: instance_id.map(ToOwned::to_owned),
                    source_path_id,
                    source_revision_id,
                    relative_path: file.candidate.relative_path.clone(),
                    kind: file.candidate.kind,
                    decision,
                    generation,
                    fingerprint: file.fingerprint.clone(),
                });
            }

            let mut missing = BTreeSet::new();
            if scope_key.starts_with("complete:") {
                let mut statement = transaction.prepare(
                    "SELECT instance_id, relative_path_key
                     FROM source_paths
                     WHERE location_id = ?1
                       AND instance_id IS ?2",
                )?;
                let rows = statement.query_map(params![location_id, instance_id], |row| {
                    Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Vec<u8>>(1)?))
                })?;
                for row in rows {
                    let (stored_instance, key) = row?;
                    if !observed_keys.contains(&key)
                        && let Some(stored_instance) = stored_instance
                    {
                        missing.insert(stored_instance);
                    }
                }
            }

            Ok(StageInventoryResult {
                summary,
                sources: staged,
                missing_source_instance_ids: missing.into_iter().collect(),
            })
        })
    }

    pub(crate) fn mark_source_parse(
        &self,
        scan_id: &str,
        source_revision_id: &str,
        status: SourceParseStatus,
        error_code: Option<&str>,
    ) -> Result<(), BackendError> {
        self.write(|transaction| {
            transaction.execute(
                "UPDATE scan_staged_sources
                 SET parse_status = ?3, parse_error_code = ?4
                 WHERE scan_id = ?1 AND source_revision_id = ?2",
                params![scan_id, source_revision_id, status.as_str(), error_code],
            )?;
            Ok(())
        })
    }

    pub(crate) fn stage_sessions_json(
        &self,
        scan_id: &str,
        stage_key: &str,
        payload_json: &str,
    ) -> Result<(), BackendError> {
        stage_json(
            self,
            "scan_staged_sessions",
            scan_id,
            stage_key,
            payload_json,
        )
    }

    pub(crate) fn promote_scan(&self, scan_id: &str) -> Result<PromotionSummary, BackendError> {
        self.write(|transaction| {
            let state: String = transaction.query_row(
                "SELECT state FROM scan_runs WHERE id = ?1",
                [scan_id],
                |row| row.get(0),
            )?;
            if !matches!(state.as_str(), "queued" | "running") {
                return Err(BackendError::BackgroundTask(format!(
                    "scan {scan_id} is not promotable from state {state}"
                )));
            }

            let now = Utc::now().timestamp_millis();
            let mut summary = promotion_counts(transaction, scan_id)?;

            let staged_rows = {
                let mut statement = transaction.prepare(
                    "SELECT location_id, instance_id, source_path_id, source_revision_id,
                            relative_path_key, relative_path_display, kind, size_bytes,
                            modified_at_ms, birthtime_ms, prefix_hash, full_hash,
                            parser_name, parser_revision, decision, parse_status,
                            parse_error_code, generation
                     FROM scan_staged_sources
                     WHERE scan_id = ?1
                     ORDER BY location_id, relative_path_key",
                )?;
                statement
                    .query_map([scan_id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, Vec<u8>>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, String>(6)?,
                            row.get::<_, i64>(7)?,
                            row.get::<_, i64>(8)?,
                            row.get::<_, Option<i64>>(9)?,
                            row.get::<_, Vec<u8>>(10)?,
                            row.get::<_, Vec<u8>>(11)?,
                            row.get::<_, String>(12)?,
                            row.get::<_, i64>(13)?,
                            row.get::<_, String>(14)?,
                            row.get::<_, String>(15)?,
                            row.get::<_, Option<String>>(16)?,
                            row.get::<_, i64>(17)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };

            for row in staged_rows {
                let (
                    location_id,
                    instance_id,
                    source_path_id,
                    source_revision_id,
                    path_key,
                    path_display,
                    kind,
                    size_bytes,
                    modified_at_ms,
                    birthtime_ms,
                    prefix_hash,
                    full_hash,
                    parser_name,
                    parser_revision,
                    decision,
                    parse_status,
                    parse_error_code,
                    generation,
                ) = row;

                transaction.execute(
                    "INSERT INTO source_paths (
                        id, location_id, instance_id, relative_path_key,
                        relative_path_display, kind, presence, current_revision_id,
                        last_seen_scan_id
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'present', ?7, ?8)
                     ON CONFLICT(location_id, relative_path_key) DO UPDATE SET
                        instance_id = excluded.instance_id,
                        relative_path_display = excluded.relative_path_display,
                        kind = excluded.kind,
                        presence = 'present',
                        current_revision_id = excluded.current_revision_id,
                        last_seen_scan_id = excluded.last_seen_scan_id",
                    params![
                        source_path_id,
                        location_id,
                        instance_id,
                        path_key,
                        path_display,
                        kind,
                        source_revision_id,
                        scan_id,
                    ],
                )?;

                if decision != "unchanged" {
                    transaction.execute(
                        "INSERT INTO source_revisions (
                            id, source_path_id, generation, size_bytes, modified_at_ms,
                            birthtime_ms, prefix_hash, full_hash, parser_name,
                            parser_revision, parsed_offset, change_kind,
                            parse_status, created_at_ms
                         ) VALUES (
                            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?4,
                            ?11, ?12, ?13
                         )",
                        params![
                            source_revision_id,
                            source_path_id,
                            generation,
                            size_bytes,
                            modified_at_ms,
                            birthtime_ms,
                            prefix_hash,
                            full_hash,
                            parser_name,
                            parser_revision,
                            decision,
                            parse_status,
                            now,
                        ],
                    )?;
                }

                transaction.execute(
                    "INSERT INTO scan_file_results (
                        scan_id, source_path_id, source_revision_id, decision, status,
                        error_code
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(scan_id, source_path_id) DO UPDATE SET
                        source_revision_id = excluded.source_revision_id,
                        decision = excluded.decision,
                        status = excluded.status,
                        error_code = excluded.error_code",
                    params![
                        scan_id,
                        source_path_id,
                        source_revision_id,
                        public_decision(&decision),
                        parse_status,
                        parse_error_code,
                    ],
                )?;
            }

            let promoted_sessions = promote_reconstruction_payloads(transaction, scan_id, now)?;

            let scopes = {
                let mut statement = transaction.prepare(
                    "SELECT location_id, instance_id, scope_key
                     FROM scan_staged_locations WHERE scan_id = ?1",
                )?;
                statement
                    .query_map([scan_id], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let mut missing_instances = BTreeSet::new();
            for (location_id, instance_id, scope_key) in scopes {
                if !scope_key.starts_with("complete:") {
                    continue;
                }
                let missing = {
                    let mut statement = transaction.prepare(
                        "SELECT id, instance_id FROM source_paths source
                         WHERE source.location_id = ?1
                           AND source.instance_id IS ?2
                           AND source.presence = 'present'
                           AND NOT EXISTS (
                               SELECT 1 FROM scan_staged_sources staged
                               WHERE staged.scan_id = ?3
                                 AND staged.location_id = source.location_id
                                 AND staged.relative_path_key = source.relative_path_key
                           )",
                    )?;
                    statement
                        .query_map(params![location_id, instance_id, scan_id], |row| {
                            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                        })?
                        .collect::<Result<Vec<_>, _>>()?
                };
                for (source_id, missing_instance) in missing {
                    transaction.execute(
                        "UPDATE source_paths SET presence = 'missing' WHERE id = ?1",
                        [source_id],
                    )?;
                    summary.missing_files += 1;
                    if let Some(missing_instance) = missing_instance {
                        missing_instances.insert(missing_instance);
                    }
                }
            }
            summary.missing_source_instance_ids = missing_instances.into_iter().collect();

            let staged_payloads: i64 = transaction.query_row(
                "SELECT
                    (SELECT COUNT(*) FROM scan_staged_evidence WHERE scan_id = ?1) +
                    (SELECT COUNT(*) FROM scan_staged_sessions WHERE scan_id = ?1)",
                [scan_id],
                |row| row.get(0),
            )?;
            let changed = summary.new_files
                + summary.appended_files
                + summary.replaced_files
                + summary.reparsed_files
                + summary.missing_files
                + promoted_sessions
                + usize::try_from(staged_payloads.max(0)).unwrap_or_default();
            let revision: i64 = transaction.query_row(
                "SELECT revision FROM dataset_state WHERE id = 1",
                [],
                |row| row.get(0),
            )?;
            let promoted_revision = if changed > 0 { revision + 1 } else { revision };
            if changed > 0 {
                transaction.execute(
                    "UPDATE dataset_state
                     SET revision = ?1, updated_at_ms = ?2 WHERE id = 1",
                    params![promoted_revision, now],
                )?;
            }
            summary.dataset_revision = promoted_revision;

            transaction.execute(
                "UPDATE scan_runs
                 SET state = 'completed', phase = 'complete', finished_at_ms = ?2,
                     dataset_revision_after = ?3
                 WHERE id = ?1",
                params![scan_id, now, promoted_revision],
            )?;
            clear_staging(transaction, scan_id)?;
            Ok(summary)
        })
    }

    pub(crate) fn rollback_scan(
        &self,
        scan_id: &str,
        kind: RollbackKind,
    ) -> Result<(), BackendError> {
        self.write(|transaction| {
            clear_staging(transaction, scan_id)?;
            let (state, phase, error) = match kind {
                RollbackKind::Cancelled => ("cancelled", "cancelled", None),
                RollbackKind::Failed { error_code } => ("failed", "failed", Some(error_code)),
            };
            transaction.execute(
                "UPDATE scan_runs
                 SET state = ?2, phase = ?3, error_code = ?4,
                     finished_at_ms = ?5,
                     dataset_revision_after = dataset_revision_before
                 WHERE id = ?1 AND state IN ('queued', 'running', 'paused')",
                params![scan_id, state, phase, error, Utc::now().timestamp_millis()],
            )?;
            Ok(())
        })
    }

    #[allow(dead_code)]
    pub(crate) fn scan_snapshot(
        &self,
        scan_id: &str,
    ) -> Result<Option<ScanSnapshot>, BackendError> {
        self.read(|connection| {
            connection
                .query_row(
                    "SELECT id, mode, state, phase, dataset_revision_before,
                            dataset_revision_after
                     FROM scan_runs WHERE id = ?1",
                    [scan_id],
                    |row| {
                        let mode: String = row.get(1)?;
                        let state: String = row.get(2)?;
                        Ok(ScanSnapshot {
                            id: row.get(0)?,
                            mode: ScanMode::parse(&mode).unwrap_or(ScanMode::Standard),
                            state: ScanState::parse(&state).unwrap_or(ScanState::Interrupted),
                            phase: row.get(3)?,
                            dataset_revision_before: row.get(4)?,
                            dataset_revision_after: row.get(5)?,
                        })
                    },
                )
                .optional()
        })
    }
}

fn load_current_revision(
    transaction: &rusqlite::Transaction<'_>,
    revision_id: &str,
) -> Result<CurrentRevision, BackendError> {
    transaction
        .query_row(
            "SELECT id, generation, size_bytes, full_hash,
                    parser_name, parser_revision, parse_status
             FROM source_revisions WHERE id = ?1",
            [revision_id],
            |row| {
                Ok(CurrentRevision {
                    id: row.get(0)?,
                    generation: row.get(1)?,
                    size_bytes: row.get::<_, i64>(2)?.max(0) as u64,
                    full_hash: row.get(3)?,
                    parser_name: row.get(4)?,
                    parser_revision: row.get::<_, i64>(5)?.max(0) as u32,
                    parse_status: row.get(6)?,
                })
            },
        )
        .map_err(Into::into)
}

fn decide(
    current: Option<&CurrentRevision>,
    next: &crate::scan::FileFingerprint,
    parser: &ParserStamp,
) -> FileDecision {
    let Some(current) = current else {
        return FileDecision::New;
    };
    if !matches!(current.parse_status.as_str(), "parsed" | "warning") {
        return FileDecision::Reparse;
    }
    if current.parser_name != parser.name || current.parser_revision != parser.revision {
        return FileDecision::Reparse;
    }
    if current.size_bytes == next.size_bytes && current.full_hash == next.full_hash {
        return FileDecision::Unchanged;
    }
    let exact_previous_content_matches = next.comparison_prefix_len == Some(current.size_bytes)
        && next
            .comparison_prefix_hash
            .as_ref()
            .is_some_and(|hash| hash.as_slice() == current.full_hash.as_slice());
    if next.size_bytes > current.size_bytes && exact_previous_content_matches {
        return FileDecision::Appended;
    }
    FileDecision::Replaced
}

fn promotion_counts(
    transaction: &rusqlite::Transaction<'_>,
    scan_id: &str,
) -> Result<PromotionSummary, BackendError> {
    let mut summary = PromotionSummary::default();
    let mut statement = transaction.prepare(
        "SELECT decision, COUNT(*) FROM scan_staged_sources
         WHERE scan_id = ?1 GROUP BY decision",
    )?;
    let rows = statement.query_map([scan_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (decision, count) = row?;
        let count = usize::try_from(count.max(0)).unwrap_or_default();
        match decision.as_str() {
            "new" => summary.new_files = count,
            "appended" => summary.appended_files = count,
            "replaced" => summary.replaced_files = count,
            "reparse" => summary.reparsed_files = count,
            "unchanged" => summary.unchanged_files = count,
            _ => {}
        }
    }
    Ok(summary)
}

fn stage_json(
    database: &Database,
    table: &str,
    scan_id: &str,
    stage_key: &str,
    payload_json: &str,
) -> Result<(), BackendError> {
    serde_json::from_str::<serde_json::Value>(payload_json).map_err(|error| {
        BackendError::BackgroundTask(format!("invalid staged JSON payload: {error}"))
    })?;
    database.write(|transaction| {
        transaction.execute(
            &format!(
                "INSERT INTO {table} (scan_id, stage_key, payload_json)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(scan_id, stage_key) DO UPDATE SET
                    payload_json = excluded.payload_json"
            ),
            params![scan_id, stage_key, payload_json],
        )?;
        Ok(())
    })
}

fn clear_staging(
    transaction: &rusqlite::Transaction<'_>,
    scan_id: &str,
) -> Result<(), rusqlite::Error> {
    for table in [
        "scan_staged_files",
        "scan_staged_evidence",
        "scan_staged_sessions",
        "scan_staged_sources",
        "scan_staged_locations",
    ] {
        transaction.execute(
            &format!("DELETE FROM {table} WHERE scan_id = ?1"),
            [scan_id],
        )?;
    }
    Ok(())
}

fn public_decision(decision: &str) -> &'static str {
    match decision {
        "new" => "new",
        "unchanged" => "unchanged",
        "reparse" => "reparse",
        "appended" | "replaced" => "changed",
        _ => "skipped",
    }
}

fn bounded_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut boundary = max_bytes.min(value.len());
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].to_owned()
}

fn unique_id(namespace: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let sequence = ID_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    stable_id(
        namespace,
        &[
            &nanos.to_le_bytes(),
            &u64::from(std::process::id()).to_le_bytes(),
            &sequence.to_le_bytes(),
        ],
    )
}

fn stable_id(namespace: &str, pieces: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(namespace.as_bytes());
    for piece in pieces {
        hasher.update(&(piece.len() as u64).to_le_bytes());
        hasher.update(piece);
    }
    let digest = hasher.finalize().to_hex().to_string();
    format!("{namespace}_{}", &digest[..24])
}

fn as_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::{CurrentRevision, Database, decide};
    use crate::{
        domain::{
            Confidence, PlatformKind,
            location::{AdapterKind, DiscoveredInstallation},
        },
        platform::native_path_key,
        scan::{
            FileDecision, FileFingerprint, FingerprintOptions, InventoryOptions, ParserStamp,
            RollbackKind, ScanMessageSeverity, ScanMode, SourceParseStatus, fingerprint_inventory,
            inventory_logs,
        },
    };

    fn location(path: PathBuf) -> DiscoveredInstallation {
        DiscoveredInstallation {
            id: "loc_test".to_owned(),
            name: "Test Game".to_owned(),
            kind_label: "Official".to_owned(),
            adapter_kind: AdapterKind::Official,
            path,
            instances: 1,
            confidence: Confidence::Verified,
            validation_score: 95,
            enabled: true,
            platform: PlatformKind::Linux,
            origin: "automatic",
        }
    }

    #[test]
    fn unchanged_second_inventory_reuses_the_source_generation() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("game");
        fs::create_dir_all(root.join("logs")).expect("logs");
        fs::write(root.join("logs/latest.log"), "one").expect("log");
        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = location(root.clone());
        database.upsert_scan_location(&location).expect("location");
        database
            .write(|tx| {
                tx.execute(
                    "INSERT INTO instances (
                    id, location_id, relative_path_key, relative_path_display, name,
                    confidence_score, first_seen_at_ms, last_seen_at_ms
                 ) VALUES ('instance_test', 'loc_test', ?1, '.', 'Test Game', 95, 1, 1)",
                    [native_path_key(std::path::Path::new("."))],
                )?;
                Ok(())
            })
            .expect("instance");
        let stamp = ParserStamp::new("minecraft-log", 1);

        let files = fingerprint_inventory(
            &inventory_logs(&root, &InventoryOptions::default()).expect("inventory"),
            &FingerprintOptions::default(),
        )
        .expect("fingerprints");
        let first = database.begin_scan(ScanMode::Standard).expect("scan");
        database
            .update_scan_phase(&first.id, "indexing", "{}")
            .expect("running");
        let staged = database
            .stage_inventory(
                &first.id,
                "loc_test",
                Some("instance_test"),
                "root",
                &files,
                &stamp,
            )
            .expect("stage");
        assert_eq!(staged.summary.new_files, 1);
        database
            .mark_source_parse(
                &first.id,
                &staged.sources[0].source_revision_id,
                SourceParseStatus::Parsed,
                None,
            )
            .expect("mark first source parsed");
        database.promote_scan(&first.id).expect("promote");

        let second = database
            .begin_scan(ScanMode::Standard)
            .expect("second scan");
        database
            .update_scan_phase(&second.id, "indexing", "{}")
            .expect("running");
        let staged = database
            .stage_inventory(
                &second.id,
                "loc_test",
                Some("instance_test"),
                "root",
                &files,
                &stamp,
            )
            .expect("stage second");
        assert_eq!(staged.summary.unchanged_files, 1);
        assert_eq!(staged.sources[0].generation, 1);
        database.promote_scan(&second.id).expect("promote second");
    }

    #[test]
    fn grown_rewrite_with_matching_fixed_prefix_is_replaced_without_exact_old_content_proof() {
        let parser = ParserStamp::new("minecraft-log", 2);
        let current = CurrentRevision {
            id: "revision-old".to_owned(),
            generation: 1,
            size_bytes: 128 * 1024,
            full_hash: vec![8; 32],
            parser_name: parser.name.clone(),
            parser_revision: parser.revision,
            parse_status: "parsed".to_owned(),
        };
        let rewritten = FileFingerprint {
            size_bytes: current.size_bytes + 1,
            modified_at_ms: 2,
            birthtime_ms: None,
            prefix_hash: [7; 32],
            full_hash: [9; 32],
            comparison_prefix_len: Some(current.size_bytes),
            comparison_prefix_hash: Some([10; 32]),
        };

        assert_eq!(
            decide(Some(&current), &rewritten, &parser),
            FileDecision::Replaced
        );
    }

    #[test]
    fn reconstruction_bundle_revision_upgrade_forces_reparse_of_unchanged_bytes() {
        let parser = ParserStamp::new("minecraft-java-log", 3);
        let current = CurrentRevision {
            id: "revision-v2".to_owned(),
            generation: 1,
            size_bytes: 100,
            full_hash: vec![4; 32],
            parser_name: parser.name.clone(),
            parser_revision: 2,
            parse_status: "parsed".to_owned(),
        };
        let unchanged = FileFingerprint {
            size_bytes: 100,
            modified_at_ms: 1,
            birthtime_ms: None,
            prefix_hash: [4; 32],
            full_hash: [4; 32],
            comparison_prefix_len: Some(100),
            comparison_prefix_hash: Some([4; 32]),
        };

        assert_eq!(
            decide(Some(&current), &unchanged, &parser),
            FileDecision::Reparse
        );
    }

    #[test]
    fn durable_terminal_snapshot_counts_every_issue_and_bounds_the_returned_list() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("diagnostics.sqlite3")).expect("database");
        let scan = database.begin_scan(ScanMode::Standard).expect("scan");
        database
            .update_scan_phase(
                &scan.id,
                "parsing",
                r#"{"current":7,"total":9,"warnings":0,"errors":0}"#,
            )
            .expect("phase");
        for index in 0..25 {
            database
                .record_scan_message(
                    &scan.id,
                    ScanMessageSeverity::Warning,
                    "parser_warning",
                    Some(&format!("logs/rotation-{index}.log")),
                    "A source contained incomplete evidence; canonical history was preserved.",
                )
                .expect("warning");
        }
        database
            .record_scan_message(
                &scan.id,
                ScanMessageSeverity::Error,
                "log_read_failed",
                Some("logs/latest.log"),
                "A source could not be decoded; canonical history was preserved.",
            )
            .expect("error");
        database
            .rollback_scan(
                &scan.id,
                RollbackKind::Failed {
                    error_code: "scan_failed".to_owned(),
                },
            )
            .expect("rollback");

        let snapshot = database
            .latest_terminal_scan()
            .expect("terminal snapshot")
            .expect("latest scan");

        assert_eq!(snapshot.id, scan.id);
        assert_eq!(snapshot.warning_count, 25);
        assert_eq!(snapshot.error_count, 1);
        assert_eq!(snapshot.messages.len(), 20);
        assert_eq!(
            snapshot.messages.last().expect("latest").code,
            "log_read_failed"
        );
        assert_eq!(
            snapshot
                .messages
                .last()
                .expect("latest")
                .entity_ref
                .as_deref(),
            Some("logs/latest.log")
        );
        assert_eq!(snapshot.error_code.as_deref(), Some("scan_failed"));
    }
}
