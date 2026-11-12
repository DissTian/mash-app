use super::runner::{
    classify_screen, current_level_from_result, decide_main_action, filter_class_point,
    main_shows_empty_selection, parse_bare_level, read_level_from_ocr,
    resolve_current_level_reads, EntryOutcome, MainAction, Screen, SelectionFailure,
    SelectionOutcome, SelectionProgress,
};
use crate::screen::{EnhancementCurrentLevelResult, NormRect, OcrFragment, OcrRegionResult};
use super::*;

#[test]
fn current_level_consensus_requires_two_matching_valid_reads() {
    for level in [1, 37, 90, 150] {
        assert_eq!(resolve_current_level_reads(Ok(level), Ok(level)), Ok(level));
    }
    assert!(resolve_current_level_reads(Ok(37), Ok(38)).unwrap_err().contains("不一致"));
    assert!(resolve_current_level_reads(Err("未找到数字".into()), Ok(37))
        .unwrap_err()
        .contains("不稳定"));
    assert!(resolve_current_level_reads(Err("模型失败".into()), Err("超时".into()))
        .unwrap_err()
        .contains("两次均未识别"));
}

#[test]
fn current_level_result_requires_found_value_inside_bounds() {
    let result = |found, current, text: &str, fail_reason| EnhancementCurrentLevelResult {
        found,
        current,
        text: text.into(),
        fail_reason,
    };
    assert_eq!(current_level_from_result(result(true, Some(37), "37", None)), Ok(37));
    assert!(current_level_from_result(result(false, Some(37), "37/40", Some("ambiguous".into())))
        .unwrap_err()
        .contains("ambiguous"));
    assert!(current_level_from_result(result(true, None, "", None)).is_err());
    for level in [0, 151] {
        assert!(current_level_from_result(result(true, Some(level), "", None)).is_err());
    }
}

#[test]
fn main_action_uses_level_then_brightness_and_routes_without_material_inference() {
    assert_eq!(decide_main_action(80, 80, false, true, false, false), MainAction::Complete);
    assert_eq!(decide_main_action(37, 80, true, true, true, true), MainAction::Enhance);
    assert_eq!(decide_main_action(37, 80, false, false, true, false), MainAction::Grail);
    assert_eq!(decide_main_action(37, 80, false, false, false, true), MainAction::Ascension);
    assert_eq!(decide_main_action(40, 80, false, true, false, true), MainAction::Ascension);
    assert_eq!(decide_main_action(80, 100, false, true, true, false), MainAction::Grail);
    assert_eq!(decide_main_action(37, 80, false, false, false, false), MainAction::Recommend);
    assert_eq!(decide_main_action(37, 80, false, true, false, false), MainAction::SkipUnlitButton);
}

#[test]
fn selection_results_preserve_cancellation_and_real_scroll_count() {
    assert_eq!(SelectionOutcome::Selected.entry_outcome("黄飞虎"), None);
    assert_eq!(
        SelectionOutcome::Cancelled.entry_outcome("黄飞虎"),
        Some(EntryOutcome::Cancelled)
    );
    for (reason, label) in [
        (SelectionFailure::ScrollLimit, "达到列表滑动上限"),
        (SelectionFailure::NavigationLimit, "达到选择页面导航上限"),
        (SelectionFailure::UnknownLimit, "连续多次无法识别选择画面"),
    ] {
        let Some(EntryOutcome::Skipped(message)) =
            (SelectionOutcome::NotFound { reason, scrolls: 4 }).entry_outcome("黄飞虎")
        else {
            panic!("selection failure must retain its skip reason");
        };
        assert!(message.contains(label), "{message}");
        assert!(message.contains("实际滑动 4 次"), "{message}");
        assert!(!message.contains("60"), "{message}");
    }
}

#[test]
fn selection_unknown_limit_requires_consecutive_unrecognized_screens() {
    let mut progress = SelectionProgress::default();
    for _ in 0..16 {
        assert_eq!(progress.observe_screen(Screen::Unknown), None);
    }
    assert_eq!(progress.observe_screen(Screen::Select), None);
    for _ in 0..16 {
        assert_eq!(progress.observe_screen(Screen::Unknown), None);
    }
    assert_eq!(
        progress.observe_screen(Screen::Unknown),
        Some(SelectionOutcome::NotFound {
            reason: SelectionFailure::UnknownLimit,
            scrolls: 0,
        })
    );
}

#[test]
fn selection_scroll_limit_counts_completed_swipes_and_preserves_other_exit_causes() {
    let mut progress = SelectionProgress::default();
    assert_eq!(progress.next_scroll(), Some(1));
    // Planning/rechecking a swipe does not count it as completed.
    assert_eq!(progress.next_scroll(), Some(1));
    for scroll in 1..=60 {
        assert_eq!(progress.next_scroll(), Some(scroll));
        progress.record_scroll();
        if scroll == 4 {
            assert_eq!(
                progress.failure(SelectionFailure::NavigationLimit),
                SelectionOutcome::NotFound {
                    reason: SelectionFailure::NavigationLimit,
                    scrolls: 4,
                }
            );
        }
    }
    assert_eq!(progress.next_scroll(), None);
    assert_eq!(
        progress.failure(SelectionFailure::ScrollLimit),
        SelectionOutcome::NotFound { reason: SelectionFailure::ScrollLimit, scrolls: 60 }
    );
}

#[test]
fn natural_caps_follow_rarity() {
    assert_eq!(natural_max_for_rarity(5), 90);
    assert_eq!(natural_max_for_rarity(4), 80);
    assert_eq!(natural_max_for_rarity(3), 70);
    assert_eq!(natural_max_for_rarity(2), 65);
    assert_eq!(natural_max_for_rarity(1), 60);
    assert_eq!(natural_max_for_rarity(0), 60);
}

#[test]
fn levels_are_clamped_to_supported_range() {
    assert_eq!(normalize_level(0), 1);
    assert_eq!(normalize_level(1), 1);
    assert_eq!(normalize_level(90), 90);
    assert_eq!(normalize_level(120), 120);
    assert_eq!(normalize_level(121), 120);
}

#[test]
fn lifecycle_transitions_are_guarded() {
    use super::ServantEnhancementLifecycleEvent;
    use super::ServantEnhancementRunnerState;

    let started = servant_enhancement_lifecycle_transition(
        ServantEnhancementRunnerState::Starting,
        ServantEnhancementLifecycleEvent::WorkerStarted,
    );
    assert!(started.accepted);
    assert_eq!(started.next, ServantEnhancementRunnerState::Running);

    let finished = servant_enhancement_lifecycle_transition(
        ServantEnhancementRunnerState::Running,
        ServantEnhancementLifecycleEvent::Finished,
    );
    assert!(finished.accepted);
    assert_eq!(finished.next, ServantEnhancementRunnerState::Finished);

    let stopped = servant_enhancement_lifecycle_transition(
        ServantEnhancementRunnerState::Running,
        ServantEnhancementLifecycleEvent::StopRequested,
    );
    assert!(stopped.accepted);
    assert_eq!(stopped.next, ServantEnhancementRunnerState::Finished);

    let failed = servant_enhancement_lifecycle_transition(
        ServantEnhancementRunnerState::Running,
        ServantEnhancementLifecycleEvent::Failed {
            message: "no device".into(),
        },
    );
    assert!(failed.accepted);
    assert_eq!(
        failed.next,
        ServantEnhancementRunnerState::Error {
            message: "no device".into()
        }
    );

    let ignored = servant_enhancement_lifecycle_transition(
        ServantEnhancementRunnerState::Idle,
        ServantEnhancementLifecycleEvent::WorkerStarted,
    );
    assert!(!ignored.accepted);
    assert_eq!(ignored.next, ServantEnhancementRunnerState::Idle);
}

#[test]
fn entry_terminal_states_are_terminal_only_after_completion_skip_or_failure() {
    assert!(!EntryStatus::Pending.is_terminal());
    assert!(!EntryStatus::Running.is_terminal());
    assert!(EntryStatus::Completed.is_terminal());
    assert!(EntryStatus::Skipped.is_terminal());
    assert!(EntryStatus::Failed.is_terminal());
}

#[test]
fn entry_state_starts_pending_and_copies_settings() {
    let entry = ServantEnhancementEntry {
        id: "entry-1".into(),
        servant_id: 100100,
        variant_key: "100100:1".into(),
        name_cn: "阿尔托莉雅·Caster".into(),
        class_name: "caster".into(),
        rarity: 5,
        target_level: 100,
        two_exp: true,
        face_key: Some("face_servant_3".into()),
    };
    let state = ServantEnhancementEntryState::from_entry(&entry);
    assert_eq!(state.status, EntryStatus::Pending);
    assert_eq!(state.target_level, 100);
    assert!(state.two_exp);
    assert_eq!(state.face_key.as_deref(), Some("face_servant_3"));
    assert_eq!(state.current_level, None);
    assert!(state.message.is_empty());
}

#[test]
fn cn_screens_are_classified_from_ocr_text() {
    assert_eq!(classify_screen("推荐素材选择 将自动选择符合条件的所持从者（经验值）"), Screen::Recommend);
    assert_eq!(classify_screen("圣杯转临 是否进行「圣杯转临」？ 取消 决定"), Screen::GrailConfirm);
    assert_eq!(classify_screen("圣杯转临结果 圣杯转临所必需的道具 强化"), Screen::Grail);
    assert_eq!(classify_screen("灵基再临结果 灵基再临必要道具 强化"), Screen::Ascension);
    assert_eq!(classify_screen("已到达等级上限 请点击游戏界面"), Screen::Result);
    assert_eq!(classify_screen("筛选设定 仅在列表中显示选择的项目 稀有度 职阶 决定"), Screen::Filter);
    assert_eq!(classify_screen("从者强化 包含★3以上的素材！ 是否进行强化合成？ 取消 决定"), Screen::EnhanceConfirm);
    assert_eq!(classify_screen("请选择想要强化的从者 持有从者 378/1000 等级顺序"), Screen::Select);
    assert_eq!(classify_screen("强化用素材 请选择想在强化时使用的卡牌"), Screen::Main);
    assert_eq!(classify_screen("从者技能强化 概念礼装强化&进化"), Screen::EnhanceMenu);
    assert_eq!(classify_screen("个人空间 召唤 商店 强化"), Screen::HomeOpen);
    assert_eq!(classify_screen("通知 活动报酬 菜单"), Screen::HomeClosed);
    assert_eq!(classify_screen("无法识别的画面"), Screen::Unknown);
}

#[test]
fn level_pair_parser_validates_cn_levels() {
    for (text, expected) in [
        ("等级 1 / 40", (1, 40)),
        ("等级90/90", (90, 90)),
        ("等级 100 / 120", (100, 120)),
        ("120/120", (120, 120)),
        ("等级1/1 等级1/1 等级 40 / 50", (40, 50)),
        ("1/40 1/40", (1, 40)),
    ] {
        assert_eq!(parse_bare_level(text), Some(expected), "{text}");
    }
    for text in [
        "等级 50 / 40", "等级1/1", "等级 7 / 528",
        "0/40", "1/151", "等级 HP75 ATK90", "等级1\nHP40",
        "1/40,000", "1/40.5", "1,1/40", "1/40/50",
    ] {
        assert_eq!(parse_bare_level(text), None, "{text}");
    }
}

#[test]
fn empty_main_page_is_detected_before_reselect() {
    assert!(main_shows_empty_selection("请先选择想要强化的从者"));
    assert!(main_shows_empty_selection("点击选择 想要强化的从者"));
    assert!(!main_shows_empty_selection("请点击「强化」按钮 强化结果 等级 1 / 40"));
    assert!(!main_shows_empty_selection("重新选择从者 强化结果 等级 40 / 40"));
}

#[test]
fn cn_anchor_template_is_full_width_at_reference_scale() {
    // The grid inference uses the anchor's left edge as the card's left edge.
    // A text-only crop shifts every cell right; the CN template must span the
    // full bottom bar and be authored at the 2560 reference width.
    let png = include_bytes!("../../resources/servers/cn/templates/text_servant_avatar_bottom_line.png");
    assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    assert!(width > 200, "anchor template must span the card width, got {width}");
    assert!(height < 60, "anchor template should stay a bottom-line strip, got {height}");
}

#[test]
fn filter_class_point_accepts_catalog_class_names() {
    assert!(filter_class_point("Rider").is_some());
    assert!(filter_class_point("saber").is_some());
    assert!(filter_class_point("骑兵").is_some());
    assert!(filter_class_point("MoonCancer").is_some());
    assert!(filter_class_point("unknown").is_none());
}

#[test]
fn bare_level_parser_handles_level_region_without_label() {
    assert_eq!(parse_bare_level("1 / 40"), Some((1, 40)));
    assert_eq!(parse_bare_level("1/40"), Some((1, 40)));
    assert_eq!(parse_bare_level("等级 1 / 40"), Some((1, 40)));
    assert_eq!(parse_bare_level("强化 0/1,000"), None);
    assert_eq!(parse_bare_level("等级1/1 等级1/1"), None);
    assert_eq!(parse_bare_level("37/40 1/40"), None);
    assert_eq!(parse_bare_level("7 / 528"), None);
}

fn level_ocr(fragments: Vec<(&str, f64, f64, f64, f64)>) -> OcrRegionResult {
    OcrRegionResult {
        full_text: fragments.iter().map(|f| f.0).collect::<Vec<_>>().join("\n"),
        fragments: fragments
            .into_iter()
            .map(|(text, x, y, w, h)| OcrFragment {
                text: text.into(),
                region: NormRect { x, y, w, h },
                ocr_confidence: 0.99,
            })
            .collect(),
    }
}

#[test]
fn current_level_uses_position_and_preserves_fragment_boundaries() {
    let ocr = level_ocr(vec![
        ("等级1/1", 0.34, 0.27, 0.03, 0.02),
        ("等级提升!90/90", 0.65, 0.63, 0.15, 0.06),
        ("1/40", 0.35, 0.63, 0.10, 0.06),
        ("1234", 0.50, 0.73, 0.06, 0.04),
    ]);
    assert_eq!(read_level_from_ocr(&ocr), Some((1, 40)));
    let only_preview = level_ocr(vec![("等级90/90", 0.65, 0.63, 0.15, 0.06)]);
    assert_eq!(read_level_from_ocr(&only_preview), None);
    let text_without_position = OcrRegionResult {
        full_text: "等级90/90".into(), fragments: vec![],
    };
    assert_eq!(read_level_from_ocr(&text_without_position), None);
}

#[test]
fn current_level_accepts_full_width_digits_and_bbox_edge_overlap() {
    let ocr = level_ocr(vec![("等级 １／４０", 0.305, 0.60, 0.15, 0.11)]);
    assert_eq!(read_level_from_ocr(&ocr), Some((1, 40)));
}

#[test]
fn current_level_joins_split_boxes_only_on_the_same_row() {
    let ocr = level_ocr(vec![
        ("40", 0.40, 0.63, 0.05, 0.06),
        ("等级", 0.305, 0.635, 0.045, 0.05),
        ("40/", 0.35, 0.63, 0.06, 0.06),
    ]);
    assert_eq!(read_level_from_ocr(&ocr), Some((40, 40)));
    let split = level_ocr(vec![
        ("40", 0.40, 0.63, 0.05, 0.06),
        ("/", 0.39, 0.63, 0.01, 0.06),
        ("1", 0.37, 0.63, 0.015, 0.06),
    ]);
    assert_eq!(read_level_from_ocr(&split), Some((1, 40)));
    let different_rows = level_ocr(vec![
        ("1", 0.37, 0.615, 0.02, 0.025),
        ("/40", 0.39, 0.675, 0.05, 0.025),
    ]);
    assert_eq!(read_level_from_ocr(&different_rows), None);
}

#[test]
fn current_level_rejects_conflicts_and_invalid_pairs() {
    for text in ["0/40", "41/40", "1/1", "1/151", "1/40,000", "1/40.5", "401 40"] {
        assert_eq!(read_level_from_ocr(&level_ocr(vec![(text, 0.35, 0.63, 0.10, 0.06)])), None, "{text}");
    }
    let duplicates = level_ocr(vec![
        ("1/40", 0.35, 0.63, 0.10, 0.06),
        ("1/40", 0.35, 0.63, 0.10, 0.06),
    ]);
    assert_eq!(read_level_from_ocr(&duplicates), Some((1, 40)));
    let conflicts = level_ocr(vec![
        ("1/40", 0.35, 0.63, 0.10, 0.06),
        ("2/40", 0.35, 0.63, 0.10, 0.06),
    ]);
    assert_eq!(read_level_from_ocr(&conflicts), None);
}

#[test]
fn current_level_rejects_truncated_numbers_in_adjacent_boxes() {
    let prefix = level_ocr(vec![
        ("10", 0.35, 0.63, 0.025, 0.06),
        ("1/120", 0.38, 0.63, 0.07, 0.06),
    ]);
    assert_eq!(read_level_from_ocr(&prefix), None);
    let suffix = level_ocr(vec![
        ("10/12", 0.35, 0.63, 0.07, 0.06),
        ("0", 0.425, 0.63, 0.015, 0.06),
    ]);
    assert_eq!(read_level_from_ocr(&suffix), None);
    let split_current = level_ocr(vec![
        ("1", 0.35, 0.63, 0.01, 0.06),
        ("2", 0.365, 0.63, 0.01, 0.06),
        ("/", 0.38, 0.63, 0.01, 0.06),
        ("40", 0.395, 0.63, 0.04, 0.06),
    ]);
    assert_eq!(read_level_from_ocr(&split_current), None);
    let split_in_one_box = level_ocr(vec![("1 2 / 40", 0.35, 0.63, 0.10, 0.06)]);
    assert_eq!(read_level_from_ocr(&split_in_one_box), None);
    let split_cap = level_ocr(vec![
        ("1/", 0.35, 0.63, 0.025, 0.06),
        ("40", 0.38, 0.63, 0.025, 0.06),
        ("0", 0.41, 0.63, 0.01, 0.06),
    ]);
    assert_eq!(read_level_from_ocr(&split_cap), None);
}

#[test]
fn post_enhancement_checks_use_current_level_instead_of_preview() {
    let before = level_ocr(vec![
        ("1/40", 0.35, 0.63, 0.10, 0.06),
        ("40/40", 0.70, 0.63, 0.10, 0.06),
    ]);
    let preview_changed = level_ocr(vec![
        ("1/40", 0.35, 0.63, 0.10, 0.06),
        ("39/40", 0.70, 0.63, 0.10, 0.06),
    ]);
    let after = level_ocr(vec![("2/40", 0.35, 0.63, 0.10, 0.06)]);
    assert_eq!(read_level_from_ocr(&before), read_level_from_ocr(&preview_changed));
    assert_ne!(read_level_from_ocr(&before), read_level_from_ocr(&after));
}

#[test]
fn cn_screenshot_ocr_responses_read_levels_at_both_resolutions() {
    #[derive(serde::Deserialize)]
    struct Case {
        screenshot: String,
        width: u32,
        expected: (u32, u32),
        full: OcrRegionResult,
        crop: OcrRegionResult,
    }
    // PP-OCRv6 responses from the saved CN main-screen samples, including
    // selected materials, the right-hand preview and capped/uncapped levels.
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/level_ocr_cn.json")).unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        if case.expected == (40, 40) {
            // The crop OCR reads 401 + 40; never invent the missing slash.
            assert_eq!(read_level_from_ocr(&case.crop), None);
        }
        let level = read_level_from_ocr(&case.full).or_else(|| read_level_from_ocr(&case.crop));
        assert_eq!(level, Some(case.expected), "{} at {}px", case.screenshot, case.width);
    }
}
