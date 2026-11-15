//! Support-list OCR and craft-essence verification debug command.

use super::*;
use crate::commands::catalog::ce_card_path;

/// Per-row CE verification info computed by `debug_find_supports` when a
/// `craft_essence_id` is supplied. Surfaces both the search region (so
/// the overlay can draw a second box per row to confirm the offset) and
/// the raw `TM_CCOEFF_NORMED` score (so the user can tune
/// `SUPPORT_CE_THRESHOLD` against real numbers).
#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSupportCeInfo {
    pub region: NormRect,
    pub score: f64,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artwork_checks: Vec<SupportCeArtworkCheck>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub icon_checks: Vec<SupportCeIconCheck>,
    /// Threshold the runner would have applied. Returned alongside the
    /// score so the debug UI doesn't have to mirror the constant.
    pub threshold: f64,
    /// Resolved template path, useful for "we couldn't find the file"
    /// diagnostics. `None` when the template isn't on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_path: Option<String>,
    /// Sidecar error message when verification failed before scoring.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn format_ce_artwork_checks(checks: &[SupportCeArtworkCheck]) -> String {
    checks
        .iter()
        .map(|check| {
            let marker = if check.selected {
                "*"
            } else if check.passed {
                "✓"
            } else {
                "✗"
            };
            format!(
                "{}:{} {:.3}/{:.2}{}",
                check.region_kind, check.variant, check.score, check.threshold, marker
            )
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSupportRow {
    #[serde(flatten)]
    pub row: SupportRowMatch,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_filter_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score_filter_reason: Option<String>,
    /// Same verdict for the level / NP / skill requirements configured in the
    /// shared battle-style support settings panel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_filter_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level_filter_reason: Option<String>,
    /// Present only when the caller passed a `craft_essence_id` *and*
    /// the template was resolvable. Absent rows render exactly like the
    /// pre-CE behaviour so the overlay stays backwards-compatible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ce: Option<DebugSupportCeInfo>,
    #[serde(rename = "grandCes", skip_serializing_if = "Vec::is_empty")]
    pub grand_ces: Vec<DebugSupportCeInfo>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSupportScoreFilter {
    pub grand_mode: bool,
    pub star_map_score_min: Option<u32>,
    pub grand_star_map_score_min: Option<u32>,
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugSupportLevelFilter {
    pub servant_level_min: Option<u32>,
    pub noble_phantasm_level_min: Option<u32>,
    pub skill_level_mins: [Option<u32>; 3],
    pub append_skill_level_mins: [Option<u32>; 5],
}

/// Evaluate one row against the runner's level / NP / skill requirements.
///
/// Skill panels are read from whatever the sidecar OCR'd (the debug page
/// cannot toggle them for the user), so a configured skill requirement whose
/// panel isn't on screen is reported as such instead of silently passing.
pub(crate) fn debug_support_level_filter_reason(
    row: &SupportRowMatch,
    servant_level_min: Option<u32>,
    noble_phantasm_level_min: Option<u32>,
    skill_level_mins: &[Option<u32>; 3],
    append_skill_level_mins: &[Option<u32>; 5],
) -> Option<String> {
    if let Some(min) = servant_level_min {
        if !row.servant_level.is_some_and(|level| level >= min) {
            return Some(format!(
                "从者等级 ≥ {}（实际 {}）",
                min,
                runner::format_actual_level(row.servant_level),
            ));
        }
    }
    if let Some(min) = noble_phantasm_level_min {
        if !row.np_level.is_some_and(|level| level >= min) {
            return Some(format!(
                "宝具 ≥ {}（实际 {}）",
                min,
                runner::format_actual_level(row.np_level),
            ));
        }
    }
    if skill_level_mins.iter().any(Option::is_some) {
        match row.skill_panel.as_deref() {
            Some("owned") => {
                if let Some((index, actual, min)) =
                    runner::support_first_level_mismatch(&row.skill_levels, skill_level_mins)
                {
                    return Some(format!(
                        "持有技能 {} ≥ {}（实际 {}）",
                        index + 1,
                        min,
                        runner::format_actual_level(actual),
                    ));
                }
            }
            _ => return Some("持有技能面板未展开（切到持有技能面板后再识别）".into()),
        }
    }
    if append_skill_level_mins.iter().any(Option::is_some) {
        match row.skill_panel.as_deref() {
            Some("append") => {
                if let Some((index, actual, min)) = runner::support_first_level_mismatch(
                    &row.append_skill_levels,
                    append_skill_level_mins,
                ) {
                    return Some(format!(
                        "追加技能 {} ≥ {}（实际 {}）",
                        index + 1,
                        min,
                        runner::format_actual_level(actual),
                    ));
                }
            }
            _ => return Some("追加技能面板未展开（切到追加技能面板后再识别）".into()),
        }
    }
    None
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DebugFindSupportsResult {
    pub supports: Vec<DebugSupportRow>,
    pub diagnostics: SupportDiagnostics,
    pub score_filter: DebugSupportScoreFilter,
    pub level_filter: DebugSupportLevelFilter,
}

/// Use the same confirm-button-anchored CE strip as the runner.
fn ce_search_region(row: &SupportRowMatch) -> Option<NormRect> {
    row.score_anchor.map(runner::ce_search_region)
}

fn grand_ce_search_region(row: &SupportRowMatch, slot: usize) -> Option<NormRect> {
    runner::grand_ce_search_region(row, slot)
}

/// Run the OCR-based support-row detector against the most recent debug
/// screenshot. Loads the servant's metadata (name + every Noble Phantasm
/// name) from ``assets/servants/{servant_id}/servant.json`` and returns
/// every row whose OCR'd name fragment + NP fragment fuzzy-match within
/// the proximity tolerance, plus diagnostics for missed candidates so the
/// debug overlay can render misses too.
///
/// When `craft_essence_id` is supplied, also runs the runner's CE
/// verification per row and returns the anchored search region + score
/// so the user can inspect the detected CE strip without restarting a
/// real run.
#[tauri::command]
pub fn debug_find_supports(
    app: tauri::AppHandle,
    server_state: tauri::State<'_, Mutex<Server>>,
    debug_state: tauri::State<'_, DebugSidecar>,
    recognition_settings_state: tauri::State<'_, Mutex<RecognitionSettings>>,
    handle_state: tauri::State<'_, Mutex<RunnerHandle>>,
    enhancement_handle_state: tauri::State<'_, Mutex<EnhancementRunnerHandle>>,
    ce_enhancement_handle_state: tauri::State<'_, Mutex<CraftEssenceEnhancementRunnerHandle>>,
    friend_point_summon_handle_state: tauri::State<'_, Mutex<FriendPointSummonRunnerHandle>>,
    servant_id: u32,
    servant_variant_key: Option<String>,
    craft_essence_id: Option<u32>,
    grand_craft_essence_ids: Option<[Option<u32>; 3]>,
    craft_essence_mlb_required: Option<bool>,
    grand_craft_essence_mlb_required: Option<[bool; 3]>,
    grand_bond_ce_mode: Option<String>,
    support_grand_mode: Option<bool>,
    support_star_map_score_min: Option<u32>,
    support_grand_star_map_score_min: Option<u32>,
    support_servant_level_min: Option<u32>,
    support_noble_phantasm_level_min: Option<u32>,
    support_skill_level_mins: Option<[Option<u32>; 3]>,
    support_append_skill_level_mins: Option<[Option<u32>; 5]>,
    release_ocr: Option<bool>,
) -> Result<DebugFindSupportsResult, String> {
    require_automation_idle(
        &handle_state,
        &enhancement_handle_state,
        &ce_enhancement_handle_state,
        &friend_point_summon_handle_state,
    )?;
    let recognition_settings = *recognition_settings_state.lock().unwrap();
    let support_ce_threshold = recognition_settings.support_ce_threshold;
    let support_ce_full_gate_threshold = recognition_settings.support_ce_full_gate_threshold;
    let support_mlb_icon_threshold = recognition_settings.support_mlb_icon_threshold;
    let support_bond_icon_threshold = recognition_settings.support_bond_icon_threshold;
    let support_full_list_ocr_fallback = recognition_settings.support_full_list_ocr_fallback;
    let support_grand_mode = support_grand_mode.unwrap_or(false);
    let support_star_map_score_min = support_star_map_score_min.map(|score| score.min(62));
    let support_grand_star_map_score_min =
        support_grand_star_map_score_min.map(|score| score.min(16));
    let score_filter_configured = support_star_map_score_min.is_some()
        || (support_grand_mode && support_grand_star_map_score_min.is_some());
    let skill_level_mins = support_skill_level_mins.unwrap_or([None; 3]);
    let append_skill_level_mins = support_append_skill_level_mins.unwrap_or([None; 5]);
    let level_filter_configured = support_servant_level_min.is_some()
        || support_noble_phantasm_level_min.is_some()
        || skill_level_mins.iter().any(Option::is_some)
        || append_skill_level_mins.iter().any(Option::is_some);

    let image_path = debug_image_path(&app);
    if !image_path.exists() {
        return Err("尚未截取画面，请先点击 截取画面".into());
    }

    let server = current_server(&server_state);
    let meta = load_servant_metadata_for_variant(
        &app,
        servant_id,
        server,
        servant_variant_key.as_deref(),
    )?;
    eprintln!(
        "[debug_find_supports] servant_id={servant_id} variant={:?} server={server} name={:?} np_names={:?} ce={:?} grand={:?}",
        servant_variant_key, meta.name, meta.np_names, craft_essence_id, grand_craft_essence_ids
    );

    ensure_debug_sidecar(&app, &debug_state, server)?;

    // Resolve the CE template up-front (outside the sidecar lock). A
    // missing or unconfigured CE just leaves `ce_template = None` so the
    // OCR pass still runs and the response carries no per-row CE info.
    let ce_template: Option<PathBuf> = craft_essence_id.and_then(|id| {
        let dir = resolve_ce_assets_dir(&app)?;
        let path = ce_card_path(&dir, id);
        if path.is_file() {
            Some(path)
        } else {
            eprintln!(
                "[debug_find_supports] CE template missing: {} (skipping CE verify)",
                path.display()
            );
            None
        }
    });
    let grand_ce_template_paths: [Option<PathBuf>; 3] = std::array::from_fn(|index| {
        grand_craft_essence_ids
            .and_then(|ids| ids[index])
            .and_then(|id| resolve_ce_assets_dir(&app).map(|dir| ce_card_path(&dir, id)))
    });

    let mut guard = debug_state.0.lock().unwrap();
    let client = guard
        .as_mut()
        .ok_or_else(|| "debug sidecar not initialized".to_string())?;
    let find_result: Result<FindSupportsResult, crate::screen::CvError> = client.find_supports(
        Some(&image_path),
        &meta.name,
        &meta.names,
        &meta.excluded_names,
        &meta.np_names,
        meta.require_np_match,
        true,
        support_full_list_ocr_fallback,
        false,
        true,
    );
    // Keep the OCR worker warm by default; the debug page can opt into
    // releasing it after each recognition (useful when memory matters).
    let release_result = if release_ocr.unwrap_or(false) {
        client.release_ocr()
    } else {
        Ok(())
    };
    let result = find_result?;
    release_result?;
    eprintln!(
        "[debug_find_supports] {} match(es), {} name cand(s), {} np cand(s), {} fragment(s)",
        result.supports.len(),
        result.diagnostics.name_candidates.len(),
        result.diagnostics.np_candidates.len(),
        result.diagnostics.fragment_count,
    );

    let template_path_str = ce_template
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());

    let mut supports: Vec<DebugSupportRow> = Vec::with_capacity(result.supports.len());
    for (row_index, row) in result.supports.into_iter().enumerate() {
        if row.score_anchor.is_none() {
            eprintln!(
                "[debug_find_supports] row {} y={:.3} support row anchor missing",
                row_index + 1,
                row.row_region.y
            );
        }
        let ce = match (ce_template.as_deref(), ce_search_region(&row)) {
            (Some(template), Some(region)) => {
                let info = match client.verify_support_ce(
                    Some(&image_path),
                    region,
                    template,
                    support_ce_threshold,
                    SupportCeVerificationOptions {
                        mlb_required: craft_essence_mlb_required.unwrap_or(true),
                        grand_bond_ce_mode: None,
                        full_gate_threshold: support_ce_full_gate_threshold,
                        mlb_icon_threshold: support_mlb_icon_threshold,
                        bond_icon_threshold: support_bond_icon_threshold,
                    },
                ) {
                    Ok(result) => {
                        let effective_threshold = if result.threshold > 0.0 {
                            result.threshold
                        } else {
                            support_ce_threshold
                        };
                        eprintln!(
                            "[debug_find_supports] row y={:.3} CE score={:.3} threshold={:.2} -> {}",
                            row.row_region.y,
                            result.score,
                            effective_threshold,
                            if result.passed { "PASS" } else { "skip" },
                        );
                        if !result.artwork_checks.is_empty() {
                            eprintln!(
                                "[debug_find_supports] row y={:.3} CE variants: {} | {}",
                                row.row_region.y,
                                format_ce_artwork_checks(&result.artwork_checks),
                                runner::format_ce_verification_summary(
                                    &result.artwork_checks,
                                    &result.icon_checks
                                ),
                            );
                        }
                        DebugSupportCeInfo {
                            region,
                            score: result.score,
                            passed: result.passed,
                            artwork_checks: result.artwork_checks,
                            icon_checks: result.icon_checks,
                            threshold: effective_threshold,
                            template_path: template_path_str.clone(),
                            error: None,
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "[debug_find_supports] row y={:.3} CE verify failed: {e}",
                            row.row_region.y,
                        );
                        DebugSupportCeInfo {
                            region,
                            score: 0.0,
                            passed: false,
                            artwork_checks: Vec::new(),
                            icon_checks: Vec::new(),
                            threshold: support_ce_threshold,
                            template_path: template_path_str.clone(),
                            error: Some(e.to_string()),
                        }
                    }
                };
                Some(info)
            }
            _ => None,
        };
        let mut grand_ces = Vec::new();
        for (index, template_path) in grand_ce_template_paths.iter().enumerate() {
            let Some(template_path) = template_path.as_deref() else {
                continue;
            };
            let Some(region) = grand_ce_search_region(&row, index) else {
                continue;
            };
            let template_path_str = Some(template_path.to_string_lossy().to_string());
            let info = if template_path.is_file() {
                match client.verify_support_ce(
                    Some(&image_path),
                    region,
                    template_path,
                    support_ce_threshold,
                    SupportCeVerificationOptions {
                        mlb_required: grand_craft_essence_mlb_required.unwrap_or([true; 3])[index],
                        grand_bond_ce_mode: if index == 1 {
                            grand_bond_ce_mode.clone().filter(|mode| mode != "any")
                        } else {
                            None
                        },
                        full_gate_threshold: support_ce_full_gate_threshold,
                        mlb_icon_threshold: support_mlb_icon_threshold,
                        bond_icon_threshold: support_bond_icon_threshold,
                    },
                ) {
                    Ok(result) => {
                        let effective_threshold = if result.threshold > 0.0 {
                            result.threshold
                        } else {
                            support_ce_threshold
                        };
                        if !result.artwork_checks.is_empty() {
                            eprintln!(
                                "[debug_find_supports] row y={:.3} Grand CE {} variants: {} | {}",
                                row.row_region.y,
                                index + 1,
                                format_ce_artwork_checks(&result.artwork_checks),
                                runner::format_ce_verification_summary(
                                    &result.artwork_checks,
                                    &result.icon_checks
                                ),
                            );
                        }
                        DebugSupportCeInfo {
                            region,
                            score: result.score,
                            passed: result.passed,
                            artwork_checks: result.artwork_checks,
                            icon_checks: result.icon_checks,
                            threshold: effective_threshold,
                            template_path: template_path_str,
                            error: result.error,
                        }
                    }
                    Err(e) => DebugSupportCeInfo {
                        region,
                        score: 0.0,
                        passed: false,
                        artwork_checks: Vec::new(),
                        icon_checks: Vec::new(),
                        threshold: support_ce_threshold,
                        template_path: template_path_str,
                        error: Some(e.to_string()),
                    },
                }
            } else {
                eprintln!(
                    "[debug_find_supports] grand CE template missing: {}",
                    template_path.display()
                );
                DebugSupportCeInfo {
                    region,
                    score: 0.0,
                    passed: false,
                    artwork_checks: Vec::new(),
                    icon_checks: Vec::new(),
                    threshold: support_ce_threshold,
                    template_path: template_path_str,
                    error: Some("礼装模板不存在".into()),
                }
            };
            grand_ces.push(info);
        }
        let score_filter_reason = score_filter_configured
            .then(|| {
                runner::support_score_filter_mismatch(
                    &row,
                    support_grand_mode,
                    support_star_map_score_min,
                    support_grand_star_map_score_min,
                )
            })
            .flatten();
        let score_filter_passed = score_filter_configured.then_some(score_filter_reason.is_none());
        let level_filter_reason = level_filter_configured
            .then(|| {
                debug_support_level_filter_reason(
                    &row,
                    support_servant_level_min,
                    support_noble_phantasm_level_min,
                    &skill_level_mins,
                    &append_skill_level_mins,
                )
            })
            .flatten();
        let level_filter_passed = level_filter_configured.then_some(level_filter_reason.is_none());
        supports.push(DebugSupportRow {
            row,
            score_filter_passed,
            score_filter_reason,
            level_filter_passed,
            level_filter_reason,
            ce,
            grand_ces,
        });
    }

    Ok(DebugFindSupportsResult {
        supports,
        diagnostics: result.diagnostics,
        score_filter: DebugSupportScoreFilter {
            grand_mode: support_grand_mode,
            star_map_score_min: support_star_map_score_min,
            grand_star_map_score_min: support_grand_star_map_score_min,
        },
        level_filter: DebugSupportLevelFilter {
            servant_level_min: support_servant_level_min,
            noble_phantasm_level_min: support_noble_phantasm_level_min,
            skill_level_mins,
            append_skill_level_mins,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::debug_support_level_filter_reason;
    use crate::screen::{NormRect, Point, SupportRowMatch};

    fn support_row(
        servant_level: Option<u32>,
        np_level: Option<u32>,
        panel: Option<&str>,
        skills: Vec<Option<u32>>,
        append: Vec<Option<u32>>,
    ) -> SupportRowMatch {
        let region = NormRect {
            x: 0.1,
            y: 0.5,
            w: 0.4,
            h: 0.1,
        };
        SupportRowMatch {
            row_region: region,
            tap: Point::new(0.3, 0.55),
            name_text: "哈贝特洛特".into(),
            name_score: 1.0,
            name_matched_name: None,
            name_region: region,
            np_text: "为你纺织的时光之轮等级5".into(),
            np_score: 1.0,
            np_region: region,
            score_anchor: None,
            score_region: None,
            star_map_score: None,
            grand_star_map_score: None,
            score_text: None,
            score_confidence: None,
            np_matched_name: "为你纺织的时光之轮".into(),
            np_level,
            servant_level,
            skill_panel: panel.map(str::to_string),
            skill_levels: skills,
            append_skill_levels: append,
            skill_level_diagnostics: Vec::new(),
            elapsed_ms: None,
        }
    }

    const NO_SKILLS: [Option<u32>; 3] = [None, None, None];
    const NO_APPEND: [Option<u32>; 5] = [None, None, None, None, None];

    #[test]
    fn level_filter_passes_when_every_requirement_is_met() {
        let row = support_row(
            Some(120),
            Some(5),
            Some("owned"),
            vec![Some(10), Some(9), Some(8)],
            vec![],
        );
        let mins: [Option<u32>; 3] = [Some(10), Some(8), None];
        assert_eq!(
            debug_support_level_filter_reason(&row, Some(100), Some(3), &mins, &NO_APPEND),
            None
        );
    }

    #[test]
    fn level_filter_reports_servant_level_and_np_level_gaps() {
        let row = support_row(Some(90), Some(2), None, vec![], vec![]);
        assert_eq!(
            debug_support_level_filter_reason(&row, Some(100), None, &NO_SKILLS, &NO_APPEND),
            Some("从者等级 ≥ 100（实际 90）".into())
        );
        assert_eq!(
            debug_support_level_filter_reason(&row, None, Some(3), &NO_SKILLS, &NO_APPEND),
            Some("宝具 ≥ 3（实际 2）".into())
        );
        let unread = support_row(None, None, None, vec![], vec![]);
        assert_eq!(
            debug_support_level_filter_reason(&unread, Some(100), None, &NO_SKILLS, &NO_APPEND),
            Some("从者等级 ≥ 100（实际 -）".into())
        );
    }

    #[test]
    fn level_filter_needs_the_matching_skill_panel() {
        let row = support_row(
            Some(120),
            Some(5),
            Some("owned"),
            vec![Some(10), Some(8), Some(8)],
            vec![],
        );
        let mins: [Option<u32>; 3] = [Some(10), Some(10), None];
        assert_eq!(
            debug_support_level_filter_reason(&row, None, None, &mins, &NO_APPEND),
            Some("持有技能 2 ≥ 10（实际 8）".into())
        );

        let append_only: [Option<u32>; 5] = [Some(10), None, None, None, None];
        assert_eq!(
            debug_support_level_filter_reason(&row, None, None, &NO_SKILLS, &append_only),
            Some("追加技能面板未展开（切到追加技能面板后再识别）".into())
        );

        let append_row = support_row(
            Some(120),
            Some(5),
            Some("append"),
            vec![],
            vec![Some(10), Some(10), None, None, None],
        );
        assert_eq!(
            debug_support_level_filter_reason(
                &append_row,
                None,
                None,
                &NO_SKILLS,
                &append_only
            ),
            None
        );
    }
}
