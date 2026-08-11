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
        let checksum = blake3::hash(migration.sql.as_bytes()).to_hex().to_string();
        let recorded: Option<String> = connection
            .query_row(
                "SELECT checksum FROM schema_migrations WHERE version = ?1",
                [migration.version],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(recorded) = recorded {
            if recorded != checksum {
                return Err(BackendError::MigrationChecksum {
                    version: migration.version,
                    name: migration.name,
                });
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
