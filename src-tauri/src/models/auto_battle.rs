//! Auto-battle strategy profiles.
//!
//! A profile is a named, reusable decision policy. Battle tasks reference a
//! profile by id and the runner only interprets the strategy, so future
//! "higher tiers" are new option values rather than new code paths.
//!
//! Every option here must stay inside what the CV sidecar can observe today
//! or is explicitly added to the plan. Options that need a known servant
//! identity (skill type / per-servant skill choice) or new recognition
//! (enemy HP, buffs) are intentionally absent.

use std::collections::HashSet;

/// Id of the seeded, undeletable basic profile.
pub const BUILTIN_BASIC_PROFILE_ID: &str = "builtin-basic";

pub const AUTO_BATTLE_PROFILE_NAME_MAX_CHARS: usize = 80;
pub const AUTO_BATTLE_PROFILE_SCHEMA_VERSION: u32 = 1;

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoBattleProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Built-in profiles are seeded on read when missing and cannot be
    /// deleted. Everything else is user owned.
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub strategy: AutoBattleStrategy,
    /// Milliseconds since the Unix epoch. Used for list ordering only.
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct AutoBattleStrategy {
    #[serde(default)]
    pub skill: SkillStrategy,
    #[serde(default)]
    pub skill_target: SkillTargetStrategy,
    #[serde(default)]
    pub noble_phantasm: NoblePhantasmStrategy,
    #[serde(default)]
    pub command_card: CommandCardStrategy,
    #[serde(default)]
    pub enemy_target: EnemyTargetStrategy,
    #[serde(default)]
    pub failure: FailurePolicy,
}

// ---------------------------------------------------------------------------
// Skill usage
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SkillUseMode {
    /// Tap every servant skill slot; the game turns down anything on cooldown.
    All,
    /// Tap only skills whose icon shows ready. Unknown readiness means the
    /// runner tries the skill anyway, so a missing CV probe degrades to `All`.
    #[default]
    ReadyOnly,
    /// Tap only the entries in `selected`.
    Selected,
    /// Never use servant skills.
    Off,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SkillOrder {
    /// Servant 1 skill 1→3, then servant 2, then servant 3.
    #[default]
    ServantMajor,
    /// Skill 1 of every servant, then skill 2, then skill 3.
    SkillMajor,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillSlotRef {
    /// 1-based front-line slot.
    pub servant_slot: u8,
    /// 1-based skill slot.
    pub skill_slot: u8,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SkillStrategy {
    #[serde(default)]
    pub mode: SkillUseMode,
    #[serde(default)]
    pub selected: Vec<SkillSlotRef>,
    /// Mystic-code skills are position-fixed too, but Order Change needs two
    /// targets, so the basic profile leaves them off by default.
    #[serde(default)]
    pub include_mystic_code: bool,
    #[serde(default)]
    pub order: SkillOrder,
}

impl Default for SkillStrategy {
    fn default() -> Self {
        Self {
            mode: SkillUseMode::ReadyOnly,
            selected: Vec::new(),
            include_mystic_code: false,
            order: SkillOrder::ServantMajor,
        }
    }
}

// ---------------------------------------------------------------------------
// Skill targeting
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SkillTargetMode {
    /// Tap the skill owner's own slot. Most skills are self/party buffs and
    /// this never needs to know which servant occupies the slot.
    #[default]
    OwnerSlot,
    /// Tap the leftmost front-line slot.
    LeftmostFront,
    /// Tap `selected_slot`.
    SelectedSlot,
    /// Probe: own slot, then enemy 1, then cancel the picker.
    Probe,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillTargetStrategy {
    #[serde(default)]
    pub mode: SkillTargetMode,
    /// 1-based front-line slot, used by `SelectedSlot`.
    #[serde(default)]
    pub selected_slot: Option<u8>,
}

// ---------------------------------------------------------------------------
// Noble Phantasm usage
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum NoblePhantasmUseMode {
    /// Use every ready NP, capped by `max_count`, left to right.
    #[default]
    AllReady,
    /// Use one ready NP per turn.
    Single,
    /// Use ready NPs only as the third card.
    LastCard,
    /// Never use NPs.
    Off,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NoblePhantasmStrategy {
    #[serde(default)]
    pub mode: NoblePhantasmUseMode,
    #[serde(default = "default_np_max_count")]
    pub max_count: u8,
}

fn default_np_max_count() -> u8 {
    3
}

impl Default for NoblePhantasmStrategy {
    fn default() -> Self {
        Self {
            mode: NoblePhantasmUseMode::AllReady,
            max_count: default_np_max_count(),
        }
    }
}

// ---------------------------------------------------------------------------
// Command-card selection
// ---------------------------------------------------------------------------

#[derive(
    serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default,
)]
#[serde(rename_all = "camelCase")]
pub enum CommandCardColor {
    #[default]
    Buster,
    Arts,
    Quick,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum CommandCardMode {
    /// Prefer three cards of the same color, in `color_order`; fall back to
    /// rainbow (one per color) and then to the leftmost actionable cards.
    #[default]
    SameColor,
    /// One card per color before anything else.
    Rainbow,
    /// Ignore colors; take actionable cards left to right.
    Leftmost,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandCardStrategy {
    #[serde(default)]
    pub mode: CommandCardMode,
    /// Preference order for `SameColor` and the rainbow fallback. Duplicates
    /// are rejected by validation.
    #[serde(default = "default_color_order")]
    pub color_order: Vec<CommandCardColor>,
}

fn default_color_order() -> Vec<CommandCardColor> {
    vec![
        CommandCardColor::Buster,
        CommandCardColor::Arts,
        CommandCardColor::Quick,
    ]
}

impl Default for CommandCardStrategy {
    fn default() -> Self {
        Self {
            mode: CommandCardMode::SameColor,
            color_order: default_color_order(),
        }
    }
}

// ---------------------------------------------------------------------------
// Enemy targeting and failure handling
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum EnemyTargetMode {
    /// Do not tap an enemy; the game keeps its default target.
    #[default]
    Default,
    /// Tap enemy 1.
    Leftmost,
    /// Tap `selected` (1–6).
    Selected,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnemyTargetStrategy {
    #[serde(default)]
    pub mode: EnemyTargetMode,
    #[serde(default)]
    pub selected: Option<u8>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum FailurePolicy {
    /// Log the step and continue with the rest of the turn.
    #[default]
    SkipStep,
    /// Surface the failure and stop the run.
    StopRun,
}

// ---------------------------------------------------------------------------
// Defaults, validation, builtin seed
// ---------------------------------------------------------------------------

impl Default for AutoBattleProfile {
    fn default() -> Self {
        builtin_basic_profile()
    }
}

/// The seeded "基础方案": ready-only skills, NP first, three-of-a-kind cards.
pub fn builtin_basic_profile() -> AutoBattleProfile {
    AutoBattleProfile {
        id: BUILTIN_BASIC_PROFILE_ID.into(),
        name: "基础方案".into(),
        description:
            "技能只放冷却好的，宝具就绪即用，指令卡优先三同色（红→蓝→绿），不主动选择敌方目标。"
                .into(),
        builtin: true,
        strategy: AutoBattleStrategy::default(),
        updated_at: 0,
    }
}

/// Trim user-visible text and drop values the runner cannot interpret.
/// The caller decides whether to reject or silently repair; saving uses
/// `validate` after this normalization so the stored shape is canonical.
pub fn normalize_auto_battle_profile(mut profile: AutoBattleProfile) -> AutoBattleProfile {
    profile.name = profile.name.trim().to_string();
    profile.description = profile.description.trim().to_string();
    profile.strategy.skill.selected = dedupe_skill_refs(&profile.strategy.skill.selected);
    profile.strategy.command_card.color_order =
        dedupe_colors(&profile.strategy.command_card.color_order);
    profile
}

fn dedupe_skill_refs(refs: &[SkillSlotRef]) -> Vec<SkillSlotRef> {
    let mut seen = HashSet::new();
    refs.iter()
        .filter(|entry| seen.insert((entry.servant_slot, entry.skill_slot)))
        .cloned()
        .collect()
}

fn dedupe_colors(colors: &[CommandCardColor]) -> Vec<CommandCardColor> {
    let mut seen = HashSet::new();
    colors
        .iter()
        .copied()
        .filter(|color| seen.insert(*color))
        .collect()
}

pub fn validate_auto_battle_profile(profile: &AutoBattleProfile) -> Result<(), String> {
    if profile.id.trim().is_empty() {
        return Err("自动战斗方案缺少 id".into());
    }
    if profile.name.trim().is_empty() {
        return Err("自动战斗方案名称不能为空".into());
    }
    if profile.name.trim().chars().count() > AUTO_BATTLE_PROFILE_NAME_MAX_CHARS {
        return Err(format!(
            "自动战斗方案名称不能超过 {AUTO_BATTLE_PROFILE_NAME_MAX_CHARS} 个字符"
        ));
    }

    let strategy = &profile.strategy;
    if strategy.skill.mode == SkillUseMode::Selected && strategy.skill.selected.is_empty() {
        return Err("技能策略为“仅指定技能”时至少需要勾选一个技能".into());
    }
    for entry in &strategy.skill.selected {
        if !(1..=3).contains(&entry.servant_slot) {
            return Err(format!("技能从者槽位必须为 1–3：{}", entry.servant_slot));
        }
        if !(1..=3).contains(&entry.skill_slot) {
            return Err(format!("技能槽位必须为 1–3：{}", entry.skill_slot));
        }
    }

    if strategy.skill_target.mode == SkillTargetMode::SelectedSlot {
        match strategy.skill_target.selected_slot {
            Some(slot) if (1..=3).contains(&slot) => {}
            _ => return Err("技能目标为“指定槽位”时必须选择 1–3 号前排".into()),
        }
    }

    if !(1..=3).contains(&strategy.noble_phantasm.max_count) {
        return Err("宝具使用上限必须为 1–3".into());
    }

    if strategy.command_card.color_order.is_empty() {
        return Err("指令卡颜色顺序不能为空".into());
    }
    let unique_colors: HashSet<CommandCardColor> =
        strategy.command_card.color_order.iter().copied().collect();
    if unique_colors.len() != strategy.command_card.color_order.len() {
        return Err("指令卡颜色顺序不能重复".into());
    }

    if strategy.enemy_target.mode == EnemyTargetMode::Selected {
        match strategy.enemy_target.selected {
            Some(index) if (1..=6).contains(&index) => {}
            _ => return Err("目标选择为“指定敌人”时必须选择 1–6 号敌人".into()),
        }
    }

    Ok(())
}

/// Guarantee the builtin profile exists exactly once and is marked builtin.
pub fn ensure_builtin_profile(profiles: &mut Vec<AutoBattleProfile>) {
    let builtin = builtin_basic_profile();
    if let Some(existing) = profiles
        .iter_mut()
        .find(|profile| profile.id == BUILTIN_BASIC_PROFILE_ID)
    {
        existing.builtin = true;
        // Keep user edits to the builtin profile; only repair a missing name.
        if existing.name.trim().is_empty() {
            *existing = builtin;
        }
        return;
    }
    profiles.insert(0, builtin);
}

pub fn is_builtin_profile_id(id: &str) -> bool {
    id == BUILTIN_BASIC_PROFILE_ID
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profile_is_the_basic_tier() {
        let profile = builtin_basic_profile();
        assert_eq!(profile.id, BUILTIN_BASIC_PROFILE_ID);
        assert!(profile.builtin);
        assert_eq!(profile.strategy.skill.mode, SkillUseMode::ReadyOnly);
        assert_eq!(profile.strategy.skill.order, SkillOrder::ServantMajor);
        assert!(!profile.strategy.skill.include_mystic_code);
        assert_eq!(
            profile.strategy.skill_target.mode,
            SkillTargetMode::OwnerSlot
        );
        assert_eq!(
            profile.strategy.noble_phantasm.mode,
            NoblePhantasmUseMode::AllReady
        );
        assert_eq!(profile.strategy.noble_phantasm.max_count, 3);
        assert_eq!(
            profile.strategy.command_card.mode,
            CommandCardMode::SameColor
        );
        assert_eq!(
            profile.strategy.command_card.color_order,
            vec![
                CommandCardColor::Buster,
                CommandCardColor::Arts,
                CommandCardColor::Quick
            ]
        );
        assert_eq!(profile.strategy.enemy_target.mode, EnemyTargetMode::Default);
        assert_eq!(profile.strategy.failure, FailurePolicy::SkipStep);
    }

    #[test]
    fn serde_round_trip_uses_camel_case_and_survives_missing_fields() {
        let profile = builtin_basic_profile();
        let json = serde_json::to_value(&profile).unwrap();
        assert_eq!(json["strategy"]["skillTarget"]["mode"], "ownerSlot");
        assert_eq!(json["strategy"]["noblePhantasm"]["mode"], "allReady");
        assert_eq!(json["strategy"]["commandCard"]["colorOrder"][0], "buster");

        // A profile saved before later fields existed must still deserialize.
        let legacy = serde_json::json!({
            "id": "user-1",
            "name": "旧方案",
            "strategy": {}
        });
        let parsed: AutoBattleProfile = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.strategy, AutoBattleStrategy::default());
        assert!(!parsed.builtin);
        assert_eq!(parsed.updated_at, 0);
    }

    #[test]
    fn normalize_trims_and_dedupes() {
        let mut profile = AutoBattleProfile {
            id: "user-1".into(),
            name: "  我的方案  ".into(),
            description: "  说明  ".into(),
            builtin: false,
            updated_at: 0,
            strategy: AutoBattleStrategy::default(),
        };
        profile.strategy.skill.selected = vec![
            SkillSlotRef {
                servant_slot: 1,
                skill_slot: 2,
            },
            SkillSlotRef {
                servant_slot: 1,
                skill_slot: 2,
            },
            SkillSlotRef {
                servant_slot: 3,
                skill_slot: 1,
            },
        ];
        profile.strategy.command_card.color_order = vec![
            CommandCardColor::Arts,
            CommandCardColor::Arts,
            CommandCardColor::Buster,
        ];
        let normalized = normalize_auto_battle_profile(profile);
        assert_eq!(normalized.name, "我的方案");
        assert_eq!(normalized.description, "说明");
        assert_eq!(normalized.strategy.skill.selected.len(), 2);
        assert_eq!(
            normalized.strategy.command_card.color_order,
            vec![CommandCardColor::Arts, CommandCardColor::Buster]
        );
    }

    #[test]
    fn validation_rejects_out_of_range_and_empty_values() {
        let mut profile = builtin_basic_profile();
        assert!(validate_auto_battle_profile(&profile).is_ok());

        profile.name = "   ".into();
        assert!(validate_auto_battle_profile(&profile).is_err());

        let mut profile = builtin_basic_profile();
        profile.strategy.noble_phantasm.max_count = 4;
        assert!(validate_auto_battle_profile(&profile).is_err());

        let mut profile = builtin_basic_profile();
        profile.strategy.command_card.color_order = vec![];
        assert!(validate_auto_battle_profile(&profile).is_err());

        let mut profile = builtin_basic_profile();
        profile.strategy.command_card.color_order =
            vec![CommandCardColor::Arts, CommandCardColor::Arts];
        assert!(validate_auto_battle_profile(&profile).is_err());

        let mut profile = builtin_basic_profile();
        profile.strategy.enemy_target.mode = EnemyTargetMode::Selected;
        profile.strategy.enemy_target.selected = Some(7);
        assert!(validate_auto_battle_profile(&profile).is_err());

        let mut profile = builtin_basic_profile();
        profile.strategy.skill.mode = SkillUseMode::Selected;
        profile.strategy.skill.selected.clear();
        assert!(validate_auto_battle_profile(&profile).is_err());
    }

    #[test]
    fn ensure_builtin_profile_is_idempotent_and_marks_builtin() {
        let mut profiles = Vec::new();
        ensure_builtin_profile(&mut profiles);
        ensure_builtin_profile(&mut profiles);
        assert_eq!(profiles.len(), 1);
        assert!(profiles[0].builtin);

        // A hand-edited file that lost the flag gets repaired without losing
        // the user's strategy tweaks.
        profiles[0].builtin = false;
        profiles[0].strategy.noble_phantasm.mode = NoblePhantasmUseMode::Single;
        ensure_builtin_profile(&mut profiles);
        assert!(profiles[0].builtin);
        assert_eq!(
            profiles[0].strategy.noble_phantasm.mode,
            NoblePhantasmUseMode::Single
        );

        // A profile that was renamed to empty is restored.
        profiles[0].name.clear();
        ensure_builtin_profile(&mut profiles);
        assert_eq!(profiles[0].name, "基础方案");
    }
}
