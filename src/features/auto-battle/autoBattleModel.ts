import type {
  AutoBattleProfile,
  AutoBattleStrategy,
  CommandCardColor,
  CommandCardMode,
  EnemyTargetMode,
  NoblePhantasmUseMode,
  SkillOrder,
  SkillTargetMode,
  SkillUseMode,
} from "../../types/autoBattle";

/** Presentation-only option lists; validation and defaults stay in Rust. */

export const SKILL_USE_OPTIONS: { value: SkillUseMode; label: string }[] = [
  { value: "readyOnly", label: "只放冷却好的" },
  { value: "all", label: "全部尝试" },
  { value: "selected", label: "仅指定技能" },
  { value: "off", label: "不使用技能" },
];

export const SKILL_ORDER_OPTIONS: { value: SkillOrder; label: string }[] = [
  { value: "servantMajor", label: "按从者（1 号位技能 1→3）" },
  { value: "skillMajor", label: "按技能（技能 1 全队→3）" },
];

export const SKILL_TARGET_OPTIONS: { value: SkillTargetMode; label: string }[] = [
  { value: "ownerSlot", label: "技能所属从者自己" },
  { value: "leftmostFront", label: "最左前排" },
  { value: "selectedSlot", label: "指定前排" },
  { value: "probe", label: "探测兜底（自己→敌方 1→取消）" },
];

export const NOBLE_PHANTASM_OPTIONS: { value: NoblePhantasmUseMode; label: string }[] = [
  { value: "allReady", label: "就绪就用" },
  { value: "single", label: "每回合 1 张" },
  { value: "lastCard", label: "只作第三张" },
  { value: "off", label: "不使用宝具" },
];

export const COMMAND_CARD_OPTIONS: { value: CommandCardMode; label: string }[] = [
  { value: "sameColor", label: "三同色优先" },
  { value: "rainbow", label: "三色各一" },
  { value: "leftmost", label: "左到右" },
];

export const ENEMY_TARGET_OPTIONS: { value: EnemyTargetMode; label: string }[] = [
  { value: "default", label: "不选择（游戏默认）" },
  { value: "leftmost", label: "最左敌人" },
  { value: "selected", label: "指定敌人" },
];

export const COLOR_LABELS: Record<CommandCardColor, string> = {
  buster: "红卡",
  arts: "蓝卡",
  quick: "绿卡",
};

export const COLOR_OPTIONS: CommandCardColor[] = ["buster", "arts", "quick"];

/** Clone a profile for editing so the list stays untouched until save. */
export function cloneProfile(profile: AutoBattleProfile): AutoBattleProfile {
  return JSON.parse(JSON.stringify(profile)) as AutoBattleProfile;
}

/** Always display three color slots; save writes whatever the user arranged. */
export function fullColorOrder(order: CommandCardColor[]): CommandCardColor[] {
  const result = order.filter((color, index) => order.indexOf(color) === index);
  for (const color of COLOR_OPTIONS) {
    if (result.length >= 3) break;
    if (!result.includes(color)) result.push(color);
  }
  return result.slice(0, 3);
}

/** Swap a color into one slot, preserving uniqueness. */
export function moveColor(
  strategy: AutoBattleStrategy,
  index: number,
  color: CommandCardColor
): AutoBattleStrategy {
  const order = fullColorOrder(strategy.commandCard.colorOrder);
  const existing = order.indexOf(color);
  if (existing === index) return strategy;
  if (existing >= 0) {
    [order[index], order[existing]] = [order[existing], order[index]];
  } else {
    order[index] = color;
  }
  return {
    ...strategy,
    commandCard: { ...strategy.commandCard, colorOrder: order },
  };
}

export function toggleSkillRef(
  strategy: AutoBattleStrategy,
  servantSlot: number,
  skillSlot: number
): AutoBattleStrategy {
  const exists = strategy.skill.selected.some(
    (entry) => entry.servantSlot === servantSlot && entry.skillSlot === skillSlot
  );
  const selected = exists
    ? strategy.skill.selected.filter(
        (entry) => !(entry.servantSlot === servantSlot && entry.skillSlot === skillSlot)
      )
    : [...strategy.skill.selected, { servantSlot, skillSlot }];
  return { ...strategy, skill: { ...strategy.skill, selected } };
}

export function hasSelectedSkill(
  strategy: AutoBattleStrategy,
  servantSlot: number,
  skillSlot: number
): boolean {
  return strategy.skill.selected.some(
    (entry) => entry.servantSlot === servantSlot && entry.skillSlot === skillSlot
  );
}

/** One-line summary shown under each profile name. */
export function describeStrategy(strategy: AutoBattleStrategy): string {
  const skill = SKILL_USE_OPTIONS.find((option) => option.value === strategy.skill.mode)?.label;
  const np = NOBLE_PHANTASM_OPTIONS.find(
    (option) => option.value === strategy.noblePhantasm.mode
  )?.label;
  const card = COMMAND_CARD_OPTIONS.find(
    (option) => option.value === strategy.commandCard.mode
  )?.label;
  return [skill ? `技能：${skill}` : null, np ? `宝具：${np}` : null, card ? `卡色：${card}` : null]
    .filter(Boolean)
    .join(" · ");
}
