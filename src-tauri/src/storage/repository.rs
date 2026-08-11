use std::path::Path;

use chrono::Utc;
use rusqlite::{Connection, params};

use crate::{
    domain::location::DiscoveredInstallation,
    error::BackendError,
    platform::{native_path_key, path_from_native_key},
};

use super::Database;

#[derive(Debug, Clone)]
pub struct StoredScanLocation {
    pub id: String,
    pub path_key: Vec<u8>,
    pub path_display: String,
    pub enabled: bool,
    pub validation_score: u8,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredInstance {
    pub id: String,
    pub relative_root: std::path::PathBuf,
    pub name: String,
}

impl Database {
    pub fn upsert_scan_location(
        &self,
        installation: &DiscoveredInstallation,
    ) -> Result<(), BackendError> {
        self.upsert_scan_locations(std::slice::from_ref(installation))?;
        Ok(())
    }

    /// Persists automatic discovery results before source rows reference them.
    ///
    /// Discovery itself remains read-only; the ScanService calls this after the
    /// user confirms which automatic roots are enabled.
    pub(crate) fn upsert_scan_locations(
        &self,
        installations: &[DiscoveredInstallation],
    ) -> Result<usize, BackendError> {
        self.write(|transaction| {
            let now = Utc::now().timestamp_millis();
            for installation in installations {
                upsert_scan_location(transaction, installation, now)?;
            }
            Ok(installations.len())
        })
    }

    pub fn list_saved_locations(&self) -> Result<Vec<StoredScanLocation>, BackendError> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, path_key, path_display, enabled, validation_score
             FROM scan_locations
             ORDER BY created_at_ms, id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(StoredScanLocation {
                id: row.get(0)?,
                path_key: row.get(1)?,
                path_display: row.get(2)?,
                enabled: row.get::<_, i64>(3)? != 0,
                validation_score: row.get::<_, i64>(4)?.clamp(0, 100) as u8,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub(crate) fn upsert_instance(
        &self,
        installation: &DiscoveredInstallation,
        relative_root: &Path,
        name: &str,
    ) -> Result<StoredInstance, BackendError> {
        let relative_key = native_path_key(relative_root);
        let installation_id = stable_row_id("installation", &[installation.id.as_bytes()]);
        let instance_id = stable_row_id(
            "instance",
            &[installation.id.as_bytes(), relative_key.as_slice()],
        );
        let relative_display = if relative_root.as_os_str().is_empty() {
            ".".to_owned()
        } else {
            relative_root.to_string_lossy().into_owned()
        };
        let now = Utc::now().timestamp_millis();

        self.write(|transaction| {
            transaction.execute(
                "INSERT INTO launcher_installations (
                    id, location_id, kind, display_name, confidence_score,
                    first_seen_at_ms, last_seen_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    location_id = excluded.location_id,
                    kind = excluded.kind,
                    display_name = excluded.display_name,
                    confidence_score = excluded.confidence_score,
                    last_seen_at_ms = excluded.last_seen_at_ms",
                params![
                    installation_id,
                    installation.id,
                    installation.adapter_kind.as_str(),
                    installation.kind_label,
                    i64::from(installation.validation_score),
                    now,
                ],
            )?;
            transaction.execute(
                "INSERT INTO instances (
                    id, installation_id, location_id, relative_path_key,
                    relative_path_display, name, confidence_score,
                    first_seen_at_ms, last_seen_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
                 ON CONFLICT(location_id, relative_path_key) DO UPDATE SET
                    installation_id = excluded.installation_id,
                    name = excluded.name,
                    confidence_score = excluded.confidence_score,
                    last_seen_at_ms = excluded.last_seen_at_ms",
                params![
                    instance_id,
                    installation_id,
                    installation.id,
                    relative_key,
                    relative_display,
                    name,
                    i64::from(installation.validation_score),
                    now,
                ],
            )?;
            Ok(())
        })?;

        Ok(StoredInstance {
            id: instance_id,
            relative_root: relative_root.to_path_buf(),
            name: name.to_owned(),
        })
    }

    pub(crate) fn list_instances_for_location(
        &self,
        location_id: &str,
    ) -> Result<Vec<StoredInstance>, BackendError> {
        self.read(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, relative_path_key, name
                 FROM instances WHERE location_id = ?1
                 ORDER BY relative_path_key",
            )?;
            statement
                .query_map([location_id], |row| {
                    let key: Vec<u8> = row.get(1)?;
                    Ok(StoredInstance {
                        id: row.get(0)?,
                        relative_root: path_from_native_key(&key).unwrap_or_default(),
                        name: row.get(2)?,
                    })
                })?
                .collect()
        })
    }
}

fn upsert_scan_location(
    connection: &Connection,
    installation: &DiscoveredInstallation,
    now: i64,
) -> Result<(), rusqlite::Error> {
    let path_key = native_path_key(&installation.path);
    connection.execute(
        "INSERT INTO scan_locations (
            id, origin, adapter_kind, platform, path_key, path_display, enabled,
            validation_score, status, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'available', ?9, ?9)
         ON CONFLICT(path_key) DO UPDATE SET
            origin = excluded.origin,
            adapter_kind = excluded.adapter_kind,
            path_display = excluded.path_display,
            enabled = excluded.enabled,
            validation_score = excluded.validation_score,
            status = 'available',
            updated_at_ms = excluded.updated_at_ms",
        params![
            installation.id,
            installation.origin,
            installation.adapter_kind.as_str(),
            installation.platform.as_str(),
            path_key,
            installation.path.to_string_lossy(),
            i64::from(installation.enabled),
            i64::from(installation.validation_score),
            now,
        ],
    )?;
    Ok(())
}

fn stable_row_id(namespace: &str, pieces: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(namespace.as_bytes());
    for piece in pieces {
        hasher.update(&(piece.len() as u64).to_le_bytes());
        hasher.update(piece);
    }
    let digest = hasher.finalize().to_hex().to_string();
    format!("{namespace}_{}", &digest[..24])
}
