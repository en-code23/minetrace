use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Confidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlatformKind {
    Windows,
    Macos,
    Linux,
}

impl PlatformKind {
    pub const fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(target_os = "macos")]
        {
            Self::Macos
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Self::Linux
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Macos => "macos",
            Self::Linux => "linux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AdapterKind {
    Official,
    Prism,
    MultiMc,
    Manual,
}

impl AdapterKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Official => "official",
            Self::Prism => "prism",
            Self::MultiMc => "multimc",
            Self::Manual => "manual",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Official => "Official",
            Self::Prism => "Prism",
            Self::MultiMc => "MultiMC",
            Self::Manual => "Custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryKind {
    GameDirectory,
    LauncherRoot,
    InstanceDirectory,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub kind: DirectoryKind,
    pub score: u8,
    pub instance_count: usize,
    pub markers: Vec<&'static str>,
}

impl ValidationReport {
    pub fn is_supported(&self) -> bool {
        self.kind != DirectoryKind::Unknown && self.markers.len() >= 2 && self.score >= 30
    }

    pub fn confidence(&self) -> Confidence {
        Confidence::from_score(self.score)
    }
}

#[derive(Debug, Clone)]
pub struct DiscoveredInstallation {
    pub id: String,
    pub name: String,
    pub kind_label: String,
    pub adapter_kind: AdapterKind,
    pub path: PathBuf,
    pub instances: usize,
    pub confidence: Confidence,
    pub validation_score: u8,
    pub enabled: bool,
    pub platform: PlatformKind,
    pub origin: &'static str,
}
