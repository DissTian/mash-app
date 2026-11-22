import type { BattleApRecoveryItem, BattleApRecoveryLimits } from "./project";

export type InterludeQuestMode = "all" | "single";
export type InterludeDefeatStrategy = "retreat" | "commandSpell";

export interface InterludeSupportConfig {
  servantId: number | null;
  servantVariantKey: string | null;
  craftEssenceIds: number[];
  craftEssenceMlbRequired: boolean;
  servantLevelMin: number | null;
  noblePhantasmLevelMin: number | null;
  starMapScoreMin: number | null;
  skillLevelMins: [number | null, number | null, number | null];
  appendSkillLevelMins: [number | null, number | null, number | null, number | null, number | null];
}

export interface InterludeTaskConfig {
  mode: InterludeQuestMode;
  support: InterludeSupportConfig;
  autoBattleProfileId: string | null;
  apRecoveryItems: BattleApRecoveryItem[];
  apRecoveryLimits: BattleApRecoveryLimits;
  autoFormation: boolean;
  allowTurnLimitQuest: boolean;
  defeatStrategy: InterludeDefeatStrategy;
  maxConsecutiveFailures: number;
  maxRepeatAttempts: number;
  dialogueGraceSeconds: number;
  stageRecoveryRetryLimit: number;
}

export interface InterludeRowRegion {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface InterludeQuestRow {
  candidateId: string;
  region: InterludeRowRegion;
  titleRegion: InterludeRowRegion;
  titleSignature: string;
  kind: "normal" | "quickLink" | "freeQuest" | string;
  actionable: boolean;
  anchorScore: number;
  meanLuma: number;
  meanSaturation: number;
  meanValue: number;
}

export interface InterludeCaptureResult {
  captureId: string;
  imagePath: string;
  rows: InterludeQuestRow[];
}

export const DEFAULT_INTERLUDE_TASK: InterludeTaskConfig = {
  mode: "all",
  support: {
    servantId: null,
    servantVariantKey: null,
    craftEssenceIds: [],
    craftEssenceMlbRequired: true,
    servantLevelMin: null,
    noblePhantasmLevelMin: null,
    starMapScoreMin: null,
    skillLevelMins: [null, null, null],
    appendSkillLevelMins: [null, null, null, null, null],
  },
  autoBattleProfileId: null,
  apRecoveryItems: [],
  apRecoveryLimits: {
    rainbow: null,
    gold: null,
    silver: null,
    bronze: null,
    copper: null,
  },
  autoFormation: true,
  allowTurnLimitQuest: false,
  defeatStrategy: "retreat",
  maxConsecutiveFailures: 3,
  maxRepeatAttempts: 10,
  dialogueGraceSeconds: 15,
  stageRecoveryRetryLimit: 30,
};
