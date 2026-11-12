//! CN multi-servant enhancement runner.
//!
//! Screen classification is OCR-only on purpose: the CN CV catalogue has no
//! enhancement screens/templates yet, and every transition this runner needs
//! is driven by stable Chinese labels plus the bottom-right 强化 button's
//! bright/dim state.

use super::*;
use crate::adb::Adb;
use crate::runner::LogLevel;
use crate::screen::{
    EnhancementCurrentLevelResult, NormRect, OcrRegionResult, Point, SidecarClient,
};
use crate::touch::{self, TouchBackend};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use unicode_normalization::UnicodeNormalization;

const SCREEN_HOME_CLOSED: &str = "Home";
const SCREEN_HOME_OPEN: &str = "Menu";
const SCREEN_ENHANCE_MENU: &str = "EnhanceMenu";
const SCREEN_MAIN: &str = "ServantEnhance";
const SCREEN_SELECT: &str = "ServantSelect";
const SCREEN_RECOMMEND: &str = "RecommendDialog";
const SCREEN_RESULT: &str = "EnhanceResult";
const SCREEN_OVERFLOW: &str = "OverflowDialog";
const SCREEN_ASCENSION: &str = "Ascension";
const SCREEN_GRAIL: &str = "Grail";
const SCREEN_GRAIL_CONFIRM: &str = "GrailConfirm";
const SCREEN_UNKNOWN: &str = "Unknown";

const FULL_SCREEN: NormRect = NormRect {
    x: 0.0,
    y: 0.0,
    w: 1.0,
    h: 1.0,
};
// Include the complete CN level row, including the slash and digit baseline.
// The old crop ended at y=0.69 and truncated glyphs on 1920×1080 frames.
#[cfg(test)]
const LEVEL_REGION: NormRect = NormRect {
    x: 0.30,
    y: 0.61,
    w: 0.16,
    h: 0.10,
};
const BUTTON_LUMA_REGION: NormRect = NormRect {
    x: 0.833,
    y: 0.895,
    w: 0.035,
    h: 0.06,
};
const SERVANT_LIST_REGION: NormRect = NormRect {
    x: 0.05,
    y: 0.18,
    w: 0.78,
    h: 0.76,
};
pub(super) const SERVANT_FACE_MATCH_CROP: NormRect = NormRect {
    x: 0.195,
    y: 0.265,
    w: 0.805,
    h: 0.438,
};
/// The 2EXP badge is painted across the face centre, which is exactly where
/// `SERVANT_FACE_MATCH_CROP` looks, so masking that band leaves almost nothing
/// to match. The card's bottom-left corner stays clean, so 2EXP entries compare
/// only that corner: the correct card scores 0.97 there while the runner-up
/// card stays at 0.46, versus 0.19/0.27 with the masked centre band.
pub(super) const TWO_EXP_FACE_MATCH_CROP: NormRect = NormRect {
    x: 0.0,
    y: 0.66,
    w: 0.30,
    h: 0.34,
};

const HOME_MENU_BUTTON: Point = Point::new(0.90, 0.90);
const HOME_CLOSE_BUTTON: Point = Point::new(0.935, 0.655);
const HOME_STRENGTHEN_BUTTON: Point = Point::new(0.345, 0.87);
const ENHANCE_SERVANT_ENTRY: Point = Point::new(0.75, 0.23);
const SERVANT_SELECT_PLUS: Point = Point::new(0.15, 0.50);
const SERVANT_RESELECT_BUTTON: Point = Point::new(0.18, 0.05);
const RECOMMEND_BUTTON: Point = Point::new(0.78, 0.21);
const RECOMMEND_AUTO_SWITCH: Point = Point::new(0.622, 0.604);
const RECOMMEND_EXECUTE: Point = Point::new(0.62, 0.74);
const RECOMMEND_CLOSE: Point = Point::new(0.29, 0.74);
// Bottom-right 强化 button center (spans x 0.80-0.99, y 0.87-0.97);
// the old 0.833/0.833 point sat above the button and could hit the
// material/QP area instead.
const ENHANCE_BUTTON: Point = Point::new(0.89, 0.91);
// Entry buttons sit on the right side of the "level capped" panel; the old
// 0.755/0.565 point landed on the orange explanatory text, not the button.
const ASCENSION_ENTRY: Point = Point::new(0.83, 0.79);
const GRAIL_ENTRY: Point = Point::new(0.83, 0.79);
const ASCENSION_BACK_TO_SERVANT: Point = Point::new(0.78, 0.40);
const GRAIL_BACK_TO_SERVANT: Point = Point::new(0.82, 0.40);
const TOP_LEFT_BACK: Point = Point::new(0.07, 0.05);
const GRAIL_CONFIRM_DECIDE: Point = Point::new(0.676, 0.805);
const GRAIL_CONFIRM_CANCEL: Point = Point::new(0.31, 0.805);
// Confirmation shown when auto-selected EXP cards include 3-star or above.
const ENHANCE_CONFIRM_DECIDE: Point = Point::new(0.641, 0.805);
const ENHANCE_CONFIRM_CANCEL: Point = Point::new(0.36, 0.805);
const OVERFLOW_CLOSE: Point = Point::new(0.50, 0.772);
// Advance dialogue/result screens at the very bottom centre. Tapping the
// screen centre can land on a material slot after an ascension/grail
// transition and open the material picker by accident.
const BOTTOM_TAP: Point = Point::new(0.50, 0.95);
// The servant grid itself does not accept a drag on a card (the game treats
// it as a tap), so both gestures run along the scrollbar track on the right
// edge of the list. X=0.80 is the track; Y stays inside the visible list.
const LIST_SCROLL_FROM: Point = Point::new(0.50, 0.86);
const LIST_SCROLL_TO: Point = Point::new(0.50, 0.30);
const LIST_RESET_FROM: Point = Point::new(0.50, 0.30);
const LIST_RESET_TO: Point = Point::new(0.50, 0.86);

// Filter dialog opened by 筛选 on the servant list. The dialog is a fixed
// 1920x1080 layout; rarity and class are radio-like rows.
const FILTER_OPEN: Point = Point::new(0.772, 0.148);
const FILTER_RESET: Point = Point::new(0.14, 0.88);
const FILTER_APPLY: Point = Point::new(0.86, 0.87);
const FILTER_CANCEL: Point = Point::new(0.66, 0.87);
const FILTER_RARITY_5: Point = Point::new(0.207, 0.343);
const FILTER_RARITY_4: Point = Point::new(0.361, 0.343);
const FILTER_RARITY_3: Point = Point::new(0.517, 0.343);
const FILTER_RARITY_2: Point = Point::new(0.673, 0.343);
const FILTER_RARITY_1: Point = Point::new(0.828, 0.343);
const FILTER_CLASS_SABER: Point = Point::new(0.207, 0.548);
const FILTER_CLASS_ARCHER: Point = Point::new(0.361, 0.548);
const FILTER_CLASS_LANCER: Point = Point::new(0.517, 0.548);
const FILTER_CLASS_RIDER: Point = Point::new(0.673, 0.548);
const FILTER_CLASS_CASTER: Point = Point::new(0.828, 0.548);
const FILTER_CLASS_ASSASSIN: Point = Point::new(0.207, 0.631);
const FILTER_CLASS_BERSERKER: Point = Point::new(0.361, 0.631);
const FILTER_CLASS_SHIELDER: Point = Point::new(0.517, 0.631);
const FILTER_CLASS_RULER: Point = Point::new(0.673, 0.631);
const FILTER_CLASS_AVENGER: Point = Point::new(0.828, 0.631);
const FILTER_CLASS_MOONCANCER: Point = Point::new(0.207, 0.713);
const FILTER_CLASS_ALTEREGO: Point = Point::new(0.361, 0.713);
const FILTER_CLASS_FOREIGNER: Point = Point::new(0.517, 0.713);
const FILTER_CLASS_PRETENDER: Point = Point::new(0.673, 0.713);
const FILTER_CLASS_BEAST: Point = Point::new(0.828, 0.713);

const SELECT_MAX_SCROLLS: u32 = 60;
const NAV_MAX_TICKS: u32 = 70;
const UNKNOWN_MAX_TICKS: u32 = 16;
const LEVEL_CONFIRM_INTERVAL: Duration = Duration::from_millis(250);
const ANIMATION_TIMEOUT: Duration = Duration::from_secs(75);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Screen {
    HomeClosed,
    HomeOpen,
    EnhanceMenu,
    Main,
    Select,
    Filter,
    Recommend,
    Result,
    Overflow,
    EnhanceConfirm,
    Ascension,
    Grail,
    GrailConfirm,
    Unknown,
}

impl Screen {
    fn label(self) -> &'static str {
        match self {
            Self::HomeClosed => SCREEN_HOME_CLOSED,
            Self::HomeOpen => SCREEN_HOME_OPEN,
            Self::EnhanceMenu => SCREEN_ENHANCE_MENU,
            Self::Main => SCREEN_MAIN,
            Self::Select => SCREEN_SELECT,
            Self::Filter => "Filter",
            Self::Recommend => SCREEN_RECOMMEND,
            Self::Result => SCREEN_RESULT,
            Self::Overflow => SCREEN_OVERFLOW,
            Self::EnhanceConfirm => "EnhanceConfirm",
            Self::Ascension => SCREEN_ASCENSION,
            Self::Grail => SCREEN_GRAIL,
            Self::GrailConfirm => SCREEN_GRAIL_CONFIRM,
            Self::Unknown => SCREEN_UNKNOWN,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum EntryOutcome {
    Completed,
    Skipped(String),
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CurrentLevelRead {
    Confirmed(u32),
    Unknown(String),
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MainAction {
    Complete,
    Enhance,
    Grail,
    Ascension,
    Recommend,
    SkipUnlitButton,
}

pub(super) fn resolve_current_level_reads(
    first: Result<u32, String>,
    second: Result<u32, String>,
) -> Result<u32, String> {
    match (first, second) {
        (Ok(first), Ok(second)) if first == second => Ok(first),
        (Ok(first), Ok(second)) => Err(format!("两次读数不一致：{first} 与 {second}")),
        (Err(first), Err(second)) => Err(format!("两次均未识别：{first}；{second}")),
        (Err(reason), Ok(level)) | (Ok(level), Err(reason)) => {
            Err(format!("读数不稳定：{reason}；另一帧为 {level}"))
        }
    }
}

pub(super) fn current_level_from_result(
    result: EnhancementCurrentLevelResult,
) -> Result<u32, String> {
    let describe_failure = |fallback: &str| {
        match (result.fail_reason.as_deref(), result.text.is_empty()) {
            (Some(reason), false) => format!("{fallback}：{reason}（识别文本：{:?}）", result.text),
            (Some(reason), true) => format!("{fallback}：{reason}"),
            (None, false) => format!("{fallback}（识别文本：{:?}）", result.text),
            (None, true) => fallback.to_string(),
        }
    };
    if !result.found {
        return Err(describe_failure("未识别当前等级"));
    }
    let Some(current) = result.current else {
        return Err(describe_failure("识别结果缺少当前等级"));
    };
    if !(1..=150).contains(&current) {
        return Err(format!("当前等级超出有效范围：{current}"));
    }
    Ok(current)
}

pub(super) fn decide_main_action(
    current: u32,
    target: u32,
    button_bright: bool,
    recommendation_attempted: bool,
    go_grail: bool,
    go_ascension: bool,
) -> MainAction {
    if current >= target {
        MainAction::Complete
    } else if button_bright {
        MainAction::Enhance
    } else if go_grail {
        MainAction::Grail
    } else if go_ascension {
        MainAction::Ascension
    } else if recommendation_attempted {
        MainAction::SkipUnlitButton
    } else {
        MainAction::Recommend
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectionFailure {
    ScrollLimit,
    NavigationLimit,
    UnknownLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SelectionOutcome {
    Selected,
    NotFound { reason: SelectionFailure, scrolls: u32 },
    Cancelled,
}

impl SelectionOutcome {
    pub(super) fn entry_outcome(self, name: &str) -> Option<EntryOutcome> {
        match self {
            Self::Selected => None,
            Self::Cancelled => Some(EntryOutcome::Cancelled),
            Self::NotFound { reason, scrolls } => {
                let reason = match reason {
                    SelectionFailure::ScrollLimit => "达到列表滑动上限",
                    SelectionFailure::NavigationLimit => "达到选择页面导航上限",
                    SelectionFailure::UnknownLimit => "连续多次无法识别选择画面",
                };
                Some(EntryOutcome::Skipped(format!(
                    "未在从者列表中找到{name}（{reason}，实际滑动 {scrolls} 次）"
                )))
            }
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct SelectionProgress {
    pub(super) scrolls: u32,
    consecutive_unknown: u32,
}

impl SelectionProgress {
    pub(super) fn observe_screen(&mut self, screen: Screen) -> Option<SelectionOutcome> {
        if screen == Screen::Unknown {
            self.consecutive_unknown += 1;
        } else {
            self.consecutive_unknown = 0;
        }
        (self.consecutive_unknown > UNKNOWN_MAX_TICKS)
            .then(|| self.failure(SelectionFailure::UnknownLimit))
    }

    pub(super) fn next_scroll(&self) -> Option<u32> {
        (self.scrolls < SELECT_MAX_SCROLLS).then_some(self.scrolls + 1)
    }

    pub(super) fn record_scroll(&mut self) {
        self.scrolls += 1;
    }

    pub(super) fn failure(&self, reason: SelectionFailure) -> SelectionOutcome {
        SelectionOutcome::NotFound { reason, scrolls: self.scrolls }
    }
}

pub struct ServantEnhancementRunner {
    touch: Box<dyn TouchBackend>,
    sidecar: Option<SidecarClient>,
    sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    app: tauri::AppHandle,
    state: Arc<Mutex<ServantEnhancementRunnerState>>,
    entries: Arc<Mutex<Vec<ServantEnhancementEntryState>>>,
    current_entry_id: Arc<Mutex<Option<String>>>,
    cancel: Arc<AtomicBool>,
    screen_w: u32,
    screen_h: u32,
    targets: Vec<ServantEnhancementTarget>,
    template_size: (u32, u32),
    last_level_emit: Option<(String, u32)>,
    last_level_failure_emit: Option<String>,
}

impl ServantEnhancementRunner {
    pub fn new(
        adb: Adb,
        sidecar: SidecarClient,
        app: tauri::AppHandle,
        state: Arc<Mutex<ServantEnhancementRunnerState>>,
        entries: Arc<Mutex<Vec<ServantEnhancementEntryState>>>,
        current_entry_id: Arc<Mutex<Option<String>>>,
        cancel: Arc<AtomicBool>,
        screen_size: (u32, u32),
        targets: Vec<ServantEnhancementTarget>,
        sidecar_cache: Option<Arc<Mutex<Option<SidecarClient>>>>,
    ) -> Self {
        let touch = touch::build(&adb, &sidecar, &app, screen_size);
        let template_size = (
            ((screen_size.0 as f64 * 223.0 / 2560.0).round() as u32).max(64),
            ((screen_size.1 as f64 * 223.0 / 1440.0).round() as u32).max(64),
        );
        Self {
            touch,
            sidecar: Some(sidecar),
            sidecar_cache,
            app,
            state,
            entries,
            current_entry_id,
            cancel,
            screen_w: screen_size.0,
            screen_h: screen_size.1,
            targets,
            template_size,
            last_level_emit: None,
            last_level_failure_emit: None,
        }
    }

    pub fn run(mut self) {
        self.transition(ServantEnhancementLifecycleEvent::WorkerStarted);
        self.emit("", "从者强化队列已启动", LogLevel::Info);

        for index in 0..self.targets.len() {
            if self.is_cancelled() {
                self.transition(ServantEnhancementLifecycleEvent::StopRequested);
                self.emit("", "已停止从者强化队列", LogLevel::Info);
                return;
            }
            match self.run_entry(index) {
                Ok(EntryOutcome::Cancelled) => {
                    self.transition(ServantEnhancementLifecycleEvent::StopRequested);
                    self.emit("", "已停止从者强化队列", LogLevel::Info);
                    return;
                }
                Ok(_) => {}
                Err(message) => {
                    self.fail(&message);
                    return;
                }
            }
        }
        if self.is_cancelled() {
            self.transition(ServantEnhancementLifecycleEvent::StopRequested);
            self.emit("", "已停止从者强化队列", LogLevel::Info);
            return;
        }

        let (done, skipped) = {
            let entries = self.entries.lock().unwrap();
            (
                entries
                    .iter()
                    .filter(|entry| entry.status == EntryStatus::Completed)
                    .count(),
                entries
                    .iter()
                    .filter(|entry| entry.status == EntryStatus::Skipped)
                    .count(),
            )
        };
        if let Err(message) = self.return_to_home() {
            self.emit("", &format!("强化完成，但返回主页失败: {message}"), LogLevel::Warn);
        }
        if self.is_cancelled() {
            self.transition(ServantEnhancementLifecycleEvent::StopRequested);
            self.emit("", "已停止从者强化队列", LogLevel::Info);
            return;
        }
        self.transition(ServantEnhancementLifecycleEvent::Finished);
        self.emit(
            "",
            &format!("从者强化队列处理完成：完成 {done}，跳过 {skipped}"),
            LogLevel::Info,
        );
    }

    /// Back out of the enhancement flow so the game is left on the home screen
    /// after the queue finishes, matching where the task started.
    fn return_to_home(&mut self) -> Result<(), String> {
        for _ in 0..NAV_MAX_TICKS {
            if self.is_cancelled() {
                return Ok(());
            }
            let (screen, _) = self.detect()?;
            match screen {
                Screen::HomeClosed => return Ok(()),
                Screen::HomeOpen => self.navigate(HOME_CLOSE_BUTTON, "关闭菜单")?,
                Screen::EnhanceMenu => self.navigate(TOP_LEFT_BACK, "关闭强化菜单")?,
                Screen::Main => self.navigate(TOP_LEFT_BACK, "返回强化菜单")?,
                Screen::Ascension | Screen::Grail => {
                    self.navigate(TOP_LEFT_BACK, "返回强化菜单")?
                }
                Screen::Select => self.navigate(TOP_LEFT_BACK, "关闭从者选择")?,
                Screen::Recommend => self.navigate(RECOMMEND_CLOSE, "关闭推荐选择")?,
                Screen::Result | Screen::Overflow => self.handle_result_screen(screen)?,
                Screen::GrailConfirm => {
                    self.navigate(GRAIL_CONFIRM_CANCEL, "取消圣杯转临")?
                }
                Screen::EnhanceConfirm => {
                    self.navigate(ENHANCE_CONFIRM_CANCEL, "取消强化确认")?
                }
                Screen::Filter => self.navigate(FILTER_CANCEL, "关闭筛选")?,
                Screen::Unknown => self.sleep(500),
            }
        }
        Ok(())
    }

    fn run_entry(&mut self, index: usize) -> Result<EntryOutcome, String> {
        let target = self.targets[index].clone();
        self.set_current_entry(&target.entry_id, Some(&target.name_cn));
        self.update_entry(&target.entry_id, |entry| {
            entry.status = EntryStatus::Running;
            entry.message = "准备选择从者".into();
        });
        self.emit(
            SCREEN_SELECT,
            &format!("开始处理 {}（目标 Lv.{}）", target.name_cn, target.target_level),
            LogLevel::Info,
        );

        let selection = self.select_target(&target);
        let outcome = if self.is_cancelled() {
            EntryOutcome::Cancelled
        } else if let Some(outcome) = selection?.entry_outcome(&target.name_cn) {
            outcome
        } else {
            let result = self.drive_entry(&target);
            if self.is_cancelled() {
                EntryOutcome::Cancelled
            } else {
                result?
            }
        };
        match &outcome {
            EntryOutcome::Completed => self.complete_entry(&target),
            EntryOutcome::Skipped(reason) => self.skip_entry(&target, reason.clone()),
            EntryOutcome::Cancelled => {
                self.update_entry(&target.entry_id, |entry| {
                    entry.status = EntryStatus::Pending;
                    entry.message = "已停止".into();
                });
                self.set_current_entry("", None);
            }
        }
        Ok(outcome)
    }

    /// Navigate from wherever the game is to the target-selection list and
    /// pick the requested servant by face template.
    fn select_target(&mut self, target: &ServantEnhancementTarget) -> Result<SelectionOutcome, String> {
        let mut progress = SelectionProgress::default();
        let mut list_prepared = false;
        let mut filter_applied = false;
        for _ in 0..NAV_MAX_TICKS {
            if self.is_cancelled() {
                return Ok(SelectionOutcome::Cancelled);
            }
            let (screen, ocr) = self.detect()?;
            if self.is_cancelled() {
                return Ok(SelectionOutcome::Cancelled);
            }
            if let Some(outcome) = progress.observe_screen(screen) {
                return Ok(outcome);
            }
            let text = normalize_text(&ocr.full_text);
            match screen {
                Screen::HomeClosed => self.navigate(HOME_MENU_BUTTON, "打开菜单")?,
                Screen::HomeOpen => self.navigate(HOME_STRENGTHEN_BUTTON, "进入强化")?,
                Screen::EnhanceMenu => self.navigate(ENHANCE_SERVANT_ENTRY, "进入从者强化")?,
                Screen::Main => {
                    if main_shows_empty_selection(&text) {
                        self.navigate(SERVANT_SELECT_PLUS, "选择从者")?;
                    } else if text.contains(&target.name_cn) {
                        // The target is already on the enhancement page; do not
                        // bounce back to the selection list just because the
                        // level OCR is temporarily empty.
                        return Ok(SelectionOutcome::Selected);
                    } else {
                        self.navigate(SERVANT_SELECT_PLUS, "选择从者")?;
                    }
                }
                Screen::EnhanceConfirm => {
                    self.tap(ENHANCE_CONFIRM_CANCEL, "取消强化确认")?
                }
                Screen::Filter => self.tap(FILTER_CANCEL, "关闭筛选")?,
                Screen::Select => {
                    if !filter_applied {
                        // Filtering by the target's class and rarity keeps the
                        // list short enough to avoid relying on scrolling.
                        if self.apply_filter(target)? {
                            filter_applied = true;
                            self.sleep(700);
                        }
                        if self.is_cancelled() {
                            return Ok(SelectionOutcome::Cancelled);
                        }
                        list_prepared = true;
                    }
                    if !list_prepared {
                        for _ in 0..3 {
                            self.swipe(
                                LIST_RESET_FROM,
                                LIST_RESET_TO,
                                2000,
                                "回到从者列表顶部",
                            )?;
                            self.sleep(250);
                        }
                        list_prepared = true;
                    }
                    let threshold = if filter_applied { 0.60 } else { 0.85 };
                    let found = self.find_target_in_list(target, threshold)?;
                    if self.is_cancelled() {
                        return Ok(SelectionOutcome::Cancelled);
                    }
                    match found {
                        Some(()) => {
                            self.sleep(900);
                            return Ok(SelectionOutcome::Selected);
                        }
                        None => {
                            let Some(scroll) = progress.next_scroll() else {
                                return Ok(progress.failure(SelectionFailure::ScrollLimit));
                            };
                            self.emit(
                                SCREEN_SELECT,
                                &format!("向上滑动列表（{scroll}/{SELECT_MAX_SCROLLS}）"),
                                LogLevel::Debug,
                            );
                            self.swipe(
                                LIST_SCROLL_FROM,
                                LIST_SCROLL_TO,
                                2000,
                                "继续查找从者",
                            )?;
                            progress.record_scroll();
                            self.sleep(700);
                        }
                    }
                }
                Screen::Recommend => {
                    self.navigate(RECOMMEND_CLOSE, "关闭推荐素材选择")?;
                }
                Screen::Result | Screen::Overflow => {
                    self.handle_result_screen(screen)?;
                }
                Screen::Ascension => {
                    self.navigate(TOP_LEFT_BACK, "返回强化菜单")?;
                }
                Screen::Grail => {
                    self.navigate(TOP_LEFT_BACK, "返回强化菜单")?;
                }
                Screen::GrailConfirm => {
                    self.navigate(GRAIL_CONFIRM_CANCEL, "取消圣杯转临确认")?;
                }
                Screen::Unknown => self.sleep(800),
            }
        }
        if self.is_cancelled() {
            Ok(SelectionOutcome::Cancelled)
        } else {
            Ok(progress.failure(SelectionFailure::NavigationLimit))
        }
    }

    fn apply_filter(&mut self, target: &ServantEnhancementTarget) -> Result<bool, String> {
        self.tap(FILTER_OPEN, "打开筛选")?;
        self.sleep(500);
        let (screen, _) = self.detect()?;
        if screen != Screen::Filter {
            return Ok(false);
        }
        // The dialog remembers the previous selection. Reset to "no filter"
        // (all buttons off) so re-selecting the same rarity/class does not
        // toggle it back off.
        self.tap(FILTER_RESET, "重置筛选")?;
        self.sleep(250);
        let rarity = match target.rarity {
            5 => FILTER_RARITY_5,
            4 => FILTER_RARITY_4,
            3 => FILTER_RARITY_3,
            2 => FILTER_RARITY_2,
            1 => FILTER_RARITY_1,
            _ => return Ok(false),
        };
        self.tap(rarity, "筛选稀有度")?;
        self.sleep(250);
        if let Some(class) = filter_class_point(&target.class_name) {
            self.tap(class, "筛选职阶")?;
            self.sleep(250);
        }
        self.tap(FILTER_APPLY, "应用筛选")?;
        self.sleep(600);
        Ok(true)
    }

    fn find_target_in_list(
        &mut self,
        target: &ServantEnhancementTarget,
        threshold: f64,
    ) -> Result<Option<()>, String> {
        let paths: Vec<std::path::PathBuf> = target
            .face_template_paths
            .iter()
            .map(|(_, path)| path.clone())
            .collect();
        if paths.is_empty() {
            return Ok(None);
        }

        let template_size = self.template_size;
        if target.two_exp {
            let corner = self.sidecar().find_enhancement_servant_grid(
                None,
                &paths,
                SERVANT_LIST_REGION,
                TWO_EXP_FACE_MATCH_CROP,
                Some(template_size),
                threshold.max(0.72),
                1.0,
                None,
            );
            match corner {
                Ok(result) if result.found => {
                    self.emit(
                        SCREEN_SELECT,
                        &format!(
                            "命中 2EXP 左下角模板 {}（score {:.3}，method={}）",
                            target.name_cn,
                            result.score,
                            if result.best.as_ref().is_some_and(|best| best.row.is_none()) {
                                "direct"
                            } else {
                                "grid"
                            }
                        ),
                        LogLevel::Info,
                    );
                    self.tap(Point::new(result.x, result.y), "选择从者")?;
                    return Ok(Some(()));
                }
                Ok(result) => {
                    self.emit(
                        SCREEN_SELECT,
                        &format!(
                            "2EXP 左下角未命中 {}（score {:.3}），回退普通模板",
                            target.name_cn, result.score
                        ),
                        LogLevel::Debug,
                    );
                }
                Err(err) => {
                    self.emit(
                        SCREEN_SELECT,
                        &format!("2EXP 左下角匹配失败: {err}，回退普通模板"),
                        LogLevel::Debug,
                    );
                }
            }
        }

        let result = self
            .sidecar()
            .find_enhancement_servant_grid(
                None,
                &paths,
                SERVANT_LIST_REGION,
                SERVANT_FACE_MATCH_CROP,
                Some(template_size),
                threshold,
                1.0,
                None,
            )
            .map_err(|err| format!("头像匹配失败: {err}"))?;
        self.emit(
            SCREEN_SELECT,
            &format!(
                "查找 {}：anchors={} cells={} best={:.3} method={} reason={}",
                target.name_cn,
                result.anchors.len(),
                result.grid_cells.len(),
                result.score,
                if result.best.as_ref().is_some_and(|best| best.row.is_none()) {
                    "direct"
                } else {
                    "grid"
                },
                result.diagnostics.fail_reason.as_deref().unwrap_or("none")
            ),
            LogLevel::Debug,
        );
        if result.found {
            self.tap(Point::new(result.x, result.y), "选择从者")?;
            Ok(Some(()))
        } else {
            Ok(None)
        }
    }

    fn drive_entry(&mut self, target: &ServantEnhancementTarget) -> Result<EntryOutcome, String> {
        let mut unknown = 0_u32;
        let mut level_retries = 0_u32;
        let mut recommendation_attempted = false;
        loop {
            if self.is_cancelled() {
                return Ok(EntryOutcome::Cancelled);
            }
            let (screen, ocr) = self.detect()?;
            let text = normalize_text(&ocr.full_text);
            match screen {
                Screen::Main => {
                    unknown = 0;
                    if main_shows_empty_selection(&text) {
                        self.navigate(SERVANT_SELECT_PLUS, "选择从者")?;
                        continue;
                    }
                    let current = match self.read_level() {
                        CurrentLevelRead::Confirmed(current) => current,
                        CurrentLevelRead::Unknown(reason) => {
                            level_retries += 1;
                            self.emit(
                                SCREEN_MAIN,
                                &format!("当前等级暂不可确认（第 {level_retries}/7 次）：{reason}"),
                                LogLevel::Warn,
                            );
                            if level_retries > 6 {
                                return Ok(EntryOutcome::Skipped(format!(
                                    "无法确认当前从者等级：{reason}"
                                )));
                            }
                            self.sleep(500);
                            continue;
                        }
                        CurrentLevelRead::Cancelled => return Ok(EntryOutcome::Cancelled),
                    };
                    level_retries = 0;
                    self.note_level(target, current);
                    if current >= target.target_level {
                        return Ok(EntryOutcome::Completed);
                    }
                    let bright = self.button_is_bright()?;
                    match decide_main_action(
                        current,
                        target.target_level,
                        bright,
                        recommendation_attempted,
                        text.contains("前往圣杯转临"),
                        text.contains("前往灵基再临"),
                    ) {
                        MainAction::Complete => return Ok(EntryOutcome::Completed),
                        MainAction::Enhance => {
                            recommendation_attempted = false;
                            self.trigger_enhance()?;
                            self.wait_for_enhance_result(current)?;
                        }
                        MainAction::Grail => {
                            self.emit(
                                SCREEN_MAIN,
                                &format!("{} 当前强化按钮未亮，前往圣杯转临", target.name_cn),
                                LogLevel::Info,
                            );
                            self.navigate(GRAIL_ENTRY, "前往圣杯转临")?;
                            if let Some(reason) = self.handle_grail(target)? {
                                return Ok(EntryOutcome::Skipped(reason));
                            }
                            recommendation_attempted = false;
                        }
                        MainAction::Ascension => {
                            self.emit(
                                SCREEN_MAIN,
                                &format!("{} 当前强化按钮未亮，前往灵基再临", target.name_cn),
                                LogLevel::Info,
                            );
                            self.navigate(ASCENSION_ENTRY, "前往灵基再临")?;
                            if let Some(reason) = self.handle_ascension(target)? {
                                return Ok(EntryOutcome::Skipped(reason));
                            }
                            recommendation_attempted = false;
                        }
                        MainAction::Recommend => {
                            recommendation_attempted = true;
                            self.auto_select_materials()?;
                        }
                        MainAction::SkipUnlitButton => {
                            return Ok(EntryOutcome::Skipped("强化按钮未亮".into()));
                        }
                    }
                }
                Screen::Select => {
                    unknown = 0;
                    match self.find_target_in_list(target, 0.85)? {
                        Some(()) => {
                            self.sleep(900);
                        }
                        None => {
                            self.swipe(LIST_SCROLL_FROM, LIST_SCROLL_TO, 2000, "查找从者")?;
                            self.sleep(700);
                        }
                    }
                }
                Screen::Recommend => {
                    unknown = 0;
                    self.ensure_recommend_dialog_and_execute()?;
                }
                Screen::EnhanceConfirm => {
                    self.tap(ENHANCE_CONFIRM_CANCEL, "取消强化确认")?;
                }
                Screen::Result | Screen::Overflow => {
                    unknown = 0;
                    self.handle_result_screen(screen)?;
                }
                Screen::Ascension => {
                    unknown = 0;
                    if let Some(reason) = self.handle_ascension(target)? {
                        return Ok(EntryOutcome::Skipped(reason));
                    }
                }
                Screen::Grail => {
                    unknown = 0;
                    if let Some(reason) = self.handle_grail(target)? {
                        return Ok(EntryOutcome::Skipped(reason));
                    }
                }
                Screen::GrailConfirm => {
                    unknown = 0;
                    self.tap(GRAIL_CONFIRM_DECIDE, "确认圣杯转临")?;
                    self.sleep(1200);
                }
                Screen::HomeClosed | Screen::HomeOpen | Screen::EnhanceMenu => {
                    unknown = 0;
                    self.navigate_to_enhancement()?;
                }
                Screen::Filter => self.tap(FILTER_CANCEL, "关闭筛选")?,
                Screen::Unknown => {
                    unknown += 1;
                    if unknown > UNKNOWN_MAX_TICKS {
                        return Ok(EntryOutcome::Skipped(
                            "连续多次无法识别当前画面".into(),
                        ));
                    }
                    self.sleep(800);
                }
            }
        }
    }

    fn handle_ascension(
        &mut self,
        target: &ServantEnhancementTarget,
    ) -> Result<Option<String>, String> {
        let (screen, ocr) = self.detect()?;
        let text = normalize_text(&ocr.full_text);
        if screen != Screen::Ascension {
            return Ok(None);
        }
        let bright = self.button_is_bright()?;
        if bright {
            self.emit(
                SCREEN_ASCENSION,
                &format!("{} 执行灵基再临", target.name_cn),
                LogLevel::Info,
            );
            self.tap(ENHANCE_BUTTON, "执行灵基再临")?;
            self.wait_for_ascension_result()?;
            return Ok(None);
        }
        if text.contains("未达到可灵基再临的等级") {
            self.navigate(ASCENSION_BACK_TO_SERVANT, "返回强化从者")?;
            return Ok(None);
        }
        // The game can expose the grail route before its required level; keep
        // the existing return-to-normal-enhancement navigation for that state.
        if text.contains("尚未达到圣杯转临所需的等级") || text.contains("圣杯转临结果") {
            self.navigate(GRAIL_BACK_TO_SERVANT, "返回强化从者")?;
            return Ok(None);
        }
        self.emit(
            SCREEN_ASCENSION,
            &format!("{} 灵基再临按钮未亮", target.name_cn),
            LogLevel::Info,
        );
        Ok(Some("灵基再临按钮未亮".into()))
    }

    fn handle_grail(
        &mut self,
        target: &ServantEnhancementTarget,
    ) -> Result<Option<String>, String> {
        let (screen, ocr) = self.detect()?;
        let text = normalize_text(&ocr.full_text);
        if screen != Screen::Grail {
            return Ok(None);
        }
        if !self.button_is_bright()? {
            // The grail screen can be shown before the servant reaches the
            // natural max (e.g. right after the final ascension at 70/80).
            // That is "not ready yet", not a material shortage; go back and
            // keep levelling.
            if text.contains("尚未达到圣杯转临所需的等级") {
                self.navigate(GRAIL_BACK_TO_SERVANT, "返回强化从者")?;
                return Ok(None);
            }
            self.emit(
                SCREEN_GRAIL,
                &format!("{} 圣杯转临按钮未亮", target.name_cn),
                LogLevel::Info,
            );
            return Ok(Some("圣杯转临按钮未亮".into()));
        }
        self.emit(
            SCREEN_GRAIL,
            &format!("{} 执行圣杯转临", target.name_cn),
            LogLevel::Info,
        );
        self.tap(ENHANCE_BUTTON, "执行圣杯转临")?;
        self.wait_for_grail_result()?;
        Ok(None)
    }

    fn auto_select_materials(&mut self) -> Result<(), String> {
        self.tap(RECOMMEND_BUTTON, "打开推荐选择")?;
        self.sleep(900);

        let mut opened = false;
        for _ in 0..12 {
            if self.is_cancelled() {
                return Ok(());
            }
            let (screen, _) = self.detect()?;
            if screen == Screen::Recommend {
                opened = true;
                break;
            }
            if screen == Screen::Main {
                self.sleep(400);
                continue;
            }
            break;
        }
        if !opened {
            return Ok(());
        }

        let (_, ocr) = self.detect()?;
        let text = normalize_text(&ocr.full_text);
        if text.contains("自动配置关") || text.contains("自动配置off") {
            self.emit(
                SCREEN_RECOMMEND,
                "自动配置为关，切换为开",
                LogLevel::Info,
            );
            self.tap(RECOMMEND_AUTO_SWITCH, "打开自动配置")?;
            self.sleep(500);
        }
        self.tap(RECOMMEND_EXECUTE, "执行推荐选择")?;
        self.sleep(1400);
        Ok(())
    }

    fn ensure_recommend_dialog_and_execute(&mut self) -> Result<(), String> {
        let (_, ocr) = self.detect()?;
        let text = normalize_text(&ocr.full_text);
        if text.contains("自动配置关") || text.contains("自动配置off") {
            self.tap(RECOMMEND_AUTO_SWITCH, "打开自动配置")?;
            self.sleep(500);
        }
        self.tap(RECOMMEND_EXECUTE, "执行推荐选择")?;
        self.sleep(1200);
        Ok(())
    }

    fn button_is_bright(&mut self) -> Result<bool, String> {
        let luma = self
            .sidecar()
            .read_region_color(None, BUTTON_LUMA_REGION)
            .map_err(|err| format!("读取强化按钮亮度失败: {err}"))?
            .mean_luma;
        Ok(luma >= 150.0)
    }

    fn read_current_level_once(&mut self) -> Result<u32, String> {
        let result = self
            .sidecar()
            .read_enhancement_current_level(None)
            .map_err(|error| format!("等级识别命令失败：{error}"))?;
        current_level_from_result(result)
    }

    fn read_level(&mut self) -> CurrentLevelRead {
        if self.is_cancelled() {
            return CurrentLevelRead::Cancelled;
        }
        let first = self.read_current_level_once();
        let wait_started = Instant::now();
        while wait_started.elapsed() < LEVEL_CONFIRM_INTERVAL {
            if self.is_cancelled() {
                return CurrentLevelRead::Cancelled;
            }
            let remaining = LEVEL_CONFIRM_INTERVAL.saturating_sub(wait_started.elapsed());
            thread::sleep(remaining.min(Duration::from_millis(25)));
        }
        if self.is_cancelled() {
            return CurrentLevelRead::Cancelled;
        }
        let second = self.read_current_level_once();
        if self.is_cancelled() {
            return CurrentLevelRead::Cancelled;
        }
        match resolve_current_level_reads(first, second) {
            Ok(current) => {
                self.last_level_failure_emit = None;
                CurrentLevelRead::Confirmed(current)
            }
            Err(reason) => {
                if self.last_level_failure_emit.as_ref() != Some(&reason) {
                    self.emit(
                        SCREEN_MAIN,
                        &format!("当前等级无法确认：{reason}"),
                        LogLevel::Warn,
                    );
                    self.last_level_failure_emit = Some(reason.clone());
                }
                CurrentLevelRead::Unknown(reason)
            }
        }
    }

    fn note_level(&mut self, target: &ServantEnhancementTarget, current: u32) {
        let key = (target.entry_id.clone(), current);
        if self.last_level_emit.as_ref() == Some(&key) {
            return;
        }
        self.last_level_emit = Some(key);
        self.update_entry(&target.entry_id, |entry| {
            entry.current_level = Some(current);
            entry.max_level = None;
        });
        self.emit(
            SCREEN_MAIN,
            &format!("{} 当前 Lv.{current}，目标 Lv.{}", target.name_cn, target.target_level),
            LogLevel::Debug,
        );
    }

    fn trigger_enhance(&mut self) -> Result<(), String> {
        self.tap(ENHANCE_BUTTON, "执行强化")?;
        self.sleep(1600);
        let (screen, _) = self.detect()?;
        if screen == Screen::EnhanceConfirm {
            self.tap(ENHANCE_CONFIRM_DECIDE, "确认强化合成")?;
            self.sleep(1200);
        }
        Ok(())
    }

    fn wait_for_enhance_result(&mut self, before_level: u32) -> Result<(), String> {
        self.wait_for_animation("强化动画", |runner, screen, _ocr| {
            match screen {
                Screen::Result | Screen::Overflow => true,
                // If the pre-tap already accepted the confirmation and dismissed
                // the result screen, the main page only counts as finished when
                // the level actually changed. This also implements the required
                // "re-check after every enhancement" loop.
                Screen::Main => match runner.read_level() {
                    CurrentLevelRead::Confirmed(current) => current != before_level,
                    CurrentLevelRead::Unknown(_) | CurrentLevelRead::Cancelled => false,
                },
                _ => false,
            }
        })?;
        let (screen, _) = self.detect()?;
        self.handle_result_screen(screen)
    }

    fn handle_result_screen(&mut self, screen: Screen) -> Result<(), String> {
        match screen {
            Screen::Overflow => {
                self.tap(OVERFLOW_CLOSE, "关闭溢出提示")?;
                self.sleep(800);
            }
            Screen::Result => {
                self.tap(BOTTOM_TAP, "关闭强化结果")?;
                self.sleep(1600);
            }
            _ => {}
        }
        Ok(())
    }

    fn wait_for_ascension_result(&mut self) -> Result<(), String> {
        self.wait_for_animation("灵基再临动画", |_runner, screen, ocr| {
            let text = normalize_text(&ocr.full_text);
            matches!(screen, Screen::Result | Screen::Main)
                || (screen == Screen::Ascension
                    && (text.contains("未达到可灵基再临的等级")
                        || text.contains("无法进行灵基再临")))
                || (screen == Screen::Grail
                    && text.contains("尚未达到圣杯转临所需的等级"))
        })
    }

    fn wait_for_grail_result(&mut self) -> Result<(), String> {
        self.wait_for_animation("圣杯转临动画", |_runner, screen, ocr| {
            let text = normalize_text(&ocr.full_text);
            matches!(screen, Screen::Result | Screen::Main)
                || (screen == Screen::Grail
                    && (text.contains("无法进行圣杯转临")
                        || text.contains("尚未达到圣杯转临所需的等级")))
        })
    }

    /// Poll while the long servant dialogue plays. Periodic centre taps advance
    /// the dialogue, matching the game's own "tap to continue" affordance.
    fn wait_for_animation<F>(&mut self, label: &str, done: F) -> Result<(), String>
    where
        F: Fn(&mut ServantEnhancementRunner, Screen, &OcrRegionResult) -> bool,
    {
        let start = Instant::now();
        let mut last_tap = Instant::now() - Duration::from_secs(4);
        loop {
            if self.is_cancelled() {
                return Ok(());
            }
            let (screen, ocr) = self.detect()?;
            if done(self, screen, &ocr) {
                return Ok(());
            }
            match screen {
                Screen::EnhanceConfirm => {
                    self.tap(ENHANCE_CONFIRM_DECIDE, "确认强化合成")?;
                    self.sleep(1200);
                    continue;
                }
                Screen::GrailConfirm => {
                    self.tap(GRAIL_CONFIRM_DECIDE, "确认圣杯转临")?;
                    self.sleep(1200);
                    continue;
                }
                Screen::Overflow => {
                    self.tap(OVERFLOW_CLOSE, "关闭溢出提示")?;
                    self.sleep(700);
                    continue;
                }
                Screen::Result => {
                    self.tap(BOTTOM_TAP, "关闭结果画面")?;
                    self.sleep(1400);
                    continue;
                }
                _ => {}
            }
            if start.elapsed() >= ANIMATION_TIMEOUT {
                return Err(format!("{label}等待超时"));
            }
            if last_tap.elapsed() >= Duration::from_millis(2000) {
                self.tap(BOTTOM_TAP, "点击跳过对话")?;
                last_tap = Instant::now();
            }
            self.sleep(500);
        }
    }

    fn navigate_to_enhancement(&mut self) -> Result<(), String> {
        let (screen, _) = self.detect()?;
        match screen {
            Screen::HomeClosed => self.navigate(HOME_MENU_BUTTON, "打开菜单"),
            Screen::HomeOpen => self.navigate(HOME_STRENGTHEN_BUTTON, "进入强化"),
            Screen::EnhanceMenu => self.navigate(ENHANCE_SERVANT_ENTRY, "进入从者强化"),
            _ => Ok(()),
        }
    }

    fn navigate(&mut self, point: Point, label: &str) -> Result<(), String> {
        self.tap(point, label)?;
        self.sleep(1000);
        Ok(())
    }

    fn detect(&mut self) -> Result<(Screen, OcrRegionResult), String> {
        let ocr = self
            .sidecar()
            .ocr_region(None, FULL_SCREEN)
            .map_err(|err| format!("OCR 失败: {err}"))?;
        Ok((classify_screen(&normalize_text(&ocr.full_text)), ocr))
    }

    fn complete_entry(&mut self, target: &ServantEnhancementTarget) {
        self.update_entry(&target.entry_id, |entry| {
            entry.status = EntryStatus::Completed;
            entry.message = format!("已到达 Lv.{}", target.target_level);
        });
        self.remove_from_store(&target.entry_id);
        self.set_current_entry("", None);
        self.emit(
            SCREEN_MAIN,
            &format!("{} 已完成，移出强化队列", target.name_cn),
            LogLevel::Info,
        );
    }

    fn skip_entry(&mut self, target: &ServantEnhancementTarget, reason: String) {
        self.update_entry(&target.entry_id, |entry| {
            entry.status = EntryStatus::Skipped;
            entry.message = reason.clone();
        });
        self.set_current_entry("", None);
        self.emit(
            SCREEN_MAIN,
            &format!("{} 跳过：{reason}", target.name_cn),
            LogLevel::Info,
        );
    }

    fn remove_from_store(&self, entry_id: &str) {
        let store = self
            .app
            .state::<Mutex<ServantEnhancementQueueStore>>();
        let mut guard = store.lock().unwrap();
        guard.entries.retain(|entry| entry.id != entry_id);
        if let Err(err) = guard.save(&self.app) {
            eprintln!("[servant-enhancement] failed to save queue: {err}");
        }
    }

    fn update_entry<F>(&self, entry_id: &str, update: F)
    where
        F: FnOnce(&mut ServantEnhancementEntryState),
    {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.iter_mut().find(|entry| entry.id == entry_id) {
            update(entry);
        }
    }

    fn set_current_entry(&self, entry_id: &str, name: Option<&str>) {
        *self.current_entry_id.lock().unwrap() = if entry_id.is_empty() {
            None
        } else {
            Some(entry_id.to_string())
        };
        if let Some(name) = name {
            self.emit(SCREEN_SELECT, &format!("当前从者：{name}"), LogLevel::Info);
        }
    }

    fn transition(&self, event: ServantEnhancementLifecycleEvent) {
        let mut state = self.state.lock().unwrap();
        let transition = servant_enhancement_lifecycle_transition(state.clone(), event);
        if transition.accepted {
            *state = transition.next;
        }
    }

    fn fail(&self, message: &str) {
        self.transition(ServantEnhancementLifecycleEvent::Failed {
            message: message.to_string(),
        });
        self.emit(SCREEN_UNKNOWN, message, LogLevel::Warn);
    }

    fn emit(&self, screen: &str, message: &str, level: LogLevel) {
        let (state_str, status) = {
            let state = self.state.lock().unwrap();
            (format!("{:?}", *state), state.status())
        };
        let entries = self.entries.lock().unwrap().clone();
        let current_entry_id = self.current_entry_id.lock().unwrap().clone();
        crate::recognition_diagnostics::logs::record(&self.app, screen, message);
        let _ = self.app.emit(
            EVENT_NAME,
            ServantEnhancementAutomationEvent {
                state: state_str,
                status,
                current_screen: screen.to_string(),
                message: message.to_string(),
                level,
                entries,
                current_entry_id,
            },
        );
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn sidecar(&mut self) -> &mut SidecarClient {
        self.sidecar
            .as_mut()
            .expect("servant enhancement sidecar missing")
    }

    fn tap(&mut self, point: Point, label: &str) -> Result<(), String> {
        if self.is_cancelled() {
            return Err("从者强化已取消".into());
        }
        let (px, py) = point.to_physical(self.screen_w, self.screen_h);
        self.touch
            .tap(px, py)
            .map_err(|err| format!("{label}点击失败: {err}"))
    }

    fn swipe(
        &mut self,
        from: Point,
        to: Point,
        duration_ms: u32,
        label: &str,
    ) -> Result<(), String> {
        if self.is_cancelled() {
            return Err("从者强化已取消".into());
        }
        let from = from.to_physical(self.screen_w, self.screen_h);
        let to = to.to_physical(self.screen_w, self.screen_h);
        self.touch
            .swipe(from, to, duration_ms)
            .map_err(|err| format!("{label}滑动失败: {err}"))
    }

    fn sleep(&self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
    }
}

impl Drop for ServantEnhancementRunner {
    fn drop(&mut self) {
        if let Some(sidecar) = self.sidecar.take() {
            sidecar.return_to_cache(self.sidecar_cache.as_deref());
        }
    }
}

fn normalize_text(text: &str) -> String {
    text.nfkc()
        .collect::<String>()
        .to_lowercase()
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

pub(super) fn filter_class_point(class_name: &str) -> Option<Point> {
    let name = class_name.trim().to_ascii_lowercase();
    Some(match name.as_str() {
        "剑士" | "saber" => FILTER_CLASS_SABER,
        "弓兵" | "archer" => FILTER_CLASS_ARCHER,
        "枪兵" | "lancer" => FILTER_CLASS_LANCER,
        "骑兵" | "rider" => FILTER_CLASS_RIDER,
        "魔术师" | "caster" => FILTER_CLASS_CASTER,
        "暗匿者" | "assassin" => FILTER_CLASS_ASSASSIN,
        "狂战士" | "berserker" => FILTER_CLASS_BERSERKER,
        "盾兵" | "shielder" => FILTER_CLASS_SHIELDER,
        "裁定者" | "ruler" => FILTER_CLASS_RULER,
        "复仇者" | "avenger" => FILTER_CLASS_AVENGER,
        "月之癌" | "mooncancer" => FILTER_CLASS_MOONCANCER,
        "他人格" | "alterego" => FILTER_CLASS_ALTEREGO,
        "降临者" | "foreigner" => FILTER_CLASS_FOREIGNER,
        "身披角色者" | "pretender" => FILTER_CLASS_PRETENDER,
        "兽" | "beast" => FILTER_CLASS_BEAST,
        _ => return None,
    })
}

pub(super) fn classify_screen(text: &str) -> Screen {
    if text.contains("推荐素材选择") {
        return Screen::Recommend;
    }
    if text.contains("经验值溢出") || text.contains("并未使用") {
        return Screen::Overflow;
    }
    if text.contains("是否进行") && text.contains("圣杯转临") {
        return Screen::GrailConfirm;
    }
    if text.contains("圣杯转临结果") || text.contains("圣杯转临所必需") {
        return Screen::Grail;
    }
    if text.contains("灵基再临结果") || text.contains("灵基再临必要道具") {
        return Screen::Ascension;
    }
    if text.contains("请点击游戏界面")
        || text.contains("已到达等级上限")
        || text.contains("习得新的技能")
        || text.contains("圣肖像进化")
    {
        return Screen::Result;
    }
    if text.contains("是否进行强化合成")
        || text.contains("包含★3以上的素材")
        || text.contains("包含★3以上的素材！")
    {
        return Screen::EnhanceConfirm;
    }
    if text.contains("仅在列表中显示选择的项目")
        || (text.contains("稀有度") && text.contains("职阶") && text.contains("决定"))
    {
        return Screen::Filter;
    }
    if text.contains("请选择想要强化的从者")
        || text.contains("持有从者")
        || text.contains("等级顺序")
    {
        return Screen::Select;
    }
    if text.contains("强化用素材") || text.contains("请先选择想要强化的从者") {
        return Screen::Main;
    }
    if text.contains("从者技能强化") || text.contains("概念礼装强化") {
        return Screen::EnhanceMenu;
    }
    if text.contains("个人空间") || text.contains("召唤") {
        return Screen::HomeOpen;
    }
    if text.contains("通知") && text.contains("菜单") {
        return Screen::HomeClosed;
    }
    if text.contains("请点击「强化」按钮") && text.contains("灵基再临") {
        return Screen::Ascension;
    }
    if text.contains("请点击「强化」按钮") && text.contains("圣杯转临") {
        return Screen::Grail;
    }
    Screen::Unknown
}

pub(super) fn main_shows_empty_selection(text: &str) -> bool {
    text.contains("请先选择想要强化的从者") || text.contains("点击选择")
}

/// OCR crop fragments retain full-screen normalized coordinates. Keep the
/// current row separate from material cards and the right-hand level preview.
#[cfg(test)]
pub(super) fn read_level_from_ocr(ocr: &OcrRegionResult) -> Option<(u32, u32)> {
    let mut fragments: Vec<_> = ocr
        .fragments
        .iter()
        .filter(|fragment| {
            let x = fragment.region.x + fragment.region.w / 2.0;
            let y = fragment.region.y + fragment.region.h / 2.0;
            (LEVEL_REGION.x..=LEVEL_REGION.x + LEVEL_REGION.w).contains(&x)
                && (LEVEL_REGION.y..=LEVEL_REGION.y + LEVEL_REGION.h).contains(&y)
        })
        .collect();
    let mut level = None;
    while let Some(first) = fragments.pop() {
        let mut row = vec![first];
        // Group every adjacent box before parsing: a complete-looking pair can
        // still be a truncated reading such as [10/12] [0] or [10] [1/120].
        loop {
            let mut changed = false;
            for index in (0..fragments.len()).rev() {
                let next = fragments[index];
                if row.iter().any(|part| {
                    let overlap = (part.region.y + part.region.h)
                        .min(next.region.y + next.region.h)
                        - part.region.y.max(next.region.y);
                    let gap = part.region.x.max(next.region.x)
                        - (part.region.x + part.region.w)
                            .min(next.region.x + next.region.w);
                    overlap >= part.region.h.min(next.region.h) * 0.5 && gap <= 0.025
                }) {
                    row.push(fragments.remove(index));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        row.sort_by(|a, b| a.region.x.total_cmp(&b.region.x));
        row.dedup_by(|a, b| {
            a.text == b.text
                && a.region.x == b.region.x
                && a.region.y == b.region.y
                && a.region.w == b.region.w
                && a.region.h == b.region.h
        });
        let text = row.iter().map(|part| part.text.nfkc().collect::<String>()).collect::<Vec<_>>().join(" ");
        let text = text.strip_prefix("等级").unwrap_or(&text).trim();
        let Some((current, max_level)) = text.split_once('/') else {
            continue;
        };
        // Each number must be one intact run. Never guess whether [1] [2]
        // means 12 or accept only its suffix as 2. Split slash boxes are safe.
        let current = current.trim();
        let max_level = max_level.trim();
        if current.is_empty() || max_level.is_empty()
            || !current.chars().all(|c| c.is_ascii_digit())
            || !max_level.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        if let Some(candidate) = parse_bare_level(&format!("{current}/{max_level}")) {
            if level.is_some_and(|previous| previous != candidate) {
                return None;
            }
            level = Some(candidate);
        }
    }
    level
}

#[cfg(test)]
pub(super) fn parse_bare_level(text: &str) -> Option<(u32, u32)> {
    let chars: Vec<char> = text.chars().collect();
    let mut best: Option<(u32, u32)> = None;
    let mut index = 0;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let current_start = index;
        while index < chars.len() && chars[index].is_ascii_digit() {
            index += 1;
        }
        let current: u32 = chars[current_start..index]
            .iter()
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        let mut cursor = index;
        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        if cursor >= chars.len() || chars[cursor] != '/' {
            continue;
        }
        cursor += 1;
        while cursor < chars.len() && chars[cursor].is_whitespace() {
            cursor += 1;
        }
        let max_start = cursor;
        while cursor < chars.len() && chars[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == max_start {
            continue;
        }
        let max_level: u32 = chars[max_start..cursor]
            .iter()
            .collect::<String>()
            .parse()
            .unwrap_or(0);
        // Do not truncate HP/QP/decimal values such as 1/40,000 or 1/40.5.
        if matches!(chars.get(cursor), Some(',' | '.' | '/'))
            || (current_start > 0
                && matches!(chars[current_start - 1], ',' | '.' | '/'))
            || max_level < 10
            || max_level > 150
            || current == 0
            || current > max_level
        {
            continue;
        }
        let candidate = (current, max_level);
        if best.is_some_and(|previous| previous != candidate) {
            return None;
        }
        best = Some(candidate);
    }
    best
}

