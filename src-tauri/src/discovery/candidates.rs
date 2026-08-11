use std::path::PathBuf;

use crate::{domain::PlatformKind, platform::PlatformPaths};

pub fn official(paths: &PlatformPaths) -> Vec<PathBuf> {
    match paths.platform {
        PlatformKind::Windows => paths
            .roaming_data
            .iter()
            .map(|base| base.join(".minecraft"))
            .collect(),
        PlatformKind::Macos => paths
            .home
            .iter()
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("minecraft")
            })
            .collect(),
        PlatformKind::Linux => paths
            .home
            .iter()
            .map(|home| home.join(".minecraft"))
            .collect(),
    }
}

pub fn prism(paths: &PlatformPaths) -> Vec<PathBuf> {
    match paths.platform {
        PlatformKind::Windows => paths
            .roaming_data
            .iter()
            .map(|base| base.join("PrismLauncher"))
            .collect(),
        PlatformKind::Macos => paths
            .home
            .iter()
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("PrismLauncher")
            })
            .collect(),
        PlatformKind::Linux => paths
            .home
            .iter()
            .map(|home| home.join(".local").join("share").join("PrismLauncher"))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{domain::PlatformKind, platform::PlatformPaths};

    #[test]
    fn windows_candidates_use_roaming_application_data() {
        let paths = PlatformPaths::test(
            PlatformKind::Windows,
            PathBuf::from("C:/Users/Alex"),
            PathBuf::from("C:/Users/Alex/AppData/Roaming"),
        );

        assert_eq!(
            super::official(&paths),
            vec![PathBuf::from("C:/Users/Alex/AppData/Roaming/.minecraft")]
        );
        assert_eq!(
            super::prism(&paths),
            vec![PathBuf::from("C:/Users/Alex/AppData/Roaming/PrismLauncher")]
        );
    }

    #[test]
    fn macos_candidates_use_application_support() {
        let paths = PlatformPaths::test(
            PlatformKind::Macos,
            PathBuf::from("/Users/alex"),
            PathBuf::from("/unused"),
        );

        assert_eq!(
            super::official(&paths),
            vec![PathBuf::from(
                "/Users/alex/Library/Application Support/minecraft"
            )]
        );
        assert_eq!(
            super::prism(&paths),
            vec![PathBuf::from(
                "/Users/alex/Library/Application Support/PrismLauncher"
            )]
        );
    }
}
