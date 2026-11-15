import type {
  Project,
  SupportAppendSkillLevelMins,
  SupportGrandBondCeMode,
  SupportSkillLevelMins,
} from "../../types/project";
import type {
  SupportCeArtworkCheckDto,
  SupportCeIconCheckDto,
  SupportRowMatchDto,
} from "./debugTypes";

export const BATTLE_SCENE_FAIL_HINTS: Record<string, string> = {
  empty_region: "区域为空（NormRect 越界？）",
  missing_label_template: "缺少 text_battle_label 模板",
  region_smaller_than_label: "区域比锚点模板还小，请扩大",
  anchor_below_threshold: "未找到 BATTLE 锚点（被遮挡或区域偏移）",
  strip_too_narrow: "锚点右侧空间不足以容纳数字",
  no_digit_candidates: "右侧未匹配到任何数字（阈值 ≥ 0.8）",
  fewer_than_two_digits: "数字不足 2 个（无法切分 m/n）",
  no_separator_gap: "数字间无足够间隔（找不到斜杠位置）",
  cohesion_trim_emptied_side: "切分后某一侧无有效数字（误检过多）",
  parse_error: "数字解析失败",
};

export const EMPTY_DEBUG_SELECT_VALUE = "__empty__";

const DEBUG_PREFS_STORAGE_KEY = "mash.debugPagePrefs.v1";

export interface DebugPagePrefs {
  debugView?: "canvas" | "tools" | "flow";
  selectedScreen?: string;
  selectedElement?: string;
  rawTemplateInput?: string;
  threshold?: number;
  selectedCardServantIds?: number[];
  supportTeamProjectId?: string | null;
  supportServantId?: number | null;
  supportServantVariantKey?: string | null;
  supportServantLevelMin?: number | null;
  supportNoblePhantasmLevelMin?: number | null;
  supportSkillLevelMins?: (number | null)[];
  supportAppendSkillLevelMins?: (number | null)[];
  supportCraftEssenceId?: number | null;
  supportCraftEssenceMlbRequired?: boolean;
  supportGrandCraftEssenceIds?: [number | null, number | null, number | null];
  supportGrandCraftEssenceMlbRequired?: [boolean, boolean, boolean];
  supportGrandBondCeMode?: SupportGrandBondCeMode;
  supportScoreGrandMode?: boolean;
  supportStarMapScoreMin?: number | null;
  supportGrandStarMapScoreMin?: number | null;
  supportReleaseOcr?: boolean;
  enhancementServantId?: number | null;
  enhancementServantThreshold?: string;
  enhancementTwoExp?: boolean;
  enhancementUseLiveFrame?: boolean;
  showDigitRecognitionRegions?: boolean;
  visibleCoordGroups?: string[];
}

/**
 * Rebuild the 3-slot owned-skill minimum tuple from persisted prefs, filling
 * missing entries with `null` so the shared settings panel always gets a
 * complete tuple.
 */
export function asSupportSkillLevelMins(value: unknown): SupportSkillLevelMins {
  const list = Array.isArray(value) ? value : [];
  return [0, 1, 2].map((index) => asNumberOrNull(list[index])) as SupportSkillLevelMins;
}

/** Same as {@link asSupportSkillLevelMins} for the 5-slot append skills. */
export function asSupportAppendSkillLevelMins(
  value: unknown
): SupportAppendSkillLevelMins {
  const list = Array.isArray(value) ? value : [];
  return [0, 1, 2, 3, 4].map((index) =>
    asNumberOrNull(list[index])
  ) as SupportAppendSkillLevelMins;
}

/**
 * Filter/rank template keys for the raw-template picker.
 *
 * Substring hits come first (prefix before mid-string), then subsequence
 * ("fuzzy") hits, so `batack` still finds `battle/attack_button`. Ties keep
 * the original alphabetical order.
 */
export function filterTemplateKeys(keys: string[], query: string): string[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return keys;
  const ranked: { key: string; rank: number; index: number }[] = [];
  keys.forEach((key, index) => {
    const haystack = key.toLowerCase();
    const substring = haystack.indexOf(needle);
    if (substring >= 0) {
      ranked.push({ key, rank: substring === 0 ? 0 : 1, index });
      return;
    }
    let position = 0;
    for (const char of needle) {
      const found = haystack.indexOf(char, position);
      if (found < 0) return;
      position = found + 1;
    }
    ranked.push({ key, rank: 2, index });
  });
  ranked.sort((a, b) => a.rank - b.rank || a.index - b.index);
  return ranked.map((entry) => entry.key);
}

/**
 * Support-recognition parameters derived from one project ("队伍"), mirroring
 * the field mapping the runner uses for the same project. The debug page
 * applies this so its manual recognition matches production behaviour.
 */
export interface SupportDebugConfig {
  supportServantId: number | null;
  supportServantVariantKey: string | null;
  supportServantLevelMin: number | null;
  supportNoblePhantasmLevelMin: number | null;
  supportSkillLevelMins: SupportSkillLevelMins;
  supportAppendSkillLevelMins: SupportAppendSkillLevelMins;
  supportCraftEssenceId: number | null;
  supportCraftEssenceMlbRequired: boolean;
  supportGrandCraftEssenceIds: [number | null, number | null, number | null];
  supportGrandCraftEssenceMlbRequired: [boolean, boolean, boolean];
  supportGrandBondCeMode: SupportGrandBondCeMode;
  supportScoreGrandMode: boolean;
  supportStarMapScoreMin: number | null;
  supportGrandStarMapScoreMin: number | null;
}

export function supportDebugConfigFromProject(project: Project): SupportDebugConfig {
  const supportSlot =
    project.slots?.find((slot) => slot.type === "support") ?? null;
  const ceIds = supportSlot?.craftEssenceIds ?? [];
  const singleCeId =
    ceIds.length > 0 ? ceIds[0] : (supportSlot?.craftEssenceId ?? null);
  return {
    supportServantId: project.supportServantId ?? null,
    supportServantVariantKey: project.supportServantVariantKey ?? null,
    supportServantLevelMin: project.supportServantLevelMin ?? null,
    supportNoblePhantasmLevelMin: project.supportNoblePhantasmLevelMin ?? null,
    supportSkillLevelMins: asSupportSkillLevelMins(project.supportSkillLevelMins),
    supportAppendSkillLevelMins: asSupportAppendSkillLevelMins(
      project.supportAppendSkillLevelMins
    ),
    supportCraftEssenceId: singleCeId,
    supportCraftEssenceMlbRequired:
      supportSlot?.craftEssenceMlbRequired ?? true,
    supportGrandCraftEssenceIds: asGrandCeIds(project.supportGrandCraftEssenceIds),
    supportGrandCraftEssenceMlbRequired: asGrandMlbRequired(
      project.supportGrandCraftEssenceMlbRequired
    ),
    supportGrandBondCeMode: project.supportGrandBondCeMode ?? "any",
    supportScoreGrandMode: project.supportGrandMode === true,
    supportStarMapScoreMin: project.supportStarMapScoreMin ?? null,
    supportGrandStarMapScoreMin: project.supportGrandStarMapScoreMin ?? null,
  };
}

export function supportPanelLabel(panel: SupportRowMatchDto["skillPanel"]): string {
  if (panel === "owned") return "持有技能";
  if (panel === "append") return "追加技能";
  return "技能面板未知";
}

export function supportLevelList(levels: (number | null)[] | undefined): string {
  if (!levels || levels.length === 0) return "未识别";
  return levels.map((level) => (level == null ? "-" : String(level))).join("/");
}

export function supportSkillDiagnosticsText(
  diagnostics: SupportRowMatchDto["skillLevelDiagnostics"]
): string {
  if (!diagnostics || diagnostics.length === 0) return "";
  return diagnostics
    .map((item, index) => {
      const level = item.level == null ? "-" : String(item.level);
      const score =
        typeof item.score === "number" ? item.score.toFixed(2) : "0.00";
      return `${index + 1}:${level}@${score}${item.source ? `/${item.source}` : ""}`;
    })
    .join(" ");
}

export function supportCeArtworkChecksText(
  checks: SupportCeArtworkCheckDto[] | undefined
): string {
  if (!checks || checks.length === 0) return "";
  const selectedRegionKind = checks.find((check) => check.selected)?.regionKind;
  return checks
    .filter((check) => !selectedRegionKind || check.regionKind === selectedRegionKind)
    .map((check) => {
      const marker = check.selected ? "*" : check.passed ? "✓" : "✗";
      return `${supportCeArtworkVariantLabel(check.variant)}（${check.score.toFixed(2)}/${check.threshold.toFixed(2)}）${marker}`;
    })
    .join(" · ");
}

export function supportCeIconChecksText(
  checks: SupportCeIconCheckDto[] | undefined
): string {
  if (!checks || checks.length === 0) return "";
  return checks
    .map(
      (check) =>
        `${supportCeIconKindLabel(check.kind)}（${check.score.toFixed(2)}/${check.threshold.toFixed(2)}）${check.passed ? "✓" : "✗"}`
    )
    .join(" · ");
}

function supportCeArtworkVariantLabel(variant: string): string {
  if (variant === "full") return "完整匹配";
  if (variant === "center") return "中间匹配";
  if (variant === "top_right") return "右上匹配";
  return variant;
}

function supportCeIconKindLabel(kind: string): string {
  if (kind === "mlb") return "满破图标";
  if (kind === "grandBond" || kind === "grandBondNp") return "牵绊图标";
  return kind;
}

export function timestamp(): string {
  const d = new Date();
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}

export function readDebugPrefs(): DebugPagePrefs {
  try {
    const raw = window.localStorage.getItem(DEBUG_PREFS_STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as DebugPagePrefs;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

export function writeDebugPrefs(prefs: DebugPagePrefs) {
  try {
    window.localStorage.setItem(DEBUG_PREFS_STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // Debug preferences are best-effort; private mode/quota errors should
    // never break the page.
  }
}

export function asNumberOrNull(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

export function asGrandCeIds(value: unknown): [number | null, number | null, number | null] {
  if (!Array.isArray(value)) return [null, null, null];
  return [
    asNumberOrNull(value[0]),
    asNumberOrNull(value[1]),
    asNumberOrNull(value[2]),
  ];
}

export function asGrandMlbRequired(value: unknown): [boolean, boolean, boolean] {
  if (!Array.isArray(value)) return [true, true, true];
  return [value[0] !== false, value[1] !== false, value[2] !== false];
}
