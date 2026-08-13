use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::error::BackendError;

struct Migration {
    version: i64,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "core",
        sql: include_str!("../../migrations/0001_core.sql"),
    },
    Migration {
        version: 2,
        name: "sources",
        sql: include_str!("../../migrations/0002_sources.sql"),
    },
    Migration {
        version: 3,
        name: "evidence_sessions",
        sql: include_str!("../../migrations/0003_evidence_sessions.sql"),
    },
    Migration {
        version: 4,
        name: "user_state",
        sql: include_str!("../../migrations/0004_user_state.sql"),
    },
    Migration {
        version: 5,
        name: "analytics",
        sql: include_str!("../../migrations/0005_analytics.sql"),
    },
    Migration {
        version: 6,
        name: "incremental_scan_storage",
        sql: include_str!("../../migrations/0006_incremental_scan_storage.sql"),
    },
    Migration {
        version: 7,
        name: "session_observation_metadata",
        sql: include_str!("../../migrations/0007_session_observation_metadata.sql"),
    },
    Migration {
        version: 8,
        name: "source_revision_lineage",
        sql: include_str!("../../migrations/0008_source_revision_lineage.sql"),
    },
];

pub fn run(connection: &mut Connection) -> Result<(), BackendError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            checksum TEXT NOT NULL,
            applied_at_ms INTEGER NOT NULL
        ) STRICT;",
    )?;

    for migration in MIGRATIONS {
        let checksum = migration_checksum(migration.sql);
        let recorded: Option<String> = connection
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(recorded) = recorded {
            if recorded != checksum {
                let is_known_legacy = recorded == windows_line_ending_checksum(migration.sql);

                if !is_known_legacy || !schema_matches_recorded_migrations(connection)? {
                    return Err(BackendError::MigrationChecksum {
                        version: migration.version,
                        name: migration.name,
                    });
                }

                let transaction =
                    connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
                transaction.execute(
                    "UPDATE schema_migrations
                     SET checksum = ?1
                     WHERE version = ?2 AND checksum = ?3",
                    params![checksum, migration.version, recorded],
                )?;
                transaction.commit()?;
            }
            continue;
        }

        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(migration.sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                migration.version,
                migration.name,
                checksum,
                Utc::now().timestamp_millis()
            ],
        )?;
        transaction.pragma_update(None, "user_version", migration.version)?;
        transaction.commit()?;
    }

    Ok(())
}

fn schema_matches_recorded_migrations(connection: &Connection) -> Result<bool, rusqlite::Error> {
    let latest_recorded: i64 = connection.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    let expected = Connection::open_in_memory()?;
    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version <= latest_recorded)
    {
        expected.execute_batch(migration.sql)?;
    }
    let mut statement = expected.prepare(
        "SELECT type, name, sql
         FROM sqlite_schema
         WHERE name NOT LIKE 'sqlite_%' AND sql IS NOT NULL
         ORDER BY type, name",
    )?;
    let expected_objects = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;

    for (object_type, name, sql) in expected_objects {
        let actual: Option<(String, String)> = connection
            .query_row(
                "SELECT type, sql FROM sqlite_schema WHERE name = ?1",
                [&name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let actual_matches = actual.is_some_and(|(actual_type, actual_sql)| {
            actual_type == object_type
                && normalize_line_endings(&actual_sql) == normalize_line_endings(&sql)
        });
        if !actual_matches {
            return Ok(false);
        }
    }

    Ok(true)
}

fn migration_checksum(sql: &str) -> String {
    blake3::hash(normalize_line_endings(sql).as_bytes())
        .to_hex()
        .to_string()
}

fn windows_line_ending_checksum(sql: &str) -> String {
    let windows_sql = normalize_line_endings(sql).replace('\n', "\r\n");
    blake3::hash(windows_sql.as_bytes()).to_hex().to_string()
}

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};

    use super::{
        MIGRATIONS, migration_checksum, normalize_line_endings, run, windows_line_ending_checksum,
    };
    use crate::error::BackendError;

    fn create_migration_table(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    checksum TEXT NOT NULL,
                    applied_at_ms INTEGER NOT NULL
                ) STRICT;",
            )
            .expect("migration table");
    }

    fn record_core_checksum(connection: &Connection, checksum: &str) {
        connection
            .execute(
                "INSERT INTO schema_migrations (version, name, checksum, applied_at_ms)
                 VALUES (1, 'core', ?1, 0)",
                [checksum],
            )
            .expect("record core migration");
    }

    #[test]
    fn windows_line_ending_checksum_is_upgraded_when_schema_matches() {
        let mut connection = Connection::open_in_memory().expect("database");
        create_migration_table(&connection);
        let windows_sql = normalize_line_endings(MIGRATIONS[0].sql).replace('\n', "\r\n");
        connection
            .execute_batch(&windows_sql)
            .expect("Windows core schema");
        let windows_checksum = windows_line_ending_checksum(MIGRATIONS[0].sql);
        record_core_checksum(&connection, &windows_checksum);

        run(&mut connection).expect("compatible legacy database");

        let current_checksum = migration_checksum(MIGRATIONS[0].sql);
        let (recorded_checksum, migration_count): (String, i64) = connection
            .query_row(
                "SELECT checksum, (SELECT COUNT(*) FROM schema_migrations)
                 FROM schema_migrations WHERE version = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("upgraded migration record");

        assert_ne!(windows_checksum, current_checksum);
        assert_eq!(recorded_checksum, current_checksum);
        assert_eq!(migration_count, MIGRATIONS.len() as i64);
    }

    #[test]
    fn canonical_checksum_is_independent_of_line_endings() {
        let windows_sql = normalize_line_endings(MIGRATIONS[0].sql).replace('\n', "\r\n");

        assert_eq!(
            migration_checksum(MIGRATIONS[0].sql),
            migration_checksum(&windows_sql)
        );
    }

    #[test]
    fn all_windows_line_ending_checksums_are_upgraded_together() {
        let mut connection = Connection::open_in_memory().expect("database");
        create_migration_table(&connection);

        for migration in MIGRATIONS {
            let windows_sql = normalize_line_endings(migration.sql).replace('\n', "\r\n");
            connection
                .execute_batch(&windows_sql)
                .expect("Windows migration schema");
            connection
                .execute(
                    "INSERT INTO schema_migrations (version, name, checksum, applied_at_ms)
                     VALUES (?1, ?2, ?3, 0)",
                    params![
                        migration.version,
                        migration.name,
                        windows_line_ending_checksum(migration.sql)
                    ],
                )
                .expect("record Windows migration");
        }

        run(&mut connection).expect("compatible Windows database");

        for migration in MIGRATIONS {
            let recorded: String = connection
                .query_row(
                    "SELECT checksum FROM schema_migrations WHERE version = ?1",
                    [migration.version],
                    |row| row.get(0),
                )
                .expect("normalized checksum");
            assert_eq!(recorded, migration_checksum(migration.sql));
        }
    }

    #[test]
    fn windows_line_ending_checksum_is_rejected_when_core_schema_does_not_match() {
        let mut connection = Connection::open_in_memory().expect("database");
        create_migration_table(&connection);
        let windows_checksum = windows_line_ending_checksum(MIGRATIONS[0].sql);
        record_core_checksum(&connection, &windows_checksum);

        let error = run(&mut connection).expect_err("incomplete schema must be rejected");
        assert!(matches!(
            error,
            BackendError::MigrationChecksum {
                version: 1,
                name: "core"
            }
        ));
    }

    #[test]
    fn unknown_checksum_is_rejected_even_when_core_schema_matches() {
        let mut connection = Connection::open_in_memory().expect("database");
        create_migration_table(&connection);
        connection
            .execute_batch(MIGRATIONS[0].sql)
            .expect("core schema");
        record_core_checksum(&connection, "unknown-checksum");

        let error = run(&mut connection).expect_err("unknown checksum must be rejected");
        assert!(matches!(
            error,
            BackendError::MigrationChecksum {
                version: 1,
                name: "core"
            }
        ));

        let checksum: String = connection
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = ?1",
                params![1],
                |row| row.get(0),
            )
            .expect("unchanged checksum");
        assert_eq!(checksum, "unknown-checksum");
    }
}
