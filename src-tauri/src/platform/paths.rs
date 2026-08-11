use std::{env, path::PathBuf};

use tauri::{AppHandle, Manager, Runtime};

use crate::domain::PlatformKind;

#[derive(Debug, Clone)]
pub struct PlatformPaths {
    pub platform: PlatformKind,
    pub home: Option<PathBuf>,
    pub roaming_data: Option<PathBuf>,
}

impl PlatformPaths {
    pub fn from_app<R: Runtime>(app: &AppHandle<R>) -> Self {
        let user_profile = env::var_os("USERPROFILE").map(PathBuf::from);
        let home = app
            .path()
            .home_dir()
            .ok()
            .or_else(|| env::var_os("HOME").map(PathBuf::from))
            .or_else(|| user_profile.clone());

        Self {
            platform: PlatformKind::current(),
            home,
            roaming_data: windows_roaming_data(
                env::var_os("APPDATA").map(PathBuf::from),
                user_profile,
            ),
        }
    }

    #[cfg(test)]
    pub fn test(platform: PlatformKind, home: PathBuf, roaming_data: PathBuf) -> Self {
        Self {
            platform,
            home: Some(home),
            roaming_data: Some(roaming_data.clone()),
        }
    }
}

fn windows_roaming_data(
    app_data: Option<PathBuf>,
    user_profile: Option<PathBuf>,
) -> Option<PathBuf> {
    app_data.or_else(|| user_profile.map(|profile| profile.join("AppData").join("Roaming")))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::windows_roaming_data;

    #[test]
    fn explicit_windows_appdata_wins_over_the_profile_fallback() {
        assert_eq!(
            windows_roaming_data(
                Some(PathBuf::from("D:/Roaming")),
                Some(PathBuf::from("C:/Users/Alex")),
            ),
            Some(PathBuf::from("D:/Roaming"))
        );
    }

    #[test]
    fn windows_profile_supplies_a_roaming_fallback() {
        assert_eq!(
            windows_roaming_data(None, Some(PathBuf::from("C:/Users/Alex"))),
            Some(PathBuf::from("C:/Users/Alex/AppData/Roaming"))
        );
    }
}
