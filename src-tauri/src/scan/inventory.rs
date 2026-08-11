use std::{ffi::OsString, fs, path::Path};

use crate::platform::native_path_key;

use super::{
    ScanError,
    model::{InventoryReport, InventoryWarning, InventoryWarningKind, LogCandidate, LogFileKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TraversalScope {
    Root,
    InstanceCollection,
    Instance,
    GameDirectory,
    Logs,
}

#[derive(Debug, Clone)]
pub(crate) struct InventoryOptions {
    pub max_depth: usize,
    pub max_entries: usize,
    pub max_file_size_bytes: u64,
}

impl Default for InventoryOptions {
    fn default() -> Self {
        Self {
            max_depth: 24,
            max_entries: 250_000,
            max_file_size_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

pub(crate) fn inventory_logs(
    root: &Path,
    options: &InventoryOptions,
) -> Result<InventoryReport, ScanError> {
    inventory_logs_with_control(root, options, || false)
}

pub(crate) fn inventory_logs_with_control<C>(
    root: &Path,
    options: &InventoryOptions,
    mut is_cancelled: C,
) -> Result<InventoryReport, ScanError>
where
    C: FnMut() -> bool,
{
    check_cancelled(&mut is_cancelled)?;
    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| ScanError::io("read scan root metadata", root, error))?;
    if root_metadata.file_type().is_symlink() {
        return Err(ScanError::RootIsSymlink(
            root.to_string_lossy().into_owned(),
        ));
    }
    if !root_metadata.is_dir() {
        return Err(ScanError::RootNotDirectory(
            root.to_string_lossy().into_owned(),
        ));
    }

    let mut report = InventoryReport {
        root: root.to_path_buf(),
        candidates: Vec::new(),
        warnings: Vec::new(),
        visited_entries: 0,
        skipped_symlinks: 0,
    };
    let scope = if path_name_eq(root, "logs") {
        TraversalScope::Logs
    } else {
        TraversalScope::Root
    };
    walk(
        root,
        root,
        0,
        scope,
        options,
        &mut report,
        &mut is_cancelled,
    )?;
    report
        .candidates
        .sort_by(|left, right| left.relative_path_key.cmp(&right.relative_path_key));
    Ok(report)
}

fn walk<C>(
    root: &Path,
    directory: &Path,
    depth: usize,
    scope: TraversalScope,
    options: &InventoryOptions,
    report: &mut InventoryReport,
    is_cancelled: &mut C,
) -> Result<(), ScanError>
where
    C: FnMut() -> bool,
{
    check_cancelled(is_cancelled)?;
    let read_dir = match fs::read_dir(directory) {
        Ok(read_dir) => read_dir,
        Err(error) if depth > 0 => {
            report.warnings.push(InventoryWarning {
                kind: InventoryWarningKind::UnreadableEntry,
                path: relative_or_absolute(root, directory),
                message: error.to_string(),
            });
            return Ok(());
        }
        Err(error) => return Err(ScanError::io("read scan root", directory, error)),
    };

    let remaining_entries = options.max_entries.saturating_sub(report.visited_entries);
    let mut entries = Vec::<(OsString, fs::DirEntry)>::new();
    for entry in read_dir {
        check_cancelled(is_cancelled)?;
        match entry {
            Ok(entry) => {
                if entries.len() >= remaining_entries {
                    return Err(ScanError::EntryLimitExceeded {
                        root: root.to_string_lossy().into_owned(),
                        limit: options.max_entries,
                    });
                }
                entries.push((entry.file_name(), entry));
            }
            Err(error) => report.warnings.push(InventoryWarning {
                kind: InventoryWarningKind::UnreadableEntry,
                path: relative_or_absolute(root, directory),
                message: error.to_string(),
            }),
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    for (_, entry) in entries {
        check_cancelled(is_cancelled)?;
        report.visited_entries += 1;
        if report.visited_entries > options.max_entries {
            return Err(ScanError::EntryLimitExceeded {
                root: root.to_string_lossy().into_owned(),
                limit: options.max_entries,
            });
        }

        let path = entry.path();
        let relative = relative_or_absolute(root, &path);
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                report.warnings.push(InventoryWarning {
                    kind: InventoryWarningKind::UnreadableEntry,
                    path: relative,
                    message: error.to_string(),
                });
                continue;
            }
        };

        if file_type.is_symlink() {
            report.skipped_symlinks += 1;
            report.warnings.push(InventoryWarning {
                kind: InventoryWarningKind::SymlinkSkipped,
                path: relative,
                message: "symbolic links are not followed".to_owned(),
            });
            continue;
        }

        if file_type.is_dir() {
            let Some(next_scope) = child_scope(scope, &entry.file_name()) else {
                continue;
            };
            if depth >= options.max_depth {
                report.warnings.push(InventoryWarning {
                    kind: InventoryWarningKind::DepthLimitReached,
                    path: relative,
                    message: format!("maximum scan depth {} reached", options.max_depth),
                });
                continue;
            }
            walk(
                root,
                &path,
                depth + 1,
                next_scope,
                options,
                report,
                is_cancelled,
            )?;
            continue;
        }

        if scope != TraversalScope::Logs || !file_type.is_file() {
            continue;
        }
        let Some(kind) = classify_log_name(&entry.file_name()) else {
            continue;
        };
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                report.warnings.push(InventoryWarning {
                    kind: InventoryWarningKind::UnreadableEntry,
                    path: relative,
                    message: error.to_string(),
                });
                continue;
            }
        };
        if metadata.len() > options.max_file_size_bytes {
            report.warnings.push(InventoryWarning {
                kind: InventoryWarningKind::FileTooLarge,
                path: relative,
                message: format!(
                    "file is {} bytes; limit is {} bytes",
                    metadata.len(),
                    options.max_file_size_bytes
                ),
            });
            continue;
        }

        let relative_path = path
            .strip_prefix(root)
            .expect("walked paths remain inside the root")
            .to_path_buf();
        report.candidates.push(LogCandidate {
            approved_root: root.to_path_buf(),
            absolute_path: path,
            relative_path_key: native_path_key(&relative_path),
            relative_path,
            kind,
            observed_size_bytes: metadata.len(),
        });
    }

    Ok(())
}

fn check_cancelled<C>(is_cancelled: &mut C) -> Result<(), ScanError>
where
    C: FnMut() -> bool,
{
    if is_cancelled() {
        Err(ScanError::Cancelled)
    } else {
        Ok(())
    }
}

fn child_scope(scope: TraversalScope, name: &std::ffi::OsStr) -> Option<TraversalScope> {
    match scope {
        TraversalScope::Root => {
            if name_eq(name, "logs") {
                Some(TraversalScope::Logs)
            } else if name_eq(name, ".minecraft") || name_eq(name, "minecraft") {
                Some(TraversalScope::GameDirectory)
            } else if name_eq(name, "instances") {
                Some(TraversalScope::InstanceCollection)
            } else {
                None
            }
        }
        TraversalScope::InstanceCollection => Some(TraversalScope::Instance),
        TraversalScope::Instance => {
            if name_eq(name, "logs") {
                Some(TraversalScope::Logs)
            } else if name_eq(name, ".minecraft") || name_eq(name, "minecraft") {
                Some(TraversalScope::GameDirectory)
            } else {
                None
            }
        }
        TraversalScope::GameDirectory => name_eq(name, "logs").then_some(TraversalScope::Logs),
        TraversalScope::Logs => Some(TraversalScope::Logs),
    }
}

fn path_name_eq(path: &Path, expected: &str) -> bool {
    path.file_name().is_some_and(|name| name_eq(name, expected))
}

fn name_eq(name: &std::ffi::OsStr, expected: &str) -> bool {
    name.to_string_lossy().eq_ignore_ascii_case(expected)
}

fn classify_log_name(name: &std::ffi::OsStr) -> Option<LogFileKind> {
    let name = name.to_string_lossy();
    let bytes = name.as_bytes();
    if bytes
        .get(bytes.len().saturating_sub(7)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".log.gz"))
    {
        Some(LogFileKind::CompressedLog)
    } else if bytes
        .get(bytes.len().saturating_sub(4)..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(b".log"))
    {
        Some(LogFileKind::Log)
    } else {
        None
    }
}

fn relative_or_absolute(root: &Path, path: &Path) -> std::path::PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, fs};

    use tempfile::tempdir;

    use super::{InventoryOptions, inventory_logs, inventory_logs_with_control};
    use crate::scan::{InventoryWarningKind, LogFileKind, ScanError};

    #[test]
    fn inventories_only_plain_and_compressed_logs_in_stable_order() {
        let temp = tempdir().expect("tempdir");
        let logs = temp.path().join("logs");
        fs::create_dir_all(&logs).expect("logs");
        fs::write(logs.join("z.LOG"), "z").expect("log");
        fs::write(logs.join("a.log.gz"), "a").expect("gzip");
        fs::write(logs.join("ignore.txt"), "ignore").expect("text");

        let report = inventory_logs(temp.path(), &InventoryOptions::default()).expect("inventory");

        assert_eq!(report.candidates.len(), 2);
        assert_eq!(report.candidates[0].kind, LogFileKind::CompressedLog);
        assert_eq!(report.candidates[1].kind, LogFileKind::Log);
        assert_eq!(report.candidates[0].observed_size_bytes, 1);
    }

    #[test]
    fn cancellation_is_checked_while_walking_directory_entries() {
        let temp = tempdir().expect("tempdir");
        let logs = temp.path().join("logs");
        fs::create_dir_all(&logs).expect("logs");
        for index in 0..100 {
            fs::write(logs.join(format!("{index}.log")), "entry").expect("log entry");
        }
        let checks = Cell::new(0_u32);

        let error = inventory_logs_with_control(temp.path(), &InventoryOptions::default(), || {
            let next = checks.get().saturating_add(1);
            checks.set(next);
            next >= 10
        })
        .expect_err("inventory must stop cooperatively");

        assert!(matches!(error, ScanError::Cancelled));
        assert_eq!(checks.get(), 10);
    }

    #[test]
    fn prunes_large_irrelevant_game_trees_before_the_entry_limit() {
        let temp = tempdir().expect("tempdir");
        for directory in ["assets", "libraries", "mods", "resourcepacks", "saves"] {
            let irrelevant = temp.path().join(directory).join("nested");
            fs::create_dir_all(&irrelevant).expect("irrelevant directory");
            for index in 0..100 {
                fs::write(irrelevant.join(format!("entry-{index:03}.log")), "ignored")
                    .expect("irrelevant entry");
            }
        }
        let logs = temp.path().join("logs");
        fs::create_dir_all(&logs).expect("logs");
        fs::write(logs.join("latest.log"), "session").expect("log");

        let report = inventory_logs(
            temp.path(),
            &InventoryOptions {
                max_entries: 10,
                ..InventoryOptions::default()
            },
        )
        .expect("pruned inventory");

        assert_eq!(report.candidates.len(), 1);
        assert_eq!(
            report.candidates[0].relative_path,
            logs_relative("latest.log")
        );
        assert_eq!(report.visited_entries, 7);
    }

    #[test]
    fn finds_prism_instance_logs_without_entering_game_content_trees() {
        let temp = tempdir().expect("tempdir");
        let game = temp.path().join("instances/Redstone/.minecraft");
        let assets = game.join("assets/indexes");
        fs::create_dir_all(&assets).expect("assets");
        for index in 0..100 {
            fs::write(assets.join(format!("index-{index:03}.log")), "ignored")
                .expect("asset entry");
        }
        fs::create_dir_all(game.join("mods")).expect("mods");
        fs::write(game.join("mods/not-a-source.log"), "ignored").expect("mod file");
        fs::create_dir_all(game.join("logs/archive")).expect("logs");
        fs::write(game.join("logs/latest.log"), "latest").expect("latest log");
        fs::write(game.join("logs/archive/older.log.gz"), "older").expect("older log");

        let report = inventory_logs(
            temp.path(),
            &InventoryOptions {
                max_entries: 10,
                ..InventoryOptions::default()
            },
        )
        .expect("launcher inventory");

        assert_eq!(report.candidates.len(), 2);
        assert!(report.candidates.iter().all(|candidate| {
            candidate
                .relative_path
                .starts_with("instances/Redstone/.minecraft/logs")
        }));
        assert!(report.visited_entries <= 10);
    }

    #[test]
    fn supports_a_manually_selected_instance_or_logs_directory() {
        let instance = tempdir().expect("instance tempdir");
        fs::create_dir_all(instance.path().join(".minecraft/logs")).expect("instance logs");
        fs::write(
            instance.path().join(".minecraft/logs/latest.log"),
            "instance",
        )
        .expect("instance log");

        let instance_report =
            inventory_logs(instance.path(), &InventoryOptions::default()).expect("instance scan");
        assert_eq!(instance_report.candidates.len(), 1);

        let direct = tempdir().expect("direct tempdir");
        let logs = direct.path().join("logs");
        fs::create_dir_all(&logs).expect("direct logs");
        fs::write(logs.join("latest.log"), "direct").expect("direct log");

        let logs_report =
            inventory_logs(&logs, &InventoryOptions::default()).expect("direct logs scan");
        assert_eq!(logs_report.candidates.len(), 1);
        assert_eq!(
            logs_report.candidates[0].relative_path,
            std::path::PathBuf::from("latest.log")
        );
    }

    #[test]
    fn enforces_entry_and_depth_limits() {
        let temp = tempdir().expect("tempdir");
        let deep = temp.path().join("logs").join("two");
        fs::create_dir_all(&deep).expect("deep");
        fs::write(deep.join("latest.log"), "log").expect("log");

        let report = inventory_logs(
            temp.path(),
            &InventoryOptions {
                max_depth: 0,
                ..InventoryOptions::default()
            },
        )
        .expect("inventory");
        assert!(report.candidates.is_empty());
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.kind == InventoryWarningKind::DepthLimitReached)
        );

        let error = inventory_logs(
            temp.path(),
            &InventoryOptions {
                max_entries: 0,
                ..InventoryOptions::default()
            },
        )
        .expect_err("entry limit");
        assert!(matches!(error, ScanError::EntryLimitExceeded { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_symbolic_links() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("tempdir");
        let outside = tempdir().expect("outside");
        fs::write(outside.path().join("private.log"), "secret").expect("outside log");
        symlink(outside.path(), temp.path().join("linked-logs")).expect("symlink");

        let report = inventory_logs(temp.path(), &InventoryOptions::default()).expect("inventory");
        assert!(report.candidates.is_empty());
        assert_eq!(report.skipped_symlinks, 1);
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.kind == InventoryWarningKind::SymlinkSkipped)
        );
    }

    fn logs_relative(name: &str) -> std::path::PathBuf {
        std::path::Path::new("logs").join(name)
    }
}
