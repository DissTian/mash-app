/**
 * Auto-battle strategy profiles. Mirrors the Rust types in
 * `src-tauri/src/models/auto_battle.rs` — keep the two in sync.
 *
 * The profile only describes decisions the runner can already observe
 * (slot positions, card colors, NP readiness, target pickers). Options that
 * would need servant identity or new recognition are intentionally absent.
 */

export const AUTO_BATTLE_BUILTIN_PROFILE_ID = "builtin-basic";

export type SkillUseMode = "all" | "readyOnly" | "selected" | "off";
export type SkillOrder = "servantMajor" | "skillMajor";

export interface SkillSlotRef {
  /** 1-based front-line slot. */
  servantSlot: number;
  /** 1-based skill slot. */
  skillSlot: number;
}

export interface SkillStrategy {
  mode: SkillUseMode;
  selected: SkillSlotRef[];
  /** Includes mystic-code skills after servant skills. Off in the basic profile. */
  includeMysticCode: boolean;
  order: SkillOrder;
}

export type SkillTargetMode = "ownerSlot" | "leftmostFront" | "selectedSlot" | "probe";

export interface SkillTargetStrategy {
  mode: SkillTargetMode;
  selectedSlot: number | null;
}

export type NoblePhantasmUseMode = "allReady" | "single" | "lastCard" | "off";

export interface NoblePhantasmStrategy {
  mode: NoblePhantasmUseMode;
  maxCount: number;
}

export type CommandCardColor = "buster" | "arts" | "quick";
export type CommandCardMode = "sameColor" | "rainbow" | "leftmost";

export interface CommandCardStrategy {
  mode: CommandCardMode;
  colorOrder: CommandCardColor[];
}

export type EnemyTargetMode = "default" | "leftmost" | "selected";

export interface EnemyTargetStrategy {
  mode: EnemyTargetMode;
  selected: number | null;
}

export type AutoBattleFailurePolicy = "skipStep" | "stopRun";

export interface AutoBattleStrategy {
  skill: SkillStrategy;
  skillTarget: SkillTargetStrategy;
  noblePhantasm: NoblePhantasmStrategy;
  commandCard: CommandCardStrategy;
  enemyTarget: EnemyTargetStrategy;
  failure: AutoBattleFailurePolicy;
}

export interface AutoBattleProfile {
  id: string;
  name: string;
  description: string;
  /** Seeded profiles cannot be deleted; only the strategy stays editable. */
  builtin: boolean;
  strategy: AutoBattleStrategy;
  /** Milliseconds since the Unix epoch; list ordering only. */
  updatedAt: number;
}
