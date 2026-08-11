use chrono::{DateTime, FixedOffset};
use rusqlite::Connection;

use super::read_models::{
    BoundedCollection, InstanceAccent, InstanceSummary, RuntimeBasis, ServerSummary, VersionKind,
    VersionSummary, WorldSummary,
};
use crate::{domain::Confidence, error::BackendError, storage::Database};

const EXPLORER_RESULT_LIMIT: i64 = 500;

#[derive(Debug, Clone, Copy)]
pub struct ExplorerService;

impl ExplorerService {
    pub fn instance_collection(
        self,
        database: &Database,
    ) -> Result<BoundedCollection<InstanceSummary>, BackendError> {
        database.read(|connection| {
            if !has_completed_dataset(connection)? {
                return Ok(empty_collection());
            }

            let mut statement = connection.prepare(
                "WITH visible_sessions AS (
                    SELECT session.*
                    FROM sessions session
                    LEFT JOIN session_user_state user_state
                      ON user_state.session_id = session.id
                    WHERE COALESCE(user_state.ignored, 0) = 0
                 ),
                 eligible_instances AS (
                    SELECT DISTINCT instance_id AS id FROM visible_sessions
                    UNION
                    SELECT DISTINCT source.instance_id AS id
                    FROM source_paths source
                    JOIN scan_runs run ON run.id = source.last_seen_scan_id
                    WHERE source.instance_id IS NOT NULL
                      AND source.current_revision_id IS NOT NULL
                      AND run.state = 'completed'
                 ),
                 session_stats AS (
                    SELECT
                        instance_id,
                        COALESCE(SUM(duration_seconds), 0) AS total_seconds,
                        COUNT(*) AS session_count,
                        MAX(COALESCE(ended_at_utc_ms, started_at_utc_ms)) AS last_played_at_ms,
                        SUM(CASE WHEN exit_kind = 'crash' THEN 1 ELSE 0 END) AS crash_count,
                        MIN(confidence_score) AS confidence_score
                    FROM visible_sessions
                    GROUP BY instance_id
                 ),
                 world_stats AS (
                    SELECT
                        session.instance_id,
                        COUNT(DISTINCT segment.world_id) AS world_count
                    FROM visible_sessions session
                    JOIN activity_segments segment ON segment.session_id = session.id
                    WHERE segment.kind = 'world'
                      AND NULLIF(TRIM(segment.world_id), '') IS NOT NULL
                    GROUP BY session.instance_id
                 )
                 SELECT
                    instance.id,
                    instance.name,
                    COALESCE(installation.display_name, location.adapter_kind, 'Custom'),
                    COALESCE(
                        (
                            SELECT NULLIF(TRIM(session.minecraft_version), '')
                            FROM visible_sessions session
                            WHERE session.instance_id = instance.id
                              AND NULLIF(TRIM(session.minecraft_version), '') IS NOT NULL
                            ORDER BY session.started_at_utc_ms DESC, session.id
                            LIMIT 1
                        ),
                        NULLIF(TRIM(instance.minecraft_version), '')
                    ),
                    COALESCE(
                        (
                            SELECT NULLIF(TRIM(session.loader), '')
                            FROM visible_sessions session
                            WHERE session.instance_id = instance.id
                              AND NULLIF(TRIM(session.loader), '') IS NOT NULL
                            ORDER BY session.started_at_utc_ms DESC, session.id
                            LIMIT 1
                        ),
                        NULLIF(TRIM(instance.loader), '')
                    ),
                    COALESCE(stats.total_seconds, 0),
                    COALESCE(stats.session_count, 0),
                    stats.last_played_at_ms,
                    (
                        SELECT session.utc_offset_minutes
                        FROM visible_sessions session
                        WHERE session.instance_id = instance.id
                        ORDER BY COALESCE(session.ended_at_utc_ms, session.started_at_utc_ms) DESC,
                                 session.id
                        LIMIT 1
                    ),
                    COALESCE(worlds.world_count, 0),
                    COALESCE(stats.crash_count, 0),
                    CASE
                        WHEN COALESCE(stats.session_count, 0) > 0
                        THEN stats.confidence_score
                        ELSE 0
                    END,
                    COUNT(*) OVER()
                 FROM instances instance
                 JOIN eligible_instances eligible ON eligible.id = instance.id
                 JOIN scan_locations location ON location.id = instance.location_id
                 LEFT JOIN launcher_installations installation
                   ON installation.id = instance.installation_id
                 LEFT JOIN session_stats stats ON stats.instance_id = instance.id
                 LEFT JOIN world_stats worlds ON worlds.instance_id = instance.id
                 ORDER BY COALESCE(stats.total_seconds, 0) DESC,
                          stats.last_played_at_ms DESC,
                          instance.name COLLATE NOCASE,
                          instance.id
                 LIMIT ?1",
            )?;

            let rows = statement.query_map([EXPLORER_RESULT_LIMIT], |row| {
                let id: String = row.get(0)?;
                let last_played_at_ms: Option<i64> = row.get(7)?;
                let utc_offset_minutes: Option<i32> = row.get(8)?;
                Ok((
                    InstanceSummary {
                        accent: accent_for(&id),
                        id,
                        name: row.get(1)?,
                        launcher: display_launcher(&row.get::<_, String>(2)?),
                        version: row.get(3)?,
                        loader: row.get(4)?,
                        total_minutes: nonnegative(row.get(5)?) / 60,
                        sessions: nonnegative(row.get(6)?),
                        last_played_at: last_played_at_ms
                            .map(|value| observed_rfc3339(value, utc_offset_minutes)),
                        mod_count: None,
                        world_count: nonnegative(row.get(9)?),
                        crash_count: nonnegative(row.get(10)?),
                        confidence: Confidence::from_score(score(row.get(11)?)),
                    },
                    row.get::<_, i64>(12)?,
                ))
            })?;
            collect_bounded(rows)
        })
    }

    #[cfg(test)]
    pub(crate) fn instances(
        self,
        database: &Database,
    ) -> Result<Vec<InstanceSummary>, BackendError> {
        Ok(self.instance_collection(database)?.items)
    }

    pub fn world_collection(
        self,
        database: &Database,
    ) -> Result<BoundedCollection<WorldSummary>, BackendError> {
        database.read(|connection| {
            if !has_completed_dataset(connection)? {
                return Ok(empty_collection());
            }

            let mut statement = connection.prepare(
                "WITH visible_sessions AS (
                    SELECT session.*,
                           CASE
                               WHEN session.duration_seconds IS NOT NULL
                                AND (
                                    session.duration_seconds > 2678400
                                    OR (session.ended_at_utc_ms IS NOT NULL AND (
                                        session.ended_at_utc_ms < session.started_at_utc_ms
                                        OR session.ended_at_utc_ms - session.started_at_utc_ms > 2678400000
                                        OR ABS(
                                            (session.ended_at_utc_ms - session.started_at_utc_ms)
                                            - session.duration_seconds * 1000
                                        ) >= 1000
                                    ))
                                ) THEN 1 ELSE 0
                           END AS bounds_invalid
                    FROM sessions session
                    LEFT JOIN session_user_state user_state
                      ON user_state.session_id = session.id
                    WHERE COALESCE(user_state.ignored, 0) = 0
                 ),
                 observations AS (
                    SELECT
                        segment.world_id AS world_name,
                        session.id AS session_id,
                        instance.id AS instance_id,
                        instance.name AS instance_name,
                        CASE WHEN session.bounds_invalid = 1
                             THEN 0 ELSE COALESCE(session.duration_seconds, 0) END AS duration_seconds,
                        CASE WHEN session.bounds_invalid = 1
                             THEN session.started_at_utc_ms
                             ELSE COALESCE(session.ended_at_utc_ms, session.started_at_utc_ms)
                        END AS last_played_at_ms,
                        session.utc_offset_minutes,
                        NULLIF(TRIM(session.minecraft_version), '') AS version,
                        MIN(
                            MIN(
                                CASE WHEN session.bounds_invalid = 1
                                     THEN MIN(session.confidence_score, 54)
                                     ELSE session.confidence_score END,
                                segment.confidence_score
                            )
                        ) AS confidence_score
                    FROM visible_sessions session
                    JOIN instances instance ON instance.id = session.instance_id
                    JOIN activity_segments segment ON segment.session_id = session.id
                    WHERE segment.kind = 'world'
                      AND NULLIF(TRIM(segment.world_id), '') IS NOT NULL
                    GROUP BY segment.world_id, session.id, instance.id, instance.name
                 ),
                 summaries AS (
                    SELECT
                        instance_id,
                        MAX(instance_name) AS instance_name,
                        world_name,
                        SUM(duration_seconds) AS total_seconds,
                        MAX(last_played_at_ms) AS last_played_at_ms,
                        MIN(confidence_score) AS confidence_score
                    FROM observations
                    GROUP BY instance_id, world_name
                 )
                 SELECT
                    summary.instance_id,
                    summary.instance_name,
                    summary.world_name,
                    summary.total_seconds,
                    summary.last_played_at_ms,
                    (
                        SELECT observation.utc_offset_minutes
                        FROM observations observation
                        WHERE observation.instance_id = summary.instance_id
                          AND observation.world_name = summary.world_name
                        ORDER BY observation.last_played_at_ms DESC, observation.session_id
                        LIMIT 1
                    ),
                    (
                        SELECT observation.version
                        FROM observations observation
                        WHERE observation.instance_id = summary.instance_id
                          AND observation.world_name = summary.world_name
                          AND observation.version IS NOT NULL
                        ORDER BY observation.last_played_at_ms DESC, observation.session_id
                        LIMIT 1
                    ),
                    summary.confidence_score,
                    COUNT(*) OVER()
                 FROM summaries summary
                 ORDER BY summary.total_seconds DESC,
                          summary.last_played_at_ms DESC,
                          summary.world_name COLLATE NOCASE,
                          summary.instance_id
                 LIMIT ?1",
            )?;

            let rows = statement.query_map([EXPLORER_RESULT_LIMIT], |row| {
                let instance_id: String = row.get(0)?;
                let name: String = row.get(2)?;
                let last_played_at_ms: i64 = row.get(4)?;
                let utc_offset_minutes: Option<i32> = row.get(5)?;
                Ok((
                    WorldSummary {
                        id: stable_summary_id("world", &[&instance_id, &name]),
                        name,
                        instance: row.get(1)?,
                        mode: None,
                        version: row.get(6)?,
                        total_minutes: nonnegative(row.get(3)?) / 60,
                        last_played_at: Some(observed_rfc3339(
                            last_played_at_ms,
                            utc_offset_minutes,
                        )),
                        size_label: None,
                        confidence: Confidence::from_score(score(row.get(7)?)),
                        runtime_basis: RuntimeBasis::SessionLinked,
                    },
                    row.get::<_, i64>(8)?,
                ))
            })?;
            collect_bounded(rows)
        })
    }

    #[cfg(test)]
    pub(crate) fn worlds(self, database: &Database) -> Result<Vec<WorldSummary>, BackendError> {
        Ok(self.world_collection(database)?.items)
    }

    pub fn server_collection(
        self,
        database: &Database,
    ) -> Result<BoundedCollection<ServerSummary>, BackendError> {
        database.read(|connection| {
            if !has_completed_dataset(connection)? {
                return Ok(empty_collection());
            }

            let mut statement = connection.prepare(
                "WITH visible_sessions AS (
                    SELECT session.*,
                           CASE
                               WHEN session.duration_seconds IS NOT NULL
                                AND (
                                    session.duration_seconds > 2678400
                                    OR (session.ended_at_utc_ms IS NOT NULL AND (
                                        session.ended_at_utc_ms < session.started_at_utc_ms
                                        OR session.ended_at_utc_ms - session.started_at_utc_ms > 2678400000
                                        OR ABS(
                                            (session.ended_at_utc_ms - session.started_at_utc_ms)
                                            - session.duration_seconds * 1000
                                        ) >= 1000
                                    ))
                                ) THEN 1 ELSE 0
                           END AS bounds_invalid
                    FROM sessions session
                    LEFT JOIN session_user_state user_state
                      ON user_state.session_id = session.id
                    WHERE COALESCE(user_state.ignored, 0) = 0
                 ),
                 observations AS (
                    SELECT
                        server.id AS server_id,
                        COALESCE(NULLIF(TRIM(server.display_name), ''), server.original_address) AS name,
                        server.original_address AS address,
                        session.id AS session_id,
                        CASE WHEN session.bounds_invalid = 1
                             THEN 0 ELSE COALESCE(session.duration_seconds, 0) END AS duration_seconds,
                        CASE WHEN session.bounds_invalid = 1
                             THEN session.started_at_utc_ms
                             ELSE COALESCE(session.ended_at_utc_ms, session.started_at_utc_ms)
                        END AS last_played_at_ms,
                        session.utc_offset_minutes,
                        NULLIF(TRIM(session.minecraft_version), '') AS version,
                        MIN(
                            MIN(
                                CASE WHEN session.bounds_invalid = 1
                                     THEN MIN(session.confidence_score, 54)
                                     ELSE session.confidence_score END,
                                segment.confidence_score
                            )
                        ) AS confidence_score
                    FROM visible_sessions session
                    JOIN activity_segments segment ON segment.session_id = session.id
                    JOIN servers server ON server.id = segment.server_id
                    WHERE segment.kind = 'server'
                    GROUP BY server.id, server.display_name, server.original_address, session.id
                 ),
                 summaries AS (
                    SELECT
                        server_id,
                        MAX(name) AS name,
                        MAX(address) AS address,
                        COUNT(*) AS session_count,
                        SUM(duration_seconds) AS total_seconds,
                        MAX(last_played_at_ms) AS last_played_at_ms,
                        MIN(confidence_score) AS confidence_score
                    FROM observations
                    GROUP BY server_id
                 )
                 SELECT
                    summary.server_id,
                    summary.name,
                    summary.address,
                    summary.session_count,
                    summary.total_seconds,
                    summary.last_played_at_ms,
                    (
                        SELECT observation.utc_offset_minutes
                        FROM observations observation
                        WHERE observation.server_id = summary.server_id
                        ORDER BY observation.last_played_at_ms DESC, observation.session_id
                        LIMIT 1
                    ),
                    (
                        SELECT observation.version
                        FROM observations observation
                        WHERE observation.server_id = summary.server_id
                          AND observation.version IS NOT NULL
                        GROUP BY observation.version
                        ORDER BY SUM(observation.duration_seconds) DESC,
                                 COUNT(*) DESC,
                                 observation.version
                        LIMIT 1
                    ),
                    summary.confidence_score,
                    COUNT(*) OVER()
                 FROM summaries summary
                 ORDER BY summary.total_seconds DESC,
                          summary.last_played_at_ms DESC,
                          summary.name COLLATE NOCASE,
                          summary.server_id
                 LIMIT ?1",
            )?;

            let rows = statement.query_map([EXPLORER_RESULT_LIMIT], |row| {
                let last_played_at_ms: i64 = row.get(5)?;
                let utc_offset_minutes: Option<i32> = row.get(6)?;
                Ok((
                    ServerSummary {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        address: row.get(2)?,
                        sessions: nonnegative(row.get(3)?),
                        total_minutes: nonnegative(row.get(4)?) / 60,
                        last_played_at: Some(observed_rfc3339(
                            last_played_at_ms,
                            utc_offset_minutes,
                        )),
                        favorite_version: row.get(7)?,
                        confidence: Confidence::from_score(score(row.get(8)?)),
                        runtime_basis: RuntimeBasis::SessionLinked,
                    },
                    row.get::<_, i64>(9)?,
                ))
            })?;
            collect_bounded(rows)
        })
    }

    #[cfg(test)]
    pub(crate) fn servers(self, database: &Database) -> Result<Vec<ServerSummary>, BackendError> {
        Ok(self.server_collection(database)?.items)
    }

    pub fn version_collection(
        self,
        database: &Database,
    ) -> Result<BoundedCollection<VersionSummary>, BackendError> {
        database.read(|connection| {
            if !has_completed_dataset(connection)? {
                return Ok(empty_collection());
            }

            let mut statement = connection.prepare(
                "WITH visible_sessions AS (
                    SELECT session.*,
                           CASE
                               WHEN session.duration_seconds IS NOT NULL
                                AND (
                                    session.duration_seconds > 2678400
                                    OR (session.ended_at_utc_ms IS NOT NULL AND (
                                        session.ended_at_utc_ms < session.started_at_utc_ms
                                        OR session.ended_at_utc_ms - session.started_at_utc_ms > 2678400000
                                        OR ABS(
                                            (session.ended_at_utc_ms - session.started_at_utc_ms)
                                            - session.duration_seconds * 1000
                                        ) >= 1000
                                    ))
                                ) THEN 1 ELSE 0
                           END AS bounds_invalid
                    FROM sessions session
                    LEFT JOIN session_user_state user_state ON user_state.session_id = session.id
                    WHERE COALESCE(user_state.ignored, 0) = 0
                      AND NULLIF(TRIM(session.minecraft_version), '') IS NOT NULL
                 ),
                 observations AS (
                    SELECT
                        id AS session_id,
                        minecraft_version AS name,
                        CASE WHEN bounds_invalid = 1
                             THEN 0 ELSE COALESCE(duration_seconds, 0) END AS duration_seconds,
                        started_at_utc_ms,
                        CASE WHEN bounds_invalid = 1
                             THEN started_at_utc_ms
                             ELSE COALESCE(ended_at_utc_ms, started_at_utc_ms)
                        END AS last_played_at_ms,
                        utc_offset_minutes,
                        NULLIF(TRIM(loader), '') AS loader,
                        CASE WHEN bounds_invalid = 1
                             THEN MIN(confidence_score, 54)
                             ELSE confidence_score END AS confidence_score
                    FROM visible_sessions
                 ),
                 summaries AS (
                    SELECT
                        name,
                        SUM(duration_seconds) AS total_seconds,
                        COUNT(*) AS session_count,
                        MIN(started_at_utc_ms) AS first_played_at_ms,
                        MAX(last_played_at_ms) AS last_played_at_ms,
                        MIN(confidence_score) AS confidence_score
                    FROM observations
                    GROUP BY name
                 )
                 SELECT
                    summary.name,
                    summary.total_seconds,
                    summary.session_count,
                    summary.first_played_at_ms,
                    (
                        SELECT observation.utc_offset_minutes
                        FROM observations observation
                        WHERE observation.name = summary.name
                        ORDER BY observation.started_at_utc_ms, observation.session_id
                        LIMIT 1
                    ),
                    summary.last_played_at_ms,
                    (
                        SELECT observation.utc_offset_minutes
                        FROM observations observation
                        WHERE observation.name = summary.name
                        ORDER BY observation.last_played_at_ms DESC, observation.session_id
                        LIMIT 1
                    ),
                    COALESCE((
                        SELECT json_group_array(loader)
                        FROM (
                            SELECT DISTINCT observation.loader AS loader
                            FROM observations observation
                            WHERE observation.name = summary.name
                              AND observation.loader IS NOT NULL
                            ORDER BY observation.loader
                        )
                    ), '[]'),
                    summary.confidence_score,
                    COUNT(*) OVER()
                 FROM summaries summary
                 ORDER BY summary.last_played_at_ms DESC,
                          summary.total_seconds DESC,
                          summary.name
                 LIMIT ?1",
            )?;
            let rows = statement.query_map([EXPLORER_RESULT_LIMIT], |row| {
                let name: String = row.get(0)?;
                let loader_json: String = row.get(7)?;
                Ok((
                    VersionSummary {
                        id: stable_summary_id("version", &[&name]),
                        kind: classify_version(&name),
                        name,
                        total_minutes: nonnegative(row.get(1)?) / 60,
                        sessions: nonnegative(row.get(2)?),
                        first_played_at: observed_rfc3339(row.get(3)?, row.get(4)?),
                        last_played_at: observed_rfc3339(row.get(5)?, row.get(6)?),
                        loaders: serde_json::from_str(&loader_json).unwrap_or_default(),
                        confidence: Confidence::from_score(score(row.get(8)?)),
                    },
                    row.get::<_, i64>(9)?,
                ))
            })?;
            collect_bounded(rows)
        })
    }

    #[cfg(test)]
    pub(crate) fn versions(self, database: &Database) -> Result<Vec<VersionSummary>, BackendError> {
        Ok(self.version_collection(database)?.items)
    }
}

fn empty_collection<T>() -> BoundedCollection<T> {
    BoundedCollection {
        items: Vec::new(),
        total: 0,
        truncated: false,
    }
}

fn collect_bounded<T>(
    rows: impl Iterator<Item = Result<(T, i64), rusqlite::Error>>,
) -> Result<BoundedCollection<T>, rusqlite::Error> {
    let mut items = Vec::new();
    let mut total = 0_u64;
    for row in rows {
        let (item, row_total) = row?;
        total = nonnegative(row_total);
        items.push(item);
    }
    Ok(BoundedCollection {
        truncated: total > u64::try_from(items.len()).unwrap_or(u64::MAX),
        total,
        items,
    })
}

fn has_completed_dataset(connection: &Connection) -> Result<bool, rusqlite::Error> {
    connection.query_row(
        "SELECT revision > 0 FROM dataset_state WHERE id = 1",
        [],
        |row| row.get(0),
    )
}

fn classify_version(value: &str) -> VersionKind {
    let release_parts = value.split('.').collect::<Vec<_>>();
    if (2..=3).contains(&release_parts.len())
        && release_parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return VersionKind::Release;
    }

    if let Some((year, release)) = value.split_once('w')
        && year.len() == 2
        && year.bytes().all(|byte| byte.is_ascii_digit())
        && release.len() >= 3
        && release.as_bytes()[..2].iter().all(u8::is_ascii_digit)
        && release.as_bytes()[2..].iter().all(u8::is_ascii_lowercase)
    {
        return VersionKind::Snapshot;
    }

    VersionKind::Other
}

fn observed_rfc3339(value: i64, utc_offset_minutes: Option<i32>) -> String {
    let utc = DateTime::from_timestamp_millis(value).unwrap_or(DateTime::UNIX_EPOCH);
    utc.with_timezone(&observed_offset(utc_offset_minutes))
        .to_rfc3339()
}

fn observed_offset(utc_offset_minutes: Option<i32>) -> FixedOffset {
    let seconds = utc_offset_minutes
        .and_then(|minutes| minutes.checked_mul(60))
        .unwrap_or(0);
    FixedOffset::east_opt(seconds)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero UTC offset is valid"))
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

fn accent_for(id: &str) -> InstanceAccent {
    match blake3::hash(id.as_bytes()).as_bytes()[0] % 4 {
        0 => InstanceAccent::Moss,
        1 => InstanceAccent::Copper,
        2 => InstanceAccent::Quartz,
        _ => InstanceAccent::Slate,
    }
}

fn stable_summary_id(namespace: &str, values: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(namespace.as_bytes());
    for value in values {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    let digest = hasher.finalize().to_hex().to_string();
    format!("{namespace}_{}", &digest[..24])
}

fn nonnegative(value: i64) -> u64 {
    u64::try_from(value.max(0)).unwrap_or(0)
}

fn score(value: i64) -> u8 {
    u8::try_from(value.clamp(0, 100)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use rusqlite::params;
    use tempfile::tempdir;

    use super::{ExplorerService, classify_version};
    use crate::application::read_models::VersionKind;
    use crate::{domain::Confidence, storage::Database};

    #[test]
    fn a_database_without_a_promoted_dataset_returns_truthful_empty_explorers() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("empty.sqlite3")).expect("database");
        let explorer = ExplorerService;

        assert!(explorer.instances(&database).expect("instances").is_empty());
        assert!(explorer.worlds(&database).expect("worlds").is_empty());
        assert!(explorer.servers(&database).expect("servers").is_empty());
        assert!(explorer.versions(&database).expect("versions").is_empty());
    }

    #[test]
    fn explorers_aggregate_only_visible_canonical_session_evidence() {
        let (_temp, database) = populated_database();
        let explorer = ExplorerService;

        let instances = explorer.instances(&database).expect("instances");
        assert_eq!(
            instances.len(),
            2,
            "an unpromoted instance must stay hidden"
        );
        let live = instances
            .iter()
            .find(|instance| instance.id == "instance_live")
            .expect("live instance");
        assert_eq!(live.total_minutes, 30);
        assert_eq!(live.sessions, 2);
        assert_eq!(live.world_count, 1);
        assert_eq!(live.crash_count, 1);
        assert_eq!(live.version.as_deref(), Some("24w10a"));
        assert_eq!(live.loader.as_deref(), Some("Fabric 0.15.11"));
        assert!(matches!(live.confidence, Confidence::High));
        assert!(
            live.last_played_at
                .as_deref()
                .is_some_and(|date| date.ends_with("-05:00"))
        );

        let empty = instances
            .iter()
            .find(|instance| instance.id == "instance_empty")
            .expect("source-backed empty instance");
        assert_eq!(empty.sessions, 0);
        assert_eq!(empty.total_minutes, 0);
        assert_eq!(empty.last_played_at, None);
        assert_eq!(empty.mod_count, None);
        assert_eq!(empty.version, None);
        assert!(
            matches!(empty.confidence, Confidence::Unknown),
            "root detection confidence is not session-backed archive confidence"
        );

        let worlds = explorer.worlds(&database).expect("worlds");
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].name, "Northbound");
        assert_eq!(worlds[0].total_minutes, 10);
        assert_eq!(worlds[0].mode, None);
        assert_eq!(worlds[0].size_label, None);
        assert!(matches!(worlds[0].confidence, Confidence::High));

        let servers = explorer.servers(&database).expect("servers");
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "Example Network");
        assert_eq!(servers[0].address, "play.example.net:25565");
        assert_eq!(servers[0].sessions, 1);
        assert_eq!(servers[0].total_minutes, 20);
        assert_eq!(servers[0].favorite_version.as_deref(), Some("24w10a"));
        assert!(matches!(servers[0].confidence, Confidence::Verified));

        let versions = explorer.versions(&database).expect("versions");
        assert_eq!(versions.len(), 2);
        assert_eq!(
            versions[0].name, "24w10a",
            "UTC chronology, not lexicographic RFC3339 offsets, controls ordering"
        );
        let release = versions
            .iter()
            .find(|version| version.name == "1.20.1")
            .expect("release");
        assert_eq!(release.total_minutes, 10);
        assert_eq!(release.loaders, ["Fabric 0.15.11"]);
        let snapshot = versions
            .iter()
            .find(|version| version.name == "24w10a")
            .expect("snapshot");
        assert_eq!(snapshot.total_minutes, 20);
        assert!(snapshot.loaders.is_empty());

        let server_json = serde_json::to_value(&servers[0]).expect("server JSON");
        assert_eq!(server_json["runtimeBasis"], "sessionLinked");
        let world_json = serde_json::to_value(&worlds[0]).expect("world JSON");
        assert_eq!(world_json["runtimeBasis"], "sessionLinked");
        assert!(world_json["mode"].is_null());
        assert!(world_json["sizeLabel"].is_null());
        let empty_json = serde_json::to_value(empty).expect("instance JSON");
        assert!(empty_json["modCount"].is_null());
        assert!(empty_json["lastPlayedAt"].is_null());
        let snapshot_json = serde_json::to_value(snapshot).expect("version JSON");
        assert_eq!(snapshot_json["type"], "snapshot");
        assert!(snapshot_json.get("firstPlayedAt").is_some());
        assert!(snapshot_json.get("lastPlayedAt").is_some());
    }

    #[test]
    fn arbitrary_log_derived_version_text_cannot_panic_classification() {
        assert!(matches!(classify_version("1.20.1"), VersionKind::Release));
        assert!(matches!(classify_version("24w10a"), VersionKind::Snapshot));
        assert!(matches!(classify_version("版本"), VersionKind::Other));
        assert!(matches!(classify_version("24w12α"), VersionKind::Other));
        assert!(matches!(classify_version("1.20-pre1"), VersionKind::Other));
    }

    #[test]
    fn linked_runtime_never_claims_narrow_activity_segment_duration() {
        let (_temp, database) = populated_database();
        let explorer = ExplorerService;

        let world = explorer.worlds(&database).expect("worlds").remove(0);
        let server = explorer.servers(&database).expect("servers").remove(0);

        // The fixture's destination markers span only part of each session.
        // Until exact transitions are reconstructed, summaries intentionally use
        // the full linked session and label that basis in the serialized contract.
        assert_eq!(world.total_minutes, 10);
        assert_eq!(server.total_minutes, 20);
        assert_eq!(
            serde_json::to_value(world).expect("world JSON")["runtimeBasis"],
            "sessionLinked"
        );
        assert_eq!(
            serde_json::to_value(server).expect("server JSON")["runtimeBasis"],
            "sessionLinked"
        );
    }

    #[test]
    fn every_explorer_reports_truthful_truncation_at_the_501_row_boundary() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("bounded.sqlite3")).expect("database");
        database
            .write(|transaction| {
                transaction.execute_batch(
                    "UPDATE dataset_state SET revision = 1, updated_at_ms = 1 WHERE id = 1;
                     INSERT INTO scan_locations (
                        id, origin, adapter_kind, platform, path_key, path_display, enabled,
                        validation_score, status, created_at_ms, updated_at_ms
                     ) VALUES (
                        'location', 'custom', 'manual', 'linux', X'01', '/redacted', 1,
                        90, 'available', 1, 1
                     );",
                )?;
                for index in 0_i64..501 {
                    let instance_id = format!("instance-{index:03}");
                    let session_id = format!("session-{index:03}");
                    let server_id = format!("server-{index:03}");
                    let started_at = 1_700_000_000_000_i64 + index * 60_000;
                    transaction.execute(
                        "INSERT INTO instances (
                            id, location_id, relative_path_key, relative_path_display, name,
                            confidence_score, first_seen_at_ms, last_seen_at_ms
                         ) VALUES (?1, 'location', ?2, ?3, ?4, 90, ?5, ?5)",
                        params![
                            instance_id,
                            index.to_le_bytes().to_vec(),
                            format!("instances/{index:03}"),
                            format!("Profile {index:03}"),
                            started_at,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO sessions (
                            id, instance_id, started_at_utc_ms, ended_at_utc_ms, duration_seconds,
                            exit_kind, confidence_score, confidence_label,
                            confidence_model_revision, reconstruction_revision, canonical_key,
                            minecraft_version, utc_offset_minutes
                         ) VALUES (?1, ?2, ?3, ?4, 60, 'clean', 90, 'verified', 1, 1, ?5, ?6, 0)",
                        params![
                            session_id,
                            instance_id,
                            started_at,
                            started_at + 60_000,
                            (index + 10_000).to_le_bytes().to_vec(),
                            format!("version-{index:03}"),
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO servers (
                            id, canonical_address, original_address, display_name,
                            first_seen_at_ms, last_seen_at_ms
                         ) VALUES (?1, ?2, ?2, ?3, ?4, ?4)",
                        params![
                            server_id,
                            format!("server-{index:03}.example:25565"),
                            format!("Server {index:03}"),
                            started_at,
                        ],
                    )?;
                    transaction.execute(
                        "INSERT INTO activity_segments (
                            id, session_id, kind, server_id, world_id,
                            started_at_utc_ms, ended_at_utc_ms, confidence_score
                         ) VALUES
                            (?1, ?2, 'world', NULL, ?3, ?4, ?5, 90),
                            (?6, ?2, 'server', ?7, NULL, ?4, ?5, 90)",
                        params![
                            format!("world-segment-{index:03}"),
                            session_id,
                            format!("World {index:03}"),
                            started_at,
                            started_at + 60_000,
                            format!("server-segment-{index:03}"),
                            server_id,
                        ],
                    )?;
                }
                Ok(())
            })
            .expect("bounded explorer fixture");

        let explorer = ExplorerService;
        let instances = explorer.instance_collection(&database).expect("instances");
        let worlds = explorer.world_collection(&database).expect("worlds");
        let servers = explorer.server_collection(&database).expect("servers");
        let versions = explorer.version_collection(&database).expect("versions");

        for (name, total, loaded, truncated) in [
            (
                "instances",
                instances.total,
                instances.items.len(),
                instances.truncated,
            ),
            ("worlds", worlds.total, worlds.items.len(), worlds.truncated),
            (
                "servers",
                servers.total,
                servers.items.len(),
                servers.truncated,
            ),
            (
                "versions",
                versions.total,
                versions.items.len(),
                versions.truncated,
            ),
        ] {
            assert_eq!(total, 501, "{name} total");
            assert_eq!(loaded, 500, "{name} loaded window");
            assert!(truncated, "{name} truncation flag");
        }
        assert_eq!(instances.items[0].id, "instance-500");
        assert_eq!(worlds.items[0].name, "World 500");
        assert_eq!(servers.items[0].id, "server-500");
        assert_eq!(versions.items[0].name, "version-500");
    }

    fn populated_database() -> (tempfile::TempDir, Database) {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("populated.sqlite3")).expect("database");
        database
            .write(|transaction| {
                transaction.execute_batch(
                    "UPDATE dataset_state SET revision = 1, updated_at_ms = 1700000000000 WHERE id = 1;

                     INSERT INTO scan_locations (
                        id, origin, adapter_kind, platform, path_key, path_display, enabled,
                        validation_score, status, created_at_ms, updated_at_ms
                     ) VALUES (
                        'location_done', 'automatic', 'prism', 'macos', X'01', '/done', 1,
                        95, 'available', 1700000000000, 1700000000000
                     );

                     INSERT INTO launcher_installations (
                        id, location_id, kind, display_name, confidence_score,
                        first_seen_at_ms, last_seen_at_ms
                     ) VALUES (
                        'launcher_prism', 'location_done', 'prism', 'Prism Launcher', 95,
                        1700000000000, 1700000000000
                     );

                     INSERT INTO instances (
                        id, installation_id, location_id, relative_path_key,
                        relative_path_display, name, minecraft_version, loader,
                        confidence_score, first_seen_at_ms, last_seen_at_ms
                     ) VALUES
                        ('instance_live', 'launcher_prism', 'location_done', X'02',
                         'instances/live', 'Live profile', '24w10a', 'Fabric 0.15.11',
                         95, 1700000000000, 1700000000000),
                        ('instance_empty', 'launcher_prism', 'location_done', X'03',
                         'instances/empty', 'Empty profile', NULL, NULL,
                         95, 1700000000000, 1700000000000),
                        ('instance_pending', 'launcher_prism', 'location_done', X'04',
                         'instances/pending', 'Unpromoted profile', '9.9.9', NULL,
                         95, 1700000000000, 1700000000000);

                     INSERT INTO scan_runs (
                        id, mode, state, phase, requested_at_ms, started_at_ms, finished_at_ms,
                        dataset_revision_before, dataset_revision_after
                     ) VALUES (
                        'scan_done', 'standard', 'completed', 'complete', 1700000000000,
                        1700000000000, 1700000001000, 0, 1
                     );

                     INSERT INTO source_paths (
                        id, location_id, instance_id, relative_path_key, relative_path_display,
                        kind, presence, current_revision_id, last_seen_scan_id
                     ) VALUES (
                        'source_empty', 'location_done', 'instance_empty', X'05',
                        'instances/empty/logs/latest.log', 'log', 'present',
                        'revision_empty', 'scan_done'
                     );

                     INSERT INTO source_revisions (
                        id, source_path_id, generation, size_bytes, modified_at_ms,
                        parser_name, parser_revision, parsed_offset, parse_status, created_at_ms
                     ) VALUES (
                        'revision_empty', 'source_empty', 1, 0, 1700000000000,
                        'minecraft_log', 1, 0, 'parsed', 1700000000000
                     );

                     INSERT INTO sessions (
                        id, instance_id, started_at_utc_ms, ended_at_utc_ms, duration_seconds,
                        exit_kind, confidence_score, confidence_label,
                        confidence_model_revision, reconstruction_revision, canonical_key,
                        minecraft_version, loader, utc_offset_minutes
                     ) VALUES
                        ('session_world', 'instance_live', 1700000000000, 1700000600000, 600,
                         'clean', 70, 'high', 1, 1, X'11', '1.20.1', 'Fabric 0.15.11', 120),
                        ('session_server', 'instance_live', 1700001000000, 1700002200000, 1200,
                         'crash', 90, 'verified', 1, 1, X'12', '24w10a', NULL, -300),
                        ('session_ignored', 'instance_live', 1700003000000, 1700006600000, 3600,
                         'clean', 95, 'verified', 1, 1, X'13', '9.9.9', 'IgnoredLoader', 0);

                     INSERT INTO session_user_state (session_id, ignored, updated_at_ms)
                     VALUES ('session_ignored', 1, 1700007000000);

                     INSERT INTO servers (
                        id, canonical_address, original_address, display_name,
                        first_seen_at_ms, last_seen_at_ms
                     ) VALUES (
                        'server_example', 'play.example.net:25565', 'play.example.net:25565',
                        'Example Network', 1700001000000, 1700006600000
                     );

                     INSERT INTO activity_segments (
                        id, session_id, kind, server_id, world_id,
                        started_at_utc_ms, ended_at_utc_ms, confidence_score
                     ) VALUES
                        ('segment_world', 'session_world', 'world', NULL, 'Northbound',
                         1700000300000, 1700000500000, 75),
                        ('segment_server', 'session_server', 'server', 'server_example', NULL,
                         1700001300000, 1700001600000, 85),
                        ('segment_ignored', 'session_ignored', 'server', 'server_example', NULL,
                         1700003100000, 1700006500000, 95);",
                )?;
                Ok(())
            })
            .expect("fixture");
        (temp, database)
    }
}
