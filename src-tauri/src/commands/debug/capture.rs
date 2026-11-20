use super::*;
use tauri_plugin_dialog::DialogExt;

/// Load a screenshot from disk and make it the current debug frame, so every
/// CV tool can be tested without re-opening the in-game screen.
#[tauri::command]
pub async fn debug_pick_image(app: tauri::AppHandle) -> Result<Option<DebugCaptureResult>, String> {
    let picked = app
        .dialog()
        .file()
        .add_filter("图片", &["png", "jpg", "jpeg", "webp"])
        .set_title("载入调试截图")
        .blocking_pick_file();
    let Some(file) = picked else {
        return Ok(None);
    };
    let source = file.into_path().map_err(|error| error.to_string())?;
    let dest = debug_image_path(&app);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("创建调试目录失败: {error}"))?;
    }
    std::fs::copy(&source, &dest).map_err(|error| format!("复制截图失败: {error}"))?;
    Ok(Some(DebugCaptureResult {
        image_path: dest.to_string_lossy().into_owned(),
        screen: "载入".to_string(),
        score: 0.0,
        screen_size: None,
    }))
}

#[tauri::command]
pub fn debug_capture(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugCaptureResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let selected_adb_serial = adb_settings_state
        .lock()
        .unwrap()
        .selected_adb_serial
        .clone();
    let server = current_server(&server_state);
    eprintln!(
        "[debug_capture] begin (selected_adb_serial={selected_adb_serial:?}, server={server})"
    );

    let mut adb_dev = adb::Adb::new(&app, selected_adb_serial);
    adb_dev.connect_for_game().map_err(|e| {
        eprintln!("[debug_capture] adb connect failed: {e}");
        e
    })?;
    let serial = adb_dev.serial().map(|s| s.to_string());
    eprintln!("[debug_capture] adb connected, serial={serial:?}");

    ensure_debug_stream(&app, &debug_state, server, &adb_dev).map_err(|e| {
        eprintln!("[debug_capture] ensure stream failed: {e}");
        e
    })?;

    let dest = debug_image_path(&app);

    let (jpeg, screen, score, stream_size) = {
        let mut guard = debug_state.0.lock().unwrap();
        let client = guard
            .as_mut()
            .ok_or_else(|| "debug sidecar not initialized".to_string())?;

        // ``ensure_debug_stream`` already waited for the warmup frame inside
        // start_stream, so a single in-sidecar wait of 2s is enough to cover
        // the post-handshake key-frame gap. No need for a polling retry loop
        // on the Rust side.
        let jpeg = client.get_frame_jpeg(2.0).map_err(|e| {
            eprintln!("[debug_capture] get_frame failed: {e}");
            format!("未获取到视频帧: {e}")
        })?;

        fs::write(&dest, &jpeg).map_err(|e| {
            eprintln!("[debug_capture] write frame failed: {e}");
            format!("failed to write frame: {e}")
        })?;
        let (screen, score) = client.detect_label_full(Some(&dest)).map_err(|e| {
            eprintln!("[debug_capture] detect failed: {e}");
            e
        })?;
        (jpeg, screen, score, client.stream_size())
    };

    eprintln!(
        "[debug_capture] frame saved: {} ({} bytes), screen={screen} score={score:.3}",
        dest.display(),
        jpeg.len(),
    );

    Ok(DebugCaptureResult {
        image_path: dest.to_string_lossy().to_string(),
        screen,
        score,
        screen_size: stream_size.map(|(w, h)| DebugScreenSize { w, h }),
    })
}

#[tauri::command]
pub fn debug_stream_connect(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugStreamStatus, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    ensure_debug_stream_for_current_device(&app, &adb_settings_state, &server_state, &debug_state)?;
    Ok(debug_stream_status(&debug_state))
}

#[tauri::command]
pub fn debug_stream_disconnect(
    debug_state: tauri::State<'_, DebugSidecar>,
) -> Result<DebugStreamStatus, String> {
    {
        let mut guard = debug_state.0.lock().unwrap();
        if let Some(client) = guard.as_mut() {
            if client.stream_size().is_some() {
                client.stop_stream()?;
            }
        }
    }
    Ok(debug_stream_status(&debug_state))
}

#[tauri::command]
pub fn debug_stream_frame(
    app: tauri::AppHandle,
    adb_settings_state: tauri::State<'_, Mutex<AdbDeviceSettings>>,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    detect_screen: Option<bool>,
) -> Result<DebugStreamFrameResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    ensure_debug_stream_for_current_device(&app, &adb_settings_state, &server_state, &debug_state)?;

    let (jpeg_base64, width, height, screen, score) = {
        let mut guard = debug_state.0.lock().unwrap();
        let client = guard
            .as_mut()
            .ok_or_else(|| "debug sidecar not initialized".to_string())?;
        let (jpeg_base64, width, height) = client.get_frame_jpeg_base64(0.2)?;
        let (screen, score) = if detect_screen.unwrap_or(false) {
            let (screen, score) = client.detect_label_full(None)?;
            (Some(screen), Some(score))
        } else {
            (None, None)
        };
        (jpeg_base64, width, height, screen, score)
    };

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);

    Ok(DebugStreamFrameResult {
        jpeg_base64,
        width,
        height,
        screen,
        score,
        timestamp_ms,
    })
}
