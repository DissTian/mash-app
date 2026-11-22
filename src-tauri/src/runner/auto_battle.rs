//! Auto-battle takeover runtime.
//!
//! Once a project's manual commands stop applying, this module drives the
//! turn: it taps ready skills, then hands the attack chain to the pure
//! selector in `attack::auto`. Every skill tap is best-effort — a cooldown or
//! unsupported dialog must skip the step, never fail the run.

use super::*;

/// How long we wait for any visible reaction after tapping a skill before
/// treating it as "not usable this turn" (cooldown / already used).
/// Cooldown pre-skip for the `readyOnly` mode. The tight bottom-right crop
/// plus recognition-only batch was calibrated on real frames (9 slots in
/// ~480ms) and is the mode the CV debug tool validates.
const AUTO_SKILL_COOLDOWN_PROBE_ENABLED: bool = true;
const AUTO_SKILL_REACTION_TIMEOUT: Duration = Duration::from_millis(700);
/// Poll cadence while watching for a skill reaction.
const AUTO_SKILL_REACTION_POLL: Duration = Duration::from_millis(150);
/// How long we wait for the skill animation to finish and the action menu to
/// come back before tapping the next skill.
const AUTO_SKILL_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
/// Settle time after tapping a skill target.
const AUTO_TARGET_SETTLE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy)]
enum AutoSkillReaction {
    Confirm,
    AlreadyUsed,
    TargetPicker(Point),
    /// Tapping the slot opened the servant-status popup; close it and skip.
    StatusPopup(Point),
    /// The action menu disappeared: the skill actually started.
    AnimationStarted,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoSkillOutcome {
    Used,
    /// Normal skip (cooldown, already used, no visible reaction).
    Skipped,
    /// The auto system could not execute the skill (unsupported dialog or an
    /// unresolved target). `failure: stopRun` turns this into a run failure.
    Blocked,
    /// A tap left the battle flow and the recovery taps failed. The run must
    /// stop instead of tapping blind on the unknown screen.
    Fatal,
}

/// 0-based `(front-line slot, skill slot)` pairs in execution order.
pub(crate) fn auto_skill_plan(strategy: &AutoBattleStrategy) -> Vec<(usize, usize)> {
    let mut plan: Vec<(usize, usize)> = match strategy.skill.mode {
        SkillUseMode::Off => Vec::new(),
        SkillUseMode::All | SkillUseMode::ReadyOnly => (0..3)
            .flat_map(|servant| (0..3).map(move |skill| (servant, skill)))
            .collect(),
        SkillUseMode::Selected => strategy
            .skill
            .selected
            .iter()
            .filter(|entry| {
                (1..=3).contains(&entry.servant_slot) && (1..=3).contains(&entry.skill_slot)
            })
            .map(|entry| {
                (
                    usize::from(entry.servant_slot) - 1,
                    usize::from(entry.skill_slot) - 1,
                )
            })
            .collect(),
    };
    plan.sort_unstable();
    plan.dedup();
    match strategy.skill.order {
        SkillOrder::ServantMajor => {
            plan.sort_unstable_by_key(|(servant, skill)| (*servant, *skill))
        }
        SkillOrder::SkillMajor => plan.sort_unstable_by_key(|(servant, skill)| (*skill, *servant)),
    }
    plan
}

/// Bottom-right corner of one front-line skill button, relative to its center.
/// This is the only crop the cooldown probe reads, so icon art and the
/// "剩余 / 残り" wording cannot interfere.
pub(crate) fn skill_cooldown_corner_region(servant_slot: usize, skill_slot: usize) -> NormRect {
    let point = SERVANT_SKILLS[servant_slot.min(2)][skill_slot.min(2)];
    cooldown_corner_around(point)
}

pub(crate) fn equipment_cooldown_corner_region(skill_slot: usize) -> NormRect {
    cooldown_corner_around(EQUIPMENT_SKILLS[skill_slot.min(2)])
}

fn cooldown_corner_around(point: Point) -> NormRect {
    NormRect {
        x: point.x + SKILL_COOLDOWN_CORNER_DX,
        y: point.y + SKILL_COOLDOWN_CORNER_DY,
        w: SKILL_COOLDOWN_CORNER_WIDTH,
        h: SKILL_COOLDOWN_CORNER_HEIGHT,
    }
}

pub(crate) fn skill_cooldown_label_region(servant_slot: usize, skill_slot: usize) -> NormRect {
    cooldown_label_around(SERVANT_SKILLS[servant_slot.min(2)][skill_slot.min(2)])
}

pub(crate) fn equipment_cooldown_label_region(skill_slot: usize) -> NormRect {
    cooldown_label_around(EQUIPMENT_SKILLS[skill_slot.min(2)])
}

fn cooldown_label_around(point: Point) -> NormRect {
    NormRect {
        x: point.x + SKILL_COOLDOWN_LABEL_DX,
        y: point.y + SKILL_COOLDOWN_LABEL_DY,
        w: SKILL_COOLDOWN_LABEL_WIDTH,
        h: SKILL_COOLDOWN_LABEL_HEIGHT,
    }
}

/// A cooldown crop is judged cooling only when the recognized text is entirely
/// digits. Stray letters (``J``) and the CN/JP label word must never count.
pub(crate) fn cooldown_text_present(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty() && trimmed.chars().all(|character| character.is_numeric())
}

/// The label strip only contains "剩余"/"残り" while the skill is cooling.
pub(crate) fn cooldown_label_present(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.contains("剩余") || trimmed.contains("残")
}

/// A slot counts as cooling only when BOTH the digit crop is pure-numeric and
/// the label strip shows the cooldown word. The label filters icon-art arrows
/// that happen to be read as digits.
pub(crate) fn cooldown_is_cooling(digit_text: &str, label_text: &str) -> bool {
    cooldown_text_present(digit_text) && cooldown_label_present(label_text)
}

/// Map the three recognition-only texts to mystic-code readiness flags.
/// Missing/short results fall back to "ready" for the remaining slots.
pub(crate) fn equipment_readiness_from_texts(
    digit_texts: &[String],
    label_texts: &[String],
) -> [bool; 3] {
    let mut ready = [true; 3];
    for index in 0..3 {
        let digit = digit_texts.get(index).map(String::as_str).unwrap_or("");
        let label = label_texts.get(index).map(String::as_str).unwrap_or("");
        ready[index] = !cooldown_is_cooling(digit, label);
    }
    ready
}

fn ready_equipment_skills(skills: &[usize], readiness: Option<[bool; 3]>) -> Vec<usize> {
    skills
        .iter()
        .copied()
        .filter(|&slot| readiness.is_none_or(|ready| ready[slot]))
        .collect()
}

/// Whether one normal-mode turn actually carries manual commands. A default
/// turn created by the command editor is empty; it must not block takeover.
pub(crate) fn battle_turn_has_manual_config(turn: &BattleTurn) -> bool {
    !turn.preparation_actions.is_empty()
        || turn.attack_mode != AttackMode::Normal
        || turn.attack_priority.iter().any(|entry| {
            entry
                .card
                .as_deref()
                .is_some_and(|card| !card.trim().is_empty())
        })
        || turn
            .enemy_target
            .as_deref()
            .is_some_and(|target| !target.trim().is_empty())
}

/// Map the strategy's enemy-target mode to the runner's `enemy_N` key.
pub(crate) fn auto_enemy_target_for(strategy: &AutoBattleStrategy) -> Option<String> {
    match strategy.enemy_target.mode {
        EnemyTargetMode::Default => None,
        EnemyTargetMode::Leftmost => Some("enemy_1".into()),
        EnemyTargetMode::Selected => strategy
            .enemy_target
            .selected
            .filter(|index| (1..=6).contains(index))
            .map(|index| format!("enemy_{index}")),
    }
}

/// 0-based mystic-code skill slots, or empty when the profile disables them.
pub(crate) fn auto_equipment_skill_plan(strategy: &AutoBattleStrategy) -> Vec<usize> {
    if !strategy.skill.include_mystic_code || strategy.skill.mode == SkillUseMode::Off {
        return Vec::new();
    }
    (0..3).collect()
}

impl Runner {
    pub(crate) fn auto_battle_enabled(&self) -> bool {
        self.auto_battle.is_some() && !self.advanced_mode
    }

    /// True when the current `(battle scene, turn)` has no manual commands
    /// left, so the auto system owns this turn.
    pub(crate) fn auto_takeover_for_current_state(&self) -> bool {
        if !self.auto_battle_enabled() {
            return false;
        }
        match normal_turn_for_current_state(
            &self.scenes,
            self.battle.current_scene_index,
            self.battle.current_turn_index,
        ) {
            None => true,
            Some((turn, over_configured)) => {
                over_configured || !battle_turn_has_manual_config(&turn)
            }
        }
    }

    pub(crate) fn auto_enemy_target(&self) -> Option<String> {
        self.auto_battle.as_ref().and_then(auto_enemy_target_for)
    }

    /// Read each planned skill's bottom-right cooldown corner with one
    /// recognition-only batch. `None` means the probe is unavailable, in which
    /// case the caller falls back to tapping every skill until the game
    /// rejects it.
    fn read_auto_skill_readiness(
        &mut self,
        plan: &[(usize, usize)],
    ) -> AutomationResult<Option<[bool; 9]>> {
        let mut regions: Vec<NormRect> = plan
            .iter()
            .map(|&(servant_slot, skill_slot)| {
                skill_cooldown_corner_region(servant_slot, skill_slot)
            })
            .collect();
        let digit_count = regions.len();
        regions.extend(plan.iter().map(|&(servant_slot, skill_slot)| {
            skill_cooldown_label_region(servant_slot, skill_slot)
        }));
        let texts = match self
            .sidecar()
            .recognize_strips(None, &regions)
            .recoverable()
        {
            Ok(Ok(texts)) => texts,
            Ok(Err(error)) | Err(error) => {
                if !self.battle.auto_cooldown_probe_warned {
                    self.battle.auto_cooldown_probe_warned = true;
                    self.emit_warn(
                        "Battle",
                        &format!("自动战斗：技能冷却识别不可用，退回逐个点击（{error}）"),
                    );
                } else {
                    self.emit_debug(
                        "Battle",
                        &format!("自动战斗：技能冷却识别仍不可用：{error}"),
                    );
                }
                return Ok(None);
            }
        };

        let mut ready = [true; 9];
        let mut cooling = Vec::new();
        for (position, &(servant_slot, skill_slot)) in plan.iter().enumerate() {
            let index = servant_slot * 3 + skill_slot;
            let text = texts.get(position).map(String::as_str).unwrap_or("");
            let label = texts
                .get(digit_count + position)
                .map(String::as_str)
                .unwrap_or("");
            self.auto_skill_label_texts[index] = text.trim().to_string();
            self.emit_local_debug(
                "Battle",
                &format!(
                    "自动战斗：从者 {} 技能 {} 右下角 OCR {:?} 剩余 {:?}",
                    servant_slot + 1,
                    skill_slot + 1,
                    text,
                    label
                ),
            );
            if cooldown_is_cooling(text, label) {
                ready[index] = false;
                cooling.push(format!("从者{}技能{}", servant_slot + 1, skill_slot + 1));
            }
        }
        if !cooling.is_empty() {
            self.emit(
                "Battle",
                &format!(
                    "自动战斗：冷却技能 {}（跳过后不再点击）",
                    cooling.join("、")
                ),
            );
        }
        Ok(Some(ready))
    }

    /// Choose and submit the auto chain for the Attack screen.
    pub(crate) fn pick_and_tap_auto(
        &mut self,
        cards: &[CommandCardMatch],
        nps: &[NoblePhantasmMatch],
    ) -> AutomationResult<()> {
        let Some(strategy) = self.auto_battle.clone() else {
            return Ok(());
        };
        let picks = auto_attack_picks(cards, nps, &strategy);
        if picks.is_empty() {
            self.emit("Attack", "自动战斗：没有可用的指令卡，跳过");
            return Ok(());
        }
        let party_ids = self.build_party_ids();
        self.emit(
            "Attack",
            &format!(
                "自动战斗：按方案选择 {} 张（宝具 {}）",
                picks.len(),
                picks
                    .iter()
                    .filter(|pick| matches!(pick, Pick::Np { .. }))
                    .count()
            ),
        );
        self.tap_picks("Attack", &picks, cards, &party_ids)
    }

    /// Parse the sidecar skill-slot response into per-slot presence flags.
    pub(crate) fn skill_presence_from_response(response: &serde_json::Value) -> Option<[bool; 9]> {
        let slots = response.get("slots")?.as_array()?;
        let mut present = [true; 9];
        let mut seen = [false; 9];
        for slot in slots {
            let index = slot.get("index").and_then(|value| value.as_u64())? as usize;
            if index >= 9 {
                continue;
            }
            present[index] = slot
                .get("present")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            seen[index] = true;
        }
        // A partial response must not silently skip slots.
        if seen.iter().any(|seen| !seen) {
            return None;
        }
        Some(present)
    }

    /// Batch-read all three mystic-code skill corners while the panel is open.
    /// `None` means the probe failed and the caller should try every skill.
    fn read_auto_equipment_readiness(&mut self) -> AutomationResult<Option<[bool; 3]>> {
        let mut regions: Vec<NormRect> = (0..3).map(equipment_cooldown_corner_region).collect();
        regions.extend((0..3).map(equipment_cooldown_label_region));
        let texts = match self
            .sidecar()
            .recognize_strips(None, &regions)
            .recoverable()
        {
            Ok(Ok(texts)) => texts,
            Ok(Err(error)) | Err(error) => {
                self.emit_debug(
                    "Battle",
                    &format!("自动战斗：御主礼装冷却识别不可用（{error}），继续尝试"),
                );
                return Ok(None);
            }
        };
        let (digits, labels) = texts.split_at(texts.len().min(3));
        let ready = equipment_readiness_from_texts(digits, labels);
        let cooling = ready.iter().filter(|ready| !**ready).count();
        if cooling > 0 {
            self.emit(
                "Battle",
                &format!("自动战斗：御主礼装技能冷却 {cooling}/3，只使用就绪的"),
            );
        }
        Ok(Some(ready))
    }

    /// Read which front-line skill slots exist on the live Battle frame.
    /// `None` means the probe is unavailable and the caller should tap every
    /// planned slot (the status-popup fallback still applies).
    fn read_auto_skill_presence(&mut self) -> AutomationResult<Option<[bool; 9]>> {
        let points: Vec<Point> = SERVANT_SKILLS.iter().flatten().copied().collect();
        let response = match self
            .sidecar()
            .find_skill_slots(
                None,
                &points,
                SKILL_SLOT_SCAN_REGION,
                SKILL_SLOT_WHITE_LUMA,
                SKILL_SLOT_MIN_FRACTION,
            )
            .recoverable()
        {
            Ok(Ok(response)) => response,
            Ok(Err(error)) | Err(error) => {
                self.emit_debug(
                    "Battle",
                    &format!("自动战斗：技能位识别不可用，按全部技能位尝试（{error}）"),
                );
                return Ok(None);
            }
        };
        Ok(Self::skill_presence_from_response(&response))
    }

    /// Execute this turn's auto skills. Returns `false` when the run must
    /// stop (cancelled or a tap error); ordinary skips return `true`.
    pub(crate) fn execute_auto_skills(&mut self) -> AutomationResult<bool> {
        let Some(strategy) = self.auto_battle.clone() else {
            return Ok(true);
        };
        let plan = auto_skill_plan(&strategy);
        let equipment = auto_equipment_skill_plan(&strategy);
        if plan.is_empty() && equipment.is_empty() {
            return Ok(true);
        }
        let readiness = if AUTO_SKILL_COOLDOWN_PROBE_ENABLED
            && strategy.skill.mode == SkillUseMode::ReadyOnly
        {
            self.read_auto_skill_readiness(&plan)?
        } else {
            None
        };
        let presence = self.read_auto_skill_presence()?;
        if let Some(present) = &presence {
            let count = present.iter().filter(|present| **present).count();
            if count < 9 {
                self.emit(
                    "Battle",
                    &format!("自动战斗：技能位 {count}/9 存在，跳过空位"),
                );
            }
        }

        self.emit(
            "Battle",
            &format!(
                "自动战斗：执行技能（从者 {} 个{}）",
                plan.len(),
                if equipment.is_empty() {
                    String::new()
                } else {
                    format!("，御主礼装 {} 个", equipment.len())
                }
            ),
        );

        for (servant_slot, skill_slot) in plan {
            if self.is_cancelled() {
                return Ok(false);
            }
            let index = servant_slot * 3 + skill_slot;
            if presence.as_ref().is_some_and(|present| !present[index]) {
                self.emit_debug(
                    "Battle",
                    &format!(
                        "自动战斗：从者 {} 技能 {} 未解锁/空位，跳过",
                        servant_slot + 1,
                        skill_slot + 1
                    ),
                );
                continue;
            }
            if readiness.as_ref().is_some_and(|ready| !ready[index]) {
                self.emit_debug(
                    "Battle",
                    &format!(
                        "自动战斗：从者 {} 技能 {} 冷却中，跳过",
                        servant_slot + 1,
                        skill_slot + 1
                    ),
                );
                continue;
            }
            if !self.execute_auto_servant_skill(servant_slot, skill_slot)? {
                return Ok(false);
            }
        }
        if !self.execute_auto_equipment_skills(&equipment)? {
            return Ok(false);
        }

        if !self.wait_for_attack_button(BATTLE_SCREEN, AUTO_SKILL_WAIT_TIMEOUT)? {
            self.emit_warn(
                "Battle",
                "自动战斗：技能结束后等待攻击按钮超时，继续尝试攻击",
            );
        }
        Ok(true)
    }

    fn execute_auto_servant_skill(
        &mut self,
        servant_slot: usize,
        skill_slot: usize,
    ) -> AutomationResult<bool> {
        let servant = format!("servant_{}", servant_slot + 1);
        let skill = format!("skill_{}", skill_slot + 1);
        let Some(pos) = skill_position(Some(&servant), Some(&skill)) else {
            return Ok(true);
        };
        let label = format!(
            "自动战斗：从者 {} 技能 {}",
            servant_slot + 1,
            skill_slot + 1
        );
        self.emit("Battle", &label);
        if !self.tap_at("Battle", pos) {
            return Ok(false);
        }
        thread::sleep(SKILL_TAP_DELAY);
        let label_text = self.auto_skill_label_texts[servant_slot * 3 + skill_slot].clone();
        let outcome = self.finish_auto_skill(Some(servant_slot), &label, &label_text)?;
        self.handle_auto_skill_outcome(outcome, &label)
    }

    fn handle_auto_skill_outcome(
        &mut self,
        outcome: AutoSkillOutcome,
        label: &str,
    ) -> AutomationResult<bool> {
        if outcome == AutoSkillOutcome::Fatal {
            return Ok(false);
        }
        if outcome != AutoSkillOutcome::Blocked {
            return Ok(true);
        }
        let stop = self
            .auto_battle
            .as_ref()
            .is_some_and(|strategy| strategy.failure == FailurePolicy::StopRun);
        if stop {
            self.fail_action(
                "Battle",
                "自动战斗技能",
                format!("{label} 无法执行，已按方案停止"),
            );
            return Ok(false);
        }
        self.emit_warn("Battle", &format!("{label}：无法执行，已跳过"));
        Ok(true)
    }

    /// Scan all three cooldowns on the first open and reuse the result for
    /// the rest of this turn's mystic-code skills.
    fn execute_auto_equipment_skills(&mut self, skills: &[usize]) -> AutomationResult<bool> {
        if skills.is_empty() {
            return Ok(true);
        }
        if !self.open_auto_equipment_panel() {
            return Ok(false);
        }
        let ready = ready_equipment_skills(skills, self.read_auto_equipment_readiness()?);
        if ready.is_empty() {
            self.emit("Battle", "自动战斗：御主礼装技能全部冷却中，关闭面板");
            if !self.tap_at("Battle", EQUIPMENT_BUTTON) {
                return Ok(false);
            }
            thread::sleep(ACTION_DELAY);
            return Ok(true);
        }

        // The first skill uses the panel opened for the scan. Each later
        // skill needs a fresh panel, but uses the same scan result.
        for (index, skill_slot) in ready.into_iter().enumerate() {
            if self.is_cancelled() || (index > 0 && !self.open_auto_equipment_panel()) {
                return Ok(false);
            }
            if !self.tap_auto_equipment_skill(skill_slot)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn open_auto_equipment_panel(&mut self) -> bool {
        if !self.tap_at("Battle", EQUIPMENT_BUTTON) {
            return false;
        }
        thread::sleep(ACTION_DELAY + EQUIPMENT_PANEL_OPEN_EXTRA_WAIT);
        true
    }

    /// Tap one equipment skill while its panel is already open.
    fn tap_auto_equipment_skill(&mut self, skill_slot: usize) -> AutomationResult<bool> {
        let skill = format!("skill_{}", skill_slot + 1);
        let Some(pos) = equipment_skill_position(Some(&skill)) else {
            return Ok(true);
        };
        let label = format!("自动战斗：御主礼装技能 {}", skill_slot + 1);
        self.emit("Battle", &label);
        if !self.tap_at("Battle", pos) {
            return Ok(false);
        }
        thread::sleep(SKILL_TAP_DELAY);
        let outcome = self.finish_auto_skill(None, &label, "")?;
        if self.is_cancelled() || outcome == AutoSkillOutcome::Fatal {
            return Ok(false);
        }
        if outcome != AutoSkillOutcome::Used {
            // A skill that never activated leaves the mystic-code panel open;
            // tapping the panel button again restores the Battle HUD.
            if !self.tap_at("Battle", EQUIPMENT_BUTTON) {
                return Ok(false);
            }
            thread::sleep(ACTION_DELAY);
        }
        self.handle_auto_skill_outcome(outcome, &label)
    }

    fn finish_auto_skill(
        &mut self,
        owner_slot: Option<usize>,
        label: &str,
        label_text: &str,
    ) -> AutomationResult<AutoSkillOutcome> {
        let start = Instant::now();
        loop {
            if self.is_cancelled() {
                return Ok(AutoSkillOutcome::Skipped);
            }
            match self.probe_auto_skill_reaction()? {
                AutoSkillReaction::Confirm => {
                    self.emit_debug("Battle", &format!("{label}：检测到确认弹窗，点击确认"));
                    if !self.tap_at("Battle", SKILL_USE_CONFIRM_POINT) {
                        return Ok(AutoSkillOutcome::Skipped);
                    }
                    thread::sleep(ACTION_DELAY);
                }
                AutoSkillReaction::AlreadyUsed => {
                    let detail = if label_text.is_empty() {
                        String::new()
                    } else {
                        format!("（右下角 OCR：{label_text:?}）")
                    };
                    self.emit(
                        "Battle",
                        &format!("{label}：技能已使用或冷却中，跳过{detail}"),
                    );
                    let _ = self.tap_skip_animation_button("技能已使用弹窗，跳过该技能");
                    thread::sleep(ACTION_DELAY);
                    return Ok(AutoSkillOutcome::Skipped);
                }
                AutoSkillReaction::TargetPicker(close_point) => {
                    let resolved = self.pick_auto_skill_target(owner_slot, close_point)?;
                    if resolved {
                        self.tap_skill_animation_skip_twice("自动战斗技能后加速");
                    }
                    if !self.wait_for_attack_button(BATTLE_SCREEN, AUTO_SKILL_WAIT_TIMEOUT)? {
                        self.emit_warn("Battle", "自动战斗：技能目标后等待攻击按钮超时");
                    }
                    return Ok(if resolved {
                        AutoSkillOutcome::Used
                    } else {
                        AutoSkillOutcome::Blocked
                    });
                }
                AutoSkillReaction::StatusPopup(point) => {
                    self.emit(
                        "Battle",
                        &format!("{label}：技能位打开了从者状态弹层，关闭并跳过"),
                    );
                    if !self.tap_at("Battle", point) {
                        return Ok(AutoSkillOutcome::Skipped);
                    }
                    thread::sleep(ACTION_DELAY);
                    if !self.wait_for_attack_button(BATTLE_SCREEN, Duration::from_secs(5))? {
                        if self.recover_from_auto_skill_detail()? {
                            return Ok(AutoSkillOutcome::Skipped);
                        }
                        self.fail_action(
                            "Battle",
                            "自动战斗技能",
                            format!("{label} 状态弹层关闭后未回到战斗界面"),
                        );
                        return Ok(AutoSkillOutcome::Fatal);
                    }
                    return Ok(AutoSkillOutcome::Skipped);
                }
                AutoSkillReaction::AnimationStarted => {
                    // Speed up the skill cut-in: tap the turn-counter/skip pill
                    // once, wait a second, then tap again before waiting for the
                    // battle HUD to return.
                    self.tap_skill_animation_skip_twice("自动战斗技能后加速");
                    if self.wait_for_attack_button(BATTLE_SCREEN, AUTO_SKILL_WAIT_TIMEOUT)? {
                        return Ok(AutoSkillOutcome::Used);
                    }
                    // A tap on an empty slot opens the servant detail screen
                    // instead of starting an animation. Return to the battle
                    // HUD instead of tapping blind and getting stuck.
                    if self.recover_from_auto_skill_detail()? {
                        self.emit_warn(
                            "Battle",
                            &format!("{label}：技能位打开了其他界面，已返回战斗"),
                        );
                        return Ok(AutoSkillOutcome::Skipped);
                    }
                    self.fail_action(
                        "Battle",
                        "自动战斗技能",
                        format!("{label} 后未回到战斗界面，无法恢复"),
                    );
                    return Ok(AutoSkillOutcome::Fatal);
                }
                AutoSkillReaction::None => {
                    if start.elapsed() >= AUTO_SKILL_REACTION_TIMEOUT {
                        if self.close_auto_skill_dialog()? {
                            self.emit("Battle", &format!("{label}：技能选项暂不支持，已取消"));
                            let _ = self
                                .wait_for_attack_button(BATTLE_SCREEN, AUTO_SKILL_WAIT_TIMEOUT)?;
                        } else {
                            self.emit_debug(
                                "Battle",
                                &format!("{label}：没有可观察的反应（可能冷却中），跳过"),
                            );
                        }
                        return Ok(AutoSkillOutcome::Skipped);
                    }
                    thread::sleep(AUTO_SKILL_REACTION_POLL);
                }
            }
        }
    }

    fn probe_auto_skill_reaction(&mut self) -> AutomationResult<AutoSkillReaction> {
        match self.probe_skill_use_dialog().recoverable()? {
            Ok((probe, _image_path)) => {
                if let Some(state) = classify_skill_use_dialog_probe(&probe) {
                    return Ok(match state {
                        SkillUseDialogState::Confirm => AutoSkillReaction::Confirm,
                        SkillUseDialogState::AlreadyUsed => AutoSkillReaction::AlreadyUsed,
                    });
                }
            }
            Err(error) => {
                self.emit_debug("Battle", &format!("自动战斗：技能弹窗探测失败：{error}"));
            }
        }

        let picker = self
            .sidecar()
            .find_element_by_name(None, BATTLE_SCREEN, SKILL_TARGET_CLOSE_BUTTON_ELEMENT)
            .recoverable()?;
        if let Ok(matched) = picker {
            if matched.found {
                return Ok(AutoSkillReaction::TargetPicker(Point::new(
                    matched.x, matched.y,
                )));
            }
        }

        let attack_visible = match self
            .sidecar()
            .find_element_by_name(None, BATTLE_SCREEN, ATTACK_BUTTON_ELEMENT)
            .recoverable()?
        {
            Ok(matched) => matched.found,
            Err(error) => {
                self.emit_debug("Battle", &format!("自动战斗：攻击按钮探测失败：{error}"));
                false
            }
        };
        let menu_visible = match self
            .sidecar()
            .find_element_by_name(None, BATTLE_SCREEN, BATTLE_ACTION_MENU_ELEMENT)
            .recoverable()?
        {
            Ok(matched) => matched.found,
            Err(error) => {
                self.emit_debug("Battle", &format!("自动战斗：操作菜单探测失败：{error}"));
                false
            }
        };
        if !attack_visible && !menu_visible {
            // The HUD is hidden: either a skill animation started or an
            // empty slot opened the servant-status popup. The popup carries
            // the shared close button in the top-right corner.
            let status_close = self
                .sidecar()
                .find_element(
                    None,
                    SKILL_SELECTION_CLOSE_BUTTON_TEMPLATE,
                    AUTO_STATUS_CLOSE_REGION,
                    AUTO_STATUS_CLOSE_THRESHOLD,
                )
                .recoverable()?;
            match status_close {
                Ok(Some(point)) => return Ok(AutoSkillReaction::StatusPopup(point)),
                Ok(None) => {}
                Err(error) => {
                    self.emit_debug(
                        "Battle",
                        &format!("自动战斗：状态弹层关闭按钮探测失败：{error}"),
                    );
                }
            }
            return Ok(AutoSkillReaction::AnimationStarted);
        }
        Ok(AutoSkillReaction::None)
    }

    fn pick_auto_skill_target(
        &mut self,
        owner_slot: Option<usize>,
        close_point: Point,
    ) -> AutomationResult<bool> {
        let strategy = self.auto_battle.clone();
        let mode = strategy
            .as_ref()
            .map(|strategy| strategy.skill_target.mode)
            .unwrap_or(SkillTargetMode::OwnerSlot);
        let selected_slot = strategy
            .as_ref()
            .and_then(|strategy| strategy.skill_target.selected_slot)
            .unwrap_or(1);
        let owner = owner_slot.unwrap_or(0).min(2);

        let mut candidates: Vec<Point> = match mode {
            SkillTargetMode::OwnerSlot => vec![SKILL_TARGETS[owner]],
            SkillTargetMode::LeftmostFront => vec![SKILL_TARGETS[0]],
            SkillTargetMode::SelectedSlot => {
                vec![SKILL_TARGETS[usize::from(selected_slot.clamp(1, 3)) - 1]]
            }
            SkillTargetMode::Probe => vec![SKILL_TARGETS[owner], ENEMY_TARGETS[0]],
        };

        for point in candidates.drain(..) {
            if !self.tap_at("Battle", point) {
                return Ok(false);
            }
            thread::sleep(AUTO_TARGET_SETTLE);
            if !self.auto_target_picker_visible()? {
                return Ok(true);
            }
        }

        if self.tap_at("Battle", close_point) {
            thread::sleep(AUTO_TARGET_SETTLE);
        }
        Ok(!self.auto_target_picker_visible()?)
    }

    /// Close the servant-detail screen opened by tapping an empty skill slot.
    /// Tries the common back/close corners and reports whether the battle HUD
    /// came back.
    fn recover_from_auto_skill_detail(&mut self) -> AutomationResult<bool> {
        for point in [Point::new(0.05, 0.05), Point::new(0.95, 0.05)] {
            if self.is_cancelled() {
                return Ok(false);
            }
            if !self.tap_at("Battle", point) {
                return Ok(false);
            }
            thread::sleep(ACTION_DELAY);
            if self.wait_for_attack_button(BATTLE_SCREEN, Duration::from_secs(5))? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn auto_target_picker_visible(&mut self) -> AutomationResult<bool> {
        let matched = self
            .sidecar()
            .find_element_by_name(None, BATTLE_SCREEN, SKILL_TARGET_CLOSE_BUTTON_ELEMENT)
            .recoverable()?;
        Ok(matched.map(|element| element.found).unwrap_or(false))
    }

    /// Close one of the option dialogs the auto system cannot answer. Returns
    /// whether a dialog was actually found and dismissed.
    fn close_auto_skill_dialog(&mut self) -> AutomationResult<bool> {
        for region in [
            SELECT_ADD_INFO_CLOSE,
            SELECT_TREASURE_DEVICE_CLOSE,
            COMMAND_TYPE_SELF_TREASURE_DEVICE_CLOSE,
        ] {
            let found = match self
                .sidecar()
                .find_element(None, SKILL_SELECTION_CLOSE_BUTTON_TEMPLATE, region, 0.8)
                .recoverable()
            {
                Ok(result) => result,
                Err(error) => {
                    // Optional cleanup probe: a missing template must never
                    // fail the run.
                    self.emit_debug(
                        "Battle",
                        &format!("自动战斗：技能选项弹窗探测不可用：{error}"),
                    );
                    return Ok(false);
                }
            };
            if let Ok(Some(point)) = found {
                if self.tap_at("Battle", point) {
                    thread::sleep(ACTION_DELAY);
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strategy() -> AutoBattleStrategy {
        AutoBattleStrategy::default()
    }

    #[test]
    fn default_plan_covers_every_servant_skill_in_servant_major_order() {
        let plan = auto_skill_plan(&strategy());
        assert_eq!(plan.len(), 9);
        assert_eq!(plan[0], (0, 0));
        assert_eq!(plan[1], (0, 1));
        assert_eq!(plan[2], (0, 2));
        assert_eq!(plan[3], (1, 0));
        assert_eq!(plan[8], (2, 2));
    }

    #[test]
    fn skill_major_order_groups_the_same_skill_slot() {
        let mut strategy = strategy();
        strategy.skill.order = SkillOrder::SkillMajor;
        let plan = auto_skill_plan(&strategy);
        assert_eq!(plan[0], (0, 0));
        assert_eq!(plan[1], (1, 0));
        assert_eq!(plan[2], (2, 0));
        assert_eq!(plan[3], (0, 1));
    }

    #[test]
    fn selected_mode_only_returns_requested_skills_sorted() {
        let mut strategy = strategy();
        strategy.skill.mode = SkillUseMode::Selected;
        strategy.skill.selected = vec![
            SkillSlotRef {
                servant_slot: 3,
                skill_slot: 1,
            },
            SkillSlotRef {
                servant_slot: 1,
                skill_slot: 2,
            },
            SkillSlotRef {
                servant_slot: 1,
                skill_slot: 2,
            },
        ];
        let plan = auto_skill_plan(&strategy);
        assert_eq!(plan, vec![(0, 1), (2, 0)]);
    }

    #[test]
    fn mystic_code_scan_keeps_only_ready_slots_in_order() {
        let skills = [0, 1, 2];
        assert_eq!(
            ready_equipment_skills(&skills, Some([true, false, true])),
            vec![0, 2]
        );
        assert!(ready_equipment_skills(&skills, Some([false; 3])).is_empty());
        assert_eq!(ready_equipment_skills(&skills, None), skills);
    }

    #[test]
    fn mystic_code_skills_follow_the_include_switch() {
        let mut strategy = strategy();
        assert!(auto_equipment_skill_plan(&strategy).is_empty());
        strategy.skill.include_mystic_code = true;
        assert_eq!(auto_equipment_skill_plan(&strategy), vec![0, 1, 2]);
        strategy.skill.mode = SkillUseMode::Off;
        assert!(auto_equipment_skill_plan(&strategy).is_empty());
    }

    fn empty_turn() -> BattleTurn {
        BattleTurn {
            id: "turn".into(),
            preparation_actions: Vec::new(),
            servant_actions: Vec::new(),
            equipment_actions: Vec::new(),
            command_spell_actions: Vec::new(),
            enemy_target: None,
            attack_priority: Vec::new(),
            attack_mode: AttackMode::Normal,
            critical_strategy: CriticalAttackStrategy::default(),
            advanced_card_strategy: AdvancedCardStrategy::default(),
        }
    }

    #[test]
    fn empty_default_turns_do_not_block_takeover() {
        assert!(!battle_turn_has_manual_config(&empty_turn()));

        let mut turn = empty_turn();
        turn.attack_priority.push(AttackCard {
            id: "a".into(),
            card: Some("servant_1_np".into()),
            member_id: None,
            servant_id: None,
            is_support: false,
        });
        assert!(battle_turn_has_manual_config(&turn));

        let mut turn = empty_turn();
        turn.enemy_target = Some("enemy_2".into());
        assert!(battle_turn_has_manual_config(&turn));

        let mut turn = empty_turn();
        turn.attack_mode = AttackMode::Critical;
        assert!(battle_turn_has_manual_config(&turn));
    }

    #[test]
    fn presence_response_requires_every_slot() {
        let complete = serde_json::json!({
            "slots": (0..9).map(|index| serde_json::json!({
                "index": index,
                "present": index != 5,
            })).collect::<Vec<_>>()
        });
        let present = Runner::skill_presence_from_response(&complete).unwrap();
        assert_eq!(present[5], false);
        assert_eq!(present.iter().filter(|present| **present).count(), 8);

        let partial = serde_json::json!({"slots": [{"index": 0, "present": true}]});
        assert!(Runner::skill_presence_from_response(&partial).is_none());

        assert!(Runner::skill_presence_from_response(&serde_json::json!({})).is_none());
    }

    #[test]
    fn cooldown_corner_boxes_use_the_calibrated_tight_crop() {
        for servant in 0..3 {
            for skill in 0..3 {
                let point = SERVANT_SKILLS[servant][skill];
                let region = skill_cooldown_corner_region(servant, skill);
                assert!((region.x - (point.x + SKILL_COOLDOWN_CORNER_DX)).abs() < 1e-9);
                assert!((region.y - (point.y + SKILL_COOLDOWN_CORNER_DY)).abs() < 1e-9);
                assert_eq!(region.w, SKILL_COOLDOWN_CORNER_WIDTH);
                assert_eq!(region.h, SKILL_COOLDOWN_CORNER_HEIGHT);
            }
        }
        for skill in 0..3 {
            let point = EQUIPMENT_SKILLS[skill];
            let region = equipment_cooldown_corner_region(skill);
            assert!((region.x - (point.x + SKILL_COOLDOWN_CORNER_DX)).abs() < 1e-9);
            assert!((region.y - (point.y + SKILL_COOLDOWN_CORNER_DY)).abs() < 1e-9);
        }
    }

    #[test]
    fn equipment_readiness_requires_numeric_digit_and_label() {
        let digits = vec!["5".to_string(), "一".to_string(), "J".to_string()];
        let labels = vec!["剩余".to_string(), String::new(), String::new()];
        assert_eq!(
            equipment_readiness_from_texts(&digits, &labels),
            [false, true, true]
        );

        let digits = vec!["5".into(), "5".into(), "2".into()];
        let labels = vec!["剩余".into(), "残".into(), "剩余".into()];
        assert_eq!(
            equipment_readiness_from_texts(&digits, &labels),
            [false, false, false]
        );

        // Icon-art digits without the label must stay ready.
        let digits = vec!["2".into(), "2".into()];
        let labels = vec![String::new(), String::new()];
        assert_eq!(
            equipment_readiness_from_texts(&digits, &labels),
            [true, true, true]
        );

        assert_eq!(equipment_readiness_from_texts(&[], &[]), [true, true, true]);
    }

    #[test]
    fn label_word_confirms_cooldown() {
        assert!(cooldown_label_present("剩余"));
        assert!(cooldown_label_present("残り"));
        assert!(!cooldown_label_present("余"));
        assert!(!cooldown_label_present(""));
        assert!(cooldown_is_cooling("5", "剩余"));
        assert!(!cooldown_is_cooling("2", "司"));
        assert!(!cooldown_is_cooling("5", ""));
        assert!(!cooldown_is_cooling("J", "剩余"));
    }

    #[test]
    fn only_fully_numeric_texts_mark_cooldown() {
        assert!(cooldown_text_present("2"));
        assert!(cooldown_text_present("15"));
        assert!(cooldown_text_present(" 3 "));
        assert!(!cooldown_text_present(""));
        assert!(!cooldown_text_present("J"));
        assert!(!cooldown_text_present("余"));
        assert!(!cooldown_text_present("剩余"));
        assert!(!cooldown_text_present("3J"));
        assert!(!cooldown_text_present("剩余3"));
        assert!(!cooldown_text_present("X 乙 剩余"));
    }

    #[test]
    fn enemy_target_maps_strategy_modes() {
        let mut strategy = strategy();
        assert_eq!(auto_enemy_target_for(&strategy), None);

        strategy.enemy_target.mode = EnemyTargetMode::Leftmost;
        assert_eq!(auto_enemy_target_for(&strategy).as_deref(), Some("enemy_1"));

        strategy.enemy_target.mode = EnemyTargetMode::Selected;
        strategy.enemy_target.selected = Some(4);
        assert_eq!(auto_enemy_target_for(&strategy).as_deref(), Some("enemy_4"));

        strategy.enemy_target.selected = Some(9);
        assert_eq!(auto_enemy_target_for(&strategy), None);
    }
}
