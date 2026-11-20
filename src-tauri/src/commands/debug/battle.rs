use super::*;

/// One above-threshold digit detection inside the BATTLE strip,
/// returned to the debug UI so the user can see exactly which glyph the
/// matcher saw, where, and how confidently. ``kept`` is true iff the
/// candidate survived greedy NMS (i.e. it actually contributed to the
/// `m/n` split). All coordinates are full-screen normalized.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugDigitMatch {
    pub value: u32,
    pub score: f64,
    pub region: NormRect,
}

/// Result of running the battle-scene OCR against the most recent
/// debug screenshot. Carries `scene`/`total` (`null` when the detector
/// bails) plus a full diagnostic snapshot so the UI can pinpoint *why*
/// detection failed: anchor score & box, strip rect, every
/// above-threshold digit candidate, the NMS-kept subset, the chosen
/// split + best gap, and a `failReason` enum that mirrors the early-
/// return labels in `_read_battle_scene`.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugBattleSceneResult {
    pub region: NormRect,
    pub scene: Option<u32>,
    pub total: Option<u32>,
    pub label_template_loaded: bool,
    pub label_threshold: f64,
    pub digit_threshold: f64,
    pub anchor_score: f64,
    pub anchor_box: Option<NormRect>,
    pub strip_region: Option<NormRect>,
    pub candidates: Vec<DebugDigitMatch>,
    pub kept: Vec<DebugDigitMatch>,
    pub split_at: Option<u32>,
    pub best_gap: f64,
    pub avg_width: f64,
    pub trimmed_left: u32,
    pub trimmed_right: u32,
    pub missing_digit_templates: Vec<u32>,
    pub fail_reason: Option<String>,
}

pub(crate) fn parse_norm_rect(value: &serde_json::Value) -> Option<NormRect> {
    Some(NormRect {
        x: value.get("x")?.as_f64()?,
        y: value.get("y")?.as_f64()?,
        w: value.get("w")?.as_f64()?,
        h: value.get("h")?.as_f64()?,
    })
}

fn parse_digit_matches(value: &serde_json::Value) -> Vec<DebugDigitMatch> {
    let arr = match value.as_array() {
        Some(a) => a,
        None => return Vec::new(),
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let region = match item.get("region").and_then(parse_norm_rect) {
            Some(r) => r,
            None => continue,
        };
        let value = item.get("value").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let score = item.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        out.push(DebugDigitMatch {
            value,
            score,
            region,
        });
    }
    out
}

/// Run `read_battle_scene` against the most recent debug screenshot in
/// **diagnostic mode** (the sidecar returns the full intermediate state
/// alongside `scene`/`total`). The UI renders every above-threshold
/// digit candidate as an overlay box and shows `failReason` when the
/// detector bails, so the user can tell the difference between
/// "anchor missed", "no digits cleared 0.8", and "digits found but no
/// separator gap" without reading source.
#[tauri::command]
pub fn debug_read_battle_scene(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugBattleSceneResult, String> {
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

    let region = runner::BATTLE_SCENE_REGION;
    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let resp = client.read_battle_scene_debug(Some(&image_path), region)?;
    let diag = resp
        .get("diagnostics")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let scene = resp["scene"].as_u64().map(|n| n as u32);
    let total = resp["total"].as_u64().map(|n| n as u32);

    let label_template_loaded = diag
        .get("labelTemplateLoaded")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let label_threshold = diag
        .get("labelThreshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let digit_threshold = diag
        .get("digitThreshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let anchor_score = diag
        .get("anchorScore")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let anchor_box = diag.get("anchorBox").and_then(parse_norm_rect);
    let strip_region = diag.get("stripRegion").and_then(parse_norm_rect);
    let candidates = diag
        .get("candidates")
        .map(parse_digit_matches)
        .unwrap_or_default();
    let kept = diag
        .get("kept")
        .map(parse_digit_matches)
        .unwrap_or_default();
    let split_at = diag
        .get("splitAt")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let best_gap = diag.get("bestGap").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let avg_width = diag.get("avgWidth").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let trimmed_left = diag
        .get("trimmedLeft")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let trimmed_right = diag
        .get("trimmedRight")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let missing_digit_templates = diag
        .get("missingDigitTemplates")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|d| d.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();
    let fail_reason = diag
        .get("failReason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    eprintln!(
        "[debug_read_battle_scene] scene={:?} total={:?} anchorScore={:.3} cands={} kept={} fail={:?}",
        scene,
        total,
        anchor_score,
        candidates.len(),
        kept.len(),
        fail_reason,
    );

    Ok(DebugBattleSceneResult {
        region,
        scene,
        total,
        label_template_loaded,
        label_threshold,
        digit_threshold,
        anchor_score,
        anchor_box,
        strip_region,
        candidates,
        kept,
        split_at,
        best_gap,
        avg_width,
        trimmed_left,
        trimmed_right,
        missing_digit_templates,
        fail_reason,
    })
}

/// One expected front-line skill slot and whether its white frame edge was
/// found in the skill bar.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSkillSlot {
    pub index: usize,
    pub present: bool,
    #[serde(default)]
    pub region: Option<NormRect>,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub border_y: Option<f64>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSkillSlotsResult {
    pub region: NormRect,
    #[serde(default)]
    pub threshold: f64,
    #[serde(default)]
    pub min_fraction: f64,
    #[serde(default)]
    pub slots: Vec<DebugSkillSlot>,
}

/// Detect which front-line skill buttons exist on the most recent debug
/// screenshot. Read-only CV tool: no runner integration yet.
#[tauri::command]
pub fn debug_find_skill_slots(
    app: tauri::AppHandle,
    threshold: f64,
    min_fraction: f64,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugSkillSlotsResult, String> {
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

    let points: Vec<Point> = crate::runner::SERVANT_SKILLS
        .iter()
        .flatten()
        .copied()
        .collect();
    let region = crate::runner::SKILL_SLOT_SCAN_REGION;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let resp =
        client.find_skill_slots(Some(&image_path), &points, region, threshold, min_fraction)?;
    serde_json::from_value(resp)
        .map_err(|error| format!("invalid find_skill_slots response: {error}"))
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSkillCooldownSlot {
    pub index: usize,
    pub region: NormRect,
    pub text: String,
    /// Text recognized to the left of the digit ("剩余"/"残り" only exists
    /// while the skill is cooling).
    pub label_text: String,
    /// True only when the recognized digit text is entirely digits.
    pub numeric: bool,
    pub label_matched: bool,
    pub cooling: bool,
    pub elapsed_ms: f64,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSkillCooldownsResult {
    pub section: String,
    pub mode: String,
    pub corner_dx: f64,
    pub corner_dy: f64,
    pub corner_w: f64,
    pub corner_h: f64,
    pub scale: f64,
    pub min_confidence: f64,
    pub require_label: bool,
    pub elapsed_ms: f64,
    pub slots: Vec<DebugSkillCooldownSlot>,
}

fn cooldown_label_matches(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.contains("剩余") || trimmed.contains("残")
}

fn cooldown_text_is_numeric(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().all(|character| character.is_numeric())
}

/// OCR each front-line / mystic-code skill button's bottom-right corner and
/// report whether a cooldown number is present. Read-only CV tool with tunable
/// crop geometry, a fast recognition-only mode, and timing information.
#[tauri::command]
pub fn debug_find_skill_cooldowns(
    app: tauri::AppHandle,
    section: String,
    mode: String,
    corner_dx: f64,
    corner_dy: f64,
    corner_w: f64,
    corner_h: f64,
    scale: f64,
    min_confidence: f64,
    require_label: bool,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugSkillCooldownsResult, String> {
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

    let points: Vec<Point> = if section == "equipment" {
        crate::runner::EQUIPMENT_SKILLS.to_vec()
    } else {
        crate::runner::SERVANT_SKILLS
            .iter()
            .flatten()
            .copied()
            .collect()
    };

    let corner_regions: Vec<NormRect> = points
        .iter()
        .map(|point| {
            let width = corner_w.clamp(0.001, 0.2);
            let height = corner_h.clamp(0.001, 0.2);
            let left = (point.x + corner_dx).clamp(0.0, (1.0 - width).max(0.0));
            let top = (point.y + corner_dy).clamp(0.0, (1.0 - height).max(0.0));
            NormRect {
                x: left,
                y: top,
                w: width,
                h: height,
            }
        })
        .collect();

    let label_regions: Vec<NormRect> = points
        .iter()
        .map(|point| {
            let width = crate::runner::SKILL_COOLDOWN_LABEL_WIDTH.clamp(0.001, 0.2);
            let height = crate::runner::SKILL_COOLDOWN_LABEL_HEIGHT.clamp(0.001, 0.2);
            let left = (point.x + crate::runner::SKILL_COOLDOWN_LABEL_DX)
                .clamp(0.0, (1.0 - width).max(0.0));
            let top = (point.y + crate::runner::SKILL_COOLDOWN_LABEL_DY)
                .clamp(0.0, (1.0 - height).max(0.0));
            NormRect {
                x: left,
                y: top,
                w: width,
                h: height,
            }
        })
        .collect();

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;

    let started = Instant::now();
    let mut slots = Vec::with_capacity(points.len());

    if mode == "rec" {
        // Recognition-only batch: one sidecar round trip for all crops.
        // Digits + labels in one recognition batch: one sidecar round trip.
        let mut combined_regions = corner_regions.clone();
        combined_regions.extend(label_regions.iter().copied());
        let texts = client
            .recognize_strips(Some(&image_path), &combined_regions)
            .map_err(|error| format!("纯识别失败: {error}"))?;
        let split_at = corner_regions.len();
        let total_ms = started.elapsed().as_secs_f64() * 1000.0;
        for (index, region) in corner_regions.into_iter().enumerate() {
            let text = texts
                .get(index)
                .map(|text| text.trim().to_string())
                .unwrap_or_default();
            let label_text = texts
                .get(split_at + index)
                .map(|text| text.trim().to_string())
                .unwrap_or_default();
            let numeric = cooldown_text_is_numeric(&text);
            let label_matched = cooldown_label_matches(&label_text);
            slots.push(DebugSkillCooldownSlot {
                index,
                region,
                text,
                label_text,
                numeric,
                label_matched,
                cooling: numeric && (!require_label || label_matched),
                elapsed_ms: total_ms / points.len().max(1) as f64,
            });
        }
    } else {
        let label_texts = client
            .recognize_strips(Some(&image_path), &label_regions)
            .map_err(|error| format!("剩余文字识别失败: {error}"))?;
        for (index, region) in corner_regions.into_iter().enumerate() {
            let slot_started = Instant::now();
            let result = client
                .ocr_region_with_scale(Some(&image_path), region, scale)
                .map_err(|error| format!("技能位 {} OCR 失败: {error}", index + 1))?;
            let mut texts = Vec::new();
            let mut numeric = false;
            for fragment in &result.fragments {
                if fragment.ocr_confidence < min_confidence {
                    continue;
                }
                let text = fragment.text.trim();
                if text.is_empty() {
                    continue;
                }
                if cooldown_text_is_numeric(text) {
                    numeric = true;
                }
                texts.push(text.to_string());
            }
            let label_text = label_texts
                .get(index)
                .map(|text| text.trim().to_string())
                .unwrap_or_default();
            let label_matched = cooldown_label_matches(&label_text);
            slots.push(DebugSkillCooldownSlot {
                index,
                region,
                text: texts.join(" "),
                label_text,
                numeric,
                label_matched,
                cooling: numeric && (!require_label || label_matched),
                elapsed_ms: slot_started.elapsed().as_secs_f64() * 1000.0,
            });
        }
    }

    Ok(DebugSkillCooldownsResult {
        section,
        mode,
        corner_dx,
        corner_dy,
        corner_w,
        corner_h,
        scale,
        min_confidence,
        require_label,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        slots,
    })
}

/// Snapshot of the attack-button probe the runner uses on the Battle
/// screen to decide whether it's our turn. Template, region, and threshold
/// come from `cv.json` (`Battle.variants.main.elements.attack_button`);
/// only the tap point remains a runner coordinate.
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugAttackButtonResult {
    pub template: String,
    pub region: NormRect,
    pub threshold: f64,
    pub tap_point: Point,
    pub found: bool,
    pub score: f64,
    pub match_x: f64,
    pub match_y: f64,
    pub match_region: Option<NormRect>,
}

/// Run the runner's exact attack-button probe against the most recent
/// debug screenshot. Returns both the cv.json threshold/region and the live
/// match score so the debug overlay can render the search box plus tap target.
#[tauri::command]
pub fn debug_find_attack_button(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
) -> Result<DebugAttackButtonResult, String> {
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

    let server = current_server(&server_state);
    ensure_debug_sidecar(&app, &debug_state, server)?;

    let (template, region, threshold) =
        cv_element_spec(&app, server, "Battle", runner::ATTACK_BUTTON_ELEMENT)?;
    let tap_point = runner::ATTACK_BUTTON;

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let m =
        client.find_element_by_name(Some(&image_path), "Battle", runner::ATTACK_BUTTON_ELEMENT)?;

    eprintln!(
        "[debug_find_attack_button] found={} score={:.3} threshold={:.2} center=({:.3},{:.3})",
        m.found, m.score, threshold, m.x, m.y,
    );

    Ok(DebugAttackButtonResult {
        template,
        region,
        threshold,
        tap_point,
        found: m.found,
        score: m.score,
        match_x: m.x,
        match_y: m.y,
        match_region: m.region,
    })
}
