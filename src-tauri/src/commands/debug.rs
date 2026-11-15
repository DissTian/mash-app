use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::adb;
use crate::commands::settings::{AdbDeviceSettings, RecognitionSettings};
use crate::craft_essence_enhancement_runner::CraftEssenceEnhancementRunnerHandle;
use crate::enhancement_runner::{
    EnhancementRunnerHandle, SERVANT_FACE_MATCH_CROP, SERVANT_FACE_TEMPLATE_SIZE,
    SERVANT_LIST_REGION,
};
use crate::friend_point_summon_runner::FriendPointSummonRunnerHandle;
use crate::runner::{self, aggregate_np_gauge_samples, RunnerHandle};
use crate::screen::{
    BondLevelUpReadResult, CommandCardMatch, ElementMatch, FindEnhancementServantGridResult,
    FindSupportsResult, NoblePhantasmMatch, NormRect, Point, ServantGridAnchor, ServantGridCell,
    ServantGridFaceMatch, SupportCeArtworkCheck, SupportCeIconCheck, SupportCeVerificationOptions,
    SupportDiagnostics, SupportRowMatch,
};
use crate::{
    load_servant_metadata_for_variant, resolve_ce_assets_dir, resolve_cv_config_paths,
    resolve_servant_assets_dir, resolve_template_dirs, Server,
};

mod capture;
mod flow;
mod session;
pub use capture::*;
pub use flow::*;

use session::{
    current_server, debug_stream_status, ensure_debug_stream_for_current_device, DebugScreenSize,
};
pub(crate) use session::{
    debug_image_path, ensure_debug_sidecar, ensure_debug_stream, require_automation_idle,
    DebugCaptureResult, DebugSidecar, DebugStreamFrameResult, DebugStreamStatus,
};

/// Deep-merge `overlay` into `base`: objects merge key-by-key, everything
/// else (arrays, scalars) is replaced by the overlay value.
fn merge_cv_config(base: &mut serde_json::Value, overlay: &serde_json::Value) {
    match (base, overlay) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                match base_map.get_mut(key) {
                    Some(existing) => merge_cv_config(existing, value),
                    None => {
                        base_map.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base_value, overlay_value) => *base_value = overlay_value.clone(),
    }
}

/// Load the same merged cv.json view the sidecar sees: `shared/cv.json`
/// first, then the server-specific file on top. Keys such as
/// `Battle.attack_button` only exist in the shared file, so reading the
/// server file alone reports elements the runner happily resolves.
fn load_merged_cv_config(
    app: &tauri::AppHandle,
    server: Server,
) -> Result<serde_json::Value, String> {
    let mut merged = serde_json::Value::Object(serde_json::Map::new());
    let paths = resolve_cv_config_paths(app, server);
    if paths.is_empty() {
        return Err("resource_dir not available".to_string());
    }
    for path in paths {
        let contents = fs::read_to_string(&path)
            .map_err(|e| format!("读取 cv.json 失败 ({}): {e}", path.display()))?;
        let config: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|e| format!("解析 cv.json 失败 ({}): {e}", path.display()))?;
        merge_cv_config(&mut merged, &config);
    }
    Ok(merged)
}

fn cv_element_spec(
    app: &tauri::AppHandle,
    server: Server,
    screen: &str,
    element: &str,
) -> Result<(String, NormRect, f64), String> {
    let cfg = load_merged_cv_config(app, server)?;
    let screen_spec = cfg
        .get("screens")
        .and_then(|screens| screens.get(screen))
        .ok_or_else(|| format!("cv.json 缺少 screen 配置: {screen}"))?;
    let spec = find_cv_target_spec(screen_spec, element)
        .ok_or_else(|| format!("cv.json 缺少元素配置: {screen}.{element}"))?;
    let template = spec
        .get("template")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("cv.json 元素缺少 template: {screen}.{element}"))?
        .to_string();
    let region = spec
        .get("region")
        .and_then(parse_norm_rect)
        .ok_or_else(|| format!("cv.json 元素缺少 region: {screen}.{element}"))?;
    let threshold = spec
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.8);
    Ok((template, region, threshold))
}

fn find_cv_target_spec<'a>(
    screen: &'a serde_json::Value,
    element: &str,
) -> Option<&'a serde_json::Value> {
    if element == "detect" {
        return screen.get("detect");
    }
    if screen
        .get("detect")
        .and_then(|detect| detect.get("template"))
        .and_then(|template| template.as_str())
        == Some(element)
    {
        return screen.get("detect");
    }
    if let Some(spec) = screen
        .get("elements")
        .and_then(|elements| elements.get(element))
    {
        return Some(spec);
    }

    let variants = screen.get("variants").and_then(|v| v.as_object())?;
    if let Some(rest) = element.strip_prefix("variants.") {
        let mut parts = rest.splitn(2, '.');
        let variant_name = parts.next()?;
        let target_name = parts.next()?;
        let variant = variants.get(variant_name)?;
        if target_name == "detect" {
            return variant.get("detect");
        }
        if let Some(element_name) = target_name.strip_prefix("elements.") {
            return variant
                .get("elements")
                .and_then(|elements| elements.get(element_name));
        }
        return variant
            .get("elements")
            .and_then(|elements| elements.get(target_name));
    }

    for variant in variants.values() {
        if variant
            .get("detect")
            .and_then(|detect| detect.get("template"))
            .and_then(|template| template.as_str())
            == Some(element)
        {
            return variant.get("detect");
        }
        if let Some(spec) = variant
            .get("elements")
            .and_then(|elements| elements.get(element))
        {
            return Some(spec);
        }
    }
    None
}

#[tauri::command]
pub fn debug_find_element(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    template_key: String,
    region: Option<NormRect>,
    threshold: Option<f64>,
) -> Result<ElementMatch, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        eprintln!(
            "[debug_find_element] no screenshot at {}",
            image_path.display()
        );
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    let region = region.unwrap_or(NormRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    });
    let threshold = threshold.unwrap_or(0.8);
    eprintln!(
        "[debug_find_element] key={template_key} threshold={threshold} region=({:.2},{:.2},{:.2},{:.2})",
        region.x, region.y, region.w, region.h
    );

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result = client.find_element_full(Some(&image_path), &template_key, region, threshold)?;
    eprintln!(
        "[debug_find_element] result: found={} score={:.3} xy=({:.3},{:.3})",
        result.found, result.score, result.x, result.y
    );
    Ok(result)
}

#[tauri::command]
pub fn debug_read_bond_level_up(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<BondLevelUpReadResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        eprintln!(
            "[debug_read_bond_level_up] no screenshot at {}",
            image_path.display()
        );
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result = client.read_bond_level_up(Some(&image_path), true)?;
    eprintln!(
        "[debug_read_bond_level_up] ok={} reason={:?} level={:?} servant={:?}",
        result.ok, result.reason, result.bond_level_after, result.servant_name_matched
    );
    Ok(result)
}

#[tauri::command]
pub fn debug_list_templates(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
) -> Vec<String> {
    let server = current_server(&server_state);
    let mut keys = Vec::new();
    for spec in resolve_template_dirs(&app, server) {
        match collect_template_keys(
            &spec.dir,
            &spec.dir,
            spec.key_prefix.as_deref().unwrap_or(""),
            &mut keys,
        ) {
            Ok(()) => {}
            Err(e) => {
                eprintln!(
                    "[debug_list_templates] cannot read {}: {e}",
                    spec.dir.display()
                );
            }
        }
    }
    keys.sort();
    keys.dedup();
    eprintln!(
        "[debug_list_templates] {} template(s) for server={server}",
        keys.len(),
    );
    keys
}

/// One bundled template PNG: its key plus the absolute file path. The debug
/// page turns `path` into an asset URL so the picker can show thumbnails
/// without base64-ing every image through IPC.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugTemplateFile {
    pub template_key: String,
    pub path: String,
}

#[tauri::command]
pub fn debug_list_template_files(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
) -> Vec<DebugTemplateFile> {
    let server = current_server(&server_state);
    let mut files = Vec::new();
    for spec in resolve_template_dirs(&app, server) {
        let mut keys = Vec::new();
        if collect_template_keys(
            &spec.dir,
            &spec.dir,
            spec.key_prefix.as_deref().unwrap_or(""),
            &mut keys,
        )
        .is_err()
        {
            continue;
        }
        for key in keys {
            if let Some(path) = resolve_template_file(&app, server, &key) {
                files.push(DebugTemplateFile {
                    template_key: key,
                    path: path.to_string_lossy().into_owned(),
                });
            }
        }
    }
    files.sort_by(|a, b| a.template_key.cmp(&b.template_key));
    files.dedup_by(|a, b| a.template_key == b.template_key);
    eprintln!(
        "[debug_list_template_files] {} template(s) for server={server}",
        files.len(),
    );
    files
}

fn collect_template_keys(
    root: &std::path::Path,
    dir: &std::path::Path,
    key_prefix: &str,
    keys: &mut Vec<String>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_template_keys(root, &path, key_prefix, keys)?;
            continue;
        }
        if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("png"))
            != Some(true)
        {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let mut key = relative.with_extension("").to_string_lossy().into_owned();
        if std::path::MAIN_SEPARATOR != '/' {
            key = key.replace(std::path::MAIN_SEPARATOR, "/");
        }
        let prefix = key_prefix.trim_matches('/');
        if !prefix.is_empty() {
            key = format!("{prefix}/{key}");
        }
        keys.push(key);
    }
    Ok(())
}

/// Strip a template key's directory prefix so it can be resolved inside one
/// template root. Returns ``None`` when the key belongs to another root or
/// contains a traversal segment.
fn template_relative_key(template_key: &str, key_prefix: &str) -> Option<String> {
    let key = template_key.trim().trim_matches('/').replace('\\', "/");
    if key.is_empty() {
        return None;
    }
    let prefix = key_prefix.trim_matches('/');
    let relative = if prefix.is_empty() {
        key.as_str()
    } else {
        key.strip_prefix(prefix)?.strip_prefix('/')?
    };
    if relative.is_empty()
        || relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some(relative.to_string())
}

/// Snapshot of one template PNG for the debug preview dialog.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugTemplateImage {
    pub template_key: String,
    pub path: String,
    pub data_url: String,
    pub size_bytes: usize,
}

fn resolve_template_file(
    app: &tauri::AppHandle,
    server: Server,
    template_key: &str,
) -> Option<PathBuf> {
    // Server templates come last in `resolve_template_dirs`; walk backwards so
    // an un-prefixed key prefers the server root over shared. Prefixed keys
    // only match their own root, so the order cannot shadow `shared/...`.
    for spec in resolve_template_dirs(app, server).iter().rev() {
        let Some(relative) = template_relative_key(
            template_key,
            spec.key_prefix.as_deref().unwrap_or(""),
        ) else {
            continue;
        };
        let mut path = spec.dir.clone();
        for segment in relative.split('/') {
            path.push(segment);
        }
        path.set_extension("png");
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Return the bundled PNG behind one template key so the debug page can show
/// the raw template next to a probe result.
#[tauri::command]
pub fn debug_read_template_image(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    template_key: String,
) -> Result<DebugTemplateImage, String> {
    use base64::Engine as _;

    let server = current_server(&server_state);
    let key = template_key.trim().to_string();
    if key.is_empty() {
        return Err("模板 key 为空".into());
    }
    let path = resolve_template_file(&app, server, &key)
        .ok_or_else(|| format!("未找到模板文件: {key}"))?;
    let bytes = fs::read(&path).map_err(|e| format!("读取模板失败: {e}"))?;
    eprintln!(
        "[debug_read_template_image] key={key} path={} bytes={}",
        path.display(),
        bytes.len(),
    );
    Ok(DebugTemplateImage {
        template_key: key,
        path: path.to_string_lossy().into_owned(),
        data_url: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        ),
        size_bytes: bytes.len(),
    })
}

#[tauri::command]
pub fn debug_find_element_by_name(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    screen: String,
    element: String,
) -> Result<ElementMatch, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    eprintln!("[debug_find_element_by_name] screen={screen} element={element}");

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result = client.find_element_by_name(Some(&image_path), &screen, &element)?;
    eprintln!(
        "[debug_find_element_by_name] result: found={} score={:.3}",
        result.found, result.score
    );
    Ok(result)
}

#[tauri::command]
pub fn debug_get_cv_config(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
) -> Result<serde_json::Value, String> {
    let server = current_server(&server_state);
    load_merged_cv_config(&app, server)
}

#[tauri::command]
pub fn debug_reload_sidecar(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<(), String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    {
        let mut guard = debug_state.0.lock().unwrap();
        guard.take();
    }
    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))
}

#[tauri::command]
pub fn debug_shutdown(debug_state: tauri::State<'_, DebugSidecar>) -> Result<(), String> {
    let mut guard = debug_state.0.lock().unwrap();
    guard.take();
    Ok(())
}

/// Snapshot of every `Point` / `NormRect` constant the runner uses, grouped
/// for display in the debug UI. Returns static data — no device/sidecar
/// interaction required.
#[tauri::command]
pub fn debug_get_runner_coordinates() -> runner::DebugCoordinates {
    runner::debug_coordinates()
}

/// Run the command-card detector against the most recent debug screenshot.
/// Pass any number of candidate ``servant_ids`` to attempt face
/// identification; pass an empty list to only locate the 5 slots + suits.
#[tauri::command]
pub fn debug_find_command_cards(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    servant_ids: Vec<u32>,
) -> Result<Vec<CommandCardMatch>, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))?;

    let assets_dir = resolve_servant_assets_dir(&app);
    eprintln!(
        "[debug_find_command_cards] servant_ids={servant_ids:?} assets_dir={}",
        assets_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "<none>".into()),
    );

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let cards = client.find_command_cards(
        Some(&image_path),
        None,
        &servant_ids,
        assets_dir.as_deref(),
        true,
    )?;
    eprintln!("[debug_find_command_cards] {} card(s) found", cards.len());
    Ok(cards)
}

mod battle;
pub use battle::*;
mod enhancement;
pub use enhancement::*;
mod noble_phantasm;
pub use noble_phantasm::*;
mod support;
pub(crate) use support::*;

/// List every servant id under ``assets/servants/`` that has at least one
/// ``card_servant_*.png`` file. The Debug UI uses this to populate the
/// candidate-id picker without the user having to know what ships.
#[tauri::command]
pub fn debug_list_servant_assets(app: tauri::AppHandle) -> Vec<u32> {
    let Some(dir) = resolve_servant_assets_dir(&app) else {
        eprintln!("[debug_list_servant_assets] no assets dir resolved");
        return Vec::new();
    };
    let mut ids: Vec<u32> = Vec::new();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "[debug_list_servant_assets] cannot read {}: {e}",
                dir.display()
            );
            return Vec::new();
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(id) = name.parse::<u32>() else {
            continue;
        };

        let mut has_face = false;
        if let Ok(inner) = fs::read_dir(&path) {
            for f in inner.flatten() {
                let fname = f.file_name();
                let Some(fname) = fname.to_str() else {
                    continue;
                };
                if fname.starts_with("card_servant_") && fname.to_lowercase().ends_with(".png") {
                    has_face = true;
                    break;
                }
            }
        }
        if has_face {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    eprintln!(
        "[debug_list_servant_assets] {} servant(s) under {}",
        ids.len(),
        dir.display()
    );
    ids
}

/// Pre-warm the debug sidecar process so the first user interaction with the
/// Debug page doesn't pay the 20-40s PyInstaller cold-boot cost. Idempotent
/// while automation is idle. The frontend should invoke this on app startup
/// (or on Debug page mount) to move the cold-boot off the critical path.
///
/// This intentionally does *not* start the scrcpy stream -- streaming would
/// conflict with a running automation session and is started lazily by
/// `debug_capture` itself.
#[tauri::command]
pub fn warm_sidecar(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<(), String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    ensure_debug_sidecar(&app, &debug_state, current_server(&server_state))
}

#[cfg(test)]
mod tests {
    use super::{merge_cv_config, template_relative_key};
    use serde_json::json;

    #[test]
    fn merge_cv_config_keeps_shared_elements_when_server_overrides_a_variant() {
        let mut merged = json!({
            "screens": {
                "Battle": {
                    "variants": {
                        "main": {
                            "elements": {
                                "attack_button": {"template": "shared/battle/attack_button"},
                                "battle_action_menu": {"template": "shared/battle/battle_action_menu"}
                            }
                        }
                    }
                }
            }
        });
        let server = json!({
            "screens": {
                "Battle": {
                    "variants": {
                        "main": {
                            "elements": {
                                "battle_action_menu": {"template": "battle/battle_action_menu"}
                            }
                        }
                    }
                }
            }
        });

        merge_cv_config(&mut merged, &server);

        let elements = &merged["screens"]["Battle"]["variants"]["main"]["elements"];
        assert_eq!(
            elements["battle_action_menu"]["template"],
            "battle/battle_action_menu"
        );
        assert_eq!(
            elements["attack_button"]["template"],
            "shared/battle/attack_button"
        );
    }

    #[test]
    fn merge_cv_config_replaces_arrays_and_scalars() {
        let mut merged = json!({"a": [1, 2, 3], "b": 1});
        merge_cv_config(&mut merged, &json!({"a": [9], "b": 2}));
        assert_eq!(merged, json!({"a": [9], "b": 2}));
    }

    #[test]
    fn template_relative_key_strips_only_matching_prefixes() {
        assert_eq!(
            template_relative_key("shared/battle/attack_button", "shared"),
            Some("battle/attack_button".to_string())
        );
        assert_eq!(
            template_relative_key("battle/attack_button", ""),
            Some("battle/attack_button".to_string())
        );
        assert_eq!(template_relative_key("battle/attack_button", "shared"), None);
        assert_eq!(template_relative_key("/shared/battle/x/", ""), Some("shared/battle/x".to_string()));
    }

    #[test]
    fn template_relative_key_rejects_traversal_and_empty_keys() {
        assert_eq!(template_relative_key("../battle/x", ""), None);
        assert_eq!(template_relative_key("battle/../x", ""), None);
        assert_eq!(template_relative_key("shared", "shared"), None);
        assert_eq!(template_relative_key("   ", ""), None);
    }
}
