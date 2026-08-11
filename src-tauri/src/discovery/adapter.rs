use std::path::PathBuf;

use crate::{
    domain::location::{AdapterKind, DirectoryKind, ValidationReport},
    platform::PlatformPaths,
};

use super::candidates;

pub trait LauncherAdapter: Send + Sync {
    fn kind(&self) -> AdapterKind;
    fn display_name(&self) -> &'static str;
    fn candidate_paths(&self, paths: &PlatformPaths) -> Vec<PathBuf>;
    fn accepts(&self, report: &ValidationReport) -> bool;
}

pub struct OfficialAdapter;

impl LauncherAdapter for OfficialAdapter {
    fn kind(&self) -> AdapterKind {
        AdapterKind::Official
    }

    fn display_name(&self) -> &'static str {
        "Official Launcher"
    }

    fn candidate_paths(&self, paths: &PlatformPaths) -> Vec<PathBuf> {
        candidates::official(paths)
    }

    fn accepts(&self, report: &ValidationReport) -> bool {
        report.kind == DirectoryKind::GameDirectory
    }
}

pub struct PrismAdapter;

impl LauncherAdapter for PrismAdapter {
    fn kind(&self) -> AdapterKind {
        AdapterKind::Prism
    }

    fn display_name(&self) -> &'static str {
        "Prism Launcher"
    }

    fn candidate_paths(&self, paths: &PlatformPaths) -> Vec<PathBuf> {
        candidates::prism(paths)
    }

    fn accepts(&self, report: &ValidationReport) -> bool {
        report.kind == DirectoryKind::LauncherRoot
    }
}
