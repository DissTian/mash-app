//! Auto-battle takeover runtime.
//!
//! Once a project's manual commands stop applying, this module drives the
//! turn: it taps ready skills, then hands the attack chain to the pure
//! selector in `attack::auto`. Every skill tap is best-effort — a cooldown or
//! unsupported dialog must skip the step, never fail the run.

use super::*;

/// How long we wait for any visible reaction after tapping a skill before
/// treating it as "not usable this turn" (cooldown / already used).
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
            if !self.execute_auto_servant_skill(servant_slot, skill_slot)? {
                return Ok(false);
            }
        }
        for skill_slot in equipment {
            if self.is_cancelled() {
                return Ok(false);
            }
            if !self.execute_auto_equipment_skill(skill_slot)? {
                return Ok(false);
            }
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
        let outcome = self.finish_auto_skill(Some(servant_slot), &label)?;
        self.handle_auto_skill_outcome(outcome, &label)
    }

    fn handle_auto_skill_outcome(
        &mut self,
        outcome: AutoSkillOutcome,
        label: &str,
    ) -> AutomationResult<bool> {
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

    fn execute_auto_equipment_skill(&mut self, skill_slot: usize) -> AutomationResult<bool> {
        let skill = format!("skill_{}", skill_slot + 1);
        let Some(pos) = equipment_skill_position(Some(&skill)) else {
            return Ok(true);
        };
        let label = format!("自动战斗：御主礼装技能 {}", skill_slot + 1);
        self.emit("Battle", &label);
        if !self.tap_at("Battle", EQUIPMENT_BUTTON) {
            return Ok(false);
        }
        thread::sleep(ACTION_DELAY);
        if !self.tap_at("Battle", pos) {
            return Ok(false);
        }
        thread::sleep(SKILL_TAP_DELAY);
        let outcome = self.finish_auto_skill(None, &label)?;
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
                    self.emit("Battle", &format!("{label}：技能已使用或冷却中，跳过"));
                    let _ = self.tap_skip_animation_button("技能已使用弹窗，跳过该技能");
                    thread::sleep(ACTION_DELAY);
                    return Ok(AutoSkillOutcome::Skipped);
                }
                AutoSkillReaction::TargetPicker(close_point) => {
                    let resolved = self.pick_auto_skill_target(owner_slot, close_point)?;
                    if !self.wait_for_attack_button(BATTLE_SCREEN, AUTO_SKILL_WAIT_TIMEOUT)? {
                        self.emit_warn("Battle", "自动战斗：技能目标后等待攻击按钮超时");
                    }
                    return Ok(if resolved {
                        AutoSkillOutcome::Used
                    } else {
                        AutoSkillOutcome::Blocked
                    });
                }
                AutoSkillReaction::AnimationStarted => {
                    if !self.wait_for_attack_button(BATTLE_SCREEN, AUTO_SKILL_WAIT_TIMEOUT)? {
                        self.emit_warn("Battle", "自动战斗：技能动画后等待攻击按钮超时");
                    }
                    return Ok(AutoSkillOutcome::Used);
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
