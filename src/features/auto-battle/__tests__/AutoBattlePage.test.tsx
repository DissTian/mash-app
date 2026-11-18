import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { AutoBattleProfile } from "../../../types/autoBattle";
import { AutoBattlePage } from "../AutoBattlePage";

function strategy() {
  return {
    skill: {
      mode: "readyOnly" as const,
      selected: [],
      includeMysticCode: false,
      order: "servantMajor" as const,
    },
    skillTarget: { mode: "ownerSlot" as const, selectedSlot: null },
    noblePhantasm: { mode: "allReady" as const, maxCount: 3 },
    commandCard: {
      mode: "sameColor" as const,
      colorOrder: ["buster", "arts", "quick"] as const,
    },
    enemyTarget: { mode: "default" as const, selected: null },
    failure: "skipStep" as const,
  };
}

const BUILTIN: AutoBattleProfile = {
  id: "builtin-basic",
  name: "基础方案",
  description: "内置说明",
  builtin: true,
  strategy: {
    ...strategy(),
    commandCard: { ...strategy().commandCard, colorOrder: [...strategy().commandCard.colorOrder] },
    skill: { ...strategy().skill, selected: [] },
  },
  updatedAt: 0,
};

const USER: AutoBattleProfile = {
  id: "user-1",
  name: "我的方案",
  description: "自建说明",
  builtin: false,
  strategy: {
    ...strategy(),
    commandCard: { ...strategy().commandCard, colorOrder: [...strategy().commandCard.colorOrder] },
    skill: { ...strategy().skill, selected: [] },
  },
  updatedAt: 10,
};

let profiles: AutoBattleProfile[] = [];

function renderPage() {
  renderWithTheme(<AutoBattlePage onBack={vi.fn()} />);
}

beforeEach(() => {
  profiles = JSON.parse(JSON.stringify([BUILTIN, USER]));
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    const input = (args ?? {}) as Record<string, unknown>;
    switch (command) {
      case "list_auto_battle_profiles":
        return profiles;
      case "save_auto_battle_profile": {
        const profile = input.profile as AutoBattleProfile;
        profiles = profiles.map((item) => (item.id === profile.id ? profile : item));
        return profile;
      }
      case "create_auto_battle_profile": {
        const created: AutoBattleProfile = {
          ...USER,
          id: "user-2",
          name: String(input.name ?? ""),
          updatedAt: 20,
        };
        profiles = [...profiles, created];
        return created;
      }
      case "delete_auto_battle_profile":
        profiles = profiles.filter((item) => item.id !== input.id);
        return undefined;
      case "reset_auto_battle_profile":
        return BUILTIN;
      default:
        throw new Error(`unexpected command: ${command}`);
    }
  });
});

describe("AutoBattlePage", () => {
  it("loads the builtin profile and keeps its name and description fixed", async () => {
    renderPage();

    expect(await screen.findByRole("button", { name: "方案 基础方案" })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    expect(screen.getByRole("textbox", { name: "方案名称" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "方案说明" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "恢复默认" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "删除" })).not.toBeInTheDocument();
  });

  it("saves edited strategy options", async () => {
    renderPage();
    await screen.findByRole("button", { name: "方案 基础方案" });
    const user = userEvent.setup();

    await user.click(screen.getByRole("combobox", { name: "技能使用方式" }));
    await user.click(await screen.findByRole("option", { name: "仅指定技能" }));
    await user.click(screen.getByRole("checkbox", { name: "从者 2 技能 3" }));
    await user.click(screen.getByRole("button", { name: "保存方案" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        "save_auto_battle_profile",
        expect.objectContaining({
          profile: expect.objectContaining({
            id: "builtin-basic",
            strategy: expect.objectContaining({
              skill: expect.objectContaining({
                mode: "selected",
                selected: [{ servantSlot: 2, skillSlot: 3 }],
              }),
            }),
          }),
        })
      );
    });
  });

  it("creates a profile from the dialog and selects it", async () => {
    renderPage();
    await screen.findByRole("button", { name: "方案 基础方案" });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "新建" }));
    await user.type(screen.getByRole("textbox", { name: "新方案名称" }), "周回速刷");
    await user.click(screen.getByRole("button", { name: "创建" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("create_auto_battle_profile", { name: "周回速刷" });
    });
    expect(await screen.findByRole("button", { name: "方案 周回速刷" })).toHaveAttribute(
      "aria-pressed",
      "true"
    );
  });

  it("deletes a user profile after confirmation", async () => {
    renderPage();
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "方案 我的方案" }));
    await user.click(screen.getByRole("button", { name: "删除" }));
    await user.click(await screen.findByRole("button", { name: "确认" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("delete_auto_battle_profile", { id: "user-1" });
    });
    expect(screen.queryByRole("button", { name: "方案 我的方案" })).not.toBeInTheDocument();
  });
});
