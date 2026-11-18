//! Auto-battle strategy profile storage and CRUD commands.
//!
//! Profiles are app-global so several projects (and both battle tasks) can
//! share one decision policy. Projects only store the referenced profile id;
//! the runner resolves and snapshots the strategy when a run starts.

use super::projects::ProjectStoreLock;
use super::*;
use crate::paths::app_data_dir;
use crate::storage::{read_json_or_default, write_json_atomic};

pub(crate) const AUTO_BATTLE_PROFILES_FILE: &str = "auto_battle_profiles.json";

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutoBattleProfilesFile {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub profiles: Vec<AutoBattleProfile>,
}

fn default_schema_version() -> u32 {
    AUTO_BATTLE_PROFILE_SCHEMA_VERSION
}

impl Default for AutoBattleProfilesFile {
    fn default() -> Self {
        Self {
            schema_version: AUTO_BATTLE_PROFILE_SCHEMA_VERSION,
            profiles: vec![builtin_basic_profile()],
        }
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn auto_battle_profiles_file_path(app: &tauri::AppHandle) -> PathBuf {
    app_data_dir(app).join(AUTO_BATTLE_PROFILES_FILE)
}

pub(crate) fn read_auto_battle_profiles_from_path(
    path: &Path,
) -> Result<AutoBattleProfilesFile, String> {
    let mut file: AutoBattleProfilesFile = read_json_or_default(path, "自动战斗方案")?;
    file.schema_version = AUTO_BATTLE_PROFILE_SCHEMA_VERSION;
    ensure_builtin_profile(&mut file.profiles);
    file.profiles = file
        .profiles
        .into_iter()
        .map(normalize_auto_battle_profile)
        .collect();
    Ok(file)
}

pub(crate) fn write_auto_battle_profiles_to_path(
    path: &Path,
    file: &AutoBattleProfilesFile,
) -> Result<(), String> {
    let mut file = file.clone();
    file.schema_version = AUTO_BATTLE_PROFILE_SCHEMA_VERSION;
    ensure_builtin_profile(&mut file.profiles);
    write_json_atomic(path, &file, "自动战斗方案")
}

/// List order: builtin first, then most recently edited.
pub(crate) fn sorted_profiles(file: &AutoBattleProfilesFile) -> Vec<AutoBattleProfile> {
    let mut profiles = file.profiles.clone();
    profiles.sort_by(|left, right| {
        right
            .builtin
            .cmp(&left.builtin)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| left.name.cmp(&right.name))
    });
    profiles
}

fn find_profile_index(file: &AutoBattleProfilesFile, id: &str) -> Option<usize> {
    file.profiles.iter().position(|profile| profile.id == id)
}

/// Insert or update one profile. The builtin profile keeps its identity and
/// display text; only its strategy is editable, and "reset" restores it.
pub(crate) fn upsert_profile(
    file: &mut AutoBattleProfilesFile,
    profile: AutoBattleProfile,
) -> Result<AutoBattleProfile, String> {
    let mut next = normalize_auto_battle_profile(profile);
    next.updated_at = now_millis();

    if let Some(index) = find_profile_index(file, &next.id) {
        let existing = &file.profiles[index];
        if existing.builtin || is_builtin_profile_id(&existing.id) {
            next.name = existing.name.clone();
            next.description = existing.description.clone();
            next.builtin = true;
        } else {
            next.builtin = false;
        }
        validate_auto_battle_profile(&next)?;
        file.profiles[index] = next.clone();
        return Ok(next);
    }

    next.builtin = false;
    if next.id.trim().is_empty() {
        next.id = uuid::Uuid::new_v4().to_string();
    }
    validate_auto_battle_profile(&next)?;
    file.profiles.push(next.clone());
    Ok(next)
}

pub(crate) fn create_profile(
    file: &mut AutoBattleProfilesFile,
    name: String,
) -> Result<AutoBattleProfile, String> {
    let profile = AutoBattleProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        description: String::new(),
        builtin: false,
        strategy: AutoBattleStrategy::default(),
        updated_at: now_millis(),
    };
    upsert_profile(file, profile)
}

pub(crate) fn duplicate_profile(
    file: &mut AutoBattleProfilesFile,
    source_id: &str,
    name: String,
) -> Result<AutoBattleProfile, String> {
    let source = file
        .profiles
        .iter()
        .find(|profile| profile.id == source_id)
        .cloned()
        .ok_or_else(|| format!("自动战斗方案不存在：{source_id}"))?;
    let profile = AutoBattleProfile {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        description: source.description,
        builtin: false,
        strategy: source.strategy,
        updated_at: now_millis(),
    };
    upsert_profile(file, profile)
}

pub(crate) fn delete_profile(file: &mut AutoBattleProfilesFile, id: &str) -> Result<(), String> {
    if is_builtin_profile_id(id) {
        return Err("基础方案不能删除".into());
    }
    let Some(index) = find_profile_index(file, id) else {
        return Err(format!("自动战斗方案不存在：{id}"));
    };
    if file.profiles[index].builtin {
        return Err("内置方案不能删除".into());
    }
    file.profiles.remove(index);
    Ok(())
}

pub(crate) fn reset_profile(
    file: &mut AutoBattleProfilesFile,
    id: &str,
) -> Result<AutoBattleProfile, String> {
    if !is_builtin_profile_id(id) {
        return Err("只有基础方案支持恢复默认".into());
    }
    let builtin = builtin_basic_profile();
    match find_profile_index(file, id) {
        Some(index) => {
            file.profiles[index] = builtin.clone();
        }
        None => file.profiles.insert(0, builtin.clone()),
    }
    Ok(builtin)
}

/// Pick the strategy a run should use. Referenced profiles win, a missing id
/// falls back to the builtin profile, and an invalid profile falls back to the
/// builtin strategy instead of failing the run.
pub(crate) fn resolve_auto_battle_strategy_from_file(
    file: &AutoBattleProfilesFile,
    enabled: bool,
    profile_id: Option<&str>,
    advanced_mode: bool,
) -> Option<AutoBattleStrategy> {
    if !enabled || advanced_mode {
        return None;
    }
    let requested = profile_id.and_then(|id| file.profiles.iter().find(|profile| profile.id == id));
    let builtin = file
        .profiles
        .iter()
        .find(|profile| profile.id == BUILTIN_BASIC_PROFILE_ID);
    requested
        .filter(|profile| validate_auto_battle_profile(profile).is_ok())
        .or_else(|| builtin.filter(|profile| validate_auto_battle_profile(profile).is_ok()))
        .map(|profile| profile.strategy.clone())
}

pub(crate) fn resolve_auto_battle_strategy(
    app: &tauri::AppHandle,
    enabled: bool,
    profile_id: Option<&str>,
    advanced_mode: bool,
) -> Result<Option<AutoBattleStrategy>, String> {
    let file = read_auto_battle_profiles_from_path(&auto_battle_profiles_file_path(app))?;
    Ok(resolve_auto_battle_strategy_from_file(
        &file,
        enabled,
        profile_id,
        advanced_mode,
    ))
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub(crate) fn list_auto_battle_profiles(
    app: tauri::AppHandle,
    lock: tauri::State<'_, ProjectStoreLock>,
) -> Result<Vec<AutoBattleProfile>, String> {
    let _guard = lock.inner().lock()?;
    let file = read_auto_battle_profiles_from_path(&auto_battle_profiles_file_path(&app))?;
    Ok(sorted_profiles(&file))
}

#[tauri::command]
pub(crate) fn create_auto_battle_profile(
    app: tauri::AppHandle,
    name: String,
    lock: tauri::State<'_, ProjectStoreLock>,
) -> Result<AutoBattleProfile, String> {
    let _guard = lock.inner().lock()?;
    let path = auto_battle_profiles_file_path(&app);
    let mut file = read_auto_battle_profiles_from_path(&path)?;
    let profile = create_profile(&mut file, name)?;
    write_auto_battle_profiles_to_path(&path, &file)?;
    Ok(profile)
}

#[tauri::command]
pub(crate) fn duplicate_auto_battle_profile(
    app: tauri::AppHandle,
    source_id: String,
    name: String,
    lock: tauri::State<'_, ProjectStoreLock>,
) -> Result<AutoBattleProfile, String> {
    let _guard = lock.inner().lock()?;
    let path = auto_battle_profiles_file_path(&app);
    let mut file = read_auto_battle_profiles_from_path(&path)?;
    let profile = duplicate_profile(&mut file, &source_id, name)?;
    write_auto_battle_profiles_to_path(&path, &file)?;
    Ok(profile)
}

#[tauri::command]
pub(crate) fn save_auto_battle_profile(
    app: tauri::AppHandle,
    profile: AutoBattleProfile,
    lock: tauri::State<'_, ProjectStoreLock>,
) -> Result<AutoBattleProfile, String> {
    let _guard = lock.inner().lock()?;
    let path = auto_battle_profiles_file_path(&app);
    let mut file = read_auto_battle_profiles_from_path(&path)?;
    let saved = upsert_profile(&mut file, profile)?;
    write_auto_battle_profiles_to_path(&path, &file)?;
    Ok(saved)
}

#[tauri::command]
pub(crate) fn delete_auto_battle_profile(
    app: tauri::AppHandle,
    id: String,
    lock: tauri::State<'_, ProjectStoreLock>,
) -> Result<(), String> {
    let _guard = lock.inner().lock()?;
    let path = auto_battle_profiles_file_path(&app);
    let mut file = read_auto_battle_profiles_from_path(&path)?;
    delete_profile(&mut file, &id)?;
    write_auto_battle_profiles_to_path(&path, &file)
}

#[tauri::command]
pub(crate) fn reset_auto_battle_profile(
    app: tauri::AppHandle,
    id: String,
    lock: tauri::State<'_, ProjectStoreLock>,
) -> Result<AutoBattleProfile, String> {
    let _guard = lock.inner().lock()?;
    let path = auto_battle_profiles_file_path(&app);
    let mut file = read_auto_battle_profiles_from_path(&path)?;
    let profile = reset_profile(&mut file, &id)?;
    write_auto_battle_profiles_to_path(&path, &file)?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_file() -> AutoBattleProfilesFile {
        let mut file = AutoBattleProfilesFile::default();
        let profile = AutoBattleProfile {
            id: "user-1".into(),
            name: "我的方案".into(),
            description: "说明".into(),
            builtin: false,
            strategy: AutoBattleStrategy::default(),
            updated_at: 0,
        };
        file.profiles.push(profile);
        file
    }

    #[test]
    fn missing_file_starts_with_the_builtin_profile() {
        let dir = tempfile::tempdir().unwrap();
        let file =
            read_auto_battle_profiles_from_path(&dir.path().join("auto_battle_profiles.json"))
                .unwrap();
        assert_eq!(file.schema_version, AUTO_BATTLE_PROFILE_SCHEMA_VERSION);
        assert_eq!(file.profiles.len(), 1);
        assert_eq!(file.profiles[0].id, BUILTIN_BASIC_PROFILE_ID);
    }

    #[test]
    fn round_trip_preserves_profiles_and_seeds_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auto_battle_profiles.json");
        let file = fixture_file();
        write_auto_battle_profiles_to_path(&path, &file).unwrap();

        let loaded = read_auto_battle_profiles_from_path(&path).unwrap();
        assert!(loaded.profiles.iter().any(|profile| profile.id == "user-1"));
        assert!(loaded
            .profiles
            .iter()
            .any(|profile| profile.id == BUILTIN_BASIC_PROFILE_ID && profile.builtin));
    }

    #[test]
    fn builtin_keeps_identity_and_text_while_strategy_edits_apply() {
        let mut file = AutoBattleProfilesFile::default();
        let mut edited = file.profiles[0].clone();
        edited.name = "改名".into();
        edited.description = "新说明".into();
        edited.strategy.noble_phantasm.mode = NoblePhantasmUseMode::Single;

        let saved = upsert_profile(&mut file, edited).unwrap();
        assert_eq!(saved.name, "基础方案");
        assert_eq!(saved.description, file.profiles[0].description);
        assert!(saved.builtin);
        assert_eq!(
            saved.strategy.noble_phantasm.mode,
            NoblePhantasmUseMode::Single
        );
    }

    #[test]
    fn create_and_duplicate_assign_new_ids_and_keep_builtin_untouched() {
        let mut file = AutoBattleProfilesFile::default();
        let created = create_profile(&mut file, "新方案".into()).unwrap();
        assert_ne!(created.id, BUILTIN_BASIC_PROFILE_ID);
        assert!(!created.builtin);

        let duplicated =
            duplicate_profile(&mut file, BUILTIN_BASIC_PROFILE_ID, "基础方案副本".into()).unwrap();
        assert_ne!(duplicated.id, BUILTIN_BASIC_PROFILE_ID);
        assert!(!duplicated.builtin);
        assert_eq!(duplicated.strategy, builtin_basic_profile().strategy);
        assert_eq!(file.profiles.len(), 3);
    }

    #[test]
    fn builtin_cannot_be_deleted_but_can_be_reset() {
        let mut file = AutoBattleProfilesFile::default();
        assert!(delete_profile(&mut file, BUILTIN_BASIC_PROFILE_ID).is_err());

        file.profiles[0].strategy.skill.mode = SkillUseMode::Off;
        let reset = reset_profile(&mut file, BUILTIN_BASIC_PROFILE_ID).unwrap();
        assert_eq!(reset.strategy.skill.mode, SkillUseMode::ReadyOnly);
        assert_eq!(
            file.profiles[0].strategy.skill.mode,
            SkillUseMode::ReadyOnly
        );
    }

    #[test]
    fn invalid_payloads_are_rejected_before_writing() {
        let mut file = AutoBattleProfilesFile::default();
        let mut profile = AutoBattleProfile {
            id: "user-2".into(),
            name: "坏方案".into(),
            description: String::new(),
            builtin: false,
            strategy: AutoBattleStrategy::default(),
            updated_at: 0,
        };
        profile.strategy.noble_phantasm.max_count = 9;
        assert!(upsert_profile(&mut file, profile).is_err());
        assert_eq!(file.profiles.len(), 1);
    }

    #[test]
    fn run_resolution_honors_reference_defaults_and_advanced_mode() {
        let mut file = fixture_file();
        file.profiles.push(AutoBattleProfile {
            id: "user-2".into(),
            name: "关闭技能".into(),
            description: String::new(),
            builtin: false,
            strategy: AutoBattleStrategy {
                skill: SkillStrategy {
                    mode: SkillUseMode::Off,
                    selected: Vec::new(),
                    include_mystic_code: false,
                    order: SkillOrder::ServantMajor,
                },
                ..AutoBattleStrategy::default()
            },
            updated_at: 0,
        });

        let selected = resolve_auto_battle_strategy_from_file(&file, true, Some("user-2"), false)
            .expect("selected profile");
        assert_eq!(selected.skill.mode, SkillUseMode::Off);

        // Unknown reference falls back to the builtin basic profile.
        let fallback = resolve_auto_battle_strategy_from_file(&file, true, Some("missing"), false)
            .expect("fallback profile");
        assert_eq!(fallback.skill.mode, SkillUseMode::ReadyOnly);

        // No reference means builtin.
        let default = resolve_auto_battle_strategy_from_file(&file, true, None, false)
            .expect("default profile");
        assert_eq!(default.skill.mode, SkillUseMode::ReadyOnly);

        assert!(resolve_auto_battle_strategy_from_file(&file, false, None, false).is_none());
        assert!(resolve_auto_battle_strategy_from_file(&file, true, None, true).is_none());
    }

    #[test]
    fn sorted_profiles_put_builtin_first_then_recent_edits() {
        let mut file = AutoBattleProfilesFile::default();
        create_profile(&mut file, "旧方案".into()).unwrap();
        file.profiles[1].updated_at = 10;
        create_profile(&mut file, "新方案".into()).unwrap();
        file.profiles[2].updated_at = 20;

        let sorted = sorted_profiles(&file);
        assert!(sorted[0].builtin);
        assert_eq!(sorted[1].name, "新方案");
        assert_eq!(sorted[2].name, "旧方案");
    }
}
