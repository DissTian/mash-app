import goldFruitImage from "../../../src-tauri/resources/images/item_fruit_golden.png";
import silverFruitImage from "../../../src-tauri/resources/images/item_fruit_silver.png";
import bronzeFruitImage from "../../../src-tauri/resources/images/item_fruit_bronzed_cobalt.png";
import copperFruitImage from "../../../src-tauri/resources/images/item_fruit_bronze.png";
import saintQuartzImage from "../../../src-tauri/resources/images/item_saint_quartz.png";
import type {
  BattleApRecoveryItem,
  BattleApRecoveryLimits,
  Project,
} from "../../types/project";

export const AP_RECOVERY_OPTIONS: {
  value: BattleApRecoveryItem;
  label: string;
  recoveryLabel: string;
  imageSrc: string;
}[] = [
  {
    value: "gold",
    label: "黄金果实",
    recoveryLabel: "+100% 行动力",
    imageSrc: goldFruitImage,
  },
  {
    value: "silver",
    label: "白银果实",
    recoveryLabel: "+50% 行动力",
    imageSrc: silverFruitImage,
  },
  {
    value: "bronze",
    label: "青铜果实",
    recoveryLabel: "+40 行动力",
    imageSrc: bronzeFruitImage,
  },
  {
    value: "copper",
    label: "赤铜果实",
    recoveryLabel: "+10 行动力",
    imageSrc: copperFruitImage,
  },
  {
    value: "rainbow",
    label: "圣晶石",
    recoveryLabel: "+100% 行动力",
    imageSrc: saintQuartzImage,
  },
];

export const EMPTY_AP_RECOVERY_ITEMS: BattleApRecoveryItem[] = [];

export const UNLIMITED_AP_RECOVERY_LIMITS: BattleApRecoveryLimits = {
  rainbow: null,
  gold: null,
  silver: null,
  bronze: null,
  copper: null,
};

export const MAX_AP_RECOVERY_LIMIT = 4_294_967_295;

export const AP_RECOVERY_HELP_TEXT =
  "选中恢复道具后默认为无限使用；点击无限图标可设置本次运行的使用数量。某种道具达到上限后会继续尝试其他已选道具，全部达到上限后停止。";

export function orderedApRecoveryItems(
  project: Project | null | undefined,
): BattleApRecoveryItem[] {
  const configured = project?.apRecoveryItems ?? [];
  return AP_RECOVERY_OPTIONS.map((option) => option.value).filter((value) =>
    configured.includes(value),
  );
}

export function projectApRecoveryLimits(
  project: Project | null | undefined,
): BattleApRecoveryLimits {
  const configured = project?.apRecoveryLimits;
  return AP_RECOVERY_OPTIONS.reduce<BattleApRecoveryLimits>(
    (limits, option) => {
      const value = configured?.[option.value];
      limits[option.value] = normalizeApRecoveryLimit(value);
      return limits;
    },
    { ...UNLIMITED_AP_RECOVERY_LIMITS },
  );
}

/** `null` means unlimited; otherwise clamp to a positive integer. */
export function normalizeApRecoveryLimit(
  limit: number | null | undefined,
): number | null {
  if (limit == null) {
    return null;
  }
  if (typeof limit !== "number" || !Number.isInteger(limit) || limit <= 0) {
    return null;
  }
  return Math.min(MAX_AP_RECOVERY_LIMIT, limit);
}
