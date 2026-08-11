use std::path::{Component, Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SourceFileKind {
    Log,
    CompressedLog,
    CrashReport,
    Stats,
    Advancements,
    LevelDat,
    ServersDat,
    Screenshot,
    ModJar,
    Unknown,
}

#[allow(dead_code)]
pub fn classify_source_path(path: &Path) -> SourceFileKind {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if file_name.ends_with(".log.gz") {
        return SourceFileKind::CompressedLog;
    }
    if extension == "log" {
        return SourceFileKind::Log;
    }
    if file_name == "level.dat" {
        return SourceFileKind::LevelDat;
    }
    if file_name == "servers.dat" {
        return SourceFileKind::ServersDat;
    }
    if extension == "jar" {
        return SourceFileKind::ModJar;
    }

    let has_component = |expected: &str| {
        path.components().any(|component| match component {
            Component::Normal(value) => value.to_string_lossy().eq_ignore_ascii_case(expected),
            _ => false,
        })
    };

    if extension == "json" && has_component("stats") {
        return SourceFileKind::Stats;
    }
    if extension == "json" && has_component("advancements") {
        return SourceFileKind::Advancements;
    }
    if extension == "txt" && has_component("crash-reports") {
        return SourceFileKind::CrashReport;
    }
    if matches!(extension.as_str(), "png" | "jpg" | "jpeg" | "webp") && has_component("screenshots")
    {
        return SourceFileKind::Screenshot;
    }

    SourceFileKind::Unknown
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{SourceFileKind, classify_source_path};

    #[test]
    fn classifies_using_native_path_components() {
        let root = PathBuf::from("instance");
        assert_eq!(
            classify_source_path(&root.join("stats").join("player.json")),
            SourceFileKind::Stats
        );
        assert_eq!(
            classify_source_path(&root.join("advancements").join("player.json")),
            SourceFileKind::Advancements
        );
        assert_eq!(
            classify_source_path(&root.join("crash-reports").join("crash.txt")),
            SourceFileKind::CrashReport
        );
        assert_eq!(
            classify_source_path(&root.join("logs").join("2026-08-06-1.log.gz")),
            SourceFileKind::CompressedLog
        );
    }

    #[test]
    fn does_not_match_component_substrings() {
        let path = PathBuf::from("instance")
            .join("old-stats-backup")
            .join("player.json");
        assert_eq!(classify_source_path(&path), SourceFileKind::Unknown);
    }

    #[test]
    fn component_matching_is_ascii_case_insensitive_on_every_platform() {
        let path = PathBuf::from("INSTANCE")
            .join("SCREENSHOTS")
            .join("capture.PNG");
        assert_eq!(classify_source_path(&path), SourceFileKind::Screenshot);
    }
}
