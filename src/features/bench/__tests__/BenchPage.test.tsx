import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { BenchPage } from "../BenchPage";
import type {
  BenchEvent,
  BenchPresetInfo,
  BenchRunSummary,
} from "../../../types/bench";

const PRESETS: BenchPresetInfo[] = [
  {
    id: "basic.performance",
    name: "基础性能",
    description: "ADB 响应、截图、取帧、OCR 的基础耗时",
    caseIds: ["adb.connect", "ocr.full_frame"],
    deviceInput: false,
    estimatedSeconds: 45,
  },
  {
    id: "device.input",
    name: "设备输入",
    description: "ADB 点击注入耗时",
    caseIds: ["adb.tap"],
    deviceInput: true,
    estimatedSeconds: 5,
  },
];

const RUN: BenchRunSummary = {
  runId: "run-1",
  presetId: "basic.performance",
  presetName: "基础性能",
  startedAtMs: 0,
  finishedAtMs: 1000,
  deviceLabel: "emulator-5554",
  server: "CN",
  reports: [
    {
      caseId: "ocr.full_frame",
      name: "整屏 OCR",
      source: "liveFrame",
      samplesMs: [3200, 3400],
      minMs: 3200,
      medianMs: 3300,
      p95Ms: 3400,
      budgetMs: 4000,
      passed: true,
      skipped: null,
      note: "34 行文字",
    },
    {
      caseId: "adb.tap",
      name: "ADB 点击注入",
      source: "device",
      samplesMs: [],
      minMs: 0,
      medianMs: 0,
      p95Ms: 0,
      budgetMs: 400,
      passed: null,
      skipped: "该用例会真实点击设备，请在页面上勾选允许",
      note: "",
    },
  ],
};

type EventCallback = (event: Event<BenchEvent>) => void;

let benchEvents: EventCallback | null = null;

function mockCommands(overrides: Record<string, unknown> = {}) {
  const defaults: Record<string, unknown> = {
    bench_list_presets: PRESETS,
    bench_get_runs: [],
    bench_get_status: { running: false, currentCase: null, completed: 0, total: 0 },
    bench_run_preset: null,
    bench_clear_runs: null,
  };
  const commands = { ...defaults, ...overrides };
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    return command in commands ? commands[command] : null;
  });
}

function idleStatus() {
  return { running: false, currentCase: null, completed: 0, total: 0 };
}

describe("BenchPage", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    benchEvents = null;
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      benchEvents = handler as EventCallback;
      return () => {};
    });
  });

  it("lists the built-in presets and runs the selected one", async () => {
    mockCommands();
    const user = userEvent.setup();
    renderWithTheme(<BenchPage onBack={() => {}} />);

    expect(await screen.findByText("基础性能")).toBeInTheDocument();
    expect(screen.getByText(/2 项 · 约 45 秒/)).toBeInTheDocument();
    expect(screen.getByText("会点击设备")).toBeInTheDocument();

    const runButtons = screen.getAllByRole("button", { name: /运行/ });
    await user.click(runButtons[0]);

    expect(invoke).toHaveBeenCalledWith("bench_run_preset", {
      presetId: "basic.performance",
      allowDeviceInput: false,
    });
  });

  it("renders live reports and the final summary from run events", async () => {
    mockCommands();
    renderWithTheme(<BenchPage onBack={() => {}} />);

    await waitFor(() => expect(benchEvents).not.toBeNull());

    await act(async () => {
      benchEvents?.({
        payload: {
          status: { running: true, currentCase: "整屏 OCR", completed: 1, total: 2 },
          report: RUN.reports[0],
          summary: null,
          error: null,
        },
      } as Event<BenchEvent>);
    });

    expect(await screen.findByText("整屏 OCR")).toBeInTheDocument();
    expect(screen.getByText("3.30 s")).toBeInTheDocument();
    expect(screen.getByText("达标")).toBeInTheDocument();

    await act(async () => {
      benchEvents?.({
        payload: {
          status: idleStatus(),
          report: null,
          summary: RUN,
          error: null,
        },
      } as Event<BenchEvent>);
    });

    expect(await screen.findByRole("button", { name: /基础性能 ·/ })).toBeInTheDocument();
    expect(screen.getByText("跳过")).toBeInTheDocument();
  });

  it("shows skipped cases with their reason instead of a timing verdict", async () => {
    mockCommands({ bench_get_runs: [RUN] });
    renderWithTheme(<BenchPage onBack={() => {}} />);

    expect(await screen.findByText("ADB 点击注入")).toBeInTheDocument();
    expect(screen.getByText("跳过")).toBeInTheDocument();
    expect(
      screen.getByText("该用例会真实点击设备，请在页面上勾选允许"),
    ).toBeInTheDocument();
  });

  it("surfaces a busy rejection from the backend", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "bench_run_preset") {
        throw new Error("测试任务正在运行中");
      }
      if (command === "bench_list_presets") return PRESETS;
      if (command === "bench_get_runs") return [];
      return idleStatus();
    });
    const user = userEvent.setup();
    renderWithTheme(<BenchPage onBack={() => {}} />);

    await user.click((await screen.findAllByRole("button", { name: /运行/ }))[0]);
    expect(await screen.findByText(/测试任务正在运行中/)).toBeInTheDocument();
  });
});
