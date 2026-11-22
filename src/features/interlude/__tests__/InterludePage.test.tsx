import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { InterludeCaptureResult } from "../../../types/interlude";
import { InterludePage } from "../InterludePage";

const rows: InterludeCaptureResult = {
  captureId: "capture-test",
  imagePath: "/tmp/interlude.png",
  rows: [
    {
      candidateId: "capture-test:0",
      region: { x: 0.5, y: 0.4, w: 0.45, h: 0.2 },
      titleRegion: { x: 0.56, y: 0.42, w: 0.2, h: 0.05 },
      titleSignature: "0".repeat(32),
      kind: "normal",
      actionable: true,
      anchorScore: 1,
      meanLuma: 150,
      meanSaturation: 0,
      meanValue: 0,
    },
  ],
};

function renderPage() {
  return renderWithTheme(
    <InterludePage
      servants={[]}
      craftEssences={[]}
      onBack={vi.fn()}
      onAutomationStart={vi.fn()}
      onAutomationStartFailed={vi.fn()}
      onLogEntry={vi.fn()}
    />,
  );
}

describe("InterludePage", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "get_server") return "CN";
      if (command === "get_interlude_task_config") return null;
      if (command === "list_auto_battle_profiles") return [];
      return null;
    });
  });

  it("renders the standalone interlude task and starts in all mode", async () => {
    renderPage();
    expect(await screen.findByText("幕间物语")).toBeInTheDocument();
    expect(screen.getByText("完成所有可打幕间")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始幕间物语" })).toBeEnabled();
  });

  it("defaults the stage recovery retry limit to 30 and saves changes", async () => {
    renderPage();
    const input = await screen.findByRole("spinbutton", { name: "舞台恢复重试次数" });
    expect(input).toHaveValue(30);
    fireEvent.change(input, { target: { value: "45" } });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith(
      "save_interlude_task_config",
      { task: expect.objectContaining({ stageRecoveryRetryLimit: 45 }) },
    ));
  });

  it("opens the support settings dialog without a project row", async () => {
    renderPage();
    await screen.findByText("幕间物语");
    await userEvent.click(screen.getByRole("button", { name: "筛选设置" }));
    expect(await screen.findByText("助战筛选设置")).toBeInTheDocument();
  });

  it("requires a captured row in single mode", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "get_server") return "CN";
      if (command === "capture_interlude_page") return rows;
      if (command === "get_interlude_task_config") return null;
      if (command === "list_auto_battle_profiles") return [];
      return null;
    });
    renderPage();
    await screen.findByText("幕间物语");

    await userEvent.click(screen.getByText("完成指定幕间任务"));
    expect(screen.getByRole("button", { name: "开始幕间物语" })).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "截取游戏画面" }));
    await waitFor(() => expect(screen.getByText("可完成")).toBeInTheDocument());

    fireEvent.click(screen.getByText("可完成"));
    expect(screen.getByText("已选择")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始幕间物语" })).toBeEnabled();

    await userEvent.click(screen.getByRole("button", { name: "开始幕间物语" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "start_interlude_automation",
        expect.objectContaining({
          workflow: expect.objectContaining({ mode: "single", candidateId: "capture-test:0" }),
        }),
      ),
    );
  });
});
