use super::*;
use crate::commands::catalog::pick_faces_desc_in;
use crate::servant_enhancement::runner as cn_runner;

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugEnhancementServantMatchResult {
    pub servant_id: u32,
    pub search_region: NormRect,
    pub template_crop: NormRect,
    pub template_size: DebugTemplateSize,
    pub threshold: f64,
    pub found: bool,
    pub x: f64,
    pub y: f64,
    pub score: f64,
    pub best: Option<ServantGridFaceMatch>,
    pub anchors: Vec<ServantGridAnchor>,
    pub reference_anchor: Option<ServantGridAnchor>,
    pub grid_cells: Vec<ServantGridCell>,
    pub matches: Vec<ServantGridFaceMatch>,
    pub diagnostics: crate::screen::ServantGridDiagnostics,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugTemplateSize {
    pub w: u32,
    pub h: u32,
}

/// Run the enhancement servant-select face matcher against the most recent
/// debug screenshot for one servant id. This mirrors the production
/// enhancement runner crop/search region and returns every face template's
/// score so the crop can be tuned from the UI.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn debug_find_enhancement_servant(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    servant_id: u32,
    threshold: Option<f64>,
    two_exp: Option<bool>,
    use_live_frame: Option<bool>,
    direct_search_threshold: Option<f64>,
) -> Result<DebugEnhancementServantMatchResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;

    let server = current_server(&server_state);
    let two_exp = two_exp.unwrap_or(false);
    let use_live_frame = use_live_frame.unwrap_or(false);

    // Mirror the server's production runner: the CN queue uses its own list
    // region and a dedicated lower-left crop for 2EXP cards, while the JP flow
    // keeps the older region/crop pair.
    let (region, template_crop) = if server == Server::Cn {
        (
            cn_runner::SERVANT_LIST_REGION,
            if two_exp {
                cn_runner::TWO_EXP_FACE_MATCH_CROP
            } else {
                cn_runner::SERVANT_FACE_MATCH_CROP
            },
        )
    } else {
        (SERVANT_LIST_REGION, SERVANT_FACE_MATCH_CROP)
    };

    let image_path = if use_live_frame {
        None
    } else {
        let path = debug_image_path(&app);
        if !path.exists() {
            return Err("尚未截取画面，请先点击 截取画面，或勾选「使用实时帧」".into());
        }
        Some(path)
    };

    let assets_dir = resolve_servant_assets_dir(&app)
        .ok_or_else(|| "未找到从者资源目录，无法进行头像匹配".to_string())?;
    let servant_dir = assets_dir.join(servant_id.to_string());
    let templates = pick_faces_desc_in(&servant_dir);
    if templates.is_empty() {
        return Err(format!(
            "从者 #{servant_id} 缺少 face_servant_*.png: {}",
            servant_dir.display()
        ));
    }

    let threshold = threshold.unwrap_or(0.60);
    ensure_debug_sidecar(&app, &debug_state, server)?;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let result: FindEnhancementServantGridResult = client.find_enhancement_servant_grid(
        image_path.as_deref(),
        &templates,
        region,
        template_crop,
        // Let the sidecar scale the face template from the frame it loads; a
        // fixed 223 only matches 2560-wide captures.
        None,
        threshold,
        0.0,
        None,
        direct_search_threshold,
        true,
        server == Server::Cn,
    )?;
    let template_size = result
        .template_size
        .unwrap_or(crate::screen::TemplateSize {
            w: SERVANT_FACE_TEMPLATE_SIZE.0,
            h: SERVANT_FACE_TEMPLATE_SIZE.1,
        });
    eprintln!(
        "[debug_find_enhancement_servant] servant_id={} two_exp={} live={} region={:?} size={}x{} anchors={} cells={} best={:.3} method={:?}",
        servant_id,
        two_exp,
        use_live_frame,
        region,
        template_size.w,
        template_size.h,
        result.anchors.len(),
        result.grid_cells.len(),
        result.score,
        result.diagnostics.match_method,
    );
    Ok(DebugEnhancementServantMatchResult {
        servant_id,
        search_region: region,
        template_crop,
        template_size: DebugTemplateSize {
            w: template_size.w,
            h: template_size.h,
        },
        threshold,
        found: result.found,
        x: result.x,
        y: result.y,
        score: result.score,
        best: result.best,
        anchors: result.anchors,
        reference_anchor: result.reference_anchor,
        grid_cells: result.grid_cells,
        matches: result.matches,
        diagnostics: result.diagnostics,
    })
}
