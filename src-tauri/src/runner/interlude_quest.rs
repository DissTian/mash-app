//! Standalone interlude quest automation runtime.
//!
//! The interlude flow lives outside the normal battle project model: it starts
//! from the main terminal, walks the 02/03-1 lists greedily, handles the
//! confirm/formation/dialog/failure pages, and hands the actual fight to the
//! existing battle loop with auto-battle takeover enabled.

use super::*;
use crate::screen::{InterludeQuestRow, NormRect, Point, Screen};

const NAV_ENTRY_REGION: NormRect = NormRect {
    x: 0.50,
    y: 0.28,
    w: 0.45,
    h: 0.42,
};
const GATE_ENTRY_REGION: NormRect = NormRect {
    x: 0.50,
    y: 0.28,
    w: 0.45,
    h: 0.35,
};
const LIST_REGION: NormRect = NormRect {
    x: 0.48,
    y: 0.10,
    w: 0.49,
    h: 0.86,
};
const LIST_SCROLL_X: f64 = 0.90;
const LIST_SCROLL_TOP: f64 = 0.30;
const LIST_SCROLL_BOTTOM: f64 = 0.80;
const MAIN_SCROLL_X: f64 = 0.85;
const MAIN_SCROLL_TOP: f64 = 0.30;
const MAIN_SCROLL_BOTTOM: f64 = 0.72;
const MAX_NAVIGATION_SWIPES: u8 = 8;
const MAX_LIST_SWIPES: u8 = 24;
const MAX_DIALOGUE_SKIPS: u8 = 3;
const DIALOGUE_SKIP_REGION: NormRect = NormRect {
    x: 0.86,
    y: 0.00,
    w: 0.14,
    h: 0.12,
};

const QUEST_CONFIRM_START: Point = Point::new(0.665, 0.775);
const QUEST_CONFIRM_CANCEL: Point = Point::new(0.335, 0.775);
const FORMATION_LIMIT_CLOSE: Point = Point::new(0.50, 0.840);
const AUTO_FORMATION_RIGHT: Point = Point::new(0.730, 0.830);
const AUTO_FORMATION_LEFT: Point = Point::new(0.270, 0.830);
const FORMATION_DECIDE: Point = Point::new(0.910, 0.925);
const TEAM_START: Point = Point::new(0.900, 0.930);
const EMPTY_SLOT_AUTO_FORMATION: Point = Point::new(0.235, 0.890);
const DIALOGUE_YES: Point = Point::new(0.650, 0.775);
const DIALOGUE_ADVANCE: Point = Point::new(0.500, 0.800);
const BATTLE_CONTINUE_RETREAT: Point = Point::new(0.250, 0.450);
const CHARM_CONFIRM_DECIDE: Point = Point::new(0.665, 0.610);
const RETREAT_NOTICE_CLOSE: Point = Point::new(0.500, 0.810);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterludeQuestPhase {
    NavigateMain,
    NavigateGate,
    NeedRootList,
    AwaitingStage,
    NeedStageList,
    AwaitingConfirm,
    AwaitingDeparture,
    InQuest,
    AwaitingReturn,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InterludeListKind {
    Root,
    Stage,
}

pub(crate) struct InterludeQuestRuntime {
    pub(crate) config: InterludeWorkflowConfig,
    pub(crate) phase: InterludeQuestPhase,
    pub(crate) current_list: InterludeListKind,
    main_swipes: u8,
    gate_swipes: u8,
    list_swipes: u8,
    top_seeks: u8,
    dialogue_skips: u8,
    /// Signature of the row most recently tapped. Kept for repeat guarding
    /// and for skipping the row after a retreat.
    current_signature: Option<String>,
    repeat_attempts: u32,
    handled_signatures: Vec<String>,
    seen_signatures: Vec<String>,
    target_seen: bool,
    target_unavailable: bool,
    completed_count: u32,
    skipped_count: u32,
    unavailable_count: u32,
    pub(crate) consecutive_failures: u32,
    charm_used_this_run: bool,
    navigation_done: bool,
    single_failed: bool,
    awaiting_battle_start: bool,
    dialogue_skip_attempts: u8,
    dialogue_ocr_tick: u8,
    last_dialog_at: Option<Instant>,
}

impl InterludeQuestRuntime {
    pub(crate) fn new(config: InterludeWorkflowConfig) -> Self {
        Self {
            config,
            phase: InterludeQuestPhase::NavigateMain,
            current_list: InterludeListKind::Root,
            main_swipes: 0,
            gate_swipes: 0,
            list_swipes: 0,
            top_seeks: 3,
            dialogue_skips: 0,
            current_signature: None,
            repeat_attempts: 0,
            handled_signatures: Vec::new(),
            seen_signatures: Vec::new(),
            target_seen: false,
            target_unavailable: false,
            completed_count: 0,
            skipped_count: 0,
            unavailable_count: 0,
            consecutive_failures: 0,
            charm_used_this_run: false,
            navigation_done: false,
            single_failed: false,
            awaiting_battle_start: false,
            dialogue_skip_attempts: 0,
            dialogue_ocr_tick: 0,
            last_dialog_at: None,
        }
    }

    fn note_dialogue(&mut self, at: Instant) {
        self.last_dialog_at = Some(at);
    }

    fn dialogue_grace_active(&self, at: Instant) -> bool {
        let grace = Duration::from_secs(self.config.task.dialogue_grace_seconds.max(5) as u64);
        self.last_dialog_at
            .is_some_and(|seen| at.saturating_duration_since(seen) < grace)
    }

    fn observe_battle_started(&mut self, screen: Screen) {
        if matches!(screen, Screen::Battle | Screen::Attack) || is_battle_result_screen(screen) {
            self.awaiting_battle_start = false;
            if self.phase == InterludeQuestPhase::AwaitingDeparture {
                self.phase = InterludeQuestPhase::InQuest;
            }
        } else if matches!(screen, Screen::InterludeQuest | Screen::InterludeStage) {
            self.awaiting_battle_start = false;
        }
    }

    fn is_handled(&self, signature: &str) -> bool {
        self.handled_signatures.iter().any(|item| item == signature)
    }

    fn remember_handled(&mut self, signature: Option<String>) {
        if let Some(signature) = signature {
            if !self.is_handled(&signature) {
                self.handled_signatures.push(signature);
                self.skipped_count = self.skipped_count.saturating_add(1);
            }
        }
    }

    fn remember_seen(&mut self, row: &InterludeQuestRow) {
        if row.actionable
            && !self
                .seen_signatures
                .iter()
                .any(|item| item == &row.title_signature)
        {
            self.seen_signatures.push(row.title_signature.clone());
        }
    }

    fn note_clicked(&mut self, row: &InterludeQuestRow) {
        if self.current_signature.as_deref() == Some(row.title_signature.as_str()) {
            self.repeat_attempts = self.repeat_attempts.saturating_add(1);
        } else {
            self.current_signature = Some(row.title_signature.clone());
            self.repeat_attempts = 1;
        }
        self.remember_seen(row);
    }
}

fn interlude_reward_close_point(fragments: &[crate::screen::OcrFragment]) -> Option<Point> {
    let compact = |text: &str| text.split_whitespace().collect::<String>();
    let title = fragments.iter().find(|fragment| {
        let text = compact(&fragment.text);
        text.contains("任务完成") || text.contains("获得报酬") || text.contains("任务报酬")
    })?;
    let button = fragments
        .iter()
        .filter(|fragment| {
            fragment.region.y >= 0.55
                && fragment.region.y >= title.region.y + title.region.h
                && matches!(compact(&fragment.text).as_str(), "关闭" | "确定" | "OK")
        })
        .max_by(|a, b| a.region.y.total_cmp(&b.region.y))?;
    Some(Point::new(
        button.region.x + button.region.w * 0.5,
        button.region.y + button.region.h * 0.5,
    ))
}

fn interlude_title_signature_matches(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    match (
        u128::from_str_radix(left, 16),
        u128::from_str_radix(right, 16),
    ) {
        (Ok(left), Ok(right)) => (left ^ right).count_ones() <= 12,
        _ => false,
    }
}

fn interlude_transition_message(phase: InterludeQuestPhase) -> Option<&'static str> {
    match phase {
        InterludeQuestPhase::NavigateMain | InterludeQuestPhase::NavigateGate => None,
        InterludeQuestPhase::AwaitingStage => Some("等待进入场景任务列表…"),
        InterludeQuestPhase::AwaitingConfirm => Some("等待任务确认…"),
        InterludeQuestPhase::AwaitingDeparture => Some("等待进入助战选择…"),
        InterludeQuestPhase::AwaitingReturn => Some("等待返回幕间物语列表…"),
        InterludeQuestPhase::NeedRootList
        | InterludeQuestPhase::NeedStageList
        | InterludeQuestPhase::InQuest
        | InterludeQuestPhase::Finished => None,
    }
}

impl Runner {
    pub(crate) fn interlude_quest_active(&self) -> bool {
        self.interlude_quest.is_some()
    }

    pub(crate) fn interlude_quest_transition_message(&self) -> Option<&'static str> {
        let runtime = self.interlude_quest.as_ref()?;
        if runtime.awaiting_battle_start {
            return Some("已点击战斗开始，等待进入战斗画面…");
        }
        interlude_transition_message(runtime.phase)
    }

    pub(crate) fn mark_interlude_return_pending(&mut self) {
        if let Some(runtime) = self.interlude_quest.as_mut() {
            runtime.phase = InterludeQuestPhase::AwaitingReturn;
            runtime.awaiting_battle_start = false;
        }
    }

    pub(crate) fn interlude_navigation_phase(&self) -> bool {
        self.interlude_quest.as_ref().is_some_and(|runtime| {
            matches!(
                runtime.phase,
                InterludeQuestPhase::NavigateMain | InterludeQuestPhase::NavigateGate
            )
        })
    }

    pub(crate) fn interlude_dialogue_grace_active(&self) -> bool {
        self.interlude_quest
            .as_ref()
            .is_some_and(|runtime| runtime.dialogue_grace_active(Instant::now()))
    }

    pub(crate) fn interlude_completed_count(&self) -> u32 {
        self.interlude_quest
            .as_ref()
            .map(|runtime| runtime.completed_count)
            .unwrap_or(0)
    }

    pub(crate) fn interlude_finish(&mut self) {
        if let Some(runtime) = self.interlude_quest.as_mut() {
            if runtime.completed_count == 0 {
                runtime.completed_count = runtime
                    .seen_signatures
                    .iter()
                    .filter(|signature| !runtime.handled_signatures.contains(signature))
                    .count() as u32;
            }
            runtime.phase = InterludeQuestPhase::Finished;
        }
        self.emit(
            "InterludeQuest",
            &format!(
                "幕间物语已完成 {} 个任务，跳过 {} 个，剩余不可点 {} 个",
                self.interlude_completed_count(),
                self.interlude_quest
                    .as_ref()
                    .map(|runtime| runtime.skipped_count)
                    .unwrap_or(0),
                self.interlude_quest
                    .as_ref()
                    .map(|runtime| runtime.unavailable_count)
                    .unwrap_or(0)
            ),
        );
        self.transition_lifecycle(RunnerLifecycleEvent::Finished);
    }

    /// Observe the current screen while the interlude workflow owns the loop.
    /// Returns `false` if the run should stop.
    pub(crate) fn observe_interlude_quest_screen(&mut self, screen: Screen) -> bool {
        let (observation, phase) = {
            let Some(runtime) = self.interlude_quest.as_mut() else {
                return true;
            };
            if screen == Screen::Unknown
                || screen == Screen::Dialog
                || screen == Screen::DialogSkipConfirm
            {
                return true;
            }
            let previous = runtime.phase;
            runtime.observe_battle_started(screen);
            match runtime.phase {
                InterludeQuestPhase::NavigateMain => {
                    if screen == Screen::Main {
                        runtime.phase = InterludeQuestPhase::NavigateMain;
                    } else if screen == Screen::ChaldeaGate {
                        runtime.phase = InterludeQuestPhase::NavigateGate;
                    } else if matches!(screen, Screen::InterludeQuest | Screen::InterludeStage) {
                        runtime.navigation_done = true;
                        if screen == Screen::InterludeStage {
                            runtime.phase = InterludeQuestPhase::NeedStageList;
                            runtime.current_list = InterludeListKind::Stage;
                        } else {
                            runtime.phase = InterludeQuestPhase::NeedRootList;
                            runtime.current_list = InterludeListKind::Root;
                        }
                    }
                }
                InterludeQuestPhase::NavigateGate => match screen {
                    Screen::ChaldeaGate => {
                        runtime.phase = InterludeQuestPhase::NavigateGate;
                    }
                    Screen::InterludeQuest => {
                        runtime.phase = InterludeQuestPhase::NeedRootList;
                        runtime.current_list = InterludeListKind::Root;
                        runtime.navigation_done = true;
                    }
                    Screen::InterludeStage => {
                        runtime.phase = InterludeQuestPhase::NeedStageList;
                        runtime.current_list = InterludeListKind::Stage;
                        runtime.navigation_done = true;
                    }
                    _ => {}
                },
                InterludeQuestPhase::AwaitingStage => {
                    if screen == Screen::InterludeStage {
                        runtime.phase = InterludeQuestPhase::NeedStageList;
                        runtime.current_list = InterludeListKind::Stage;
                    } else if screen == Screen::InterludeQuest {
                        runtime.phase = InterludeQuestPhase::NeedRootList;
                        runtime.current_list = InterludeListKind::Root;
                    }
                }
                InterludeQuestPhase::AwaitingConfirm => match screen {
                    Screen::SupportSelect
                    | Screen::TeamConfirm
                    | Screen::TeamConfirmInterlude
                    | Screen::FormationLimitDialog
                    | Screen::AutoFormationDialog
                    | Screen::EmptySlotAutoFormationDialog
                    | Screen::FormationEditor => {
                        runtime.phase = InterludeQuestPhase::InQuest;
                    }
                    Screen::APRecovery => {
                        runtime.phase = InterludeQuestPhase::AwaitingReturn;
                    }
                    Screen::InterludeQuest => {
                        runtime.phase = InterludeQuestPhase::NeedRootList;
                        runtime.current_list = InterludeListKind::Root;
                    }
                    Screen::InterludeStage => {
                        runtime.phase = InterludeQuestPhase::NeedStageList;
                        runtime.current_list = InterludeListKind::Stage;
                    }
                    _ => {}
                },
                InterludeQuestPhase::AwaitingDeparture => match screen {
                    Screen::SupportSelect
                    | Screen::TeamConfirm
                    | Screen::TeamConfirmInterlude
                    | Screen::FormationLimitDialog
                    | Screen::AutoFormationDialog
                    | Screen::EmptySlotAutoFormationDialog
                    | Screen::FormationEditor => {
                        runtime.phase = InterludeQuestPhase::InQuest;
                    }
                    Screen::APRecovery => {
                        runtime.phase = InterludeQuestPhase::AwaitingReturn;
                    }
                    Screen::InterludeQuest => {
                        runtime.phase = InterludeQuestPhase::NeedRootList;
                        runtime.current_list = InterludeListKind::Root;
                    }
                    Screen::InterludeStage => {
                        runtime.phase = InterludeQuestPhase::NeedStageList;
                        runtime.current_list = InterludeListKind::Stage;
                    }
                    _ => {}
                },
                InterludeQuestPhase::InQuest => {
                    if screen == Screen::InterludeQuest || screen == Screen::InterludeStage {
                        runtime.phase = InterludeQuestPhase::AwaitingReturn;
                    }
                }
                InterludeQuestPhase::AwaitingReturn => match screen {
                    Screen::InterludeQuest => {
                        runtime.phase = InterludeQuestPhase::NeedRootList;
                        runtime.current_list = InterludeListKind::Root;
                        runtime.current_signature = None;
                        runtime.repeat_attempts = 0;
                        runtime.list_swipes = 0;
                        self.battle = BattleState::new();
                    }
                    Screen::InterludeStage => {
                        runtime.phase = InterludeQuestPhase::NeedStageList;
                        runtime.current_list = InterludeListKind::Stage;
                        runtime.current_signature = None;
                        runtime.repeat_attempts = 0;
                        runtime.list_swipes = 0;
                        self.battle = BattleState::new();
                    }
                    _ => {}
                },
                InterludeQuestPhase::NeedRootList | InterludeQuestPhase::NeedStageList => {}
                InterludeQuestPhase::Finished => {}
            }
            (previous, runtime.phase)
        };

        if phase != observation {
            if let Some(message) = interlude_transition_message(phase) {
                self.emit("InterludeQuest", message);
            }
        }
        true
    }

    pub(crate) fn handle_interlude_quest_screen(&mut self) -> AutomationResult<()> {
        let Some(runtime) = self.interlude_quest.as_ref() else {
            return Ok(());
        };
        match runtime.phase {
            InterludeQuestPhase::NavigateMain => self.handle_interlude_navigate_main(),
            InterludeQuestPhase::NavigateGate => self.handle_interlude_navigate_gate(),
            InterludeQuestPhase::NeedRootList => {
                self.handle_interlude_list(InterludeListKind::Root)
            }
            InterludeQuestPhase::NeedStageList => {
                self.handle_interlude_list(InterludeListKind::Stage)
            }
            InterludeQuestPhase::AwaitingStage => Ok(()),
            _ => Ok(()),
        }
    }

    fn handle_interlude_navigate_main(&mut self) -> AutomationResult<()> {
        if self.tap_named_entry("main/main_chaldea_gate", "迦勒底之门", NAV_ENTRY_REGION)? {
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.phase = InterludeQuestPhase::NavigateGate;
            }
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }
        let swipes = {
            let Some(runtime) = self.interlude_quest.as_mut() else {
                return Ok(());
            };
            if runtime.main_swipes >= MAX_NAVIGATION_SWIPES {
                self.fail_action(
                    "InterludeQuest",
                    "寻找迦勒底之门入口",
                    "主界面未找到迦勒底之门入口，已停止".into(),
                );
                return Ok(());
            }
            runtime.main_swipes = runtime.main_swipes.saturating_add(1);
            runtime.main_swipes
        };
        self.emit_debug("InterludeQuest", "主界面滚动查找迦勒底之门入口");
        if swipes <= 3 {
            // Seek the top first: finger moves down, content scrolls up.
            let _ = self.swipe_with_settle_at(
                "InterludeQuest",
                Point::new(MAIN_SCROLL_X, MAIN_SCROLL_TOP),
                Point::new(MAIN_SCROLL_X, MAIN_SCROLL_BOTTOM),
                500,
                250,
            );
        } else {
            let _ = self.swipe_with_settle_at(
                "InterludeQuest",
                Point::new(MAIN_SCROLL_X, MAIN_SCROLL_BOTTOM),
                Point::new(MAIN_SCROLL_X, MAIN_SCROLL_TOP),
                500,
                250,
            );
        }
        thread::sleep(ACTION_DELAY);
        Ok(())
    }

    fn handle_interlude_navigate_gate(&mut self) -> AutomationResult<()> {
        if self.tap_named_entry("main/main_interlude", "幕间物语", GATE_ENTRY_REGION)? {
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.phase = InterludeQuestPhase::NeedRootList;
                runtime.current_list = InterludeListKind::Root;
                runtime.navigation_done = true;
            }
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }
        let swipes = {
            let Some(runtime) = self.interlude_quest.as_mut() else {
                return Ok(());
            };
            if runtime.gate_swipes >= MAX_NAVIGATION_SWIPES {
                self.fail_action(
                    "InterludeQuest",
                    "寻找幕间物语入口",
                    "迦勒底之门菜单未找到幕间物语入口，已停止".into(),
                );
                return Ok(());
            }
            runtime.gate_swipes = runtime.gate_swipes.saturating_add(1);
            runtime.gate_swipes
        };
        self.emit_debug("InterludeQuest", "迦勒底之门菜单滚动查找幕间物语入口");
        if swipes <= 3 {
            let _ = self.swipe_with_settle_at(
                "InterludeQuest",
                Point::new(MAIN_SCROLL_X, MAIN_SCROLL_TOP),
                Point::new(MAIN_SCROLL_X, MAIN_SCROLL_BOTTOM),
                500,
                250,
            );
        } else {
            let _ = self.swipe_with_settle_at(
                "InterludeQuest",
                Point::new(MAIN_SCROLL_X, MAIN_SCROLL_BOTTOM),
                Point::new(MAIN_SCROLL_X, MAIN_SCROLL_TOP),
                500,
                250,
            );
        }
        thread::sleep(ACTION_DELAY);
        Ok(())
    }

    fn tap_named_entry(
        &mut self,
        template: &str,
        keyword: &str,
        region: NormRect,
    ) -> AutomationResult<bool> {
        let found = self
            .sidecar()
            .find_element_with_reference_width(None, template, region, 0.70, 1920.0)
            .recoverable()?
            .ok()
            .flatten();
        if let Some(point) = found {
            self.emit("InterludeQuest", &format!("点击入口 {template}"));
            return Ok(self.tap_at("InterludeQuest", point));
        }

        // Template miss fallback: OCR the entry area for the title text.
        let ocr = self.sidecar().ocr_region(None, region).recoverable()?.ok();
        if let Some(ocr) = ocr {
            if let Some(fragment) = ocr
                .fragments
                .into_iter()
                .find(|fragment| fragment.text.contains(keyword))
            {
                let point = Point::new(
                    fragment.region.x + fragment.region.w * 0.5,
                    fragment.region.y + fragment.region.h * 0.5,
                );
                self.emit("InterludeQuest", &format!("OCR 找到入口 {keyword}，点击"));
                return Ok(self.tap_at("InterludeQuest", point));
            }
        }
        Ok(false)
    }

    fn handle_interlude_list(&mut self, kind: InterludeListKind) -> AutomationResult<()> {
        if self
            .interlude_quest
            .as_ref()
            .is_some_and(|runtime| runtime.single_failed)
        {
            self.fail_action(
                "InterludeQuest",
                "完成指定幕间任务",
                "指定幕间任务未完成（已撤退或被跳过），本次运行停止".into(),
            );
            return Ok(());
        }
        let screen_name = match kind {
            InterludeListKind::Root => "InterludeQuest",
            InterludeListKind::Stage => "InterludeStage",
        };
        let seeking_top = self
            .interlude_quest
            .as_ref()
            .map(|runtime| runtime.top_seeks)
            .unwrap_or(0);
        if seeking_top > 0 {
            let at_top = self
                .sidecar()
                .find_element_by_name(None, screen_name, "list_scroll_start")
                .recoverable()?
                .map(|result| result.found)
                .unwrap_or(true);
            if at_top {
                if let Some(runtime) = self.interlude_quest.as_mut() {
                    runtime.top_seeks = 0;
                }
            } else {
                if let Some(runtime) = self.interlude_quest.as_mut() {
                    runtime.top_seeks = runtime.top_seeks.saturating_sub(1);
                }
                self.emit_debug("InterludeQuest", "回到列表顶部");
                let _ = self.swipe_with_settle_at(
                    "InterludeQuest",
                    Point::new(LIST_SCROLL_X, LIST_SCROLL_TOP),
                    Point::new(LIST_SCROLL_X, LIST_SCROLL_BOTTOM),
                    500,
                    250,
                );
                thread::sleep(ACTION_DELAY);
                return Ok(());
            }
        }

        let rows = self
            .sidecar()
            .find_interlude_quest_rows(None)
            .recoverable()?
            .map_err(|error| error.to_string());
        let rows = match rows {
            Ok(rows) => rows.rows,
            Err(error) => {
                self.fail_action("InterludeQuest", "识别幕间物语任务列表", error);
                return Ok(());
            }
        };

        if let Some(runtime) = self.interlude_quest.as_mut() {
            let limit = runtime.config.task.max_repeat_attempts;
            let exhausted: Vec<String> = rows
                .iter()
                .filter(|row| {
                    row.actionable
                        && runtime.current_signature.as_deref()
                            == Some(row.title_signature.as_str())
                        && runtime.repeat_attempts >= limit
                })
                .map(|row| row.title_signature.clone())
                .collect();
            for signature in exhausted {
                if runtime.config.task.mode == InterludeQuestMode::Single
                    && runtime.current_signature.as_deref() == Some(signature.as_str())
                {
                    runtime.single_failed = true;
                }
                runtime.remember_handled(Some(signature));
            }
        }
        let target_match = {
            let Some(runtime) = self.interlude_quest.as_ref() else {
                return Ok(());
            };
            runtime.config.target.as_ref().map(|target| {
                rows.iter()
                    .find(|row| {
                        row.actionable
                            && interlude_title_signature_matches(
                                &row.title_signature,
                                &target.title_signature,
                            )
                    })
                    .cloned()
            })
        };
        if let Some(target) = self
            .interlude_quest
            .as_ref()
            .and_then(|runtime| runtime.config.target.as_ref())
        {
            let present = rows.iter().any(|row| {
                interlude_title_signature_matches(&row.title_signature, &target.title_signature)
            });
            let actionable = target_match.as_ref().is_some_and(|row| row.is_some());
            if let Some(runtime) = self.interlude_quest.as_mut() {
                if actionable {
                    runtime.target_seen = true;
                    runtime.target_unavailable = false;
                } else if present {
                    runtime.target_unavailable = true;
                }
            }
        }
        if let Some(Some(row)) = &target_match {
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.target_seen = true;
            }
            let point = Point::new(
                row.title_region.x + row.title_region.w * 0.5,
                row.title_region.y + row.title_region.h * 0.5,
            );
            self.emit("InterludeQuest", "选择指定幕间任务");
            if !self.tap_at("InterludeQuest", point) {
                return Ok(());
            }
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.note_clicked(row);
                runtime.phase = InterludeQuestPhase::AwaitingConfirm;
                runtime.current_list = kind;
                runtime.list_swipes = 0;
                runtime.top_seeks = 3;
            }
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }
        let single_target = self.interlude_quest.as_ref().and_then(|runtime| {
            runtime
                .config
                .target
                .as_ref()
                .map(|_| runtime.config.task.mode)
        });
        let selected = if single_target == Some(InterludeQuestMode::Single) {
            None
        } else {
            let Some(runtime) = self.interlude_quest.as_ref() else {
                return Ok(());
            };
            rows.iter()
                .filter(|row| row.actionable)
                .filter(|row| !runtime.is_handled(&row.title_signature))
                .filter(|row| {
                    !(runtime.current_signature.as_deref() == Some(row.title_signature.as_str())
                        && runtime.repeat_attempts >= runtime.config.task.max_repeat_attempts)
                })
                .min_by(|a, b| {
                    a.title_region
                        .y
                        .partial_cmp(&b.title_region.y)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .cloned()
        };

        let unavailable = rows.iter().filter(|row| !row.actionable).count() as u32;
        if let Some(runtime) = self.interlude_quest.as_mut() {
            runtime.unavailable_count = unavailable;
        }

        if let Some(row) = selected {
            let point = Point::new(
                row.title_region.x + row.title_region.w * 0.5,
                row.title_region.y + row.title_region.h * 0.5,
            );
            self.emit("InterludeQuest", &format!("选择幕间任务（{}）", row.kind));
            if !self.tap_at("InterludeQuest", point) {
                return Ok(());
            }
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.note_clicked(&row);
                runtime.phase = InterludeQuestPhase::AwaitingConfirm;
                runtime.current_list = kind;
                runtime.list_swipes = 0;
                runtime.top_seeks = 3;
            }
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }

        // No playable row on the visible page. Scroll further, or finish.
        let at_bottom = self
            .sidecar()
            .find_element_by_name(None, screen_name, "list_scroll_end")
            .recoverable()?
            .map(|result| result.found)
            .unwrap_or(false);
        let swipes = self
            .interlude_quest
            .as_ref()
            .map(|runtime| runtime.list_swipes)
            .unwrap_or(0);
        if at_bottom || swipes >= MAX_LIST_SWIPES {
            let single = self
                .interlude_quest
                .as_ref()
                .is_some_and(|runtime| runtime.config.task.mode == InterludeQuestMode::Single);
            if single {
                let failed = self
                    .interlude_quest
                    .as_ref()
                    .is_some_and(|runtime| runtime.single_failed);
                if failed {
                    self.fail_action(
                        "InterludeQuest",
                        "完成指定幕间任务",
                        "指定幕间任务未完成（已撤退或被跳过），本次运行停止".into(),
                    );
                    return Ok(());
                }
                let unavailable = self
                    .interlude_quest
                    .as_ref()
                    .is_some_and(|runtime| runtime.target_unavailable);
                if unavailable {
                    self.fail_action(
                        "InterludeQuest",
                        "完成指定幕间任务",
                        "指定幕间任务当前不可挑战（可能被遮罩/未开放），本次运行停止".into(),
                    );
                    return Ok(());
                }
                let seen = self
                    .interlude_quest
                    .as_ref()
                    .is_some_and(|runtime| runtime.target_seen);
                if seen {
                    if let Some(runtime) = self.interlude_quest.as_mut() {
                        runtime.completed_count = 1;
                    }
                    self.interlude_finish();
                } else {
                    self.fail_action(
                        "InterludeQuest",
                        "定位指定幕间任务",
                        "所选的幕间任务不在当前列表位置，请重新截图选择".into(),
                    );
                }
                return Ok(());
            }
            if kind == InterludeListKind::Stage {
                self.emit("InterludeQuest", "场景内没有可打幕间，返回幕间物语列表");
                if self.tap_at("InterludeQuest", Point::new(0.085, 0.178)) {
                    if let Some(runtime) = self.interlude_quest.as_mut() {
                        runtime.phase = InterludeQuestPhase::NeedRootList;
                        runtime.current_list = InterludeListKind::Root;
                        runtime.list_swipes = 0;
                        runtime.top_seeks = 3;
                    }
                    thread::sleep(ACTION_DELAY);
                }
                return Ok(());
            }
            self.interlude_finish();
            return Ok(());
        }

        if let Some(runtime) = self.interlude_quest.as_mut() {
            runtime.list_swipes = runtime.list_swipes.saturating_add(1);
            runtime.top_seeks = 0;
        }
        self.emit_debug("InterludeQuest", "当前页没有可打任务，向下滚动");
        let _ = self.swipe_with_settle_at(
            "InterludeQuest",
            Point::new(LIST_SCROLL_X, LIST_SCROLL_BOTTOM),
            Point::new(LIST_SCROLL_X, LIST_SCROLL_TOP),
            500,
            250,
        );
        thread::sleep(ACTION_DELAY);
        Ok(())
    }

    pub(crate) fn handle_interlude_confirm(&mut self, screen: Screen) -> AutomationResult<()> {
        match screen {
            Screen::QuestConfirm => {
                self.emit("InterludeQuest", "确认任务，点击开始");
                if self.tap_at("InterludeQuest", QUEST_CONFIRM_START) {
                    if let Some(runtime) = self.interlude_quest.as_mut() {
                        runtime.phase = InterludeQuestPhase::AwaitingDeparture;
                    }
                    thread::sleep(ACTION_DELAY);
                }
            }
            Screen::TurnLimitQuestConfirm => {
                let allow = self
                    .interlude_quest
                    .as_ref()
                    .map(|runtime| runtime.config.task.allow_turn_limit_quest)
                    .unwrap_or(false);
                if allow {
                    self.emit("InterludeQuest", "允许规定回合数任务，点击开始");
                    if self.tap_at("InterludeQuest", QUEST_CONFIRM_START) {
                        if let Some(runtime) = self.interlude_quest.as_mut() {
                            runtime.phase = InterludeQuestPhase::AwaitingDeparture;
                        }
                        thread::sleep(ACTION_DELAY);
                    }
                } else {
                    self.emit("InterludeQuest", "跳过规定回合数任务");
                    let _ = self.tap_at("InterludeQuest", QUEST_CONFIRM_CANCEL);
                    if let Some(runtime) = self.interlude_quest.as_mut() {
                        let signature = runtime.current_signature.clone();
                        runtime.remember_handled(signature);
                        if runtime.config.task.mode == InterludeQuestMode::Single {
                            runtime.single_failed = true;
                        }
                        runtime.phase = InterludeQuestPhase::NeedRootList;
                    }
                    thread::sleep(ACTION_DELAY);
                }
            }
            Screen::NoBattleQuestConfirm => {
                self.emit("InterludeQuest", "无战斗任务，点击任务开始");
                if self.tap_at("InterludeQuest", QUEST_CONFIRM_START) {
                    if let Some(runtime) = self.interlude_quest.as_mut() {
                        runtime.phase = InterludeQuestPhase::AwaitingReturn;
                        runtime.consecutive_failures = 0;
                    }
                    thread::sleep(ACTION_DELAY);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Consume dialogue before the main loop observes or routes the base screen.
    pub(crate) fn probe_interlude_dialogue(&mut self, screen: Screen) -> AutomationResult<bool> {
        if self.interlude_quest.is_none() {
            return Ok(false);
        }
        if screen == Screen::DialogSkipConfirm {
            self.note_interlude_dialogue(true);
            self.emit("InterludeQuest", "确认跳过剧情");
            if self.tap_at("InterludeQuest", DIALOGUE_YES) {
                thread::sleep(ACTION_DELAY);
            }
            return Ok(true);
        }

        let choices = match self.sidecar().find_interlude_dialogue_choices().recoverable()? {
            Ok(choices) => choices,
            Err(error) => {
                self.emit_debug("InterludeQuest", &format!("剧情选项识别失败: {error}"));
                // A failed probe cannot establish that skipping is safe.
                return Ok(false);
            }
        };
        if let Some(&point) = choices.first() {
            self.select_interlude_dialogue_choice(point);
            return Ok(true);
        }

        let skip = self
            .sidecar()
            .find_element_with_reference_width(
                None,
                "interlude/button_skip",
                DIALOGUE_SKIP_REGION,
                0.62,
                1920.0,
            )
            .recoverable()?
            .ok()
            .flatten();
        let controls = self
            .sidecar()
            .find_element_with_reference_width(
                None,
                "interlude/dialogue_auto_button",
                NormRect {
                    x: 0.90,
                    y: 0.78,
                    w: 0.10,
                    h: 0.18,
                },
                0.80,
                1920.0,
            )
            .recoverable()?
            .ok()
            .flatten();
        let dialogue_box = self
            .sidecar()
            .find_element_with_reference_width(
                None,
                "interlude/dialogue_box_corner",
                NormRect {
                    x: 0.0,
                    y: 0.62,
                    w: 0.35,
                    h: 0.14,
                },
                0.70,
                1920.0,
            )
            .recoverable()?
            .ok()
            .flatten();
        let choice = self
            .sidecar()
            .find_element_with_reference_width(
                None,
                "interlude/dialogue_choice_corner",
                NormRect {
                    x: 0.09,
                    y: 0.17,
                    w: 0.08,
                    h: 0.48,
                },
                0.90,
                1920.0,
            )
            .recoverable()?
            .ok()
            .flatten();
        if let Some(corner) = choice {
            self.select_interlude_dialogue_choice(Point::new(0.5, corner.y));
            return Ok(true);
        }

        let mut skip = skip;
        if skip.is_none() && matches!(screen, Screen::Unknown | Screen::Dialog) {
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.dialogue_ocr_tick = runtime.dialogue_ocr_tick.wrapping_add(1);
                if runtime.dialogue_ocr_tick % 2 == 1 {
                    skip = self
                        .sidecar()
                        .ocr_region(None, DIALOGUE_SKIP_REGION)
                        .recoverable()?
                        .ok()
                        .and_then(|ocr| {
                            ocr.fragments.into_iter().find(|item| item.text.contains("跳过"))
                        })
                        .map(|item| {
                            Point::new(
                                item.region.x + item.region.w * 0.5,
                                item.region.y + item.region.h * 0.5,
                            )
                        });
                }
            }
        }
        if skip.is_none() && controls.is_none() && dialogue_box.is_none() && screen != Screen::Dialog {
            if screen != Screen::Unknown {
                if let Some(runtime) = self.interlude_quest.as_mut() {
                    runtime.last_dialog_at = None;
                    runtime.dialogue_skip_attempts = 0;
                }
            }
            return Ok(false);
        }

        self.note_interlude_dialogue(false);
        if let Some(point) = skip {
            let runtime = self.interlude_quest.as_ref().unwrap();
            if runtime.dialogue_skip_attempts < MAX_DIALOGUE_SKIPS {
                self.emit_debug("InterludeQuest", "点击跳过剧情");
                if self.tap_at("InterludeQuest", point) {
                    self.interlude_quest.as_mut().unwrap().dialogue_skip_attempts += 1;
                    thread::sleep(ACTION_DELAY);
                }
                return Ok(true);
            }
        }

        let _ = self.tap_at("InterludeQuest", DIALOGUE_ADVANCE);
        thread::sleep(ACTION_DELAY);
        Ok(true)
    }

    fn note_interlude_dialogue(&mut self, reset_skip_attempts: bool) {
        if let Some(runtime) = self.interlude_quest.as_mut() {
            runtime.note_dialogue(Instant::now());
            if reset_skip_attempts {
                runtime.dialogue_skip_attempts = 0;
            }
        }
    }

    fn select_interlude_dialogue_choice(&mut self, point: Point) {
        self.note_interlude_dialogue(true);
        self.emit("InterludeQuest", "选择剧情选项");
        if self.tap_at("InterludeQuest", point) {
            thread::sleep(ACTION_DELAY);
        }
    }

    pub(crate) fn handle_interlude_formation_dialog(&mut self, screen: Screen) {
        match screen {
            Screen::FormationLimitDialog => {
                self.emit("InterludeQuest", "关闭编队限制提示");
                let _ = self.tap_at("InterludeQuest", FORMATION_LIMIT_CLOSE);
            }
            Screen::AutoFormationDialog => {
                let auto = self
                    .interlude_quest
                    .as_ref()
                    .map(|runtime| runtime.config.task.auto_formation)
                    .unwrap_or(true);
                let point = if auto {
                    AUTO_FORMATION_RIGHT
                } else {
                    AUTO_FORMATION_LEFT
                };
                self.emit(
                    "InterludeQuest",
                    if auto {
                        "选择自动编成"
                    } else {
                        "选择不进行自动编成"
                    },
                );
                let _ = self.tap_at("InterludeQuest", point);
            }
            Screen::EmptySlotAutoFormationDialog => {
                self.emit("InterludeQuest", "队伍有空位，选择自动编成");
                let _ = self.tap_at("InterludeQuest", AUTO_FORMATION_RIGHT);
            }
            Screen::FormationEditor => {
                self.emit("InterludeQuest", "确认自动编成队伍");
                let _ = self.tap_at("InterludeQuest", FORMATION_DECIDE);
            }
            Screen::TeamConfirmInterlude => {
                self.emit("InterludeQuest", "点击战斗开始");
                if self.tap_at("InterludeQuest", TEAM_START) {
                    if let Some(runtime) = self.interlude_quest.as_mut() {
                        runtime.awaiting_battle_start = true;
                    }
                }
            }
            _ => {}
        }
        thread::sleep(ACTION_DELAY);
    }

    pub(crate) fn handle_interlude_battle_continue(&mut self) {
        let Some(runtime) = self.interlude_quest.as_ref() else {
            return;
        };
        let want_charm = runtime.config.task.defeat_strategy
            == InterludeDefeatStrategy::CommandSpell
            && !runtime.charm_used_this_run;
        if want_charm {
            let charm = self
                .sidecar()
                .find_element_with_reference_width(
                    None,
                    "interlude/button_command_spell",
                    NormRect {
                        x: 0.36,
                        y: 0.40,
                        w: 0.28,
                        h: 0.10,
                    },
                    0.72,
                    1920.0,
                )
                .recoverable()
                .ok()
                .and_then(|result| result.ok())
                .flatten();
            if let Some(point) = charm {
                self.emit("InterludeQuest", "使用令咒续关");
                if self.tap_at("InterludeQuest", point) {
                    if let Some(runtime) = self.interlude_quest.as_mut() {
                        runtime.charm_used_this_run = true;
                    }
                    thread::sleep(ACTION_DELAY);
                }
                return;
            }
            self.emit("InterludeQuest", "令咒不可用，改为撤退");
        } else {
            self.emit("InterludeQuest", "战斗失败，撤退");
        }
        if self.tap_at("InterludeQuest", BATTLE_CONTINUE_RETREAT) {
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.consecutive_failures = runtime.consecutive_failures.saturating_add(1);
                let signature = runtime.current_signature.clone();
                runtime.remember_handled(signature);
                if runtime.config.task.mode == InterludeQuestMode::Single {
                    runtime.single_failed = true;
                }
                runtime.phase = InterludeQuestPhase::AwaitingReturn;
            }
            thread::sleep(ACTION_DELAY);
            let reached_limit = self.interlude_quest.as_ref().is_some_and(|runtime| {
                runtime.consecutive_failures >= runtime.config.task.max_consecutive_failures
            });
            if reached_limit {
                self.fail_action(
                    "InterludeQuest",
                    "连续战斗失败",
                    "连续战斗失败达到上限，已停止幕间物语自动化".into(),
                );
            }
        }
    }

    pub(crate) fn handle_interlude_charm_confirm(&mut self) {
        self.emit("InterludeQuest", "确认使用令咒");
        if self.tap_at("InterludeQuest", CHARM_CONFIRM_DECIDE) {
            self.battle = BattleState::new();
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.phase = InterludeQuestPhase::InQuest;
            }
            thread::sleep(ACTION_DELAY);
        }
    }

    /// Save one live frame when the interlude flow has been stuck on Unknown
    /// for a while; makes "what screen is it waiting for" debuggable.
    pub(crate) fn capture_interlude_unknown_frame(&mut self, count: u32) {
        let result = (|| -> Result<PathBuf, String> {
            let dir = crate::paths::app_data_dir(&self.app_handle)
                .join("debug")
                .join("interlude-quest");
            std::fs::create_dir_all(&dir).map_err(|error| format!("创建目录失败: {error}"))?;
            let path = dir.join(format!(
                "unknown-{}-{count}.png",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis())
                    .unwrap_or(0)
            ));
            let png = self
                .sidecar()
                .get_frame_png(0.0)
                .map_err(|error| error.to_string())?;
            std::fs::write(&path, png).map_err(|error| format!("写入失败: {error}"))?;
            Ok(path)
        })();
        match result {
            Ok(path) => self.emit_warn(
                "InterludeQuest",
                &format!("未知画面截图已保存: {}", path.display()),
            ),
            Err(error) => {
                self.emit_debug("InterludeQuest", &format!("未知画面截图保存失败: {error}"))
            }
        }
    }

    /// Close one recognized reward popup. The next poll reads a fresh frame
    /// because more rewards may follow without a list frame between them.
    pub(crate) fn handle_interlude_reward_popup(&mut self) -> AutomationResult<bool> {
        if !self.interlude_quest.as_ref().is_some_and(|runtime| {
            matches!(runtime.phase, InterludeQuestPhase::InQuest | InterludeQuestPhase::AwaitingReturn)
        }) {
            return Ok(false);
        }
        let ocr = match self
            .sidecar()
            .ocr_region(
                None,
                NormRect {
                    x: 0.10,
                    y: 0.10,
                    w: 0.80,
                    h: 0.87,
                },
            )
            .recoverable()?
        {
            Ok(ocr) => ocr,
            Err(_) => return Ok(false),
        };
        let Some(point) = interlude_reward_close_point(&ocr.fragments) else {
            return Ok(false);
        };
        self.mark_interlude_return_pending();
        self.emit("InterludeQuest", "识别到任务报酬，点击关闭");
        if self.tap_at("InterludeQuest", point) {
            thread::sleep(ACTION_DELAY);
        }
        Ok(true)
    }

    pub(crate) fn handle_interlude_profile_update(&mut self) {
        self.emit("InterludeQuest", "关闭从者资料更新提示");
        if self.tap_at("InterludeQuest", Point::new(0.340, 0.770)) {
            thread::sleep(ACTION_DELAY);
        }
    }

    pub(crate) fn handle_interlude_retreat_notice(&mut self) {
        self.emit("InterludeQuest", "关闭撤退提示");
        if self.tap_at("InterludeQuest", RETREAT_NOTICE_CLOSE) {
            if let Some(runtime) = self.interlude_quest.as_mut() {
                runtime.phase = InterludeQuestPhase::AwaitingReturn;
            }
            thread::sleep(ACTION_DELAY);
        }
    }

    pub(crate) fn handle_interlude_support_select(&mut self) -> AutomationResult<()> {
        if !self.support_selected {
            let npc = self
                .sidecar()
                .find_element_with_reference_width(
                    None,
                    "interlude/npc_badge",
                    NormRect {
                        x: 0.85,
                        y: 0.25,
                        w: 0.15,
                        h: 0.55,
                    },
                    0.85,
                    1920.0,
                )
                .recoverable()?
                .ok()
                .flatten();
            if let Some(point) = npc {
                self.emit("InterludeQuest", "优先选择 NPC 助战");
                if self.tap_at("SupportSelect", Point::new(0.55, point.y)) {
                    self.support_selected = true;
                    thread::sleep(ACTION_DELAY);
                }
                return Ok(());
            }
            // Some clients render the guest banner without the trailing
            // "NPC" badge; use the "客将从者" header as a second anchor.
            let guest = self
                .sidecar()
                .find_element_with_reference_width(
                    None,
                    "interlude/text_guest_servant",
                    NormRect {
                        x: 0.10,
                        y: 0.25,
                        w: 0.40,
                        h: 0.40,
                    },
                    0.72,
                    1920.0,
                )
                .recoverable()?
                .ok()
                .flatten();
            if let Some(point) = guest {
                self.emit("InterludeQuest", "优先选择客将从者");
                if self.tap_at("SupportSelect", Point::new(0.55, point.y + 0.045)) {
                    self.support_selected = true;
                    thread::sleep(ACTION_DELAY);
                }
                return Ok(());
            }
        }
        self.handle_support_select()
    }

    pub(crate) fn handle_interlude_team_confirm(&mut self) -> AutomationResult<()> {
        let empty = self
            .sidecar()
            .find_element_with_reference_width(
                None,
                "interlude/text_empty_slot",
                NormRect {
                    x: 0.03,
                    y: 0.20,
                    w: 0.94,
                    h: 0.62,
                },
                0.75,
                1920.0,
            )
            .recoverable()?
            .ok()
            .flatten();
        if empty.is_some() {
            self.emit("InterludeQuest", "队伍有空位，点击自动编队");
            let _ = self.tap_at("InterludeQuest", EMPTY_SLOT_AUTO_FORMATION);
            thread::sleep(ACTION_DELAY);
            return Ok(());
        }
        self.handle_team_confirm()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow() -> InterludeWorkflowConfig {
        InterludeWorkflowConfig {
            task: InterludeTaskConfig::default(),
            target: None,
        }
    }

    #[test]
    fn runtime_starts_at_main_navigation() {
        let runtime = InterludeQuestRuntime::new(workflow());

        assert_eq!(runtime.phase, InterludeQuestPhase::NavigateMain);
        assert!(!runtime.navigation_done);
        assert_eq!(runtime.completed_count, 0);
    }

    #[test]
    fn title_signature_exact_and_hamming_matching() {
        let base = "0123456789abcdef0123456789abcdef";
        assert!(interlude_title_signature_matches(base, base));
        assert!(interlude_title_signature_matches(
            "00000000000000000000000000000000",
            "00000000000000000000000000000001",
        ));
        assert!(!interlude_title_signature_matches(
            "00000000000000000000000000000000",
            "ffffffffffffffffffffffffffffffff",
        ));
        assert!(!interlude_title_signature_matches("not-hex", base));
    }

    #[test]
    fn task_defaults_match_the_agreed_configuration() {
        let task = InterludeTaskConfig::default();

        assert_eq!(task.mode, InterludeQuestMode::All);
        assert!(task.auto_formation);
        assert!(!task.allow_turn_limit_quest);
        assert_eq!(task.defeat_strategy, InterludeDefeatStrategy::Retreat);
        assert_eq!(task.max_consecutive_failures, 3);
        assert_eq!(task.max_repeat_attempts, 10);
        assert_eq!(task.dialogue_grace_seconds, 15);
    }

    #[test]
    fn fixed_formation_stops_waiting_for_battle_when_battle_or_results_arrive() {
        for screen in [Screen::Battle, Screen::Attack, Screen::BattleResultLoot] {
            let mut runtime = InterludeQuestRuntime::new(workflow());
            runtime.phase = InterludeQuestPhase::AwaitingDeparture;
            runtime.awaiting_battle_start = true;
            runtime.observe_battle_started(Screen::Unknown);
            assert!(runtime.awaiting_battle_start);
            runtime.observe_battle_started(screen);
            assert!(!runtime.awaiting_battle_start);
            assert_eq!(runtime.phase, InterludeQuestPhase::InQuest);
        }
    }

    fn ocr_fragment(text: &str, x: f64, y: f64) -> crate::screen::OcrFragment {
        crate::screen::OcrFragment {
            text: text.into(),
            region: NormRect { x, y, w: 0.10, h: 0.04 },
            ocr_confidence: 1.0,
        }
    }

    #[test]
    fn consecutive_reward_popups_each_use_their_detected_button() {
        for (title, button, x, y) in [
            ("任务完成", "关闭", 0.45, 0.80),
            ("获 得 报 酬", "确定", 0.57, 0.74),
            ("任务报酬", "OK", 0.44, 0.82),
            ("获得报酬", "关闭", 0.52, 0.85),
        ] {
            let point = interlude_reward_close_point(&[
                ocr_fragment(title, 0.40, 0.23),
                ocr_fragment(button, x, y),
            ]).unwrap();
            assert!((point.x - (x + 0.05)).abs() < 1e-6);
            assert!((point.y - (y + 0.02)).abs() < 1e-6);
        }
    }

    #[test]
    fn reward_popup_requires_a_title_and_a_button_below_it() {
        assert!(interlude_reward_close_point(&[
            ocr_fragment("编制限制", 0.40, 0.23),
            ocr_fragment("关闭", 0.45, 0.82),
        ]).is_none());
        assert!(interlude_reward_close_point(&[
            ocr_fragment("关闭", 0.45, 0.61),
            ocr_fragment("获得报酬", 0.40, 0.70),
        ]).is_none());
    }

    #[test]
    fn dialogue_grace_is_extended_by_each_observation() {
        let mut runtime = InterludeQuestRuntime::new(workflow());
        let start = Instant::now();
        assert!(!runtime.dialogue_grace_active(start));
        runtime.note_dialogue(start);
        let confirm = start + Duration::from_secs(12);
        runtime.note_dialogue(confirm);
        assert!(runtime.dialogue_grace_active(confirm + Duration::from_secs(14)));
        assert!(!runtime.dialogue_grace_active(confirm + Duration::from_secs(15)));
    }

    #[test]
    fn transition_messages_cover_navigation_and_waits() {
        assert_eq!(
            interlude_transition_message(InterludeQuestPhase::NavigateMain),
            None
        );
        assert_eq!(
            interlude_transition_message(InterludeQuestPhase::AwaitingReturn),
            Some("等待返回幕间物语列表…")
        );
        assert_eq!(
            interlude_transition_message(InterludeQuestPhase::NeedRootList),
            None
        );
    }
}
