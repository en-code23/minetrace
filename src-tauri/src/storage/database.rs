use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use rusqlite::{Connection, TransactionBehavior};

use crate::error::BackendError;

use super::migrations;

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
}

impl Database {
    pub fn open(path: PathBuf) -> Result<Self, BackendError> {
        let mut connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        migrations::run(&mut connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub(super) fn lock(&self) -> Result<MutexGuard<'_, Connection>, BackendError> {
        self.connection
            .lock()
            .map_err(|_| BackendError::BackgroundTask("database lock was poisoned".to_owned()))
    }

    /// Executes a short read against the local database.
    ///
    /// Application services may use this for read models. The closure must not
    /// block on async work or retain the connection after it returns.
    pub(crate) fn read<T>(
        &self,
        reader: impl FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    ) -> Result<T, BackendError> {
        let connection = self.lock()?;
        reader(&connection).map_err(Into::into)
    }

    /// Executes one short write transaction with an immediate writer lock.
    pub(crate) fn write<T>(
        &self,
        writer: impl FnOnce(&rusqlite::Transaction<'_>) -> Result<T, BackendError>,
    ) -> Result<T, BackendError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = writer(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }

    pub fn recover_interrupted_scans(&self) -> Result<(), BackendError> {
        self.write(|transaction| {
            transaction.execute(
                "INSERT INTO scan_messages (
                    scan_id, severity, code, entity_ref, redacted_message, created_at_ms
                 )
                 SELECT id, 'warning', 'scan_interrupted', NULL,
                        'The scan was interrupted before promotion; the prior archive remains unchanged.',
                        CAST(unixepoch('subsec') * 1000 AS INTEGER)
                 FROM scan_runs
                 WHERE state IN ('queued', 'running', 'paused')",
                [],
            )?;
            transaction.execute(
                "UPDATE scan_runs
                 SET state = 'interrupted',
                     phase = 'recovery',
                     finished_at_ms = CAST(unixepoch('subsec') * 1000 AS INTEGER),
                     dataset_revision_after = dataset_revision_before,
                     counters_json = json_set(
                         counters_json,
                         '$.warnings',
                         COALESCE(json_extract(counters_json, '$.warnings'), 0) + 1
                     )
                 WHERE state IN ('queued', 'running', 'paused')",
                [],
            )?;
            for table in [
                "scan_staged_files",
                "scan_staged_evidence",
                "scan_staged_sessions",
                "scan_staged_sources",
                "scan_staged_locations",
            ] {
                transaction.execute(
                    &format!(
                        "DELETE FROM {table}
                         WHERE scan_id IN (
                             SELECT id FROM scan_runs WHERE state = 'interrupted'
                         )"
                    ),
                    [],
                )?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::Database;
    use crate::scan::ScanMode;

    #[test]
    fn opening_a_database_applies_all_migrations() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("minetrace.sqlite3")).expect("database");
        let connection = database.lock().expect("connection");
        let migration_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("migration count");

        assert_eq!(migration_count, 8);
    }

    #[test]
    fn recovery_records_one_durable_redacted_interruption_issue() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("minetrace.sqlite3")).expect("database");
        let scan = database.begin_scan(ScanMode::Standard).expect("scan");

        database.recover_interrupted_scans().expect("recover");
        database
            .recover_interrupted_scans()
            .expect("idempotent recovery");

        let (state, messages, warning_count): (String, i64, i64) = database
            .read(|connection| {
                let state = connection.query_row(
                    "SELECT state FROM scan_runs WHERE id = ?1",
                    [&scan.id],
                    |row| row.get(0),
                )?;
                let messages = connection.query_row(
                    "SELECT COUNT(*) FROM scan_messages WHERE scan_id = ?1",
                    [&scan.id],
                    |row| row.get(0),
                )?;
                let warning_count = connection.query_row(
                    "SELECT json_extract(counters_json, '$.warnings')
                     FROM scan_runs WHERE id = ?1",
                    [&scan.id],
                    |row| row.get(0),
                )?;
                Ok((state, messages, warning_count))
            })
            .expect("recovered state");
        assert_eq!(state, "interrupted");
        assert_eq!(messages, 1);
        assert_eq!(warning_count, 1);
    }
}
