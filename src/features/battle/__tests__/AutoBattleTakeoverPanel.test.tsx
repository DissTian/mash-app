import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { AutoBattleProfile } from "../../../types/autoBattle";
import { AutoBattleTakeoverPanel } from "../AutoBattleTakeoverPanel";

const PROFILES: AutoBattleProfile[] = [
  {
    id: "builtin-basic",
    name: "基础方案",
    description: "",
    builtin: true,
    updatedAt: 0,
    strategy: {
      skill: { mode: "readyOnly", selected: [], includeMysticCode: false, order: "servantMajor" },
      skillTarget: { mode: "ownerSlot", selectedSlot: null },
      noblePhantasm: { mode: "allReady", maxCount: 3 },
      commandCard: { mode: "sameColor", colorOrder: ["buster", "arts", "quick"] },
      enemyTarget: { mode: "default", selected: null },
      failure: "skipStep",
    },
  },
  {
    id: "user-1",
    name: "我的方案",
    description: "",
    builtin: false,
    updatedAt: 10,
    strategy: {
      skill: { mode: "off", selected: [], includeMysticCode: false, order: "servantMajor" },
      skillTarget: { mode: "ownerSlot", selectedSlot: null },
      noblePhantasm: { mode: "off", maxCount: 3 },
      commandCard: { mode: "leftmost", colorOrder: ["buster", "arts", "quick"] },
      enemyTarget: { mode: "default", selected: null },
      failure: "skipStep",
    },
  },
];

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue(PROFILES);
});

describe("AutoBattleTakeoverPanel", () => {
  it("enables takeover with the default profile", async () => {
    const onChange = vi.fn(async () => {});
    renderWithTheme(<AutoBattleTakeoverPanel enabled={false} profileId={null} onChange={onChange} />);
    const user = userEvent.setup();

    await user.click(await screen.findByRole("switch", { name: "自动战斗接管" }));
    expect(onChange).toHaveBeenCalledWith(true, "builtin-basic");
  });

  it("lists profiles and switches the referenced scheme", async () => {
    const onChange = vi.fn(async () => {});
    renderWithTheme(<AutoBattleTakeoverPanel enabled profileId={null} onChange={onChange} />);
    const user = userEvent.setup();

    expect(await screen.findByRole("combobox", { name: "自动战斗方案" })).toHaveTextContent(
      "基础方案"
    );
    await user.click(screen.getByRole("combobox", { name: "自动战斗方案" }));
    await user.click(await screen.findByRole("option", { name: "我的方案" }));
    expect(onChange).toHaveBeenCalledWith(true, "user-1");
  });

  it("surfaces profile load failures", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("读取方案失败"));
    renderWithTheme(
      <AutoBattleTakeoverPanel enabled={false} profileId={null} onChange={vi.fn(async () => {})} />
    );
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("读取方案失败"));
  });
});
