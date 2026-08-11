use std::{collections::BTreeMap, fs, path::PathBuf};

use crate::{
    discovery::{AdapterRegistry, validate_directory, validate_directory_with_control},
    domain::{
        Confidence,
        location::{AdapterKind, DirectoryKind, DiscoveredInstallation, ValidationReport},
    },
    error::BackendError,
    platform::{PlatformPaths, native_path_key, path_from_native_key, stable_location_id},
    storage::Database,
};

#[derive(Clone)]
pub struct DiscoveryService {
    database: Database,
    paths: PlatformPaths,
    adapters: AdapterRegistry,
}

impl DiscoveryService {
    pub fn new(database: Database, paths: PlatformPaths, adapters: AdapterRegistry) -> Self {
        Self {
            database,
            paths,
            adapters,
        }
    }

    pub fn discover(&self) -> Result<Vec<DiscoveredInstallation>, BackendError> {
        self.discover_with_control(|| false)
    }

    pub(crate) fn discover_with_control<C>(
        &self,
        mut is_cancelled: C,
    ) -> Result<Vec<DiscoveredInstallation>, BackendError>
    where
        C: FnMut() -> bool,
    {
        let mut discovered = BTreeMap::<Vec<u8>, DiscoveredInstallation>::new();

        for adapter in self.adapters.iter() {
            for candidate in adapter.candidate_paths(&self.paths) {
                discovery_checkpoint(&mut is_cancelled)?;
                if !candidate.exists() {
                    continue;
                }

                let canonical = match fs::canonicalize(&candidate) {
                    Ok(path) => path,
                    Err(_) => continue,
                };
                let report = match validate_directory_with_control(&canonical, &mut is_cancelled) {
                    Ok(report) if report.is_supported() && adapter.accepts(&report) => report,
                    Ok(_) | Err(_) => continue,
                };
                let installation = self.installation(
                    canonical,
                    adapter.kind(),
                    adapter.display_name().to_owned(),
                    report,
                    "automatic",
                    true,
                );
                discovered.insert(native_path_key(&installation.path), installation);
            }
        }

        discovery_checkpoint(&mut is_cancelled)?;
        for stored in self.database.list_saved_locations()? {
            discovery_checkpoint(&mut is_cancelled)?;
            if let Some(automatic) = discovered.get_mut(&stored.path_key) {
                automatic.id = stored.id;
                automatic.enabled = stored.enabled;
                automatic.validation_score = stored.validation_score;
                automatic.confidence = Confidence::from_score(stored.validation_score);
                continue;
            }

            let path = path_from_native_key(&stored.path_key)
                .unwrap_or_else(|| PathBuf::from(&stored.path_display));
            let validated = validate_directory_with_control(&path, &mut is_cancelled)
                .ok()
                .filter(ValidationReport::is_supported);
            let available = validated.is_some();
            let (instances, confidence) = validated
                .map(|report| (report.instance_count.max(1), report.confidence()))
                .unwrap_or((0, Confidence::Unknown));
            let name = folder_name(&path);

            discovered.insert(
                stored.path_key,
                DiscoveredInstallation {
                    id: stored.id,
                    name,
                    kind_label: if available {
                        AdapterKind::Manual.label().to_owned()
                    } else {
                        "Unavailable".to_owned()
                    },
                    adapter_kind: AdapterKind::Manual,
                    path,
                    instances,
                    confidence,
                    validation_score: stored.validation_score,
                    enabled: stored.enabled && available,
                    platform: self.paths.platform,
                    origin: "custom",
                },
            );
        }

        discovery_checkpoint(&mut is_cancelled)?;
        Ok(discovered.into_values().collect())
    }

    pub fn add_custom_location(
        &self,
        requested_path: PathBuf,
    ) -> Result<DiscoveredInstallation, BackendError> {
        if requested_path.as_os_str().is_empty() {
            return Err(BackendError::InvalidLocation {
                path: String::new(),
                reason: "The selected path is empty.".to_owned(),
                score: 0,
            });
        }

        let canonical = fs::canonicalize(&requested_path).map_err(|error| {
            BackendError::io(
                "resolve selected Minecraft location",
                &requested_path,
                error,
            )
        })?;
        let report = validate_directory(&canonical)?;
        if !report.is_supported() {
            return Err(BackendError::InvalidLocation {
                path: canonical.to_string_lossy().into_owned(),
                reason: if report.markers.is_empty() {
                    "No recognized Minecraft directory markers were found.".to_owned()
                } else {
                    format!(
                        "Only these markers were found: {}.",
                        report.markers.join(", ")
                    )
                },
                score: report.score,
            });
        }

        let installation = self.installation(
            canonical.clone(),
            AdapterKind::Manual,
            folder_name(&canonical),
            report,
            "custom",
            true,
        );
        self.database.upsert_scan_location(&installation)?;
        Ok(installation)
    }

    fn installation(
        &self,
        path: PathBuf,
        adapter_kind: AdapterKind,
        name: String,
        report: ValidationReport,
        origin: &'static str,
        enabled: bool,
    ) -> DiscoveredInstallation {
        let path_key = native_path_key(&path);
        let instances = match report.kind {
            DirectoryKind::GameDirectory | DirectoryKind::InstanceDirectory => 1,
            DirectoryKind::LauncherRoot => report.instance_count,
            DirectoryKind::Unknown => 0,
        };

        DiscoveredInstallation {
            id: stable_location_id(&path_key),
            name,
            kind_label: adapter_kind.label().to_owned(),
            adapter_kind,
            path,
            instances,
            confidence: report.confidence(),
            validation_score: report.score,
            enabled,
            platform: self.paths.platform,
            origin,
        }
    }
}

fn discovery_checkpoint<C>(is_cancelled: &mut C) -> Result<(), BackendError>
where
    C: FnMut() -> bool,
{
    if is_cancelled() {
        Err(BackendError::BackgroundTask(
            "launcher discovery cancelled".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn folder_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Custom folder".to_owned())
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::DiscoveryService;
    use crate::{
        discovery::AdapterRegistry, domain::PlatformKind, platform::PlatformPaths,
        storage::Database,
    };

    #[test]
    fn a_valid_custom_location_is_persisted_and_rediscovered() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("Custom Game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");

        let database = Database::open(temp.path().join("minetrace.sqlite3")).expect("database");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("home"),
            PathBuf::from("/unused"),
        );
        let service = DiscoveryService::new(database, paths, AdapterRegistry::standard());

        let added = service
            .add_custom_location(game.clone())
            .expect("add location");
        let discovered = service.discover().expect("discover locations");

        assert_eq!(added.name, "Custom Game");
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].id, added.id);
        assert_eq!(discovered[0].path, fs::canonicalize(game).expect("path"));
    }

    #[test]
    fn an_unavailable_saved_location_is_not_enabled_for_scanning() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("External Game");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");

        let database = Database::open(temp.path().join("minetrace.sqlite3")).expect("database");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("home"),
            PathBuf::from("/unused"),
        );
        let service = DiscoveryService::new(database, paths, AdapterRegistry::standard());
        service
            .add_custom_location(game.clone())
            .expect("add location");
        fs::remove_dir_all(&game).expect("remove location");

        let discovered = service.discover().expect("discover locations");

        assert_eq!(discovered.len(), 1);
        assert!(!discovered[0].enabled);
        assert_eq!(discovered[0].instances, 0);
        assert_eq!(discovered[0].kind_label, "Unavailable");
    }

    #[test]
    fn scan_time_discovery_honors_its_control_callback() {
        let temp = tempdir().expect("tempdir");
        let database = Database::open(temp.path().join("minetrace.sqlite3")).expect("database");
        let paths = PlatformPaths::test(
            PlatformKind::Linux,
            temp.path().join("home"),
            PathBuf::from("/unused"),
        );
        let service = DiscoveryService::new(database, paths, AdapterRegistry::standard());

        let result = service.discover_with_control(|| true);

        assert!(result.is_err());
    }
}
