//! Standalone interlude task configuration, page capture, and startup.

use crate::adb;
use crate::commands::automation::start_automation_with_project;
use crate::commands::debug::{ensure_debug_sidecar, DebugSidecar};
use crate::commands::settings::AdbDeviceSettings;
use crate::device_operation::DeviceOperationCoordinator;
use crate::models::Project;
use crate::paths::{app_data_dir, interlude_task_config_path};
use crate::runner::{
    InterludeQuestMode, InterludeStartRequest, InterludeTarget, InterludeTaskConfig,
    InterludeWorkflowConfig, RunConfig, RunnerHandle,
};
use crate::screen::{InterludeQuestRow, Screen};
use crate::server::Server;
use crate::storage::{read_json_or_default, write_json_atomic};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub(crate) struct InterludeCaptureSession {
    pub(crate) id: String,
    pub(crate) image_path: PathBuf,
    pub(crate) rows: Vec<InterludeQuestRow>,
}

#[derive(Default)]
pub(crate) struct InterludeCaptureState(pub Mutex<Option<InterludeCaptureSession>>);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InterludeCaptureResult {
    pub(crate) capture_id: String,
    pub(crate) image_path: String,
    pub(crate) rows: Vec<InterludeQuestRow>,
}

#[tauri::command]
pub(crate) fn get_interlude_task_config(
    app: tauri::AppHandle,
) -> Result<InterludeTaskConfig, String> {
    read_json_or_default(&interlude_task_config_path(&app), "幕间任务配置")
}

#[tauri::command]
pub(crate) fn save_interlude_task_config(
    app: tauri::AppHandle,
    task: InterludeTaskConfig,
) -> Result<(), String> {
    write_json_atomic(&interlude_task_config_path(&app), &task, "幕间任务配置")
}

#[tauri::command]
pub(crate) fn capture_interlude_page(
    app: tauri::AppHandle,
    adb_settings: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    debug_sidecar: tauri::State<'_, DebugSidecar>,
    capture_state: tauri::State<'_, InterludeCaptureState>,
) -> Result<InterludeCaptureResult, String> {
    coordinator.require_idle()?;
    let server = *server_state.lock().unwrap();
    if server != Server::Cn {
        return Err("幕间物语自动化首版仅支持国服".into());
    }

    let selected_serial = adb_settings.lock().unwrap().selected_adb_serial.clone();
    let mut adb_dev = adb::Adb::new(&app, selected_serial);
    adb_dev.connect_for_game()?;
    let temporary = adb_dev.screenshot_to_file()?;

    ensure_debug_sidecar(&app, &debug_sidecar, server)?;
    let mut sidecar_guard = debug_sidecar.0.lock().unwrap();
    let sidecar = sidecar_guard
        .as_mut()
        .ok_or_else(|| "CV sidecar 未初始化".to_string())?;
    sidecar.use_interlude_detection();
    let capture_result = (|| {
        let screen = sidecar.detect(Some(&temporary))?;
        if !matches!(screen, Screen::InterludeQuest | Screen::InterludeStage) {
            return Err("未识别到幕间物语页面，请先在游戏中打开幕间物语列表".into());
        }
        Ok(sidecar.find_interlude_quest_rows(Some(&temporary))?.rows)
    })();
    drop(sidecar_guard);
    let mut rows = match capture_result {
        Ok(rows) => rows,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
    };

    let capture_id = Uuid::new_v4().to_string();
    for (index, row) in rows.iter_mut().enumerate() {
        row.candidate_id = format!("{capture_id}:{index}");
    }
    let dir = app_data_dir(&app).join("debug").join("interlude-quest");
    fs::create_dir_all(&dir).map_err(|error| format!("创建幕间任务截图目录失败: {error}"))?;
    let image_path = dir.join(format!("capture-{capture_id}.png"));
    fs::copy(&temporary, &image_path).map_err(|error| format!("保存幕间任务截图失败: {error}"))?;
    let _ = fs::remove_file(&temporary);

    let mut state = capture_state.0.lock().unwrap();
    if let Some(previous) = state.replace(InterludeCaptureSession {
        id: capture_id.clone(),
        image_path: image_path.clone(),
        rows: rows.clone(),
    }) {
        let _ = fs::remove_file(previous.image_path);
    }

    Ok(InterludeCaptureResult {
        capture_id,
        image_path: image_path.to_string_lossy().into_owned(),
        rows,
    })
}

#[tauri::command]
pub(crate) fn start_interlude_automation(
    app: tauri::AppHandle,
    mut task: InterludeTaskConfig,
    workflow: InterludeStartRequest,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    coordinator: tauri::State<'_, DeviceOperationCoordinator>,
    debug_state: tauri::State<'_, DebugSidecar>,
    capture_state: tauri::State<'_, InterludeCaptureState>,
) -> Result<(), String> {
    let server = *app.state::<Mutex<Server>>().lock().unwrap();
    if server != Server::Cn {
        return Err("幕间物语自动化首版仅支持国服".into());
    }
    task.mode = workflow.mode;
    let target = resolve_interlude_target(&workflow, &capture_state)?;
    write_json_atomic(&interlude_task_config_path(&app), &task, "幕间任务配置")?;
    let config = interlude_run_config(&task)?;
    let workflow_config = InterludeWorkflowConfig {
        task: task.clone(),
        target,
    };
    let mut config = config;
    config.interlude_quest = Some(workflow_config);
    config.skip_team_verification = true;
    let project = interlude_project(&task)?;
    start_automation_with_project(
        &app,
        config,
        Some(project),
        &handle_state,
        &coordinator,
        &debug_state,
    )
}

fn resolve_interlude_target(
    request: &InterludeStartRequest,
    capture_state: &InterludeCaptureState,
) -> Result<Option<InterludeTarget>, String> {
    if request.mode == InterludeQuestMode::All {
        return Ok(None);
    }
    let capture_id = request
        .capture_id
        .clone()
        .ok_or_else(|| "请先截图并选择幕间任务".to_string())?;
    let candidate_id = request
        .candidate_id
        .clone()
        .ok_or_else(|| "请先选择一个可完成幕间任务".to_string())?;
    let state = capture_state.0.lock().unwrap();
    let session = state
        .as_ref()
        .filter(|session| session.id == capture_id)
        .ok_or_else(|| "幕间任务截图已失效，请重新截图".to_string())?;
    let row = session
        .rows
        .iter()
        .find(|row| row.candidate_id == candidate_id)
        .ok_or_else(|| "找不到所选幕间任务，请重新截图".to_string())?;
    Ok(Some(InterludeTarget {
        title_signature: row.title_signature.clone(),
    }))
}

fn interlude_project(task: &InterludeTaskConfig) -> Result<Project, String> {
    serde_json::from_value(serde_json::json!({
        "id": "interlude",
        "name": "幕间物语",
        "supportServantId": task.support.servant_id,
        "supportServantVariantKey": task.support.servant_variant_key,
        "supportServantLevelMin": task.support.servant_level_min,
        "supportNoblePhantasmLevelMin": task.support.noble_phantasm_level_min,
        "supportStarMapScoreMin": task.support.star_map_score_min,
        "supportSkillLevelMins": task.support.skill_level_mins,
        "supportAppendSkillLevelMins": task.support.append_skill_level_mins,
        "apRecoveryItems": task.ap_recovery_items,
        "apRecoveryLimits": task.ap_recovery_limits,
        "autoBattleTakeover": true,
        "autoBattleProfileId": task.auto_battle_profile_id,
    }))
    .map_err(|error| format!("构造幕间任务运行配置失败: {error}"))
}

fn interlude_run_config(task: &InterludeTaskConfig) -> Result<RunConfig, String> {
    serde_json::from_value(serde_json::json!({
        "projectId": "interlude",
        "supportServantId": task.support.servant_id,
        "supportServantVariantKey": task.support.servant_variant_key,
        "supportCraftEssenceIds": task.support.craft_essence_ids,
        "supportCraftEssenceMlbRequired": task.support.craft_essence_mlb_required,
        "supportServantLevelMin": task.support.servant_level_min,
        "supportNoblePhantasmLevelMin": task.support.noble_phantasm_level_min,
        "supportStarMapScoreMin": task.support.star_map_score_min,
        "supportSkillLevelMins": task.support.skill_level_mins,
        "supportAppendSkillLevelMins": task.support.append_skill_level_mins,
        "supportClassFilterMode": "correspondingClass",
        "servantSelections": [],
        "maxSupportScrolls": 3,
        "apRecoveryItems": task.ap_recovery_items,
        "apRecoveryLimits": task.ap_recovery_limits,
        "autoBattleTakeover": true,
        "autoBattleProfileId": task.auto_battle_profile_id,
    }))
    .map_err(|error| format!("构造幕间任务启动参数失败: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interlude_run_config_maps_support_and_ap_fields() {
        let mut task = InterludeTaskConfig::default();
        task.support.servant_id = Some(309);
        task.support.servant_level_min = Some(90);
        task.support.craft_essence_ids = vec![123];
        task.ap_recovery_items = vec![crate::runner::ApRecoveryItem::Gold];
        task.ap_recovery_limits.gold = Some(2);
        task.auto_battle_profile_id = Some("profile-a".into());

        let config = interlude_run_config(&task).unwrap();

        assert_eq!(config.project_id, "interlude");
        assert_eq!(config.support_servant_id, Some(309));
        assert_eq!(config.support_servant_level_min, Some(90));
        assert_eq!(config.support_craft_essence_ids, vec![123]);
        assert_eq!(
            config.ap_recovery_items,
            vec![crate::runner::ApRecoveryItem::Gold]
        );
        assert_eq!(config.ap_recovery_limits.gold, Some(2));
        assert_eq!(config.auto_battle_profile_id.as_deref(), Some("profile-a"));
        assert!(config.auto_battle_takeover);
    }

    #[test]
    fn interlude_project_has_no_team_members() {
        let task = InterludeTaskConfig::default();
        let project = interlude_project(&task).unwrap();

        assert_eq!(project.id, "interlude");
        assert_eq!(project.name, "幕间物语");
        assert!(project.slots.iter().all(|slot| slot.servant_id.is_none()));
        assert!(project.auto_battle_takeover);
    }

    #[test]
    fn resolves_single_target_from_capture_session() {
        let state = InterludeCaptureState::default();
        {
            let mut guard = state.0.lock().unwrap();
            *guard = Some(InterludeCaptureSession {
                id: "capture-1".into(),
                image_path: PathBuf::from("/tmp/capture.png"),
                rows: vec![InterludeQuestRow {
                    candidate_id: "capture-1:0".into(),
                    region: crate::screen::NormRect {
                        x: 0.5,
                        y: 0.4,
                        w: 0.4,
                        h: 0.2,
                    },
                    title_region: crate::screen::NormRect {
                        x: 0.55,
                        y: 0.42,
                        w: 0.2,
                        h: 0.05,
                    },
                    title_signature: "0123456789abcdef0123456789abcdef".into(),
                    kind: "normal".into(),
                    actionable: true,
                    anchor_score: 1.0,
                    mean_luma: 150.0,
                    mean_saturation: 0.0,
                    mean_value: 0.0,
                }],
            });
        }
        let request = InterludeStartRequest {
            mode: InterludeQuestMode::Single,
            capture_id: Some("capture-1".into()),
            candidate_id: Some("capture-1:0".into()),
        };
        let target = resolve_interlude_target(&request, &state).unwrap().unwrap();
        assert_eq!(target.title_signature, "0123456789abcdef0123456789abcdef");

        let all = InterludeStartRequest {
            mode: InterludeQuestMode::All,
            capture_id: None,
            candidate_id: None,
        };
        assert!(resolve_interlude_target(&all, &state).unwrap().is_none());
    }

    #[test]
    fn task_config_round_trips_with_camel_case_fields() {
        let task = InterludeTaskConfig::default();
        let value = serde_json::to_value(&task).unwrap();
        assert!(value.get("autoFormation").is_some());
        assert!(value.get("defeatStrategy").is_some());
        assert!(value.get("maxRepeatAttempts").is_some());
        let decoded: InterludeTaskConfig = serde_json::from_value(value).unwrap();
        assert_eq!(decoded.max_repeat_attempts, 10);
    }
}
