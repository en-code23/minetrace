use std::collections::{BTreeSet, HashSet};

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::error::BackendError;

use super::Database;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReconstructionPayload {
    pub instance_id: String,
    pub replace_all_instance_evidence: bool,
    pub source_path_ids: Vec<String>,
    pub minecraft_version: Option<String>,
    pub loader: Option<String>,
    pub evidence: Vec<StagedEvidence>,
    pub sessions: Vec<StagedSession>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StagedEvidence {
    pub id: String,
    pub source_revision_id: String,
    pub event_order: u64,
    pub line_number: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub kind: String,
    pub observed_local: Option<String>,
    pub occurred_at_utc_ms: Option<i64>,
    pub utc_offset_minutes: Option<i32>,
    pub timestamp_origin: String,
    pub confidence_score: u8,
    pub payload_json: String,
    pub event_key: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StagedSession {
    pub id: String,
    pub started_at_utc_ms: i64,
    pub ended_at_utc_ms: Option<i64>,
    pub duration_seconds: Option<u64>,
    pub exit_kind: String,
    pub confidence_score: u8,
    pub confidence_label: String,
    pub reconstruction_revision: u16,
    pub canonical_key: Vec<u8>,
    pub timezone_id: Option<String>,
    pub minecraft_version: Option<String>,
    pub loader: Option<String>,
    pub utc_offset_minutes: Option<i32>,
    pub evidence_links: Vec<StagedEvidenceLink>,
    pub source_revision_ids: Vec<String>,
    pub activities: Vec<StagedActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StagedEvidenceLink {
    pub evidence_event_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StagedActivity {
    Server {
        id: String,
        canonical_address: String,
        original_address: String,
        started_at_utc_ms: Option<i64>,
        ended_at_utc_ms: Option<i64>,
        confidence_score: u8,
    },
    World {
        world_name: String,
        started_at_utc_ms: Option<i64>,
        ended_at_utc_ms: Option<i64>,
        confidence_score: u8,
    },
}

impl Database {
    pub(crate) fn stage_reconstruction(
        &self,
        scan_id: &str,
        payload: &ReconstructionPayload,
    ) -> Result<(), BackendError> {
        let payload_json = serde_json::to_string(payload).map_err(|error| {
            BackendError::BackgroundTask(format!("serialize reconstruction payload: {error}"))
        })?;
        self.stage_sessions_json(scan_id, &payload.instance_id, &payload_json)
    }

    pub(crate) fn instance_has_unreconciled_replacement_history(
        &self,
        instance_id: &str,
    ) -> Result<bool, BackendError> {
        self.read(|connection| {
            let exists: i64 = connection.query_row(
                "SELECT EXISTS (
                    SELECT 1
                    FROM sessions session
                    JOIN session_sources link ON link.session_id = session.id
                    JOIN source_revisions revision ON revision.id = link.source_revision_id
                    WHERE session.instance_id = ?1
                      AND EXISTS (
                          SELECT 1 FROM source_revisions later
                          WHERE later.source_path_id = revision.source_path_id
                            AND later.generation > revision.generation
                            AND later.change_kind = 'replaced'
                      )
                 )",
                [instance_id],
                |row| row.get(0),
            )?;
            Ok(exists != 0)
        })
    }

    pub(crate) fn instance_session_archive_within_limit(
        &self,
        payload: &ReconstructionPayload,
        max_sessions: usize,
    ) -> Result<bool, BackendError> {
        let incoming = payload
            .sessions
            .iter()
            .map(|session| session.id.as_str())
            .collect::<HashSet<_>>();
        if incoming.len() > max_sessions {
            return Ok(false);
        }
        if payload.replace_all_instance_evidence {
            return Ok(true);
        }

        self.read(|connection| {
            let limit = i64::try_from(max_sessions.saturating_add(1)).unwrap_or(i64::MAX);
            let mut statement = connection.prepare(
                "SELECT id FROM sessions
                 WHERE instance_id = ?1
                 ORDER BY id
                 LIMIT ?2",
            )?;
            let existing = statement
                .query_map(rusqlite::params![payload.instance_id, limit], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<HashSet<_>, _>>()?;
            if existing.len() > max_sessions {
                return Ok(false);
            }
            let new_sessions = incoming
                .iter()
                .filter(|session_id| !existing.contains(**session_id))
                .count();
            Ok(existing.len().saturating_add(new_sessions) <= max_sessions)
        })
    }
}

pub(super) fn promote_reconstruction_payloads(
    transaction: &rusqlite::Transaction<'_>,
    scan_id: &str,
    now: i64,
) -> Result<usize, BackendError> {
    let payloads = {
        let mut statement = transaction.prepare(
            "SELECT payload_json FROM scan_staged_sessions
             WHERE scan_id = ?1 ORDER BY stage_key",
        )?;
        statement
            .query_map([scan_id], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut promoted_sessions = 0_usize;
    for payload_json in payloads {
        let payload: ReconstructionPayload =
            serde_json::from_str(&payload_json).map_err(|error| {
                BackendError::BackgroundTask(format!("decode reconstruction payload: {error}"))
            })?;

        if payload.replace_all_instance_evidence {
            transaction.execute(
                "DELETE FROM sessions WHERE instance_id = ?1",
                [&payload.instance_id],
            )?;
            transaction.execute(
                "DELETE FROM evidence_events
                 WHERE source_revision_id IN (
                     SELECT revision.id
                     FROM source_revisions revision
                     JOIN source_paths source ON source.id = revision.source_path_id
                     WHERE source.instance_id = ?1
                 )",
                [&payload.instance_id],
            )?;
        } else {
            // A complete inventory can legitimately be missing an older rotation.
            // Sessions that depend on that absent path remain canonical until the
            // path returns, but they must not prevent independent evidence from
            // present paths from being refreshed and promoted.
            let protected_sessions = sessions_linked_to_unavailable_or_replaced_sources(
                transaction,
                scan_id,
                &payload.instance_id,
            )?;
            for source_path_id in &payload.source_path_ids {
                let staged_source = transaction
                    .query_row(
                        "SELECT source_revision_id, decision, parse_status
                         FROM scan_staged_sources
                         WHERE scan_id = ?1 AND source_path_id = ?2",
                        rusqlite::params![scan_id, source_path_id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                            ))
                        },
                    )
                    .optional()?;
                let Some((source_revision_id, decision, parse_status)) = staged_source else {
                    continue;
                };
                if !matches!(parse_status.as_str(), "parsed" | "warning") {
                    continue;
                }

                match decision.as_str() {
                    // A replacement is a new physical generation occupying the
                    // same path (most notably a rotated latest.log). Its prior
                    // generation remains canonical until a complete scan can
                    // reconcile it with the rotated archive.
                    "new" | "replaced" => {}
                    "appended" | "reparse" => {
                        let candidates = {
                            let mut statement = transaction.prepare(
                                "SELECT DISTINCT link.session_id
                                 FROM session_sources link
                                 JOIN source_revisions revision
                                   ON revision.id = link.source_revision_id
                                 WHERE revision.source_path_id = ?1
                                   AND revision.id <> ?2",
                            )?;
                            statement
                                .query_map(
                                    rusqlite::params![source_path_id, source_revision_id],
                                    |row| row.get::<_, String>(0),
                                )?
                                .collect::<Result<Vec<_>, _>>()?
                        };
                        delete_unprotected_sessions(transaction, &candidates, &protected_sessions)?;
                        transaction.execute(
                            "DELETE FROM evidence_events
                             WHERE source_revision_id IN (
                                 SELECT id FROM source_revisions
                                 WHERE source_path_id = ?1 AND id <> ?2
                             )
                               AND NOT EXISTS (
                                   SELECT 1 FROM session_evidence link
                                   WHERE link.evidence_event_id = evidence_events.id
                               )",
                            rusqlite::params![source_path_id, source_revision_id],
                        )?;
                    }
                    "unchanged" => {
                        let candidates = {
                            let mut statement = transaction.prepare(
                                "SELECT DISTINCT session_id FROM session_sources
                                 WHERE source_revision_id = ?1",
                            )?;
                            statement
                                .query_map([&source_revision_id], |row| row.get::<_, String>(0))?
                                .collect::<Result<Vec<_>, _>>()?
                        };
                        delete_unprotected_sessions(transaction, &candidates, &protected_sessions)?;
                        transaction.execute(
                            "DELETE FROM evidence_events
                             WHERE source_revision_id = ?1
                               AND NOT EXISTS (
                                   SELECT 1 FROM session_evidence link
                                   WHERE link.evidence_event_id = evidence_events.id
                               )",
                            [&source_revision_id],
                        )?;
                    }
                    _ => {
                        return Err(BackendError::BackgroundTask(format!(
                            "unsupported staged source decision: {decision}"
                        )));
                    }
                }
            }
        }
        transaction.execute(
            "UPDATE instances
             SET minecraft_version = COALESCE(?2, minecraft_version),
                 loader = COALESCE(?3, loader),
                 last_seen_at_ms = ?4
             WHERE id = ?1",
            rusqlite::params![
                payload.instance_id,
                payload.minecraft_version,
                payload.loader,
                now,
            ],
        )?;

        for evidence in &payload.evidence {
            transaction.execute(
                "INSERT INTO evidence_events (
                    id, source_revision_id, event_order, line_start, line_end,
                    byte_start, byte_end, kind, observed_local, occurred_at_utc_ms,
                    utc_offset_minutes, timestamp_origin, confidence_score,
                    payload_json, event_key
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                    ?12, ?13, ?14
                 )
                 ON CONFLICT(id) DO NOTHING",
                rusqlite::params![
                    evidence.id,
                    evidence.source_revision_id,
                    as_i64(evidence.event_order),
                    as_i64(evidence.line_number),
                    as_i64(evidence.byte_start),
                    as_i64(evidence.byte_end),
                    evidence.kind,
                    evidence.observed_local,
                    evidence.occurred_at_utc_ms,
                    evidence.utc_offset_minutes,
                    evidence.timestamp_origin,
                    i64::from(evidence.confidence_score),
                    evidence.payload_json,
                    evidence.event_key,
                ],
            )?;
        }

        for session in &payload.sessions {
            reconcile_replaced_sources_with_rotations(transaction, session)?;
            transaction.execute(
                "INSERT INTO sessions (
                    id, instance_id, started_at_utc_ms, ended_at_utc_ms,
                    duration_seconds, exit_kind, confidence_score, confidence_label,
                    confidence_model_revision, reconstruction_revision, canonical_key,
                    timezone_id, minecraft_version, loader, utc_offset_minutes
                 ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?10, ?11,
                    ?12, ?13, ?14
                 )
                 ON CONFLICT(id) DO UPDATE SET
                    instance_id = excluded.instance_id,
                    started_at_utc_ms = excluded.started_at_utc_ms,
                    ended_at_utc_ms = excluded.ended_at_utc_ms,
                    duration_seconds = excluded.duration_seconds,
                    exit_kind = excluded.exit_kind,
                    confidence_score = excluded.confidence_score,
                    confidence_label = excluded.confidence_label,
                    confidence_model_revision = excluded.confidence_model_revision,
                    reconstruction_revision = excluded.reconstruction_revision,
                    canonical_key = excluded.canonical_key,
                    timezone_id = COALESCE(excluded.timezone_id, sessions.timezone_id),
                    minecraft_version = COALESCE(
                        excluded.minecraft_version, sessions.minecraft_version
                    ),
                    loader = COALESCE(excluded.loader, sessions.loader),
                    utc_offset_minutes = COALESCE(
                        excluded.utc_offset_minutes, sessions.utc_offset_minutes
                    )",
                rusqlite::params![
                    session.id,
                    payload.instance_id,
                    session.started_at_utc_ms,
                    session.ended_at_utc_ms,
                    session.duration_seconds.map(as_i64),
                    session.exit_kind,
                    i64::from(session.confidence_score),
                    session.confidence_label,
                    i64::from(session.reconstruction_revision),
                    session.canonical_key,
                    session.timezone_id,
                    session.minecraft_version,
                    session.loader,
                    session.utc_offset_minutes,
                ],
            )?;

            for link in &session.evidence_links {
                transaction.execute(
                    "INSERT OR IGNORE INTO session_evidence (
                        session_id, evidence_event_id, role
                     ) VALUES (?1, ?2, ?3)",
                    rusqlite::params![session.id, link.evidence_event_id, link.role],
                )?;
            }
            for (index, source_revision_id) in session.source_revision_ids.iter().enumerate() {
                transaction.execute(
                    "INSERT OR IGNORE INTO session_sources (
                        session_id, source_revision_id, relation
                     ) VALUES (?1, ?2, ?3)",
                    rusqlite::params![
                        session.id,
                        source_revision_id,
                        if index == 0 { "primary" } else { "supporting" },
                    ],
                )?;
            }
            for (index, activity) in session.activities.iter().enumerate() {
                match activity {
                    StagedActivity::Server {
                        id,
                        canonical_address,
                        original_address,
                        started_at_utc_ms,
                        ended_at_utc_ms,
                        confidence_score,
                    } => {
                        transaction.execute(
                            "INSERT INTO servers (
                                id, canonical_address, original_address,
                                first_seen_at_ms, last_seen_at_ms
                             ) VALUES (?1, ?2, ?3, ?4, ?5)
                             ON CONFLICT(canonical_address) DO UPDATE SET
                                original_address = excluded.original_address,
                                first_seen_at_ms = MIN(first_seen_at_ms, excluded.first_seen_at_ms),
                                last_seen_at_ms = MAX(last_seen_at_ms, excluded.last_seen_at_ms)",
                            rusqlite::params![
                                id,
                                canonical_address,
                                original_address,
                                started_at_utc_ms,
                                ended_at_utc_ms.or(*started_at_utc_ms),
                            ],
                        )?;
                        transaction.execute(
                            "INSERT INTO activity_segments (
                                id, session_id, kind, server_id, started_at_utc_ms,
                                ended_at_utc_ms, confidence_score
                             ) VALUES (?1, ?2, 'server', ?3, ?4, ?5, ?6)
                             ON CONFLICT(id) DO UPDATE SET
                                session_id = excluded.session_id,
                                kind = excluded.kind,
                                server_id = excluded.server_id,
                                world_id = excluded.world_id,
                                started_at_utc_ms = excluded.started_at_utc_ms,
                                ended_at_utc_ms = excluded.ended_at_utc_ms,
                                confidence_score = excluded.confidence_score",
                            rusqlite::params![
                                segment_id(&session.id, index),
                                session.id,
                                id,
                                started_at_utc_ms,
                                ended_at_utc_ms,
                                i64::from(*confidence_score),
                            ],
                        )?;
                    }
                    StagedActivity::World {
                        world_name,
                        started_at_utc_ms,
                        ended_at_utc_ms,
                        confidence_score,
                    } => {
                        transaction.execute(
                            "INSERT INTO activity_segments (
                                id, session_id, kind, world_id, started_at_utc_ms,
                                ended_at_utc_ms, confidence_score
                             ) VALUES (?1, ?2, 'world', ?3, ?4, ?5, ?6)
                             ON CONFLICT(id) DO UPDATE SET
                                session_id = excluded.session_id,
                                kind = excluded.kind,
                                server_id = excluded.server_id,
                                world_id = excluded.world_id,
                                started_at_utc_ms = excluded.started_at_utc_ms,
                                ended_at_utc_ms = excluded.ended_at_utc_ms,
                                confidence_score = excluded.confidence_score",
                            rusqlite::params![
                                segment_id(&session.id, index),
                                session.id,
                                world_name,
                                started_at_utc_ms,
                                ended_at_utc_ms,
                                i64::from(*confidence_score),
                            ],
                        )?;
                    }
                }
            }
            promoted_sessions += 1;
        }
    }
    Ok(promoted_sessions)
}

fn reconcile_replaced_sources_with_rotations(
    transaction: &rusqlite::Transaction<'_>,
    session: &StagedSession,
) -> Result<(), BackendError> {
    let mut superseded = BTreeSet::new();
    for incoming_revision_id in &session.source_revision_ids {
        let mut statement = transaction.prepare(
            "SELECT DISTINCT old.id
             FROM session_sources link
             JOIN source_revisions old ON old.id = link.source_revision_id
             JOIN source_revisions incoming ON incoming.id = ?2
             WHERE link.session_id = ?1
               AND old.id <> incoming.id
               AND EXISTS (
                   SELECT 1 FROM source_revisions later
                   WHERE later.source_path_id = old.source_path_id
                     AND later.generation > old.generation
                     AND later.change_kind = 'replaced'
               )",
        )?;
        superseded.extend(
            statement
                .query_map(rusqlite::params![session.id, incoming_revision_id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<Result<BTreeSet<_>, _>>()?,
        );
    }

    for revision_id in superseded {
        transaction.execute(
            "DELETE FROM session_evidence
             WHERE session_id = ?1
               AND evidence_event_id IN (
                   SELECT id FROM evidence_events WHERE source_revision_id = ?2
               )",
            rusqlite::params![session.id, revision_id],
        )?;
        transaction.execute(
            "DELETE FROM session_sources
             WHERE session_id = ?1 AND source_revision_id = ?2",
            rusqlite::params![session.id, revision_id],
        )?;
        transaction.execute(
            "DELETE FROM evidence_events
             WHERE source_revision_id = ?1
               AND NOT EXISTS (
                   SELECT 1 FROM session_evidence link
                   WHERE link.evidence_event_id = evidence_events.id
               )",
            [revision_id],
        )?;
    }
    Ok(())
}

fn sessions_linked_to_unavailable_or_replaced_sources(
    transaction: &rusqlite::Transaction<'_>,
    scan_id: &str,
    instance_id: &str,
) -> Result<BTreeSet<String>, BackendError> {
    let mut statement = transaction.prepare(
        "SELECT DISTINCT session.id
         FROM sessions session
         JOIN session_sources link ON link.session_id = session.id
         JOIN source_revisions revision ON revision.id = link.source_revision_id
         JOIN source_paths source ON source.id = revision.source_path_id
         WHERE session.instance_id = ?2
           AND (
               (
                   EXISTS (
                       SELECT 1
                       FROM scan_staged_locations scope
                       WHERE scope.scan_id = ?1
                         AND scope.location_id = source.location_id
                         AND scope.instance_id = source.instance_id
                         AND scope.scope_key LIKE 'complete:%'
                   )
                   AND NOT EXISTS (
                       SELECT 1
                       FROM scan_staged_sources staged
                       WHERE staged.scan_id = ?1
                         AND staged.source_path_id = source.id
                   )
               )
               OR EXISTS (
                   SELECT 1
                   FROM source_revisions later
                   WHERE later.source_path_id = revision.source_path_id
                     AND later.generation > revision.generation
                     AND later.change_kind = 'replaced'
               )
           )",
    )?;
    statement
        .query_map(rusqlite::params![scan_id, instance_id], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(Into::into)
}

fn delete_unprotected_sessions(
    transaction: &rusqlite::Transaction<'_>,
    candidates: &[String],
    protected: &BTreeSet<String>,
) -> Result<(), BackendError> {
    for session_id in candidates {
        if !protected.contains(session_id) {
            transaction.execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;
        }
    }
    Ok(())
}

fn segment_id(session_id: &str, index: usize) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(session_id.as_bytes());
    hasher.update(&(index as u64).to_le_bytes());
    let digest = hasher.finalize().to_hex().to_string();
    format!("segment_{}", &digest[..24])
}

fn as_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{ReconstructionPayload, StagedSession};
    use crate::{
        domain::{
            Confidence, PlatformKind,
            location::{AdapterKind, DiscoveredInstallation},
        },
        storage::Database,
    };

    #[test]
    fn canonical_session_archive_limit_counts_only_new_ids_and_replace_all_is_bounded() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("db.sqlite3")).expect("database");
        let location = DiscoveredInstallation {
            id: "location-cap".to_owned(),
            name: "Cap test".to_owned(),
            kind_label: "Manual".to_owned(),
            adapter_kind: AdapterKind::Manual,
            path: temp.path().join("game"),
            instances: 1,
            confidence: Confidence::Verified,
            validation_score: 95,
            enabled: true,
            platform: PlatformKind::Linux,
            origin: "custom",
        };
        database.upsert_scan_location(&location).expect("location");
        let instance = database
            .upsert_instance(&location, &PathBuf::new(), "Cap test")
            .expect("instance");
        database
            .write(|transaction| {
                transaction.execute(
                    "INSERT INTO sessions (
                        id, instance_id, started_at_utc_ms, exit_kind,
                        confidence_score, confidence_label,
                        confidence_model_revision, reconstruction_revision, canonical_key
                     ) VALUES ('session-existing', ?1, 1, 'unknown', 50, 'partial', 1, 1, X'01')",
                    [&instance.id],
                )?;
                Ok(())
            })
            .expect("existing session");

        let mut payload = ReconstructionPayload {
            instance_id: instance.id,
            replace_all_instance_evidence: false,
            source_path_ids: Vec::new(),
            minecraft_version: None,
            loader: None,
            evidence: Vec::new(),
            sessions: vec![staged_session("session-existing")],
        };
        assert!(
            database
                .instance_session_archive_within_limit(&payload, 1)
                .expect("existing id capacity")
        );

        payload.sessions = vec![staged_session("session-new")];
        assert!(
            !database
                .instance_session_archive_within_limit(&payload, 1)
                .expect("new id capacity")
        );

        payload.replace_all_instance_evidence = true;
        assert!(
            database
                .instance_session_archive_within_limit(&payload, 1)
                .expect("replace-all capacity")
        );
        payload.sessions.push(staged_session("session-second-new"));
        assert!(
            !database
                .instance_session_archive_within_limit(&payload, 1)
                .expect("replace-all overflow")
        );
    }

    fn staged_session(id: &str) -> StagedSession {
        StagedSession {
            id: id.to_owned(),
            started_at_utc_ms: 1,
            ended_at_utc_ms: None,
            duration_seconds: None,
            exit_kind: "unknown".to_owned(),
            confidence_score: 50,
            confidence_label: "partial".to_owned(),
            reconstruction_revision: 1,
            canonical_key: id.as_bytes().to_vec(),
            timezone_id: None,
            minecraft_version: None,
            loader: None,
            utc_offset_minutes: None,
            evidence_links: Vec::new(),
            source_revision_ids: Vec::new(),
            activities: Vec::new(),
        }
    }
}
