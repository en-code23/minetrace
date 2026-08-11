use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::{
    application::{DashboardService, DiscoveryService, ExplorerService},
    domain::location::{AdapterKind, DiscoveredInstallation},
    error::BackendError,
    storage::Database,
};

use super::read_models::WorldSummary;

const MAX_GAME_ROOTS: usize = 256;
const MAX_DIRECTORY_ENTRIES: usize = 10_000;
const MAX_WORLDS: usize = 512;
const MAX_BACKUPS: usize = 512;
const MAX_STATS_FILES: usize = 512;
const MAX_STATS_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ACCOUNT_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SKIN_BYTES: u64 = 512 * 1024;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy)]
pub struct ProfileService;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileData {
    pub generated_at: String,
    pub identity: Option<ProfileIdentity>,
    pub identities: Vec<ProfileIdentity>,
    pub current_skin: Option<SkinAsset>,
    pub previous_skins: Vec<SkinAsset>,
    pub summary: ProfileSummary,
    pub random_stats: Vec<ProfileStatistic>,
    pub statistic_sections: Vec<ProfileStatisticSection>,
    pub statistics_basis: StatisticsBasis,
    pub launchers: Vec<LauncherUsage>,
    pub worlds: Vec<ProfileWorld>,
    pub backups: Vec<WorldBackup>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileIdentity {
    pub name: String,
    pub uuid: Option<String>,
    pub source: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinAsset {
    pub id: String,
    pub data_url: String,
    pub observed_at: Option<String>,
    pub source: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub total_playtime_minutes: u64,
    pub sessions: u64,
    pub active_days: u64,
    pub most_played_version: Option<String>,
    pub most_played_world: Option<String>,
    pub launcher_count: u64,
    pub available_worlds: u64,
    pub missing_worlds: u64,
    pub backup_count: u64,
    pub statistics_worlds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileStatisticSection {
    pub id: String,
    pub label: String,
    pub items: Vec<ProfileStatistic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileStatistic {
    pub id: String,
    pub label: String,
    pub value: u64,
    pub unit: StatisticUnit,
    pub source_worlds: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatisticUnit {
    Count,
    Ticks,
    Centimeters,
    Tenths,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatisticsBasis {
    UuidMatched,
    SingleLocalPlayer,
    None,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherUsage {
    pub id: String,
    pub name: String,
    pub instances: u64,
    pub sessions: u64,
    pub total_minutes: u64,
    pub first_observed_at: Option<String>,
    pub last_observed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WorldAvailability {
    Available,
    Missing,
    BackupOnly,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileWorld {
    pub id: String,
    pub name: String,
    pub folder_name: Option<String>,
    pub instance: String,
    pub launcher: String,
    pub availability: WorldAvailability,
    pub total_minutes: Option<u64>,
    pub last_observed_at: Option<String>,
    pub stats_available: bool,
    pub stats_basis: StatisticsBasis,
    pub backup_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldBackup {
    pub id: String,
    pub name: String,
    pub instance: String,
    pub size_bytes: u64,
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone)]
struct AccountIdentity {
    name: String,
    uuid: Option<String>,
    source: String,
    active: bool,
    skin_hashes: Vec<String>,
}

#[derive(Debug, Clone)]
struct GameRoot {
    path: PathBuf,
    launcher: String,
    instance: String,
}

#[derive(Debug, Clone)]
struct BackupCandidate {
    path: PathBuf,
    name: String,
    instance: String,
}

#[derive(Debug, Clone, Default)]
struct StatValue {
    value: u64,
    worlds: BTreeSet<String>,
}

type StatMap = BTreeMap<(String, String), StatValue>;

impl ProfileService {
    pub fn load(
        self,
        database: &Database,
        discovery: &DiscoveryService,
    ) -> Result<ProfileData, BackendError> {
        let installations = discovery.discover()?;
        let game_roots = collect_game_roots(&installations);
        let account_identities = collect_identities(&installations, &game_roots);
        let identities = account_identities
            .iter()
            .map(|identity| ProfileIdentity {
                name: identity.name.clone(),
                uuid: identity.uuid.as_deref().map(display_uuid),
                source: identity.source.clone(),
                active: identity.active,
            })
            .collect::<Vec<_>>();
        let identity = identities.first().cloned();
        let primary_uuid = account_identities
            .first()
            .and_then(|candidate| candidate.uuid.as_deref());

        let (current_skin, previous_skins) =
            collect_skins(&installations, &game_roots, &account_identities);
        let dashboard = DashboardService.load(database)?;
        let archive_worlds = ExplorerService.world_collection(database)?.items;
        let launchers = launcher_usage(database)?;
        let backups = collect_backups(&game_roots);
        let (worlds, stats, statistics_basis, statistics_worlds) =
            collect_worlds(&game_roots, &archive_worlds, &backups, primary_uuid);
        let statistic_sections = build_statistic_sections(&stats);
        let random_stats = choose_random_stats(
            statistic_sections
                .iter()
                .flat_map(|section| section.items.iter())
                .cloned()
                .collect(),
            dashboard.totals.sessions,
        );

        let available_worlds = worlds
            .iter()
            .filter(|world| world.availability == WorldAvailability::Available)
            .count() as u64;
        let missing_worlds = worlds
            .iter()
            .filter(|world| world.availability == WorldAvailability::Missing)
            .count() as u64;
        let mut limitations = vec![
            "In-game statistics come from local single-player world files; multiplayer servers usually keep their statistics server-side.".to_owned(),
            "A missing save may have been deleted, moved, renamed, or excluded from the approved scan locations.".to_owned(),
            "Skin history includes only textures tied to a detected account and still present in a local launcher cache.".to_owned(),
        ];
        if identity.is_none() {
            limitations.push(
                "No local launcher account profile was found, so MineTrace is showing an anonymous local profile."
                    .to_owned(),
            );
        }
        if current_skin.is_none() {
            limitations.push(
                "The current account skin was not available in an attributable local cache."
                    .to_owned(),
            );
        }

        Ok(ProfileData {
            generated_at: Utc::now().to_rfc3339(),
            identity,
            identities,
            current_skin,
            previous_skins,
            summary: ProfileSummary {
                total_playtime_minutes: dashboard.totals.playtime_minutes,
                sessions: dashboard.totals.sessions,
                active_days: dashboard.totals.active_days,
                most_played_version: named_value(&dashboard.top.version.name),
                most_played_world: named_value(&dashboard.top.world.name),
                launcher_count: launchers.len() as u64,
                available_worlds,
                missing_worlds,
                backup_count: backups.len() as u64,
                statistics_worlds,
            },
            random_stats,
            statistic_sections,
            statistics_basis,
            launchers,
            worlds,
            backups: backups.into_iter().map(WorldBackup::from).collect(),
            limitations,
        })
    }
}

impl From<BackupCandidate> for WorldBackup {
    fn from(value: BackupCandidate) -> Self {
        let metadata = fs::metadata(&value.path).ok();
        let size_bytes = metadata.as_ref().map_or(0, fs::Metadata::len);
        let modified_at = metadata
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_rfc3339);
        Self {
            id: stable_id("backup", &[&value.instance, &value.name]),
            name: value.name,
            instance: value.instance,
            size_bytes,
            modified_at,
        }
    }
}

fn named_value(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value != "Not observed").then(|| value.to_owned())
}

fn launcher_usage(database: &Database) -> Result<Vec<LauncherUsage>, BackendError> {
    database.read(launcher_usage_rows)
}

fn launcher_usage_rows(connection: &Connection) -> Result<Vec<LauncherUsage>, rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT
            launcher.id,
            launcher.display_name,
            COUNT(DISTINCT instance.id),
            COUNT(session.id),
            COALESCE(SUM(CASE
                WHEN session.duration_seconds BETWEEN 0 AND 2678400
                THEN session.duration_seconds ELSE 0 END), 0),
            MIN(session.started_at_utc_ms),
            MAX(session.started_at_utc_ms)
         FROM launcher_installations launcher
         JOIN instances instance ON instance.installation_id = launcher.id
         LEFT JOIN sessions session ON session.instance_id = instance.id
         LEFT JOIN session_user_state user_state ON user_state.session_id = session.id
         WHERE session.id IS NULL OR COALESCE(user_state.ignored, 0) = 0
         GROUP BY launcher.id, launcher.display_name
         ORDER BY 5 DESC, 4 DESC, launcher.display_name COLLATE NOCASE
         LIMIT 64",
    )?;
    statement
        .query_map([], |row| {
            Ok(LauncherUsage {
                id: row.get(0)?,
                name: row.get(1)?,
                instances: nonnegative(row.get(2)?),
                sessions: nonnegative(row.get(3)?),
                total_minutes: nonnegative(row.get(4)?) / 60,
                first_observed_at: row.get::<_, Option<i64>>(5)?.and_then(timestamp_rfc3339),
                last_observed_at: row.get::<_, Option<i64>>(6)?.and_then(timestamp_rfc3339),
            })
        })?
        .collect()
}

fn collect_game_roots(installations: &[DiscoveredInstallation]) -> Vec<GameRoot> {
    let mut roots = BTreeMap::<PathBuf, GameRoot>::new();
    for installation in installations.iter().filter(|item| item.enabled) {
        match installation.adapter_kind {
            AdapterKind::Official => insert_game_root(
                &mut roots,
                installation.path.clone(),
                installation.kind_label.clone(),
                "Official Minecraft".to_owned(),
            ),
            AdapterKind::Prism | AdapterKind::MultiMc => {
                collect_instance_roots(installation, &mut roots)
            }
            AdapterKind::Manual => {
                if is_game_root(&installation.path) {
                    insert_game_root(
                        &mut roots,
                        installation.path.clone(),
                        installation.kind_label.clone(),
                        folder_name(&installation.path),
                    );
                }
                if is_game_root(&installation.path.join(".minecraft")) {
                    insert_game_root(
                        &mut roots,
                        installation.path.join(".minecraft"),
                        installation.kind_label.clone(),
                        folder_name(&installation.path),
                    );
                }
                collect_instance_roots(installation, &mut roots);
            }
        }
        if roots.len() >= MAX_GAME_ROOTS {
            break;
        }
    }
    roots.into_values().take(MAX_GAME_ROOTS).collect()
}

fn collect_instance_roots(
    installation: &DiscoveredInstallation,
    roots: &mut BTreeMap<PathBuf, GameRoot>,
) {
    let instances = installation.path.join("instances");
    let Ok(entries) = fs::read_dir(&instances) else {
        return;
    };
    for entry in entries.take(MAX_DIRECTORY_ENTRIES).flatten() {
        if roots.len() >= MAX_GAME_ROOTS {
            break;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let instance_root = entry.path();
        let game_path = [instance_root.join(".minecraft"), instance_root.clone()]
            .into_iter()
            .find(|path| is_game_root(path));
        let Some(game_path) = game_path else { continue };
        let instance_name = read_instance_name(&instance_root)
            .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
        insert_game_root(
            roots,
            game_path,
            installation.kind_label.clone(),
            instance_name,
        );
    }
}

fn insert_game_root(
    roots: &mut BTreeMap<PathBuf, GameRoot>,
    path: PathBuf,
    launcher: String,
    instance: String,
) {
    roots.entry(path.clone()).or_insert(GameRoot {
        path,
        launcher,
        instance,
    });
}

fn is_game_root(path: &Path) -> bool {
    path.join("saves").is_dir() || path.join("logs").is_dir() || path.join("options.txt").is_file()
}

fn read_instance_name(instance_root: &Path) -> Option<String> {
    let text = read_text_bounded(&instance_root.join("instance.cfg"), 256 * 1024)?;
    text.lines().find_map(|line| {
        line.strip_prefix("name=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn collect_identities(
    installations: &[DiscoveredInstallation],
    game_roots: &[GameRoot],
) -> Vec<AccountIdentity> {
    let mut files = BTreeMap::<PathBuf, String>::new();
    for installation in installations.iter().filter(|item| item.enabled) {
        let source = installation.kind_label.clone();
        files.insert(
            installation.path.join("launcher_accounts.json"),
            source.clone(),
        );
        files.insert(installation.path.join("accounts.json"), source);
    }
    for root in game_roots {
        files.insert(
            root.path.join("launcher_accounts.json"),
            root.launcher.clone(),
        );
        files.insert(root.path.join("accounts.json"), root.launcher.clone());
    }

    let mut identities = Vec::new();
    for (path, source) in files {
        let Some(text) = read_text_bounded(&path, MAX_ACCOUNT_FILE_BYTES) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        identities.extend(extract_identities(&value, &source));
    }

    let mut merged = BTreeMap::<String, AccountIdentity>::new();
    for identity in identities {
        let key = identity
            .uuid
            .clone()
            .unwrap_or_else(|| identity.name.to_lowercase());
        merged
            .entry(key)
            .and_modify(|current| {
                current.active |= identity.active;
                for hash in &identity.skin_hashes {
                    if !current.skin_hashes.contains(hash) {
                        current.skin_hashes.push(hash.clone());
                    }
                }
            })
            .or_insert(identity);
    }
    let mut identities = merged.into_values().collect::<Vec<_>>();
    identities.sort_by(|left, right| {
        right
            .active
            .cmp(&left.active)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    identities
}

fn extract_identities(value: &Value, source: &str) -> Vec<AccountIdentity> {
    let mut identities = Vec::new();
    let active_local_id = value
        .get("activeAccountLocalId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    if let Some(accounts) = value.get("accounts").and_then(Value::as_object) {
        for (local_id, account) in accounts {
            if let Some(profile) = account
                .get("minecraftProfile")
                .or_else(|| account.get("profile"))
                && let Some(identity) = identity_from_profile(
                    profile,
                    source,
                    active_local_id.as_deref() == Some(local_id.as_str()),
                )
            {
                identities.push(identity);
            }
        }
    }
    if let Some(accounts) = value.get("accounts").and_then(Value::as_array) {
        for account in accounts {
            let active = account
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if let Some(profile) = account
                .get("profile")
                .or_else(|| account.get("minecraftProfile"))
                && let Some(identity) = identity_from_profile(profile, source, active)
            {
                identities.push(identity);
            }
        }
    }
    identities
}

fn identity_from_profile(profile: &Value, source: &str, active: bool) -> Option<AccountIdentity> {
    let name = profile.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let uuid = profile
        .get("id")
        .and_then(Value::as_str)
        .and_then(normalize_uuid);
    let mut skin_hashes = Vec::new();
    if let Some(skins) = profile.get("skins").and_then(Value::as_array) {
        for skin in skins {
            if let Some(url) = skin.get("url").and_then(Value::as_str)
                && let Some(hash) = texture_hash(url)
                && !skin_hashes.contains(&hash)
            {
                skin_hashes.push(hash);
            }
        }
    }
    Some(AccountIdentity {
        name: name.to_owned(),
        uuid,
        source: source.to_owned(),
        active,
        skin_hashes,
    })
}

fn collect_skins(
    installations: &[DiscoveredInstallation],
    game_roots: &[GameRoot],
    identities: &[AccountIdentity],
) -> (Option<SkinAsset>, Vec<SkinAsset>) {
    let mut roots = BTreeSet::<PathBuf>::new();
    for installation in installations.iter().filter(|item| item.enabled) {
        roots.insert(installation.path.clone());
    }
    for root in game_roots {
        roots.insert(root.path.clone());
    }

    let mut skins = Vec::new();
    let mut seen = BTreeSet::new();
    for identity in identities {
        for hash in &identity.skin_hashes {
            if !seen.insert(hash.clone()) {
                continue;
            }
            for root in &roots {
                if let Some(asset) = find_skin(root, hash, &identity.source) {
                    skins.push(asset);
                    break;
                }
            }
        }
    }
    let current = skins.first().cloned();
    let previous = skins.into_iter().skip(1).take(12).collect();
    (current, previous)
}

fn find_skin(root: &Path, hash: &str, source: &str) -> Option<SkinAsset> {
    let prefix = hash.get(..2)?;
    let candidates = [
        root.join("assets").join("skins").join(prefix).join(hash),
        root.join("assets")
            .join("skins")
            .join(prefix)
            .join(format!("{hash}.png")),
        root.join("assets").join("skins").join(hash),
        root.join("cache").join("skins").join(hash),
    ];
    for path in candidates {
        let Some(bytes) = read_bytes_bounded(&path, MAX_SKIN_BYTES) else {
            continue;
        };
        let Some((width, height)) = png_dimensions(&bytes) else {
            continue;
        };
        if width != 64 || !matches!(height, 32 | 64) {
            continue;
        }
        let observed_at = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(system_time_rfc3339);
        return Some(SkinAsset {
            id: stable_id("skin", &[hash]),
            data_url: format!("data:image/png;base64,{}", base64_encode(&bytes)),
            observed_at,
            source: source.to_owned(),
            width,
            height,
        });
    }
    None
}

fn collect_backups(game_roots: &[GameRoot]) -> Vec<BackupCandidate> {
    let mut backups = Vec::new();
    let mut seen = BTreeSet::new();
    for root in game_roots {
        let backup_root = root.path.join("backups");
        let Ok(entries) = fs::read_dir(&backup_root) else {
            continue;
        };
        for entry in entries.take(MAX_DIRECTORY_ENTRIES).flatten() {
            if backups.len() >= MAX_BACKUPS {
                return backups;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("zip")
                || !seen.insert(path.clone())
            {
                continue;
            }
            backups.push(BackupCandidate {
                name: path
                    .file_stem()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "World backup".to_owned()),
                path,
                instance: root.instance.clone(),
            });
        }
    }
    backups.sort_by(|left, right| right.name.cmp(&left.name));
    backups
}

fn collect_worlds(
    game_roots: &[GameRoot],
    archive_worlds: &[WorldSummary],
    backups: &[BackupCandidate],
    primary_uuid: Option<&str>,
) -> (Vec<ProfileWorld>, StatMap, StatisticsBasis, u64) {
    let mut worlds = Vec::new();
    let mut stats = StatMap::new();
    let mut matched_archive_ids = BTreeSet::new();
    let mut matched_backups = BTreeSet::new();
    let mut stats_files = 0usize;
    let mut aggregate_basis = StatisticsBasis::None;
    let mut statistics_worlds = 0u64;

    'roots: for root in game_roots {
        let save_root = root.path.join("saves");
        let Ok(entries) = fs::read_dir(&save_root) else {
            continue;
        };
        for entry in entries.take(MAX_DIRECTORY_ENTRIES).flatten() {
            if worlds.len() >= MAX_WORLDS {
                break 'roots;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let folder_name = entry.file_name().to_string_lossy().into_owned();
            let normalized = normalize_name(&folder_name);
            let archive = archive_worlds.iter().find(|world| {
                !matched_archive_ids.contains(&world.id)
                    && normalize_name(&world.name) == normalized
            });
            if let Some(archive) = archive {
                matched_archive_ids.insert(archive.id.clone());
            }
            let backup_count = backups
                .iter()
                .enumerate()
                .filter(|(_, backup)| {
                    backup.instance == root.instance
                        && normalize_name(&backup.name).contains(&normalized)
                })
                .map(|(index, _)| {
                    matched_backups.insert(index);
                    1u64
                })
                .sum();
            let (world_stats, stats_basis) = if stats_files < MAX_STATS_FILES {
                load_world_stats(&entry.path(), primary_uuid)
            } else {
                (None, StatisticsBasis::None)
            };
            if let Some(world_stats) = world_stats {
                stats_files += 1;
                statistics_worlds += 1;
                merge_stats(&mut stats, world_stats, &folder_name);
                if aggregate_basis == StatisticsBasis::None
                    || stats_basis == StatisticsBasis::SingleLocalPlayer
                {
                    aggregate_basis = stats_basis;
                }
            }
            worlds.push(ProfileWorld {
                id: stable_id("profile-world", &[&root.instance, &folder_name]),
                name: archive.map_or_else(|| folder_name.clone(), |world| world.name.clone()),
                folder_name: Some(folder_name),
                instance: root.instance.clone(),
                launcher: root.launcher.clone(),
                availability: WorldAvailability::Available,
                total_minutes: archive.map(|world| world.total_minutes),
                last_observed_at: archive
                    .and_then(|world| world.last_played_at.clone())
                    .or_else(|| modified_rfc3339(&entry.path())),
                stats_available: stats_basis != StatisticsBasis::None,
                stats_basis,
                backup_count,
            });
        }
    }

    for archive in archive_worlds {
        if worlds.len() >= MAX_WORLDS || matched_archive_ids.contains(&archive.id) {
            continue;
        }
        let normalized = normalize_name(&archive.name);
        let backup_count = backups
            .iter()
            .enumerate()
            .filter(|(_, backup)| normalize_name(&backup.name).contains(&normalized))
            .map(|(index, _)| {
                matched_backups.insert(index);
                1u64
            })
            .sum();
        worlds.push(ProfileWorld {
            id: stable_id("missing-world", &[&archive.instance, &archive.name]),
            name: archive.name.clone(),
            folder_name: None,
            instance: archive.instance.clone(),
            launcher: "Observed archive".to_owned(),
            availability: WorldAvailability::Missing,
            total_minutes: Some(archive.total_minutes),
            last_observed_at: archive.last_played_at.clone(),
            stats_available: false,
            stats_basis: StatisticsBasis::None,
            backup_count,
        });
    }

    for (index, backup) in backups.iter().enumerate() {
        if worlds.len() >= MAX_WORLDS || matched_backups.contains(&index) {
            continue;
        }
        worlds.push(ProfileWorld {
            id: stable_id("backup-world", &[&backup.instance, &backup.name]),
            name: backup.name.clone(),
            folder_name: None,
            instance: backup.instance.clone(),
            launcher: "Local backup".to_owned(),
            availability: WorldAvailability::BackupOnly,
            total_minutes: None,
            last_observed_at: modified_rfc3339(&backup.path),
            stats_available: false,
            stats_basis: StatisticsBasis::None,
            backup_count: 1,
        });
    }

    worlds.sort_by(|left, right| {
        availability_rank(left.availability)
            .cmp(&availability_rank(right.availability))
            .then_with(|| right.total_minutes.cmp(&left.total_minutes))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    (worlds, stats, aggregate_basis, statistics_worlds)
}

fn availability_rank(value: WorldAvailability) -> u8 {
    match value {
        WorldAvailability::Available => 0,
        WorldAvailability::Missing => 1,
        WorldAvailability::BackupOnly => 2,
    }
}

fn load_world_stats(
    world_path: &Path,
    primary_uuid: Option<&str>,
) -> (Option<StatMap>, StatisticsBasis) {
    let stats_dir = world_path.join("stats");
    let Ok(entries) = fs::read_dir(stats_dir) else {
        return (None, StatisticsBasis::None);
    };
    let mut candidates = entries
        .take(64)
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let file_type = entry.file_type().ok()?;
            (file_type.is_file()
                && path.extension().and_then(|value| value.to_str()) == Some("json"))
            .then_some(path)
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return (None, StatisticsBasis::None);
    }
    candidates.sort();
    let normalized_uuid = primary_uuid.and_then(normalize_uuid);
    let exact = normalized_uuid.as_deref().and_then(|uuid| {
        candidates.iter().find(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .and_then(normalize_uuid)
                .as_deref()
                == Some(uuid)
        })
    });
    let (selected, basis) = if let Some(exact) = exact {
        (exact, StatisticsBasis::UuidMatched)
    } else if candidates.len() == 1 {
        (&candidates[0], StatisticsBasis::SingleLocalPlayer)
    } else {
        return (None, StatisticsBasis::None);
    };
    let Some(text) = read_text_bounded(selected, MAX_STATS_FILE_BYTES) else {
        return (None, StatisticsBasis::None);
    };
    let Some(parsed) = parse_stats(&text) else {
        return (None, StatisticsBasis::None);
    };
    (Some(parsed), basis)
}

fn parse_stats(text: &str) -> Option<StatMap> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    let categories = value.get("stats")?.as_object()?;
    let mut stats = StatMap::new();
    for (category, entries) in categories {
        let Some(entries) = entries.as_object() else {
            continue;
        };
        for (key, value) in entries {
            let amount = value
                .as_u64()
                .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
                .unwrap_or(0)
                .min(MAX_SAFE_INTEGER);
            if amount == 0 {
                continue;
            }
            stats.insert(
                (category.clone(), key.clone()),
                StatValue {
                    value: amount,
                    worlds: BTreeSet::new(),
                },
            );
        }
    }
    Some(stats)
}

fn merge_stats(target: &mut StatMap, source: StatMap, world: &str) {
    for (key, value) in source {
        let current = target.entry(key).or_default();
        current.value = current
            .value
            .saturating_add(value.value)
            .min(MAX_SAFE_INTEGER);
        current.worlds.insert(world.to_owned());
    }
}

fn build_statistic_sections(stats: &StatMap) -> Vec<ProfileStatisticSection> {
    let general_keys = [
        ("minecraft:play_time", "Time played", StatisticUnit::Ticks),
        ("minecraft:deaths", "Deaths", StatisticUnit::Count),
        (
            "minecraft:damage_dealt",
            "Damage dealt",
            StatisticUnit::Tenths,
        ),
        (
            "minecraft:damage_taken",
            "Damage taken",
            StatisticUnit::Tenths,
        ),
        ("minecraft:mob_kills", "Mobs defeated", StatisticUnit::Count),
        (
            "minecraft:player_kills",
            "Players defeated",
            StatisticUnit::Count,
        ),
        ("minecraft:jump", "Jumps", StatisticUnit::Count),
        (
            "minecraft:sleep_in_bed",
            "Times slept",
            StatisticUnit::Count,
        ),
        (
            "minecraft:animals_bred",
            "Animals bred",
            StatisticUnit::Count,
        ),
        ("minecraft:fish_caught", "Fish caught", StatisticUnit::Count),
        ("minecraft:raid_win", "Raids won", StatisticUnit::Count),
        (
            "minecraft:interact_with_villager",
            "Villager interactions",
            StatisticUnit::Count,
        ),
        (
            "minecraft:open_chest",
            "Chests opened",
            StatisticUnit::Count,
        ),
    ];
    let movement_keys = [
        (
            "minecraft:walk_one_cm",
            "Walked",
            StatisticUnit::Centimeters,
        ),
        (
            "minecraft:sprint_one_cm",
            "Sprinted",
            StatisticUnit::Centimeters,
        ),
        (
            "minecraft:crouch_one_cm",
            "Crouched",
            StatisticUnit::Centimeters,
        ),
        ("minecraft:swim_one_cm", "Swam", StatisticUnit::Centimeters),
        ("minecraft:fly_one_cm", "Flown", StatisticUnit::Centimeters),
        (
            "minecraft:aviate_one_cm",
            "Elytra distance",
            StatisticUnit::Centimeters,
        ),
        (
            "minecraft:boat_one_cm",
            "By boat",
            StatisticUnit::Centimeters,
        ),
        (
            "minecraft:minecart_one_cm",
            "By minecart",
            StatisticUnit::Centimeters,
        ),
    ];
    let mut sections = Vec::new();
    let general = selected_custom_stats(stats, &general_keys);
    if !general.is_empty() {
        sections.push(ProfileStatisticSection {
            id: "general".to_owned(),
            label: "General".to_owned(),
            items: general,
        });
    }
    let movement = selected_custom_stats(stats, &movement_keys);
    if !movement.is_empty() {
        sections.push(ProfileStatisticSection {
            id: "movement".to_owned(),
            label: "Movement".to_owned(),
            items: movement,
        });
    }
    for (category, id, label) in [
        ("minecraft:mined", "mined", "Top blocks mined"),
        ("minecraft:crafted", "crafted", "Top items crafted"),
        ("minecraft:used", "used", "Top items used"),
        ("minecraft:killed", "mobs", "Top mobs defeated"),
    ] {
        let items = top_category(stats, category, 12);
        if !items.is_empty() {
            sections.push(ProfileStatisticSection {
                id: id.to_owned(),
                label: label.to_owned(),
                items,
            });
        }
    }
    sections
}

fn selected_custom_stats(
    stats: &StatMap,
    keys: &[(&str, &str, StatisticUnit)],
) -> Vec<ProfileStatistic> {
    keys.iter()
        .filter_map(|(key, label, unit)| {
            let value = stats.get(&("minecraft:custom".to_owned(), (*key).to_owned()))?;
            Some(ProfileStatistic {
                id: (*key).to_owned(),
                label: (*label).to_owned(),
                value: value.value,
                unit: *unit,
                source_worlds: value.worlds.len() as u64,
            })
        })
        .collect()
}

fn top_category(stats: &StatMap, category: &str, limit: usize) -> Vec<ProfileStatistic> {
    let mut items = stats
        .iter()
        .filter(|((candidate, _), _)| candidate == category)
        .map(|((_, key), value)| ProfileStatistic {
            id: format!("{category}:{key}"),
            label: humanize_identifier(key),
            value: value.value,
            unit: StatisticUnit::Count,
            source_worlds: value.worlds.len() as u64,
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        right
            .value
            .cmp(&left.value)
            .then_with(|| left.label.cmp(&right.label))
    });
    items.truncate(limit);
    items
}

fn choose_random_stats(mut stats: Vec<ProfileStatistic>, seed: u64) -> Vec<ProfileStatistic> {
    stats.sort_by_key(|stat| {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&seed.to_le_bytes());
        hasher.update(stat.id.as_bytes());
        *hasher.finalize().as_bytes()
    });
    stats.truncate(4);
    stats
}

fn humanize_identifier(value: &str) -> String {
    let value = value.split(':').next_back().unwrap_or(value);
    value
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn texture_hash(url: &str) -> Option<String> {
    let hash = url.trim_end_matches('/').split('/').next_back()?;
    (hash.len() >= 32 && hash.chars().all(|value| value.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
}

fn normalize_uuid(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .filter(|value| *value != '-')
        .collect::<String>()
        .to_ascii_lowercase();
    (normalized.len() == 32 && normalized.chars().all(|value| value.is_ascii_hexdigit()))
        .then_some(normalized)
}

fn display_uuid(value: &str) -> String {
    let Some(value) = normalize_uuid(value) else {
        return value.to_owned();
    };
    format!(
        "{}-{}-{}-{}-{}",
        &value[0..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..32]
    )
}

fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|value| value.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn stable_id(namespace: &str, values: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(namespace.as_bytes());
    for value in values {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    format!("{namespace}_{}", &hasher.finalize().to_hex()[..24])
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Minecraft".to_owned())
}

fn read_text_bounded(path: &Path, max_bytes: u64) -> Option<String> {
    let bytes = read_bytes_bounded(path, max_bytes)?;
    String::from_utf8(bytes).ok()
}

fn read_bytes_bounded(path: &Path, max_bytes: u64) -> Option<Vec<u8>> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > max_bytes {
        return None;
    }
    fs::read(path).ok()
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(third & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    output
}

fn modified_rfc3339(path: &Path) -> Option<String> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()
        .and_then(system_time_rfc3339)
}

fn system_time_rfc3339(value: std::time::SystemTime) -> Option<String> {
    let duration = value.duration_since(UNIX_EPOCH).ok()?;
    timestamp_rfc3339(i64::try_from(duration.as_millis()).ok()?)
}

fn timestamp_rfc3339(value: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(value).map(|value| value.to_rfc3339())
}

fn nonnegative(value: i64) -> u64 {
    u64::try_from(value.max(0)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{
        StatisticsBasis, base64_encode, display_uuid, humanize_identifier, load_world_stats,
        normalize_uuid, parse_stats, png_dimensions,
    };

    #[test]
    fn uuid_helpers_normalize_launcher_formats() {
        let compact = "123456781234123412341234567890ab";
        assert_eq!(normalize_uuid(compact).as_deref(), Some(compact));
        assert_eq!(
            display_uuid(compact),
            "12345678-1234-1234-1234-1234567890ab"
        );
    }

    #[test]
    fn statistics_are_bounded_and_grouped_by_category() {
        let parsed = parse_stats(
            r#"{"stats":{"minecraft:custom":{"minecraft:jump":42,"minecraft:walk_one_cm":1200},"minecraft:mined":{"minecraft:stone":9}}}"#,
        )
        .expect("stats");
        assert_eq!(
            parsed
                .get(&("minecraft:custom".to_owned(), "minecraft:jump".to_owned()))
                .expect("jump")
                .value,
            42
        );
        assert_eq!(humanize_identifier("minecraft:oak_log"), "Oak Log");
    }

    #[test]
    fn one_local_player_file_is_used_when_no_account_uuid_is_known() {
        let temp = tempdir().expect("tempdir");
        let stats_dir = temp.path().join("stats");
        fs::create_dir_all(&stats_dir).expect("stats dir");
        fs::write(
            stats_dir.join("12345678-1234-1234-1234-1234567890ab.json"),
            r#"{"stats":{"minecraft:custom":{"minecraft:jump":7}}}"#,
        )
        .expect("stats file");
        let (stats, basis) = load_world_stats(temp.path(), None);
        assert!(stats.is_some());
        assert_eq!(basis, StatisticsBasis::SingleLocalPlayer);
    }

    #[test]
    fn png_and_base64_helpers_accept_a_skin_header() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&64u32.to_be_bytes());
        png.extend_from_slice(&64u32.to_be_bytes());
        assert_eq!(png_dimensions(&png), Some((64, 64)));
        assert_eq!(base64_encode(b"skin"), "c2tpbg==");
    }
}
