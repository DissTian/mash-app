//! Main battle automation loop and high-level screen routing.
//!
//! This module keeps the polling loop and Battle screen transition logic together.
//! Domain-specific actions are delegated to support, prebattle, battle result, AP,
//! party, and attack modules so screen routing remains the only responsibility here.

use super::*;

pub(super) fn unknown_screen_wait_message(
    server: Server,
    unknown_count: u32,
    timeout: u32,
) -> String {
    let server_label = match server {
        Server::Cn => "国服",
        Server::Jp => "日服",
    };
    format!("等待识别画面[{server_label}]... ({unknown_count}/{timeout})")
}

pub(super) fn support_search_screen_exited(previous: Screen, current: Screen) -> bool {
    previous == Screen::SupportSelect
        && current != Screen::SupportSelect
        && current != Screen::Unknown
}

impl Runner {
    // -- main loop -----------------------------------------------------------

    pub fn run(mut self) {
        if let Err(message) = self.run_loop() {
            self.transition_lifecycle(RunnerLifecycleEvent::Failed {
                message: message.to_string(),
            });
            self.emit("CV", &message.to_string());
            self.save_recognition_failure(&message.to_string());
        }
    }

    fn run_loop(&mut self) -> AutomationResult<()> {
        if self.interlude_quest_active() {
            self.sidecar().use_interlude_detection();
        } else {
            self.sidecar().use_battle_detection();
        }
        let timeout = self.config.unknown_screen_timeout_count;
        if let Err(error) = self.sidecar().configure_recognition_diagnostics(timeout) {
            self.emit_debug("CV", &error);
        }
        self.transition_lifecycle(RunnerLifecycleEvent::WorkerStarted);
        if let Some(recorder) = &self.run_recorder {
            recorder.mark_running();
        }
        self.emit_run_progress();
        self.emit("", "自动化已启动");

        let mut unknown_count: u32 = 0;
        let mut last_detected_screen = Screen::Unknown;

        loop {
            self.checkpoint_run_statistics_if_due();
            if self.is_cancelled() {
                self.transition_lifecycle(RunnerLifecycleEvent::StopRequested);
                self.emit("", "自动化已停止");
                return Ok(());
            }

            let mut screen = match self.sidecar().detect(None).recoverable()? {
                Ok(s) => s,
                Err(e) => {
                    self.transition_lifecycle(RunnerLifecycleEvent::Failed {
                        message: e.to_string(),
                    });
                    self.emit("", &format!("画面识别失败: {e}"));
                    self.save_recognition_failure(&e.to_string());
                    return Ok(());
                }
            };

            if !self.interlude_navigation_phase()
                && !self.interlude_stage_recovery_active()
                && self.probe_interlude_dialogue(screen)?
            {
                unknown_count = 0;
                self.battle.reset_battle_load_wait(Instant::now());
                thread::sleep(POLL_INTERVAL);
                continue;
            }
            // Dialogue is unexpected during navigation. A failed dialogue
            // probe must also fall through to waiting, not advance a phase.
            if matches!(screen, Screen::Dialog | Screen::DialogSkipConfirm) {
                screen = Screen::Unknown;
            }

            if screen == Screen::Unknown {
                if self.try_interlude_stage_continue_prompt()? || self.try_interlude_stage_menu(screen)? {
                    if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                        return Ok(());
                    }
                    unknown_count = 0;
                    thread::sleep(POLL_INTERVAL);
                    continue;
                }

                // A reward may appear immediately after the last dialogue.
                // Keep the unknown-screen budget so an unresponsive popup
                // cannot cause an unlimited number of close attempts.
                if unknown_count.saturating_add(1) < timeout
                    && self.handle_interlude_reward_popup()?
                {
                    unknown_count += 1;
                    thread::sleep(POLL_INTERVAL);
                    continue;
                }
                if !self.interlude_stage_recovery_active() && self.interlude_dialogue_grace_active() {
                    self.emit_debug("InterludeQuest", "剧情播放中，暂不累计未知画面超时");
                    thread::sleep(POLL_INTERVAL);
                    continue;
                }
            }

            if support_search_screen_exited(last_detected_screen, screen) {
                self.release_support_ocr()?;
            }

            self.resolve_pending_ap_recovery(screen);

            let interlude_return_tapped = self.tap_interlude_return_if_due(screen);
            let interlude_result_tapped = screen == Screen::Unknown
                && !self.interlude_stage_recovery_active()
                && unknown_count.saturating_add(1) < timeout
                && self.tap_interlude_result_popup_if_due(last_detected_screen);
            if interlude_return_tapped || interlude_result_tapped {
                if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                    return Ok(());
                }
                unknown_count += 1;
                thread::sleep(POLL_INTERVAL);
                continue;
            }

            if self.handle_interlude_stage_wait(screen)? {
                if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                    return Ok(());
                }
                unknown_count = 0;
                thread::sleep(POLL_INTERVAL);
                continue;
            }

            if self.handle_interlude_navigation_wait(screen)? {
                if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                    return Ok(());
                }
                thread::sleep(POLL_INTERVAL);
                continue;
            }

            if !self.observe_interlude_quest_screen(screen) {
                return Ok(());
            }
            if !self.observe_rank_up_quest_screen(screen) {
                return Ok(());
            }

            if screen != Screen::FrontlineIncompleteDialog && self.handle_battle_load_screen(screen)? {
                if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                    return Ok(());
                }
                thread::sleep(POLL_INTERVAL);
                continue;
            }

            // Battle needs an attack-button probe first; other observations
            // either keep animation skips running (Unknown) or stop them.
            if screen != Screen::Battle && !self.skip_attack_animation_if_due(screen, false) {
                return Ok(());
            }

            if screen != Screen::BattleResultContinue {
                self.battle_result_continue_handled = false;
            }
            if screen != Screen::BattleResultLoot {
                self.battle_result_loot_handled = false;
            }
            if screen != Screen::BattleResultFriendRequest {
                self.battle_result_friend_request_handled = false;
            }
            if screen != Screen::BattleResultBond {
                self.battle_result_bond_handled = false;
            }

            match screen {
                Screen::Main
                | Screen::ChaldeaGate
                | Screen::InterludeQuest
                | Screen::InterludeStage => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_quest_screen(screen)?;
                }
                Screen::QuestConfirm
                | Screen::TurnLimitQuestConfirm
                | Screen::NoBattleQuestConfirm => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_confirm(screen)?;
                }
                Screen::FormationSourceDialog
                | Screen::FormationConditionDialog
                | Screen::FormationUnsatisfiedDialog
                | Screen::FormationLimitDialog
                | Screen::AutoFormationDialog
                | Screen::EmptySlotAutoFormationDialog
                | Screen::FormationEditor => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_formation_dialog(screen)?;
                }
                Screen::FrontlineIncompleteDialog => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_frontline_incomplete()?;
                }
                Screen::Dialog | Screen::DialogSkipConfirm => {}
                Screen::BattleContinue => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_battle_continue();
                }
                Screen::CharmConfirm => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_charm_confirm();
                }
                Screen::InterludeUnlocked => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_unlocked()?;
                }
                Screen::MasterMissionUpdate => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_master_mission_update()?;
                }
                Screen::ServantProfileUpdate => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_profile_update();
                }
                Screen::RetreatConfirm => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_retreat_confirm()?;
                }
                Screen::RetreatNotice => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_interlude_retreat_notice();
                }
                Screen::RankUpQuest => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_rank_up_quest()?;
                }
                Screen::TeamConfirm | Screen::TeamConfirmInterlude => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    if self.interlude_quest_active() {
                        self.handle_interlude_team_confirm(screen)?;
                    } else {
                        self.handle_team_confirm()?;
                    }
                }
                Screen::TeamChange => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_team_change();
                }
                Screen::SupportSelect => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    if self.interlude_quest_active() {
                        self.handle_interlude_support_select()?;
                    } else {
                        self.handle_support_select()?;
                    }
                }
                Screen::ServantSelect => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    if self.interlude_restricted_selection_active() {
                        self.handle_interlude_support_select()?;
                    } else {
                        self.handle_servant_select()?;
                    }
                }
                Screen::Battle => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle()?;
                }
                Screen::Attack => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    if self.battle.awaiting_attack_resolution() {
                        match attack_submission_wait_gate(
                            self.battle.attack_submission_started_at(),
                            Instant::now(),
                            ATTACK_SUBMISSION_STUCK_TIMEOUT,
                        ) {
                            AttackSubmissionWaitGate::TimedOut => {
                                self.recover_stuck_attack_selection()?;
                            }
                            AttackSubmissionWaitGate::Waiting
                            | AttackSubmissionWaitGate::NotWaiting => {
                                self.emit("Attack", "已提交本轮选卡，等待攻击动画");
                            }
                        }
                    } else {
                        self.battle
                            .transition(BattleFlowEvent::AttackScreenDetected);
                        self.handle_attack()?;
                    }
                }
                Screen::BattleResultBond => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_bond()?;
                }
                Screen::BattleResultExp => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_exp()?;
                }
                Screen::BattleResultLoot => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_loot()?;
                }
                Screen::BattleResultFriendRequest => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_friend_request()?;
                }
                Screen::BattleResultContinue => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_battle_result_continue();
                }
                Screen::APRecovery => {
                    unknown_count = 0;
                    last_detected_screen = screen;
                    self.handle_ap_recovery()?;
                }
                Screen::Unknown => {
                    unknown_count += 1;
                    let timeout = self.config.unknown_screen_timeout_count;
                    if unknown_count >= timeout {
                        let message = if self.interlude_navigation_phase() {
                            "当前不是干净的主界面/迦勒底之门（可能有登录奖励或公告弹窗），请先关闭后重试"
                        } else if self.rank_up_quest_transition_message().is_some() {
                            "等待强化任务页面切换超时"
                        } else {
                            "无法识别当前画面"
                        };
                        self.transition_lifecycle(RunnerLifecycleEvent::Failed {
                            message: message.into(),
                        });
                        self.emit(
                            if self.interlude_navigation_phase() {
                                "InterludeQuest"
                            } else if self.rank_up_quest_transition_message().is_some() {
                                "RankUpQuest"
                            } else {
                                "Unknown"
                            },
                            &format!("{message}，已停止"),
                        );
                        self.save_recognition_failure(message);
                        return Ok(());
                    }
                    if let Some(message) = self.interlude_quest_transition_message() {
                        self.emit(
                            "InterludeQuest",
                            &format!("{message} ({unknown_count}/{timeout})"),
                        );
                        thread::sleep(POLL_INTERVAL);
                        continue;
                    }
                    if let Some(message) = self.rank_up_quest_transition_message() {
                        self.emit(
                            "RankUpQuest",
                            &format!("{message} ({unknown_count}/{timeout})"),
                        );
                        if is_battle_result_screen(last_detected_screen) {
                            self.emit_debug(
                                "RankUpQuest",
                                "返回任务列表时可能被结算弹窗遮挡，尝试点击跳过区域",
                            );
                            if self.tap_at("RankUpQuest", BATTLE_RESULT_POPUP_SKIP) {
                                thread::sleep(ACTION_DELAY);
                            }
                        }
                        thread::sleep(POLL_INTERVAL);
                        continue;
                    }
                    self.emit(
                        "Unknown",
                        &unknown_screen_wait_message(self.server, unknown_count, timeout),
                    );
                    if !self.interlude_quest_active() && is_battle_result_screen(last_detected_screen) {
                        self.emit_debug("Unknown", "结算页可能被弹窗遮挡，尝试点击跳过区域");
                        if self.tap_at("Unknown", BATTLE_RESULT_POPUP_SKIP) {
                            thread::sleep(ACTION_DELAY);
                        }
                    }
                }
            }

            if matches!(*self.state.lock().unwrap(), RunnerState::Error { .. }) {
                return Ok(());
            }

            if matches!(*self.state.lock().unwrap(), RunnerState::Finished) {
                self.emit("", "自动化已完成");
                return Ok(());
            }

            thread::sleep(POLL_INTERVAL);
        }
    }

    // -- battle screen handlers ----------------------------------------------

    fn skip_attack_animation_if_due(&mut self, screen: Screen, actionable: bool) -> bool {
        if self
            .battle
            .attack_animation_skip_due(screen, actionable, Instant::now())
        {
            return self.tap_skip_animation_button("出卡后等待战斗动作");
        }
        true
    }

    /// Consume transient screen observations after a successful start tap.
    /// Keep the normal screen router from treating them as fresh setup pages.
    fn handle_battle_load_screen(&mut self, screen: Screen) -> AutomationResult<bool> {
        Ok(
            match self.battle.observe_battle_load(
                screen,
                Instant::now(),
                self.config.unknown_screen_timeout_count,
            ) {
                BattleLoadGate::NotWaiting => false,
                BattleLoadGate::Waiting => {
                    self.emit("TeamConfirm", "等待进入战斗页面…");
                    true
                }
                BattleLoadGate::RetryStart { attempt } => {
                    self.retry_quest_start(attempt)?;
                    true
                }
                gate @ (BattleLoadGate::TimedOut | BattleLoadGate::StartRetriesExhausted) => {
                    let message = if gate == BattleLoadGate::TimedOut {
                        "等待进入战斗超时"
                    } else {
                        "开始任务点击后仍停留在队伍确认页，已达到重试上限"
                    };
                    self.transition_lifecycle(RunnerLifecycleEvent::Failed {
                        message: message.into(),
                    });
                    self.emit("TeamConfirm", &format!("{message}，已停止"));
                    self.save_recognition_failure(message);
                    true
                }
            },
        )
    }

    fn handle_battle(&mut self) -> AutomationResult<()> {
        match attack_screen_wait_gate(
            self.battle.attack_screen_wait_started_at(),
            Instant::now(),
            ATTACK_SCREEN_WAIT_TIMEOUT,
        ) {
            AttackScreenWaitGate::Waiting => {
                self.emit("Battle", "已点击攻击按钮，等待指令卡画面…");
                return Ok(());
            }
            AttackScreenWaitGate::TimedOut => {
                self.battle
                    .transition(BattleFlowEvent::AttackScreenWaitTimedOut);
                self.emit_warn("Battle", "等待指令卡画面超时，恢复 Battle 处理");
            }
            AttackScreenWaitGate::NotWaiting => {}
        }

        // Check if the attack button is present (our turn to act)
        let attack_present = self
            .sidecar()
            .find_element_by_name(None, BATTLE_SCREEN, ATTACK_BUTTON_ELEMENT)
            .recoverable()?
            .map(|m| m.found)
            .unwrap_or(false);

        if !self.skip_attack_animation_if_due(Screen::Battle, attack_present) {
            return Ok(());
        }

        if !attack_present {
            self.emit("Battle", "等待战斗动作…");
            return Ok(());
        }

        let attack_returned_after_submit = self.battle.awaiting_attack_resolution();
        if !attack_returned_after_submit {
            self.battle.transition(BattleFlowEvent::BattleActionable);
        }

        // Read the current battle scene (m of n) from the BATTLE label HUD.
        let screen_scene = match self
            .sidecar()
            .read_battle_scene(None, BATTLE_SCENE_REGION)
            .recoverable()?
        {
            Ok(scene) => scene,
            Err(error) => {
                self.fail_action("Battle", "读取战斗场次", error.to_string());
                return Ok(());
            }
        };
        match post_attack_hud_read_gate(
            self.advanced_mode,
            attack_returned_after_submit,
            screen_scene,
            self.battle.post_attack_hud_wait_started_at(),
            Instant::now(),
            POST_ATTACK_HUD_READ_TIMEOUT,
        ) {
            PostAttackHudReadGate::Ready => {
                if attack_returned_after_submit {
                    self.battle
                        .transition(BattleFlowEvent::PostAttackHudResolved);
                }
            }
            PostAttackHudReadGate::Waiting { started_at } => {
                self.battle
                    .transition(BattleFlowEvent::PostAttackHudWaitStarted { at: started_at });
                self.emit("Battle", "等待读取 Battle HUD…");
                return Ok(());
            }
            PostAttackHudReadGate::TimedOut => {
                self.battle
                    .transition(BattleFlowEvent::PostAttackHudResolved);
                self.emit("Battle", "Battle HUD 读取超时，沿用当前 Turn 推进逻辑");
            }
        }
        let scene_m = screen_scene.map(|(m, _)| m);

        // Decide what to do based on (prior scene, fresh read). See
        // `tick_scene_state` for the full state-transition rules; the
        // key invariant is that a failed CV read (`scene_m == None`)
        // never advances the index, never re-locks `last_screen_scene`,
        // and never re-fires skills for an already-executed scene.
        let tick = tick_scene_state(
            self.battle.last_screen_scene,
            self.battle.current_scene_index,
            self.battle.executed_scene_index,
            scene_m,
        );
        let previous_scene_index = self.battle.current_scene_index;
        self.battle.current_scene_index = tick.current_scene_index;
        self.battle.last_screen_scene = tick.last_screen_scene;
        let scene_changed = self.battle.current_scene_index != previous_scene_index;
        if !self.advanced_mode {
            if scene_changed {
                self.battle.current_turn_index = 0;
            } else if attack_returned_after_submit {
                self.battle.current_turn_index = self.battle.current_turn_index.saturating_add(1);
            }
        }

        if self.advanced_mode && tick.needs_exec {
            self.battle.scene_config_used = false;
            let scene_str = match screen_scene {
                Some((m, n)) => format!("{m}/{n}"),
                None => "?".into(),
            };
            self.emit(
                "Battle",
                &format!(
                    "执行第 {} 组指令 (画面场景: {})",
                    self.battle.current_scene_index + 1,
                    scene_str,
                ),
            );
            self.battle.scene_config_used = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .is_some();
            self.battle.executed_scene_index = Some(self.battle.current_scene_index);
        } else if self.advanced_mode {
            self.emit("Battle", "场景未变更，直接攻击");
        } else {
            self.battle.scene_config_used = false;
            let turn_key = (
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            );
            let auto_takeover = self.auto_takeover_for_current_state();
            if auto_takeover {
                if self.battle.auto_skills_turn_key != Some(turn_key) {
                    self.emit(
                        "Battle",
                        &format!(
                            "自动战斗接管：Battle {} Turn {}",
                            self.battle.current_scene_index + 1,
                            self.battle.current_turn_index + 1,
                        ),
                    );
                    if !self.execute_auto_skills()? {
                        return Ok(());
                    }
                    self.battle.auto_skills_turn_key = Some(turn_key);
                } else {
                    self.emit("Battle", "自动战斗：本回合技能已完成，直接攻击");
                }
            } else if let Some((turn_cfg, over_configured_turns)) = normal_turn_for_current_state(
                &self.scenes,
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            ) {
                let scene_str = match screen_scene {
                    Some((m, n)) => format!("{m}/{n}"),
                    None => "?".into(),
                };
                if over_configured_turns {
                    self.emit(
                        "Battle",
                        &format!(
                            "Battle {} Turn {} 超出配置，沿用最后 Turn 的攻击配置 (画面场景: {})",
                            self.battle.current_scene_index + 1,
                            self.battle.current_turn_index + 1,
                            scene_str,
                        ),
                    );
                } else if self.battle.executed_turn_key != Some(turn_key) {
                    self.emit(
                        "Battle",
                        &format!(
                            "执行 Battle {} Turn {} 指令 (画面场景: {})",
                            self.battle.current_scene_index + 1,
                            self.battle.current_turn_index + 1,
                            scene_str,
                        ),
                    );
                    let resolved_turn = self.resolve_normal_turn_for_current_members(&turn_cfg);
                    if !self.execute_turn_skills(&resolved_turn)? {
                        return Ok(());
                    }
                    self.battle.executed_turn_key = Some(turn_key);
                    self.battle.scene_config_used = true;
                } else {
                    self.emit("Battle", "Turn 未变更，直接攻击");
                    self.battle.scene_config_used = true;
                }
            } else {
                self.emit(
                    "Battle",
                    &format!(
                        "无第 {} 组 Battle 配置，直接攻击",
                        self.battle.current_scene_index + 1
                    ),
                );
            }
        }

        if !self.advanced_mode {
            if self.auto_takeover_for_current_state() {
                let target = self.auto_enemy_target();
                if target.is_some() {
                    self.select_enemy_target(target.as_deref())?;
                }
            } else if let Some((turn_cfg, over_configured_turns)) = normal_turn_for_current_state(
                &self.scenes,
                self.battle.current_scene_index,
                self.battle.current_turn_index,
            ) {
                if !over_configured_turns {
                    self.select_enemy_target(turn_cfg.enemy_target.as_deref())?;
                }
            }
        }

        if self.advanced_mode && attack_returned_after_submit {
            if let Some(scene) = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .cloned()
            {
                if !self.execute_next_grand_turn_skills(&scene)? {
                    return Ok(());
                }
            }
        }

        if self.advanced_mode {
            if let Some(scene) = self
                .advanced_scenes
                .get(self.battle.current_scene_index)
                .cloned()
            {
                let grand_servants = self.grand_servant_runtime_configs();
                if grand_startup_can_run_before_attack(&scene, &grand_servants) {
                    if !self.prepare_grand_startup_before_attack(&scene)? {
                        return Ok(());
                    }
                }
            }
        }

        if self.advanced_mode {
            let target = advanced_enemy_target_for_current_scene(
                &self.advanced_scenes,
                self.battle.current_scene_index,
                tick.needs_exec,
            )
            .map(str::to_owned);
            self.select_enemy_target(target.as_deref())?;
        }

        // Click the attack button
        self.emit("Battle", "点击攻击按钮");
        if !self.tap_attack_button()? {
            return Ok(());
        }
        thread::sleep(ACTION_DELAY);
        Ok(())
    }
}
