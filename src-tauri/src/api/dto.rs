use serde::Serialize;

use crate::domain::location::DiscoveredInstallation;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredLocationDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub path: String,
    pub instances: usize,
    pub confidence: crate::domain::Confidence,
    pub enabled: bool,
    pub platform: crate::domain::PlatformKind,
}

impl From<DiscoveredInstallation> for DiscoveredLocationDto {
    fn from(value: DiscoveredInstallation) -> Self {
        Self {
            id: value.id,
            name: value.name,
            kind: value.kind_label,
            path: value.path.to_string_lossy().into_owned(),
            instances: value.instances,
            confidence: value.confidence,
            enabled: value.enabled,
            platform: value.platform,
        }
    }
}

pub use crate::application::read_models::DashboardData;
