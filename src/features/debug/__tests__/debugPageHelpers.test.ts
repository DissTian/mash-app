import { describe, expect, it } from "vitest";
import type { Project, ProjectSlot } from "../../../types/project";
import { filterTemplateKeys, supportDebugConfigFromProject } from "../debugPageHelpers";

function supportSlot(overrides: Partial<ProjectSlot> = {}): ProjectSlot {
  return {
    id: "slot-2",
    type: "support",
    servantId: null,
    servantVariantKey: null,
    craftEssenceId: null,
    craftEssenceIds: [],
    craftEssenceMultiSelect: false,
    craftEssenceMlbRequired: true,
    ...overrides,
  };
}

function project(overrides: Partial<Project> = {}): Project {
  return {
    id: "p1",
    name: "常用队伍",
    slots: [
      { id: "slot-0", type: "servant", servantId: 150 },
      supportSlot(),
    ],
    ...overrides,
  };
}

describe("supportDebugConfigFromProject", () => {
  it("maps the team's support requirements onto the debug fields", () => {
    const config = supportDebugConfigFromProject(
      project({
        supportServantId: 284,
        supportServantVariantKey: "284",
        supportGrandMode: true,
        supportGrandCraftEssenceIds: [1001, null, 1003],
        supportGrandCraftEssenceMlbRequired: [false, true, true],
        supportGrandBondCeMode: "bondNp",
        supportStarMapScoreMin: 40,
        supportGrandStarMapScoreMin: 12,
        supportServantLevelMin: 100,
        supportNoblePhantasmLevelMin: 3,
        supportSkillLevelMins: [10, 8, null],
        supportAppendSkillLevelMins: [10, null, null, null, null],
        slots: [
          supportSlot({
            craftEssenceIds: [1485, 1001],
            craftEssenceId: 999,
            craftEssenceMlbRequired: false,
          }),
        ],
      })
    );

    expect(config).toEqual({
      supportServantId: 284,
      supportServantVariantKey: "284",
      supportServantLevelMin: 100,
      supportNoblePhantasmLevelMin: 3,
      supportSkillLevelMins: [10, 8, null],
      supportAppendSkillLevelMins: [10, null, null, null, null],
      supportCraftEssenceId: 1485,
      supportCraftEssenceMlbRequired: false,
      supportGrandCraftEssenceIds: [1001, null, 1003],
      supportGrandCraftEssenceMlbRequired: [false, true, true],
      supportGrandBondCeMode: "bondNp",
      supportScoreGrandMode: true,
      supportStarMapScoreMin: 40,
      supportGrandStarMapScoreMin: 12,
    });
  });

  it("falls back to the legacy single CE id and default flags", () => {
    const config = supportDebugConfigFromProject(
      project({
        slots: [supportSlot({ craftEssenceIds: [], craftEssenceId: 999 })],
      })
    );

    expect(config.supportServantId).toBeNull();
    expect(config.supportServantLevelMin).toBeNull();
    expect(config.supportNoblePhantasmLevelMin).toBeNull();
    expect(config.supportSkillLevelMins).toEqual([null, null, null]);
    expect(config.supportAppendSkillLevelMins).toEqual([
      null,
      null,
      null,
      null,
      null,
    ]);
    expect(config.supportCraftEssenceId).toBe(999);
    expect(config.supportCraftEssenceMlbRequired).toBe(true);
    expect(config.supportGrandCraftEssenceIds).toEqual([null, null, null]);
    expect(config.supportGrandCraftEssenceMlbRequired).toEqual([true, true, true]);
    expect(config.supportGrandBondCeMode).toBe("any");
    expect(config.supportScoreGrandMode).toBe(false);
    expect(config.supportStarMapScoreMin).toBeNull();
    expect(config.supportGrandStarMapScoreMin).toBeNull();
  });

  it("keeps working for projects without a support slot", () => {
    const config = supportDebugConfigFromProject(
      project({ slots: [{ id: "slot-0", type: "servant", servantId: 150 }] })
    );

    expect(config.supportCraftEssenceId).toBeNull();
    expect(config.supportCraftEssenceMlbRequired).toBe(true);
  });
});

describe("filterTemplateKeys", () => {
  const keys = [
    "battle/attack_button",
    "battle/battle_action_menu",
    "shared/battle/command_seal_a",
  ];

  it("returns the original list for an empty query", () => {
    expect(filterTemplateKeys(keys, "   ")).toEqual(keys);
  });

  it("ranks prefix hits before mid-string hits before fuzzy hits", () => {
    expect(filterTemplateKeys(keys, "battle")).toEqual([
      "battle/attack_button",
      "battle/battle_action_menu",
      "shared/battle/command_seal_a",
    ]);
    expect(filterTemplateKeys(keys, "attack")[0]).toBe("battle/attack_button");
    expect(filterTemplateKeys(keys, "bbam")).toEqual([
      "battle/battle_action_menu",
    ]);
  });

  it("is case-insensitive and drops non-matches", () => {
    expect(filterTemplateKeys(keys, "SEAL")).toEqual([
      "shared/battle/command_seal_a",
    ]);
    expect(filterTemplateKeys(keys, "zzz")).toEqual([]);
  });
});
