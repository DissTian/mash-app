import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { FlowTracePanel } from "../FlowTracePanel";
import type { FlowTraceSnapshotDto } from "../../../types/flowTrace";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const SNAPSHOT: FlowTraceSnapshotDto = {
  enabled: false,
  totalMs: 3450,
  currentStep: 1,
  steps: [
    {
      index: 0,
      name: "启动队列",
      kind: "misc",
      startedAtMs: 0,
      durationMs: 120,
      status: "ok",
      tools: [],
    },
    {
      index: 1,
      name: "查找目标从者",
      kind: "cv",
      startedAtMs: 120,
      durationMs: 3330,
      status: "running",
      tools: [
        {
          name: "cv",
          label: "cv:find_enhancement_servant_grid",
          durationMs: 55,
          status: "ok",
        },
        { name: "sleep", label: "等待 900 ms", durationMs: 900, status: "ok" },
      ],
    },
  ],
};

function mockInvoke(
  overrides: Record<string, unknown | (() => Promise<unknown>)> = {}
) {
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    const override = overrides[command];
    if (typeof override === "function") {
      return (override as () => Promise<unknown>)();
    }
    if (command in overrides) return override;
    if (command === "flow_trace_snapshot") return SNAPSHOT;
    if (command === "get_servant_enhancement_queue") {
      return [{ id: "entry-1" }];
    }
    return null;
  });
}

describe("FlowTracePanel", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("lists the recorded steps and shows the selected step's tools", async () => {
    mockInvoke();
    renderWithTheme(<FlowTracePanel />);

    expect(await screen.findByText("1. 启动队列")).toBeInTheDocument();
    expect(screen.getByText("2. 查找目标从者")).toBeInTheDocument();
    expect(screen.getByText(/总耗时 3.45 s · 2 步/)).toBeInTheDocument();

    // 默认跟随当前步骤（第 2 步），显示工具明细
    expect(
      await screen.findByText("cv:find_enhancement_servant_grid")
    ).toBeInTheDocument();
    expect(screen.getByText("等待 900 ms")).toBeInTheDocument();
    expect(screen.getByText(/下一步骤：/)).toHaveTextContent("（已结束）");

    const user = userEvent.setup();
    await user.click(screen.getByText("1. 启动队列"));
    expect(await screen.findByText("该步骤没有调用工具")).toBeInTheDocument();
    expect(screen.getByText(/下一步骤：/)).toHaveTextContent("查找目标从者");
  });

  it("follows the newest step until the user turns it off", async () => {
    mockInvoke();
    const user = userEvent.setup();
    renderWithTheme(<FlowTracePanel />);

    // 默认跟随：显示当前步骤（第 2 步）
    expect(
      await screen.findByText("cv:find_enhancement_servant_grid")
    ).toBeInTheDocument();

    const follow = screen.getByRole("checkbox", { name: "跟随" });
    expect(follow).toBeChecked();
    await user.click(follow);
    expect(follow).not.toBeChecked();

    await user.click(screen.getByText("1. 启动队列"));
    expect(await screen.findByText("该步骤没有调用工具")).toBeInTheDocument();

    // 重新开启跟随后回到当前步骤
    await user.click(screen.getByRole("checkbox", { name: "跟随" }));
    expect(
      await screen.findByText("cv:find_enhancement_servant_grid")
    ).toBeInTheDocument();
  });

  it("explains an empty enhancement queue instead of starting", async () => {
    mockInvoke({ get_servant_enhancement_queue: [] });
    const user = userEvent.setup();
    renderWithTheme(<FlowTracePanel />);

    expect(
      await screen.findByText(/强化队列：空（请先在从者强化页配置）/)
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "开始流程测试" }));

    expect(
      await screen.findByText(/从者强化队列为空：请先在「从者强化」页面添加从者并保存队列/)
    ).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith(
      "start_servant_enhancement_automation"
    );
    expect(invoke).not.toHaveBeenCalledWith("flow_trace_start");
  });

  it("starts tracing together with the enhancement queue", async () => {
    mockInvoke();
    const user = userEvent.setup();
    renderWithTheme(<FlowTracePanel />);

    await user.click(await screen.findByRole("button", { name: "开始流程测试" }));

    expect(invoke).toHaveBeenCalledWith("flow_trace_start");
    expect(invoke).toHaveBeenCalledWith(
      "start_servant_enhancement_automation"
    );
  });

  it("stops tracing when the queue cannot start and reports why", async () => {
    mockInvoke({
      start_servant_enhancement_automation: () =>
        Promise.reject(new Error("从者强化队列为空")),
    });
    const user = userEvent.setup();
    renderWithTheme(<FlowTracePanel />);

    await user.click(await screen.findByRole("button", { name: "开始流程测试" }));

    expect(await screen.findByText(/从者强化队列为空/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("flow_trace_stop");
  });

  it("clears the recorded steps", async () => {
    mockInvoke();
    const user = userEvent.setup();
    renderWithTheme(<FlowTracePanel />);

    await user.click(await screen.findByRole("button", { name: "清空" }));

    expect(invoke).toHaveBeenCalledWith("flow_trace_clear");
  });
});
