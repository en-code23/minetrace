use std::{fs, path::Path};

use crate::{
    domain::location::{DirectoryKind, ValidationReport},
    error::BackendError,
};

const MAX_LAUNCHER_INSTANCE_ENTRIES: usize = 10_000;
const VALIDATION_CANCELLED: &str = "directory validation cancelled";

pub fn validate_directory(path: &Path) -> Result<ValidationReport, BackendError> {
    validate_directory_with_control(path, &mut || false)
}

pub(crate) fn validate_directory_with_control<C>(
    path: &Path,
    is_cancelled: &mut C,
) -> Result<ValidationReport, BackendError>
where
    C: FnMut() -> bool,
{
    check_cancelled(is_cancelled)?;
    if path.as_os_str().is_empty() {
        return Ok(unknown());
    }

    let metadata = fs::metadata(path)
        .map_err(|error| BackendError::io("read directory metadata", path, error))?;
    if !metadata.is_dir() {
        return Ok(unknown());
    }

    // Reading once surfaces permission failures instead of treating them as an invalid layout.
    fs::read_dir(path).map_err(|error| BackendError::io("read directory", path, error))?;

    let launcher = inspect_launcher_root(path, is_cancelled)?;
    if launcher.is_supported() {
        return Ok(launcher);
    }

    let nested_game = path.join(".minecraft");
    check_cancelled(is_cancelled)?;
    if nested_game.is_dir() {
        let child = inspect_game_directory(&nested_game);
        if child.is_supported() {
            let mut markers = vec![".minecraft"];
            if path.join("instance.cfg").is_file() {
                markers.push("instance.cfg");
            }
            if path.join("mmc-pack.json").is_file() {
                markers.push("mmc-pack.json");
            }
            markers.extend(child.markers);
            markers.sort_unstable();
            markers.dedup();

            return Ok(ValidationReport {
                kind: DirectoryKind::InstanceDirectory,
                score: child.score.saturating_add(10).min(100),
                instance_count: 1,
                markers,
            });
        }
    }

    let game = inspect_game_directory(path);
    if game.is_supported() {
        return Ok(game);
    }

    Ok(unknown())
}

fn inspect_game_directory(path: &Path) -> ValidationReport {
    let mut score = 0_u8;
    let mut markers = Vec::new();

    add_marker(
        path.join("logs").is_dir(),
        "logs",
        25,
        &mut score,
        &mut markers,
    );
    add_marker(
        path.join("versions").is_dir(),
        "versions",
        18,
        &mut score,
        &mut markers,
    );
    add_marker(
        path.join("saves").is_dir(),
        "saves",
        12,
        &mut score,
        &mut markers,
    );
    add_marker(
        path.join("options.txt").is_file(),
        "options.txt",
        18,
        &mut score,
        &mut markers,
    );
    add_marker(
        path.join("launcher_profiles.json").is_file(),
        "launcher_profiles.json",
        15,
        &mut score,
        &mut markers,
    );
    add_marker(
        path.join("assets").is_dir(),
        "assets",
        8,
        &mut score,
        &mut markers,
    );
    add_marker(
        path.join("libraries").is_dir(),
        "libraries",
        8,
        &mut score,
        &mut markers,
    );

    let kind = if markers.len() >= 2 && score >= 30 {
        DirectoryKind::GameDirectory
    } else {
        DirectoryKind::Unknown
    };

    ValidationReport {
        kind,
        score,
        instance_count: usize::from(kind == DirectoryKind::GameDirectory),
        markers,
    }
}

fn inspect_launcher_root<C>(
    path: &Path,
    is_cancelled: &mut C,
) -> Result<ValidationReport, BackendError>
where
    C: FnMut() -> bool,
{
    let instances_dir = find_child_directory(path, "instances", is_cancelled)?;
    let mut score = 0_u8;
    let mut markers = Vec::new();

    add_marker(
        instances_dir.is_some(),
        "instances",
        25,
        &mut score,
        &mut markers,
    );
    add_marker(
        path.join("prismlauncher.cfg").is_file(),
        "prismlauncher.cfg",
        30,
        &mut score,
        &mut markers,
    );
    add_marker(
        path.join("multimc.cfg").is_file(),
        "multimc.cfg",
        30,
        &mut score,
        &mut markers,
    );

    let instance_count = if let Some(instances_dir) = &instances_dir {
        count_instance_directories(instances_dir, MAX_LAUNCHER_INSTANCE_ENTRIES, is_cancelled)?
    } else {
        0
    };
    if instance_count > 0 {
        markers.push("instance-game-directories");
        score = score
            .saturating_add(25)
            .saturating_add((instance_count.min(4) as u8) * 5)
            .min(100);
    }

    let has_config = markers
        .iter()
        .any(|marker| matches!(*marker, "prismlauncher.cfg" | "multimc.cfg"));
    let kind =
        if instances_dir.is_some() && (has_config || instance_count > 0) && markers.len() >= 2 {
            DirectoryKind::LauncherRoot
        } else {
            DirectoryKind::Unknown
        };

    Ok(ValidationReport {
        kind,
        score,
        instance_count,
        markers,
    })
}

fn find_child_directory<C>(
    path: &Path,
    expected_name: &str,
    is_cancelled: &mut C,
) -> Result<Option<std::path::PathBuf>, BackendError>
where
    C: FnMut() -> bool,
{
    let entries =
        fs::read_dir(path).map_err(|error| BackendError::io("read launcher root", path, error))?;
    for entry in entries {
        check_cancelled(is_cancelled)?;
        let entry =
            entry.map_err(|error| BackendError::io("read launcher root entry", path, error))?;
        let file_type = entry.file_type().map_err(|error| {
            BackendError::io("read launcher root entry type", &entry.path(), error)
        })?;
        if file_type.is_dir()
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(expected_name)
        {
            return Ok(Some(entry.path()));
        }
    }
    Ok(None)
}

fn count_instance_directories<C>(
    instances_dir: &Path,
    max_entries: usize,
    is_cancelled: &mut C,
) -> Result<usize, BackendError>
where
    C: FnMut() -> bool,
{
    let entries = fs::read_dir(instances_dir)
        .map_err(|error| BackendError::io("read launcher instances", instances_dir, error))?;
    let mut count = 0;

    for (index, entry) in entries.enumerate() {
        check_cancelled(is_cancelled)?;
        if index >= max_entries {
            return Err(BackendError::BackgroundTask(format!(
                "launcher instance validation exceeded the safe {max_entries}-entry limit"
            )));
        }
        let entry = entry
            .map_err(|error| BackendError::io("read launcher instance", instances_dir, error))?;
        let file_type = entry.file_type().map_err(|error| {
            BackendError::io("read launcher instance type", &entry.path(), error)
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let instance = entry.path();
        let prism_game = instance.join(".minecraft");
        let prism_layout = prism_game.is_dir()
            && (prism_game.join("logs").is_dir()
                || prism_game.join("options.txt").is_file()
                || prism_game.join("versions").is_dir());
        let direct_layout = instance.join("logs").is_dir()
            && (instance.join("minecraftinstance.json").is_file()
                || instance.join("options.txt").is_file()
                || instance.join("mods").is_dir()
                || instance.join("config").is_dir());
        if prism_layout || direct_layout {
            count += 1;
        }
    }

    Ok(count)
}

fn check_cancelled<C>(is_cancelled: &mut C) -> Result<(), BackendError>
where
    C: FnMut() -> bool,
{
    if is_cancelled() {
        Err(BackendError::BackgroundTask(
            VALIDATION_CANCELLED.to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn add_marker(
    present: bool,
    marker: &'static str,
    weight: u8,
    score: &mut u8,
    markers: &mut Vec<&'static str>,
) {
    if present {
        *score = score.saturating_add(weight).min(100);
        markers.push(marker);
    }
}

fn unknown() -> ValidationReport {
    ValidationReport {
        kind: DirectoryKind::Unknown,
        score: 0,
        instance_count: 0,
        markers: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{count_instance_directories, validate_directory, validate_directory_with_control};
    use crate::{domain::location::DirectoryKind, error::BackendError};

    #[test]
    fn launcher_instance_enumeration_stops_at_its_configured_entry_cap() {
        let root = tempdir().expect("tempdir");
        let instances = root.path().join("instances");
        fs::create_dir(&instances).expect("instances");
        for name in ["one", "two", "three"] {
            fs::create_dir(instances.join(name)).expect("instance entry");
        }

        let result = count_instance_directories(&instances, 2, &mut || false);

        assert!(matches!(
            result,
            Err(BackendError::BackgroundTask(message)) if message.contains("2-entry limit")
        ));
    }

    #[test]
    fn directory_validation_honors_cancellation_before_enumeration() {
        let root = tempdir().expect("tempdir");
        fs::create_dir(root.path().join("instances")).expect("instances");

        let result = validate_directory_with_control(root.path(), &mut || true);

        assert!(matches!(
            result,
            Err(BackendError::BackgroundTask(message)) if message == super::VALIDATION_CANCELLED
        ));
    }

    #[test]
    fn rejects_a_single_weak_marker() {
        let root = tempdir().expect("tempdir");
        fs::create_dir(root.path().join("logs")).expect("logs");

        let report = validate_directory(root.path()).expect("validation");
        assert_eq!(report.kind, DirectoryKind::Unknown);
        assert!(!report.is_supported());
    }

    #[test]
    fn accepts_a_multi_marker_game_directory() {
        let root = tempdir().expect("tempdir");
        fs::create_dir(root.path().join("logs")).expect("logs");
        fs::write(root.path().join("options.txt"), "fov:0.0").expect("options");

        let report = validate_directory(root.path()).expect("validation");
        assert_eq!(report.kind, DirectoryKind::GameDirectory);
        assert_eq!(report.instance_count, 1);
        assert!(report.is_supported());
    }

    #[test]
    fn accepts_a_prism_root_and_counts_instances() {
        let root = tempdir().expect("tempdir");
        fs::write(
            root.path().join("prismlauncher.cfg"),
            "InstanceDir=instances",
        )
        .expect("config");
        let game = root
            .path()
            .join("instances")
            .join("Redstone")
            .join(".minecraft");
        fs::create_dir_all(game.join("logs")).expect("instance logs");

        let report = validate_directory(root.path()).expect("validation");
        assert_eq!(report.kind, DirectoryKind::LauncherRoot);
        assert_eq!(report.instance_count, 1);
        assert!(report.is_supported());
    }

    #[test]
    fn accepts_a_manual_curseforge_style_root_with_direct_game_directories() {
        let root = tempdir().expect("tempdir");
        let profile = root.path().join("Instances/Builder Pack");
        fs::create_dir_all(profile.join("logs")).expect("logs");
        fs::create_dir_all(profile.join("mods")).expect("mods");
        fs::write(profile.join("minecraftinstance.json"), "{}").expect("manifest");

        let report = validate_directory(root.path()).expect("validation");

        assert_eq!(report.kind, DirectoryKind::LauncherRoot);
        assert!(report.is_supported());
        assert_eq!(report.instance_count, 1);
    }

    #[test]
    fn accepts_a_single_prism_instance_folder() {
        let root = tempdir().expect("tempdir");
        fs::write(root.path().join("instance.cfg"), "name=Redstone").expect("config");
        let game = root.path().join(".minecraft");
        fs::create_dir_all(game.join("logs")).expect("logs");
        fs::write(game.join("options.txt"), "fov:0.0").expect("options");

        let report = validate_directory(root.path()).expect("validation");
        assert_eq!(report.kind, DirectoryKind::InstanceDirectory);
        assert_eq!(report.instance_count, 1);
        assert!(report.is_supported());
    }
}
