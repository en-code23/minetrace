use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Datelike, Days, FixedOffset, NaiveDate, TimeZone, Utc};
use rusqlite::params;

use super::read_models::{
    ArchiveState, Coverage, CoverageQuality, DailyActivity, DailyCoverage, DashboardData,
    DashboardTotals, ExitKind, MonthlyActivity, MonthlyCoverage, NamedMinutes, Session,
    SessionContext, SessionContextKind, SessionKind, SessionPage, TopMetrics,
};
use crate::{domain::Confidence, error::BackendError, storage::Database};

const MAX_PLAUSIBLE_SESSION_SECONDS: u64 = 31 * 24 * 60 * 60;
const SESSION_PAGE_LIMIT: i64 = 500;
const DASHBOARD_BATCH_SIZE: i64 = 512;

#[derive(Debug, Clone, Copy)]
pub struct DashboardService;

impl DashboardService {
    pub fn load(self, database: &Database) -> Result<DashboardData, BackendError> {
        database.read(|connection| {
            let (revision, has_completed_scan) = archive_metadata(connection)?;
            if revision == 0 {
                return Ok(empty_dashboard(if has_completed_scan {
                    ArchiveState::ScannedNoEvidence
                } else {
                    ArchiveState::Unscanned
                }));
            }

            let top = query_top_metrics(connection)?;
            let observed_months = query_observed_month_count(connection)?;
            let mut dashboard = DashboardAccumulator::default();
            let mut cursor = None;
            loop {
                let sessions =
                    query_stored_sessions(connection, DASHBOARD_BATCH_SIZE, cursor.as_ref())?;
                let batch_len = sessions.len();
                let next_cursor = sessions.last().map(SessionCursor::from);
                for session in &sessions {
                    dashboard.add(session);
                }
                if batch_len < usize::try_from(DASHBOARD_BATCH_SIZE).unwrap_or(usize::MAX) {
                    break;
                }
                cursor = next_cursor;
            }

            Ok(dashboard.finish(top, observed_months).unwrap_or_else(|| {
                empty_dashboard(if has_completed_scan {
                    ArchiveState::ScannedNoEvidence
                } else {
                    ArchiveState::Unscanned
                })
            }))
        })
    }

    pub fn session_page(self, database: &Database) -> Result<SessionPage, BackendError> {
        database.read(|connection| {
            let total = visible_session_count(connection)?;
            let (revision, _) = archive_metadata(connection)?;
            let sessions = if revision == 0 {
                Vec::new()
            } else {
                query_stored_sessions(connection, SESSION_PAGE_LIMIT, None)?
            };
            let items = sessions.iter().map(session_dto).collect::<Vec<_>>();
            Ok(SessionPage {
                truncated: total > u64::try_from(items.len()).unwrap_or(u64::MAX),
                total,
                items,
            })
        })
    }

    #[cfg(test)]
    pub(crate) fn sessions(self, database: &Database) -> Result<Vec<Session>, BackendError> {
        let (_, _, sessions) = stored_sessions(database, i64::MAX)?;
        Ok(sessions.iter().map(session_dto).collect())
    }
}

#[derive(Debug, Clone)]
struct StoredSession {
    id: String,
    started_at_ms: i64,
    ended_at_ms: Option<i64>,
    duration_seconds: Option<u64>,
    exit_kind: String,
    confidence: String,
    instance: String,
    version: String,
    loader: Option<String>,
    launcher: String,
    kind: String,
    destination: String,
    source: String,
    utc_offset_minutes: Option<i32>,
    server_destinations: Vec<String>,
    world_destinations: Vec<String>,
}

#[derive(Debug, Clone)]
struct SessionCursor {
    started_at_ms: i64,
    id: String,
}

impl From<&StoredSession> for SessionCursor {
    fn from(session: &StoredSession) -> Self {
        Self {
            started_at_ms: session.started_at_ms,
            id: session.id.clone(),
        }
    }
}

fn visible_session_count(connection: &rusqlite::Connection) -> Result<u64, rusqlite::Error> {
    let count = connection.query_row(
        "SELECT COUNT(*)
             FROM sessions session
             LEFT JOIN session_user_state user_state ON user_state.session_id = session.id
             WHERE COALESCE(user_state.ignored, 0) = 0",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(u64::try_from(count.max(0)).unwrap_or_default())
}

#[cfg(test)]
fn stored_sessions(
    database: &Database,
    row_limit: i64,
) -> Result<(i64, bool, Vec<StoredSession>), BackendError> {
    database.read(|connection| {
        let (revision, has_completed_scan) = archive_metadata(connection)?;

        if revision == 0 {
            return Ok((revision, has_completed_scan, Vec::new()));
        }

        Ok((
            revision,
            has_completed_scan,
            query_stored_sessions(connection, row_limit, None)?,
        ))
    })
}

fn archive_metadata(connection: &rusqlite::Connection) -> Result<(i64, bool), rusqlite::Error> {
    let revision: i64 = connection.query_row(
        "SELECT revision FROM dataset_state WHERE id = 1",
        [],
        |row| row.get(0),
    )?;
    let has_completed_scan = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM scan_runs WHERE state = 'completed')",
        [],
        |row| row.get::<_, bool>(0),
    )?;
    Ok((revision, has_completed_scan))
}

fn query_stored_sessions(
    connection: &rusqlite::Connection,
    row_limit: i64,
    cursor: Option<&SessionCursor>,
) -> Result<Vec<StoredSession>, rusqlite::Error> {
    let mut statement = connection.prepare(
            "SELECT
                s.id,
                s.started_at_utc_ms,
                s.ended_at_utc_ms,
                s.duration_seconds,
                s.exit_kind,
                s.confidence_label,
                i.name,
                COALESCE(s.minecraft_version, 'Unknown'),
                s.loader,
                COALESCE(li.display_name, sl.adapter_kind, 'Custom'),
                COALESCE((
                    SELECT CASE
                        WHEN COUNT(DISTINCT segment.kind) > 1 THEN 'mixed'
                        ELSE MIN(segment.kind)
                    END
                    FROM activity_segments segment
                    WHERE segment.session_id = s.id
                ), 'menu'),
                COALESCE((
                    SELECT CASE
                        WHEN COUNT(*) > 1 THEN 'Multiple observed destinations'
                        ELSE MAX(COALESCE(server.display_name, server.original_address, segment.world_id))
                    END
                    FROM activity_segments segment
                    LEFT JOIN servers server ON server.id = segment.server_id
                    WHERE segment.session_id = s.id
                      AND segment.kind IN ('server', 'world')
                ), ''),
                COALESCE((
                    SELECT source_path.relative_path_display
                    FROM session_sources session_source
                    JOIN source_revisions revision ON revision.id = session_source.source_revision_id
                    JOIN source_paths source_path ON source_path.id = revision.source_path_id
                    WHERE session_source.session_id = s.id
                    ORDER BY CASE session_source.relation WHEN 'primary' THEN 0 ELSE 1 END,
                             source_path.relative_path_display
                    LIMIT 1
                ), 'Imported evidence'),
                s.utc_offset_minutes
             FROM sessions s
             JOIN instances i ON i.id = s.instance_id
             JOIN scan_locations sl ON sl.id = i.location_id
             LEFT JOIN launcher_installations li ON li.id = i.installation_id
             LEFT JOIN session_user_state user_state ON user_state.session_id = s.id
             WHERE COALESCE(user_state.ignored, 0) = 0
               AND (
                    ?2 IS NULL
                    OR s.started_at_utc_ms < ?2
                    OR (s.started_at_utc_ms = ?2 AND s.id > ?3)
               )
             ORDER BY s.started_at_utc_ms DESC, s.id
             LIMIT ?1",
        )?;
    let cursor_started_at = cursor.map(|value| value.started_at_ms);
    let cursor_id = cursor.map(|value| value.id.as_str());
    let rows = statement.query_map(params![row_limit, cursor_started_at, cursor_id], |row| {
        Ok(StoredSession {
            id: row.get(0)?,
            started_at_ms: row.get(1)?,
            ended_at_ms: row.get(2)?,
            duration_seconds: row
                .get::<_, Option<i64>>(3)?
                .map(|value| value.max(0) as u64),
            exit_kind: row.get(4)?,
            confidence: row.get(5)?,
            instance: row.get(6)?,
            version: row.get(7)?,
            loader: row.get(8)?,
            launcher: row.get(9)?,
            kind: row.get(10)?,
            destination: row.get(11)?,
            source: row.get(12)?,
            utc_offset_minutes: row.get(13)?,
            server_destinations: Vec::new(),
            world_destinations: Vec::new(),
        })
    })?;
    let mut sessions = rows.collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    let session_indexes = sessions
        .iter()
        .enumerate()
        .map(|(index, session)| (session.id.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut activity_statement = connection.prepare(
        "SELECT
                segment.session_id,
                segment.kind,
                CASE segment.kind
                    WHEN 'server' THEN COALESCE(server.display_name, server.original_address)
                    WHEN 'world' THEN segment.world_id
                END
             FROM activity_segments segment
             LEFT JOIN servers server ON server.id = segment.server_id
             WHERE segment.kind IN ('server', 'world')
               AND segment.session_id IN (
                    SELECT visible.id
                    FROM sessions visible
                    LEFT JOIN session_user_state visible_state
                      ON visible_state.session_id = visible.id
                    WHERE COALESCE(visible_state.ignored, 0) = 0
                      AND (
                           ?2 IS NULL
                           OR visible.started_at_utc_ms < ?2
                           OR (visible.started_at_utc_ms = ?2 AND visible.id > ?3)
                      )
                    ORDER BY visible.started_at_utc_ms DESC, visible.id
                    LIMIT ?1
               )
             ORDER BY segment.session_id, segment.kind, 3 COLLATE NOCASE",
    )?;
    let activities =
        activity_statement.query_map(params![row_limit, cursor_started_at, cursor_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
    for activity in activities {
        let (session_id, kind, destination) = activity?;
        let Some(destination) = destination.filter(|value| !value.trim().is_empty()) else {
            continue;
        };
        let Some(session) = session_indexes
            .get(&session_id)
            .and_then(|index| sessions.get_mut(*index))
        else {
            continue;
        };
        let destinations = if kind == "server" {
            &mut session.server_destinations
        } else {
            &mut session.world_destinations
        };
        if !destinations.contains(&destination) {
            destinations.push(destination);
        }
    }
    Ok(sessions)
}

fn query_top_metrics(connection: &rusqlite::Connection) -> Result<TopMetrics, rusqlite::Error> {
    let mut top = TopMetrics {
        launcher: named("No launcher evidence", 0),
        instance: named("No instance evidence", 0),
        version: named("No version evidence", 0),
        server: named("No server evidence", 0),
        world: named("No world evidence", 0),
    };
    let mut statement = connection.prepare(
        "WITH visible_sessions AS (
            SELECT
                session.*,
                CASE
                    WHEN session.duration_seconds > 2678400
                      OR (
                        session.ended_at_utc_ms IS NOT NULL
                        AND (
                            session.ended_at_utc_ms < session.started_at_utc_ms
                            OR session.ended_at_utc_ms - session.started_at_utc_ms > 2678400000
                            OR (
                                session.duration_seconds IS NOT NULL
                                AND ABS(
                                    (session.ended_at_utc_ms - session.started_at_utc_ms)
                                    - session.duration_seconds * 1000
                                ) >= 1000
                            )
                        )
                      )
                    THEN 0
                    ELSE COALESCE(session.duration_seconds, 0)
                END AS runtime_seconds
            FROM sessions session
            LEFT JOIN session_user_state user_state ON user_state.session_id = session.id
            WHERE COALESCE(user_state.ignored, 0) = 0
         ),
         destination_contexts AS (
            SELECT DISTINCT
                session.id AS session_id,
                segment.kind,
                CASE segment.kind
                    WHEN 'server' THEN COALESCE(
                        NULLIF(TRIM(server.display_name), ''),
                        NULLIF(TRIM(server.original_address), '')
                    )
                    WHEN 'world' THEN NULLIF(TRIM(segment.world_id), '')
                END AS name,
                session.runtime_seconds
            FROM visible_sessions session
            JOIN activity_segments segment ON segment.session_id = session.id
            LEFT JOIN servers server ON server.id = segment.server_id
            WHERE segment.kind IN ('server', 'world')
         ),
         candidates AS (
            SELECT
                'launcher' AS category,
                COALESCE(
                    NULLIF(TRIM(installation.display_name), ''),
                    NULLIF(TRIM(location.adapter_kind), ''),
                    'Custom'
                ) AS name,
                SUM(session.runtime_seconds) AS runtime_seconds
            FROM visible_sessions session
            JOIN instances instance ON instance.id = session.instance_id
            JOIN scan_locations location ON location.id = instance.location_id
            LEFT JOIN launcher_installations installation
              ON installation.id = instance.installation_id
            GROUP BY 2
            UNION ALL
            SELECT 'instance', instance.name, SUM(session.runtime_seconds)
            FROM visible_sessions session
            JOIN instances instance ON instance.id = session.instance_id
            GROUP BY instance.name
            UNION ALL
            SELECT
                'version',
                COALESCE(NULLIF(TRIM(session.minecraft_version), ''), 'Unknown'),
                SUM(session.runtime_seconds)
            FROM visible_sessions session
            GROUP BY 2
            UNION ALL
            SELECT kind, name, SUM(runtime_seconds)
            FROM destination_contexts
            WHERE name IS NOT NULL
            GROUP BY kind, name
         ),
         ranked AS (
            SELECT
                category,
                name,
                runtime_seconds,
                ROW_NUMBER() OVER (
                    PARTITION BY category
                    ORDER BY runtime_seconds DESC, name COLLATE NOCASE, name
                ) AS rank
            FROM candidates
            WHERE NULLIF(TRIM(name), '') IS NOT NULL
         )
         SELECT category, name, runtime_seconds
         FROM ranked
         WHERE rank = 1",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (category, raw_name, seconds) = row?;
        let display_name = if category == "launcher" {
            display_launcher(&raw_name)
        } else {
            raw_name
        };
        let value = named(
            &display_name,
            u64::try_from(seconds.max(0)).unwrap_or_default() / 60,
        );
        match category.as_str() {
            "launcher" => top.launcher = value,
            "instance" => top.instance = value,
            "version" => top.version = value,
            "server" => top.server = value,
            "world" => top.world = value,
            _ => {}
        }
    }
    Ok(top)
}

fn query_observed_month_count(connection: &rusqlite::Connection) -> Result<u32, rusqlite::Error> {
    let count = connection.query_row(
        "WITH RECURSIVE day_offsets(day) AS (
            VALUES(0)
            UNION ALL
            SELECT day + 1 FROM day_offsets WHERE day < 31
         ),
         visible_sessions AS (
            SELECT
                session.started_at_utc_ms,
                CASE
                    WHEN session.utc_offset_minutes BETWEEN -1439 AND 1439
                    THEN session.utc_offset_minutes
                    ELSE 0
                END AS utc_offset_minutes,
                CASE
                    WHEN session.duration_seconds IS NULL
                      OR session.duration_seconds > 2678400
                      OR (
                        session.ended_at_utc_ms IS NOT NULL
                        AND (
                            session.ended_at_utc_ms < session.started_at_utc_ms
                            OR session.ended_at_utc_ms - session.started_at_utc_ms > 2678400000
                            OR ABS(
                                (session.ended_at_utc_ms - session.started_at_utc_ms)
                                - session.duration_seconds * 1000
                            ) >= 1000
                        )
                      )
                    THEN 0
                    ELSE session.duration_seconds
                END AS allocation_seconds
            FROM sessions session
            LEFT JOIN session_user_state user_state ON user_state.session_id = session.id
            WHERE COALESCE(user_state.ignored, 0) = 0
         ),
         local_bounds AS (
            SELECT
                date(
                    started_at_utc_ms / 1000.0,
                    'unixepoch',
                    printf('%+d minutes', utc_offset_minutes)
                ) AS start_date,
                date(
                    (
                        started_at_utc_ms
                        + CASE WHEN allocation_seconds > 0
                               THEN allocation_seconds * 1000 - 1
                               ELSE 0 END
                    ) / 1000.0,
                    'unixepoch',
                    printf('%+d minutes', utc_offset_minutes)
                ) AS end_date
            FROM visible_sessions
         ),
         observed_months AS (
            SELECT DISTINCT strftime(
                '%Y-%m',
                date(bounds.start_date, printf('+%d days', day_offsets.day))
            ) AS month
            FROM local_bounds bounds
            JOIN day_offsets
              ON date(bounds.start_date, printf('+%d days', day_offsets.day)) <= bounds.end_date
            WHERE bounds.start_date IS NOT NULL
              AND bounds.end_date IS NOT NULL
         )
         SELECT COUNT(*) FROM observed_months",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(u32::try_from(count.max(0)).unwrap_or(u32::MAX))
}

fn empty_dashboard(archive_state: ArchiveState) -> DashboardData {
    let now = Utc::now();
    let today = now.date_naive();
    let warning = match archive_state {
        ArchiveState::Unscanned => {
            "No completed scan has run yet. Start a scan to build the local archive."
        }
        ArchiveState::ScannedNoEvidence => {
            "A scan completed, but it found no reconstructable session evidence."
        }
        ArchiveState::Ready => "No session evidence is currently included in the dashboard.",
    };
    DashboardData {
        generated_at: now.to_rfc3339(),
        archive_state,
        coverage: Coverage {
            first_detected_at: now.to_rfc3339(),
            last_detected_at: now.to_rfc3339(),
            quality: CoverageQuality::Unknown,
            score: 0,
            verified_share: 0.0,
            warning: warning.to_owned(),
            observed_months: 0,
            gap_months: 0,
        },
        totals: DashboardTotals {
            playtime_minutes: 0,
            unique_playtime_minutes: 0,
            sessions: 0,
            active_days: 0,
            longest_session_minutes: None,
            average_session_minutes: None,
        },
        top: TopMetrics {
            launcher: named("No evidence", 0),
            instance: named("No evidence", 0),
            version: named("No evidence", 0),
            server: named("No server evidence", 0),
            world: named("No world evidence", 0),
        },
        monthly: Vec::new(),
        daily: calendar_days(today, &HashMap::new()),
        recent_sessions: Vec::new(),
    }
}

#[cfg(test)]
fn build_dashboard(mut stored: Vec<StoredSession>) -> DashboardData {
    stored.sort_by(|left, right| {
        right
            .started_at_ms
            .cmp(&left.started_at_ms)
            .then_with(|| left.id.cmp(&right.id))
    });
    let top = top_metrics_from_sessions(&stored);
    let mut dashboard = DashboardAccumulator::default();
    for session in &stored {
        dashboard.add(session);
    }
    dashboard
        .finish(top, observed_month_count_from_sessions(&stored))
        .expect("non-empty session archive produces a dashboard")
}

#[derive(Debug, Default)]
struct DashboardAccumulator {
    total_seconds: u64,
    longest_seconds: u64,
    known_duration_sessions: u64,
    session_count: u64,
    verified_sessions: u64,
    bounded_sessions: u64,
    interval_union: IntervalUnion,
    active_days: ActiveDayAccumulator,
    monthly: BTreeMap<i32, MonthAccumulator>,
    daily: HashMap<NaiveDate, DayAccumulator>,
    calendar_end: Option<NaiveDate>,
    first_observation: Option<(i64, Option<i32>)>,
    last_observation: Option<(i64, Option<i32>)>,
    recent_sessions: Vec<Session>,
}

impl DashboardAccumulator {
    fn add(&mut self, session: &StoredSession) {
        let bounds = session_bounds(session);
        let seconds = bounds.duration_seconds.unwrap_or(0);
        if bounds.duration_seconds.is_some() {
            self.known_duration_sessions = self.known_duration_sessions.saturating_add(1);
        }
        let confidence = if bounds.implausible {
            weaker_confidence(parse_confidence(&session.confidence), Confidence::Partial)
        } else {
            parse_confidence(&session.confidence)
        };
        self.session_count = self.session_count.saturating_add(1);
        self.total_seconds = self.total_seconds.saturating_add(seconds);
        self.longest_seconds = self.longest_seconds.max(seconds);
        if confidence == Confidence::Verified {
            self.verified_sessions = self.verified_sessions.saturating_add(1);
        }
        if bounds.ended_at_ms.is_some() && bounds.duration_seconds.is_some() {
            self.bounded_sessions = self.bounded_sessions.saturating_add(1);
        }
        if self.recent_sessions.len() < 24 {
            self.recent_sessions.push(session_dto(session));
        }
        if self
            .first_observation
            .is_none_or(|(started_at, _)| session.started_at_ms < started_at)
        {
            self.first_observation = Some((session.started_at_ms, session.utc_offset_minutes));
        }

        let interval_end = bounds.ended_at_ms.or_else(|| {
            bounds
                .duration_seconds
                .and_then(|duration| i64::try_from(duration.saturating_mul(1_000)).ok())
                .map(|duration| session.started_at_ms.saturating_add(duration))
        });
        let observed_end = interval_end.unwrap_or(session.started_at_ms);
        if self
            .last_observation
            .is_none_or(|(ended_at, _)| observed_end > ended_at)
        {
            self.last_observation = Some((observed_end, session.utc_offset_minutes));
        }
        self.update_calendar_end(
            observed_datetime(observed_end, session.utc_offset_minutes).date_naive(),
        );
        if let Some(end) = interval_end
            && end > session.started_at_ms
        {
            self.interval_union.add(session.started_at_ms, end);
        }

        let mut session_months = BTreeSet::new();
        let allocations = session_day_allocations(session, bounds);
        let start_date =
            observed_datetime(session.started_at_ms, session.utc_offset_minutes).date_naive();
        self.active_days.add(start_date, &allocations);
        for (date, slice_seconds) in allocations {
            let month_key = month_index(date);
            if self.month_is_in_calendar(month_key) {
                session_months.insert(month_key);
                let month = self.monthly.entry(month_key).or_default();
                month.seconds = month.seconds.saturating_add(slice_seconds);
                month.confidence = Some(match month.confidence {
                    Some(current) => weaker_confidence(current, confidence),
                    None => confidence,
                });
                if confidence != Confidence::Verified {
                    month.estimated_seconds = month.estimated_seconds.saturating_add(slice_seconds);
                }
            }

            if self.day_is_in_calendar(date) {
                let day = self.daily.entry(date).or_default();
                day.seconds = day.seconds.saturating_add(slice_seconds);
                day.sessions = day.sessions.saturating_add(1);
                day.confidence = Some(match day.confidence {
                    Some(current) => weaker_confidence(current, confidence),
                    None => confidence,
                });
            }
        }
        for month_key in session_months {
            if let Some(month) = self.monthly.get_mut(&month_key) {
                month.sessions = month.sessions.saturating_add(1);
            }
        }
    }

    fn update_calendar_end(&mut self, date: NaiveDate) {
        if self.calendar_end.is_some_and(|current| date <= current) {
            return;
        }
        self.calendar_end = Some(date);
        let earliest = date
            .checked_sub_days(Days::new(364))
            .unwrap_or(NaiveDate::MIN);
        self.daily
            .retain(|observed_date, _| *observed_date >= earliest && *observed_date <= date);
        let earliest_month = month_index(date).saturating_sub(11);
        let latest_month = month_index(date);
        self.monthly
            .retain(|month, _| *month >= earliest_month && *month <= latest_month);
    }

    fn day_is_in_calendar(&self, date: NaiveDate) -> bool {
        self.calendar_end.is_some_and(|end| {
            date <= end
                && date
                    >= end
                        .checked_sub_days(Days::new(364))
                        .unwrap_or(NaiveDate::MIN)
        })
    }

    fn month_is_in_calendar(&self, month: i32) -> bool {
        self.calendar_end.is_some_and(|end| {
            let latest = month_index(end);
            month >= latest.saturating_sub(11) && month <= latest
        })
    }

    fn finish(self, top: TopMetrics, observed_months: u32) -> Option<DashboardData> {
        let (first_ms, first_offset) = self.first_observation?;
        let (last_ms, last_offset) = self.last_observation?;
        let first = observed_datetime(first_ms, first_offset);
        let last = observed_datetime(last_ms, last_offset);
        let first_date = first.date_naive();
        let last_date = last.date_naive();
        let month_span = months_inclusive(first_date, last_date);
        let gap_months = month_span.saturating_sub(observed_months);
        let verified_share = self.verified_sessions as f64 / self.session_count as f64;
        let bounded_share = self.bounded_sessions as f64 / self.session_count as f64;
        // The current log slice measures boundary quality, not lifetime completeness.
        // Keep the public coverage score conservative until roots and missing periods
        // are modeled explicitly enough to justify a verified archive label.
        let coverage_score =
            (((verified_share * 62.0) + (bounded_share * 38.0)).round() as u8).min(79);
        let quality = match coverage_score {
            55..=u8::MAX => CoverageQuality::Partial,
            20..=54 => CoverageQuality::Limited,
            _ => CoverageQuality::Unknown,
        };
        let active_days = self.active_days.finish();
        let unique_seconds = self.interval_union.finish();

        Some(DashboardData {
            generated_at: Utc::now().to_rfc3339(),
            archive_state: ArchiveState::Ready,
            coverage: Coverage {
                first_detected_at: first.to_rfc3339(),
                last_detected_at: last.to_rfc3339(),
                quality,
                score: coverage_score,
                verified_share,
                warning: format!(
                    "Reconstructed from {} source-backed sessions. Coverage remains conservative because absent logs are not proof of no play.",
                    self.session_count
                ),
                observed_months,
                gap_months,
            },
            totals: DashboardTotals {
                playtime_minutes: self.total_seconds / 60,
                unique_playtime_minutes: unique_seconds / 60,
                sessions: self.session_count,
                active_days,
                longest_session_minutes: (self.known_duration_sessions > 0)
                    .then_some(self.longest_seconds / 60),
                average_session_minutes: self
                    .total_seconds
                    .checked_div(self.known_duration_sessions)
                    .map(|seconds| seconds / 60),
            },
            top,
            monthly: calendar_months(first_date, last_date, &self.monthly),
            daily: calendar_days(last_date, &self.daily),
            recent_sessions: self.recent_sessions,
        })
    }
}

#[derive(Debug, Default)]
struct IntervalUnion {
    current: Option<(i64, i64)>,
    completed_millis: u64,
}

impl IntervalUnion {
    fn add(&mut self, start: i64, end: i64) {
        let Some((current_start, current_end)) = self.current else {
            self.current = Some((start, end));
            return;
        };
        if end >= current_start {
            self.current = Some((start.min(current_start), end.max(current_end)));
        } else {
            self.completed_millis = self.completed_millis.saturating_add(
                u64::try_from(current_end.saturating_sub(current_start)).unwrap_or_default(),
            );
            self.current = Some((start, end));
        }
    }

    fn finish(mut self) -> u64 {
        if let Some((start, end)) = self.current.take() {
            self.completed_millis = self
                .completed_millis
                .saturating_add(u64::try_from(end.saturating_sub(start)).unwrap_or_default());
        }
        self.completed_millis / 1_000
    }
}

#[derive(Debug, Default)]
struct ActiveDayAccumulator {
    pending: BTreeMap<NaiveDate, u64>,
    completed: u64,
}

impl ActiveDayAccumulator {
    fn add(&mut self, start_date: NaiveDate, allocations: &[(NaiveDate, u64)]) {
        // Sessions are streamed newest-first and a plausible session spans at
        // most 31 days. Two extra days cover fixed-offset differences between
        // adjacent sessions before a date can be finalized safely.
        let cutoff = start_date
            .checked_add_days(Days::new(33))
            .unwrap_or(NaiveDate::MAX);
        let finalized = self
            .pending
            .range((
                std::ops::Bound::Excluded(cutoff),
                std::ops::Bound::Unbounded,
            ))
            .map(|(date, _)| *date)
            .collect::<Vec<_>>();
        for date in finalized {
            if self
                .pending
                .remove(&date)
                .is_some_and(|seconds| seconds / 60 > 0)
            {
                self.completed = self.completed.saturating_add(1);
            }
        }
        for (date, seconds) in allocations {
            let total = self.pending.entry(*date).or_default();
            *total = total.saturating_add(*seconds);
        }
    }

    fn finish(self) -> u64 {
        self.completed.saturating_add(
            u64::try_from(
                self.pending
                    .into_values()
                    .filter(|seconds| seconds / 60 > 0)
                    .count(),
            )
            .unwrap_or(u64::MAX),
        )
    }
}

#[derive(Debug, Default)]
struct MonthAccumulator {
    seconds: u64,
    sessions: u64,
    estimated_seconds: u64,
    confidence: Option<Confidence>,
}

#[derive(Debug, Default)]
struct DayAccumulator {
    seconds: u64,
    sessions: u64,
    confidence: Option<Confidence>,
}

fn calendar_months(
    first: NaiveDate,
    last: NaiveDate,
    observed: &BTreeMap<i32, MonthAccumulator>,
) -> Vec<MonthlyActivity> {
    let first_index = month_index(first);
    let last_index = month_index(last);
    let start_index = first_index.max(last_index.saturating_sub(11));

    (start_index..=last_index)
        .filter_map(month_from_index)
        .map(|date| {
            let month = date.format("%Y-%m").to_string();
            let label = date.format("%b").to_string();
            match observed.get(&month_index(date)) {
                Some(values) => MonthlyActivity {
                    month,
                    label,
                    minutes: Some(values.seconds / 60),
                    sessions: Some(values.sessions),
                    estimated_share: Some(if values.seconds == 0 {
                        0.0
                    } else {
                        values.estimated_seconds as f64 / values.seconds as f64
                    }),
                    confidence: values.confidence.unwrap_or(Confidence::Unknown),
                    coverage: MonthlyCoverage::Observed,
                },
                None => MonthlyActivity {
                    month,
                    label,
                    minutes: None,
                    sessions: None,
                    estimated_share: None,
                    confidence: Confidence::Unknown,
                    coverage: MonthlyCoverage::Missing,
                },
            }
        })
        .collect()
}

fn calendar_days(
    end: NaiveDate,
    observed: &HashMap<NaiveDate, DayAccumulator>,
) -> Vec<DailyActivity> {
    (0_u64..365)
        .rev()
        .filter_map(|offset| end.checked_sub_days(Days::new(offset)))
        .map(|date| match observed.get(&date) {
            Some(day) => DailyActivity {
                date: date.format("%Y-%m-%d").to_string(),
                minutes: Some(day.seconds / 60),
                sessions: Some(day.sessions),
                confidence: day.confidence.unwrap_or(Confidence::Unknown),
                coverage: DailyCoverage::Observed,
            },
            None => DailyActivity {
                date: date.format("%Y-%m-%d").to_string(),
                minutes: None,
                sessions: None,
                confidence: Confidence::Unknown,
                coverage: DailyCoverage::Missing,
            },
        })
        .collect()
}

fn session_dto(session: &StoredSession) -> Session {
    let bounds = session_bounds(session);
    let confidence = if bounds.implausible {
        weaker_confidence(parse_confidence(&session.confidence), Confidence::Partial)
    } else {
        parse_confidence(&session.confidence)
    };
    let contexts = session
        .server_destinations
        .iter()
        .map(|name| SessionContext {
            kind: SessionContextKind::Server,
            name: name.clone(),
        })
        .chain(
            session
                .world_destinations
                .iter()
                .map(|name| SessionContext {
                    kind: SessionContextKind::World,
                    name: name.clone(),
                }),
        )
        .collect();
    Session {
        id: session.id.clone(),
        started_at: observed_datetime(session.started_at_ms, session.utc_offset_minutes)
            .to_rfc3339(),
        ended_at: bounds
            .ended_at_ms
            .map(|value| observed_datetime(value, session.utc_offset_minutes).to_rfc3339()),
        duration_minutes: bounds.duration_seconds.map(|seconds| seconds / 60),
        launcher: display_launcher(&session.launcher),
        instance: session.instance.clone(),
        version: session.version.clone(),
        loader: session.loader.clone(),
        kind: match session.kind.as_str() {
            "server" => SessionKind::Server,
            "world" => SessionKind::World,
            "mixed" => SessionKind::Mixed,
            _ => SessionKind::Menu,
        },
        destination: (!session.destination.is_empty()).then(|| session.destination.clone()),
        contexts,
        exit_kind: match session.exit_kind.as_str() {
            _ if bounds.implausible => ExitKind::Unknown,
            "clean" => ExitKind::Clean,
            "crash" => ExitKind::Crash,
            "forced" => ExitKind::Forced,
            _ => ExitKind::Unknown,
        },
        confidence,
        source: session.source.clone(),
        note: None,
    }
}

fn utc_from_millis(value: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(value).unwrap_or(DateTime::UNIX_EPOCH)
}

fn observed_datetime(value: i64, utc_offset_minutes: Option<i32>) -> DateTime<FixedOffset> {
    utc_from_millis(value).with_timezone(&observed_offset(utc_offset_minutes))
}

fn observed_offset(utc_offset_minutes: Option<i32>) -> FixedOffset {
    let offset_seconds = utc_offset_minutes
        .and_then(|minutes| minutes.checked_mul(60))
        .unwrap_or(0);
    FixedOffset::east_opt(offset_seconds)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero UTC offset is valid"))
}

#[derive(Debug, Clone, Copy)]
struct SessionBounds {
    ended_at_ms: Option<i64>,
    duration_seconds: Option<u64>,
    implausible: bool,
}

fn session_bounds(session: &StoredSession) -> SessionBounds {
    let mut implausible = session
        .duration_seconds
        .is_some_and(|duration| duration > MAX_PLAUSIBLE_SESSION_SECONDS);
    let elapsed_ms = session.ended_at_ms.and_then(|end| {
        end.checked_sub(session.started_at_ms)
            .filter(|elapsed| *elapsed >= 0)
            .and_then(|elapsed| u64::try_from(elapsed).ok())
    });
    if session.ended_at_ms.is_some()
        && elapsed_ms
            .is_none_or(|elapsed| elapsed > MAX_PLAUSIBLE_SESSION_SECONDS.saturating_mul(1_000))
    {
        implausible = true;
    }
    if let (Some(elapsed), Some(duration)) = (elapsed_ms, session.duration_seconds)
        && elapsed.abs_diff(duration.saturating_mul(1_000)) >= 1_000
    {
        implausible = true;
    }

    if implausible {
        SessionBounds {
            ended_at_ms: None,
            duration_seconds: None,
            implausible: true,
        }
    } else {
        SessionBounds {
            ended_at_ms: session.ended_at_ms,
            duration_seconds: session.duration_seconds,
            implausible: false,
        }
    }
}

fn session_day_allocations(
    session: &StoredSession,
    bounds: SessionBounds,
) -> Vec<(NaiveDate, u64)> {
    let start = session.started_at_ms;
    let duration_seconds = bounds.duration_seconds.unwrap_or(0);
    let start_date = observed_datetime(start, session.utc_offset_minutes).date_naive();
    if duration_seconds == 0 {
        return vec![(start_date, 0)];
    }

    let duration_ms = i64::try_from(duration_seconds.saturating_mul(1_000)).unwrap_or(i64::MAX);
    let end = start.saturating_add(duration_ms);
    let offset = observed_offset(session.utc_offset_minutes);
    let mut cursor = start;
    let mut allocations = Vec::new();

    while cursor < end {
        let local = utc_from_millis(cursor).with_timezone(&offset);
        let date = local.date_naive();
        let next_date = date.succ_opt().unwrap_or(date);
        let next_midnight = next_date
            .and_hms_opt(0, 0, 0)
            .and_then(|naive| offset.from_local_datetime(&naive).single())
            .map(|value| value.timestamp_millis())
            .unwrap_or(end);
        let boundary = end.min(next_midnight.max(cursor.saturating_add(1)));
        let slice_seconds =
            u64::try_from(boundary.saturating_sub(cursor) / 1_000).unwrap_or_default();
        allocations.push((date, slice_seconds));
        cursor = boundary;
    }

    allocations
}

fn parse_confidence(value: &str) -> Confidence {
    match value {
        "verified" => Confidence::Verified,
        "high" => Confidence::High,
        "partial" => Confidence::Partial,
        _ => Confidence::Unknown,
    }
}

fn weaker_confidence(left: Confidence, right: Confidence) -> Confidence {
    fn rank(value: Confidence) -> u8 {
        match value {
            Confidence::Verified => 4,
            Confidence::High => 3,
            Confidence::Partial => 2,
            Confidence::Unknown => 1,
        }
    }
    if rank(right) < rank(left) {
        right
    } else {
        left
    }
}

#[cfg(test)]
fn add_runtime(values: &mut HashMap<String, u64>, name: &str, seconds: u64) {
    if !name.trim().is_empty() {
        let entry = values.entry(name.to_owned()).or_default();
        *entry = entry.saturating_add(seconds);
    }
}

#[cfg(test)]
fn top_runtime(values: HashMap<String, u64>, empty_label: &str) -> NamedMinutes {
    values
        .into_iter()
        .max_by(|(left_name, left), (right_name, right)| {
            left.cmp(right).then_with(|| right_name.cmp(left_name))
        })
        .map(|(name, seconds)| named(&name, seconds / 60))
        .unwrap_or_else(|| named(empty_label, 0))
}

#[cfg(test)]
fn top_metrics_from_sessions(sessions: &[StoredSession]) -> TopMetrics {
    let mut launchers = HashMap::new();
    let mut instances = HashMap::new();
    let mut versions = HashMap::new();
    let mut servers = HashMap::new();
    let mut worlds = HashMap::new();
    for session in sessions {
        let seconds = session_bounds(session).duration_seconds.unwrap_or(0);
        add_runtime(
            &mut launchers,
            &display_launcher(&session.launcher),
            seconds,
        );
        add_runtime(&mut instances, &session.instance, seconds);
        add_runtime(&mut versions, &session.version, seconds);
        for destination in &session.server_destinations {
            add_runtime(&mut servers, destination, seconds);
        }
        for destination in &session.world_destinations {
            add_runtime(&mut worlds, destination, seconds);
        }
    }
    TopMetrics {
        launcher: top_runtime(launchers, "No launcher evidence"),
        instance: top_runtime(instances, "No instance evidence"),
        version: top_runtime(versions, "No version evidence"),
        server: top_runtime(servers, "No server evidence"),
        world: top_runtime(worlds, "No world evidence"),
    }
}

#[cfg(test)]
fn observed_month_count_from_sessions(sessions: &[StoredSession]) -> u32 {
    let mut observed = BTreeSet::new();
    for session in sessions {
        for (date, _) in session_day_allocations(session, session_bounds(session)) {
            observed.insert(month_index(date));
        }
    }
    u32::try_from(observed.len()).unwrap_or(u32::MAX)
}

fn display_launcher(value: &str) -> String {
    match value {
        "official" => "Official Launcher".to_owned(),
        "prism" => "Prism Launcher".to_owned(),
        "multimc" => "MultiMC".to_owned(),
        "manual" => "Custom location".to_owned(),
        other => other.to_owned(),
    }
}

fn months_inclusive(first: NaiveDate, last: NaiveDate) -> u32 {
    let first_index = month_index(first);
    let last_index = month_index(last);
    u32::try_from((last_index - first_index + 1).max(0)).unwrap_or(0)
}

fn month_index(date: NaiveDate) -> i32 {
    date.year() * 12 + i32::try_from(date.month0()).unwrap_or(0)
}

fn month_from_index(index: i32) -> Option<NaiveDate> {
    let year = index.div_euclid(12);
    let month = u32::try_from(index.rem_euclid(12) + 1).ok()?;
    NaiveDate::from_ymd_opt(year, month, 1)
}

fn named(name: &str, minutes: u64) -> NamedMinutes {
    NamedMinutes {
        name: name.to_owned(),
        minutes,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, TimeZone, Utc};
    use tempfile::tempdir;

    use super::{DashboardService, StoredSession, build_dashboard, observed_datetime};
    use crate::{
        application::read_models::{
            ArchiveState, CoverageQuality, MonthlyCoverage, SessionContextKind, SessionKind,
        },
        domain::Confidence,
        scan::ScanMode,
        storage::Database,
    };

    #[test]
    fn empty_dashboard_distinguishes_unscanned_from_completed_without_evidence() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("archive-state.sqlite3")).expect("database");

        let unscanned = DashboardService
            .load(&database)
            .expect("unscanned dashboard");
        assert_eq!(unscanned.archive_state, ArchiveState::Unscanned);
        assert!(unscanned.coverage.warning.contains("No completed scan"));

        let scan = database.begin_scan(ScanMode::Standard).expect("begin scan");
        database
            .promote_scan(&scan.id)
            .expect("complete empty scan");

        let scanned = DashboardService
            .load(&database)
            .expect("scanned-no-evidence dashboard");
        assert_eq!(scanned.archive_state, ArchiveState::ScannedNoEvidence);
        assert!(scanned.coverage.warning.contains("scan completed"));
        let serialized = serde_json::to_value(scanned).expect("dashboard JSON");
        assert_eq!(serialized["archiveState"], "scannedNoEvidence");
    }

    #[test]
    fn multiple_servers_remain_a_server_session_and_expose_every_context() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("contexts.sqlite3")).expect("database");
        database
            .write(|transaction| {
                transaction.execute_batch(
                    "INSERT INTO scan_locations (
                        id, origin, adapter_kind, platform, path_key, path_display,
                        enabled, validation_score, created_at_ms, updated_at_ms
                     ) VALUES (
                        'location', 'custom', 'manual', 'linux', X'01', '/redacted',
                        1, 90, 1, 1
                     );
                     INSERT INTO instances (
                        id, location_id, relative_path_key, relative_path_display, name,
                        confidence_score, first_seen_at_ms, last_seen_at_ms
                     ) VALUES ('instance', 'location', X'02', '.', 'Profile', 90, 1, 1);
                     INSERT INTO sessions (
                        id, instance_id, started_at_utc_ms, ended_at_utc_ms, duration_seconds,
                        exit_kind, confidence_score, confidence_label,
                        confidence_model_revision, reconstruction_revision, canonical_key
                     ) VALUES (
                        'session', 'instance', 1000000, 4600000, 3600,
                        'clean', 90, 'verified', 1, 1, X'03'
                     );
                     INSERT INTO servers (
                        id, canonical_address, original_address, first_seen_at_ms, last_seen_at_ms
                     ) VALUES
                        ('server-a', 'a.example.net', 'a.example.net', 1000000, 4600000),
                        ('server-b', 'b.example.net', 'b.example.net', 1000000, 4600000);
                     INSERT INTO activity_segments (
                        id, session_id, kind, server_id, started_at_utc_ms,
                        ended_at_utc_ms, confidence_score
                     ) VALUES
                        ('segment-a', 'session', 'server', 'server-a', 1000000, 4600000, 90),
                        ('segment-b', 'session', 'server', 'server-b', 1000000, 4600000, 90);
                     UPDATE dataset_state SET revision = 1 WHERE id = 1;",
                )?;
                Ok(())
            })
            .expect("canonical session fixture");

        let sessions = DashboardService.sessions(&database).expect("session DTOs");

        assert_eq!(sessions.len(), 1);
        assert!(matches!(sessions[0].kind, SessionKind::Server));
        assert_eq!(sessions[0].contexts.len(), 2);
        assert!(sessions[0].contexts.iter().all(|context| {
            context.kind == SessionContextKind::Server
                && matches!(context.name.as_str(), "a.example.net" | "b.example.net")
        }));
    }

    #[test]
    fn session_page_is_bounded_newest_first_and_reports_truthful_truncation() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("session-page.sqlite3")).expect("database");
        database
            .write(|transaction| {
                transaction.execute_batch(
                    "INSERT INTO scan_locations (
                        id, origin, adapter_kind, platform, path_key, path_display,
                        enabled, validation_score, created_at_ms, updated_at_ms
                     ) VALUES (
                        'location', 'custom', 'manual', 'linux', X'01', '/redacted',
                        1, 90, 1, 1
                     );
                     INSERT INTO instances (
                        id, location_id, relative_path_key, relative_path_display, name,
                        confidence_score, first_seen_at_ms, last_seen_at_ms
                     ) VALUES ('instance', 'location', X'02', '.', 'Profile', 90, 1, 1);",
                )?;
                for index in 0_i64..501 {
                    let id = format!("session-{index}");
                    let started_at = index.saturating_mul(100_000);
                    transaction.execute(
                        "INSERT INTO sessions (
                            id, instance_id, started_at_utc_ms, ended_at_utc_ms,
                            duration_seconds, exit_kind, confidence_score, confidence_label,
                            confidence_model_revision, reconstruction_revision, canonical_key
                         ) VALUES (?1, 'instance', ?2, ?3, 60, 'clean', 90, 'verified', 1, 1, ?4)",
                        rusqlite::params![
                            id,
                            started_at,
                            started_at.saturating_add(60_000),
                            format!("canonical-{index}").into_bytes(),
                        ],
                    )?;
                }
                transaction.execute("UPDATE dataset_state SET revision = 1 WHERE id = 1", [])?;
                Ok(())
            })
            .expect("session archive fixture");

        let page = DashboardService
            .session_page(&database)
            .expect("bounded session page");

        assert_eq!(page.total, 501);
        assert!(page.truncated);
        assert_eq!(page.items.len(), 500);
        assert_eq!(page.items.first().expect("newest").id, "session-500");
        assert_eq!(page.items.last().expect("page boundary").id, "session-1");
    }

    #[test]
    fn dashboard_keyset_streams_every_session_across_equal_timestamp_batch_boundaries() {
        let temp = tempdir().expect("tempdir");
        let database =
            Database::open(temp.path().join("dashboard-stream.sqlite3")).expect("database");
        database
            .write(|transaction| {
                transaction.execute_batch(
                    "INSERT INTO scan_locations (
                        id, origin, adapter_kind, platform, path_key, path_display,
                        enabled, validation_score, created_at_ms, updated_at_ms
                     ) VALUES (
                        'location', 'custom', 'manual', 'linux', X'01', '/redacted',
                        1, 90, 1, 1
                     );
                     INSERT INTO instances (
                        id, location_id, relative_path_key, relative_path_display, name,
                        confidence_score, first_seen_at_ms, last_seen_at_ms
                     ) VALUES ('instance', 'location', X'02', '.', 'Profile', 90, 1, 1);",
                )?;
                let started_at = 1_700_000_000_000_i64;
                for index in 0_i64..2_050 {
                    transaction.execute(
                        "INSERT INTO sessions (
                            id, instance_id, started_at_utc_ms, ended_at_utc_ms,
                            duration_seconds, exit_kind, confidence_score, confidence_label,
                            confidence_model_revision, reconstruction_revision, canonical_key,
                            minecraft_version, utc_offset_minutes
                         ) VALUES (
                            ?1, 'instance', ?2, ?3, 60, 'clean', 90, 'verified',
                            1, 1, ?4, '1.20.1', 0
                         )",
                        rusqlite::params![
                            format!("session-{index:04}"),
                            started_at,
                            started_at + 60_000,
                            index.to_le_bytes().to_vec(),
                        ],
                    )?;
                }
                transaction.execute("UPDATE dataset_state SET revision = 1 WHERE id = 1", [])?;
                Ok(())
            })
            .expect("streaming archive fixture");

        let dashboard = DashboardService
            .load(&database)
            .expect("streamed dashboard");

        assert_eq!(dashboard.totals.sessions, 2_050);
        assert_eq!(dashboard.totals.playtime_minutes, 2_050);
        assert_eq!(dashboard.totals.unique_playtime_minutes, 1);
        assert_eq!(dashboard.top.instance.name, "Profile");
        assert_eq!(dashboard.top.instance.minutes, 2_050);
        assert_eq!(dashboard.coverage.observed_months, 1);
        assert_eq!(dashboard.recent_sessions.len(), 24);
        assert_eq!(dashboard.recent_sessions[0].id, "session-0000");
        assert_eq!(dashboard.recent_sessions[23].id, "session-0023");
    }

    #[test]
    fn observed_offsets_keep_sessions_on_their_source_local_date() {
        let late_utc = Utc
            .with_ymd_and_hms(2026, 8, 6, 23, 30, 0)
            .single()
            .expect("valid UTC time")
            .timestamp_millis();

        let observed = observed_datetime(late_utc, Some(120));

        assert_eq!(observed.year(), 2026);
        assert_eq!(observed.month(), 8);
        assert_eq!(observed.day(), 7);
        assert_eq!(observed.offset().local_minus_utc(), 7_200);
    }

    #[test]
    fn dashboard_splits_runtime_at_observed_local_midnight_and_stays_conservative() {
        let started_at_ms = Utc
            .with_ymd_and_hms(2026, 7, 31, 21, 30, 0)
            .single()
            .expect("valid UTC time")
            .timestamp_millis();
        let dashboard = build_dashboard(vec![StoredSession {
            id: "session-midnight".to_owned(),
            started_at_ms,
            ended_at_ms: Some(started_at_ms + 2 * 60 * 60 * 1_000),
            duration_seconds: Some(2 * 60 * 60),
            exit_kind: "clean".to_owned(),
            confidence: "verified".to_owned(),
            instance: "Test profile".to_owned(),
            version: "1.20.1".to_owned(),
            loader: None,
            launcher: "manual".to_owned(),
            kind: "menu".to_owned(),
            destination: String::new(),
            source: "logs/latest.log".to_owned(),
            utc_offset_minutes: Some(120),
            server_destinations: Vec::new(),
            world_destinations: Vec::new(),
        }]);

        let july = dashboard
            .monthly
            .iter()
            .find(|month| month.month == "2026-07")
            .expect("July slice");
        let august = dashboard
            .monthly
            .iter()
            .find(|month| month.month == "2026-08")
            .expect("August slice");
        assert_eq!(july.minutes, Some(30));
        assert_eq!(august.minutes, Some(90));
        assert_eq!(dashboard.totals.active_days, 2);
        assert!(matches!(
            dashboard.coverage.quality,
            CoverageQuality::Partial
        ));
        assert_eq!(dashboard.coverage.score, 79);
    }

    #[test]
    fn monthly_window_is_contiguous_bounded_and_ends_at_the_last_observed_month() {
        let april = Utc
            .with_ymd_and_hms(2024, 4, 15, 10, 0, 0)
            .single()
            .expect("April start")
            .timestamp_millis();
        let march = Utc
            .with_ymd_and_hms(2025, 3, 15, 10, 0, 0)
            .single()
            .expect("March start")
            .timestamp_millis();
        let dashboard = build_dashboard(vec![
            stored_session("april", april, 60 * 60, "verified"),
            stored_session("march", march, 30 * 60, "high"),
        ]);

        assert_eq!(dashboard.monthly.len(), 12);
        assert_eq!(dashboard.monthly.first().expect("first").month, "2024-04");
        assert_eq!(dashboard.monthly.last().expect("last").month, "2025-03");

        let april = dashboard.monthly.first().expect("April");
        assert!(matches!(april.coverage, MonthlyCoverage::Observed));
        assert_eq!(april.minutes, Some(60));
        assert_eq!(april.sessions, Some(1));
        assert_eq!(april.confidence, Confidence::Verified);

        let may = &dashboard.monthly[1];
        assert_eq!(may.month, "2024-05");
        assert!(matches!(may.coverage, MonthlyCoverage::Missing));
        assert_eq!(may.minutes, None);
        assert_eq!(may.sessions, None);
        assert_eq!(may.estimated_share, None);
        assert_eq!(may.confidence, Confidence::Unknown);

        let serialized = serde_json::to_value(may).expect("missing month JSON");
        assert_eq!(serialized["coverage"], "missing");
        assert!(serialized["minutes"].is_null());
        assert!(serialized["sessions"].is_null());
        assert!(serialized["estimatedShare"].is_null());
    }

    #[test]
    fn daily_window_is_365_contiguous_days_ending_at_the_last_observation() {
        let last = Utc
            .with_ymd_and_hms(2026, 4, 30, 10, 0, 0)
            .single()
            .expect("last observation")
            .timestamp_millis();
        let dashboard = build_dashboard(vec![stored_session(
            "last-observation",
            last,
            60,
            "verified",
        )]);

        assert_eq!(dashboard.daily.len(), 365);
        assert_eq!(dashboard.daily.last().expect("last day").date, "2026-04-30");
        assert_eq!(
            dashboard.daily.first().expect("first day").date,
            "2025-05-01"
        );
    }

    #[test]
    fn activity_confidence_uses_the_weakest_contributing_session() {
        let morning = Utc
            .with_ymd_and_hms(2026, 1, 15, 10, 0, 0)
            .single()
            .expect("morning")
            .timestamp_millis();
        let afternoon = Utc
            .with_ymd_and_hms(2026, 1, 15, 14, 0, 0)
            .single()
            .expect("afternoon")
            .timestamp_millis();
        let dashboard = build_dashboard(vec![
            stored_session("verified", morning, 60 * 60, "verified"),
            stored_session("partial", afternoon, 30 * 60, "partial"),
        ]);

        let day = dashboard
            .daily
            .iter()
            .find(|day| day.date == "2026-01-15")
            .expect("mixed-confidence day");
        assert_eq!(day.confidence, Confidence::Partial);
        let month = dashboard.monthly.last().expect("January");
        assert_eq!(month.month, "2026-01");
        assert_eq!(month.confidence, Confidence::Partial);
    }

    #[test]
    fn unknown_session_boundary_is_not_counted_as_bounded_coverage() {
        let started = Utc
            .with_ymd_and_hms(2026, 2, 4, 10, 0, 0)
            .single()
            .expect("start")
            .timestamp_millis();
        let mut session = stored_session("open", started, 0, "partial");
        session.ended_at_ms = None;
        session.duration_seconds = None;

        let dashboard = build_dashboard(vec![session]);

        assert_eq!(dashboard.coverage.score, 0);
        assert!(matches!(
            dashboard.coverage.quality,
            CoverageQuality::Unknown
        ));
        assert_eq!(dashboard.recent_sessions[0].ended_at, None);
        assert_eq!(dashboard.recent_sessions[0].duration_minutes, None);
        assert_eq!(dashboard.totals.active_days, 0);
        assert_eq!(dashboard.totals.longest_session_minutes, None);
        assert_eq!(dashboard.totals.average_session_minutes, None);
    }

    #[test]
    fn session_duration_metrics_use_only_known_plausible_durations() {
        let started = Utc
            .with_ymd_and_hms(2026, 2, 4, 10, 0, 0)
            .single()
            .expect("start")
            .timestamp_millis();
        let known = stored_session("known", started, 60 * 60, "verified");
        let mut unknown = stored_session("unknown", started + 2 * 60 * 60 * 1_000, 0, "partial");
        unknown.ended_at_ms = None;
        unknown.duration_seconds = None;

        let dashboard = build_dashboard(vec![known, unknown]);

        assert_eq!(dashboard.totals.longest_session_minutes, Some(60));
        assert_eq!(dashboard.totals.average_session_minutes, Some(60));
    }

    #[test]
    fn implausibly_long_legacy_session_is_not_allocated_as_runtime() {
        let started = Utc
            .with_ymd_and_hms(2026, 2, 4, 10, 0, 0)
            .single()
            .expect("start")
            .timestamp_millis();
        let duration = 32 * 24 * 60 * 60;
        let session = stored_session("implausible", started, duration, "verified");

        let dashboard = build_dashboard(vec![session]);

        assert_eq!(dashboard.totals.playtime_minutes, 0);
        assert_eq!(dashboard.totals.unique_playtime_minutes, 0);
        assert_eq!(dashboard.totals.active_days, 0);
        assert_eq!(dashboard.coverage.score, 0);
        assert_eq!(dashboard.recent_sessions[0].ended_at, None);
        assert_eq!(dashboard.recent_sessions[0].duration_minutes, None);
        assert_eq!(dashboard.recent_sessions[0].confidence, Confidence::Partial);
        assert_eq!(dashboard.totals.longest_session_minutes, None);
        assert_eq!(dashboard.totals.average_session_minutes, None);
    }

    #[test]
    fn active_days_match_days_with_positive_displayed_minutes() {
        let first = Utc
            .with_ymd_and_hms(2026, 2, 4, 10, 0, 0)
            .single()
            .expect("first day")
            .timestamp_millis();
        let second = Utc
            .with_ymd_and_hms(2026, 2, 5, 10, 0, 0)
            .single()
            .expect("second day")
            .timestamp_millis();
        let dashboard = build_dashboard(vec![
            stored_session("sub-minute", first, 59, "verified"),
            stored_session("one-minute", second, 60, "verified"),
        ]);

        assert_eq!(dashboard.totals.active_days, 1);
        let first_day = dashboard
            .daily
            .iter()
            .find(|day| day.date == "2026-02-04")
            .expect("sub-minute day");
        assert_eq!(first_day.minutes, Some(0));
        let second_day = dashboard
            .daily
            .iter()
            .find(|day| day.date == "2026-02-05")
            .expect("one-minute day");
        assert_eq!(second_day.minutes, Some(1));
    }

    #[test]
    fn mixed_sessions_still_contribute_each_linked_context_to_top_metrics() {
        let started = Utc
            .with_ymd_and_hms(2026, 3, 4, 10, 0, 0)
            .single()
            .expect("start")
            .timestamp_millis();
        let mut session = stored_session("mixed", started, 90 * 60, "verified");
        session.kind = "mixed".to_owned();
        session.destination = "Multiple observed destinations".to_owned();
        session.server_destinations = vec!["play.example.net".to_owned()];
        session.world_destinations = vec!["Survival".to_owned()];

        let dashboard = build_dashboard(vec![session]);

        assert_eq!(dashboard.top.server.name, "play.example.net");
        assert_eq!(dashboard.top.server.minutes, 90);
        assert_eq!(dashboard.top.world.name, "Survival");
        assert_eq!(dashboard.top.world.minutes, 90);
        assert_eq!(dashboard.recent_sessions[0].contexts.len(), 2);
        assert!(dashboard.recent_sessions[0].contexts.iter().any(|context| {
            context.kind == SessionContextKind::Server && context.name == "play.example.net"
        }));
        assert!(dashboard.recent_sessions[0].contexts.iter().any(|context| {
            context.kind == SessionContextKind::World && context.name == "Survival"
        }));
    }

    fn stored_session(
        id: &str,
        started_at_ms: i64,
        duration_seconds: u64,
        confidence: &str,
    ) -> StoredSession {
        StoredSession {
            id: id.to_owned(),
            started_at_ms,
            ended_at_ms: Some(started_at_ms.saturating_add(
                i64::try_from(duration_seconds.saturating_mul(1_000)).unwrap_or(i64::MAX),
            )),
            duration_seconds: Some(duration_seconds),
            exit_kind: "clean".to_owned(),
            confidence: confidence.to_owned(),
            instance: "Test profile".to_owned(),
            version: "1.20.1".to_owned(),
            loader: None,
            launcher: "manual".to_owned(),
            kind: "menu".to_owned(),
            destination: String::new(),
            source: "logs/latest.log".to_owned(),
            utc_offset_minutes: Some(0),
            server_destinations: Vec::new(),
            world_destinations: Vec::new(),
        }
    }
}
