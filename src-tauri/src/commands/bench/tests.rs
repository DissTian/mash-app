use super::cases::{
    case_by_id, preset_by_id, preset_contains_device_input, summary, CASES, PRESETS,
};

#[test]
fn case_ids_are_unique_and_resolvable() {
    let mut seen = std::collections::HashSet::new();
    for case in CASES {
        assert!(
            seen.insert(case.id),
            "duplicate bench case id: {}",
            case.id
        );
        assert!(
            case_by_id(case.id).is_some(),
            "bench case cannot resolve itself: {}",
            case.id
        );
    }
}

#[test]
fn presets_only_reference_known_cases() {
    for preset in PRESETS {
        assert!(!preset.case_ids.is_empty(), "empty preset: {}", preset.id);
        for id in preset.case_ids {
            assert!(
                case_by_id(id).is_some(),
                "preset {} references unknown case {}",
                preset.id,
                id
            );
        }
        assert!(
            preset_by_id(preset.id).is_some(),
            "bench preset cannot resolve itself: {}",
            preset.id
        );
    }
}

#[test]
fn basic_preset_never_runs_device_input() {
    let preset = preset_by_id("basic.performance").expect("basic preset missing");
    assert!(!preset_contains_device_input(preset));
    assert!(preset.case_ids.contains(&"ocr.full_frame"));
    assert!(preset.case_ids.contains(&"ocr.full_frame_file"));
}

#[test]
fn device_input_preset_is_flagged_for_confirmation() {
    let preset = preset_by_id("device.input").expect("device input preset missing");
    assert!(preset_contains_device_input(preset));
}

#[test]
fn summary_uses_nearest_rank_percentile_and_median() {
    assert_eq!(summary(&[]), (0.0, 0.0, 0.0));
    assert_eq!(summary(&[7.5]), (7.5, 7.5, 7.5));

    let (min, median, p95) = summary(&[10.0, 20.0, 30.0, 40.0]);
    assert_eq!(min, 10.0);
    assert_eq!(median, 25.0);
    assert_eq!(p95, 40.0);

    let (_, median, p95) = summary(&[1.0, 2.0, 3.0]);
    assert_eq!(median, 2.0);
    assert_eq!(p95, 3.0);
}
