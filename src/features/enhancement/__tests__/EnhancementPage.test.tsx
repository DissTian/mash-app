import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { EnhancementPage } from "../EnhancementPage";
import type { Servant } from "../../../types/servant";
import type {
  ServantEnhancementAutomationEvent,
  ServantEnhancementEntryState,
  ServantEnhancementQueueEntry,
  ServantEnhancementStatePayload,
} from "../../../types/servantEnhancement";

const SERVANTS: Servant[] = [
  {
    id: 100100,
    variantKey: "100100:1",
    faceId: 1,
    name_cn: "阿尔托莉雅·Caster",
    name_jp: "アルトリア・キャスター",
    name_en: "Altria Caster",
    class: "caster",
    rarity: 5,
    noblePhantasmName: "きみをいだく希望の星",
  },
];

const ENTRY: ServantEnhancementQueueEntry = {
  id: "entry-1",
  servantId: 100100,
  variantKey: "100100:1",
  nameCn: "阿尔托莉雅·Caster",
  className: "caster",
  rarity: 5,
  targetLevel: 90,
  twoExp: false,
  faceKey: null,
  enabled: true,
};

const IDLE: ServantEnhancementStatePayload = {
  status: "idle",
  entries: [],
  currentEntryId: null,
};

function mockCommands(overrides: Record<string, unknown> = {}) {
  const defaults: Record<string, unknown> = {
    get_servant_enhancement_queue: [ENTRY],
    get_servant_enhancement_automation_status: IDLE,
    get_server: "CN",
    list_servant_faces: [],
    save_servant_enhancement_queue: [ENTRY],
    start_servant_enhancement_automation: null,
    stop_servant_enhancement_automation: null,
  };
  const commands = { ...defaults, ...overrides };
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    return command in commands ? commands[command] : null;
  });
}

function entryState(
  overrides: Partial<ServantEnhancementEntryState> = {},
): ServantEnhancementEntryState {
  return {
    id: ENTRY.id,
    servantId: ENTRY.servantId,
    variantKey: ENTRY.variantKey,
    nameCn: ENTRY.nameCn,
    className: ENTRY.className,
    rarity: ENTRY.rarity,
    targetLevel: ENTRY.targetLevel,
    twoExp: ENTRY.twoExp,
    faceKey: ENTRY.faceKey,
    enabled: ENTRY.enabled,
    status: "pending",
    currentLevel: null,
    maxLevel: null,
    message: "",
    ...overrides,
  };
}

describe("EnhancementPage", () => {
  it("loads the persisted queue and starts automation without arguments", async () => {
    mockCommands();
    const user = userEvent.setup();
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    expect(await screen.findByText("阿尔托莉雅·Caster")).toBeInTheDocument();
    const start = screen.getByRole("button", { name: "开始" });
    await waitFor(() => expect(start).toBeEnabled());
    await user.click(start);

    expect(invoke).toHaveBeenCalledWith("start_servant_enhancement_automation");
  });

  it("shows the confirmed current level when no level cap is reported", async () => {
    mockCommands({
      get_servant_enhancement_automation_status: {
        status: "running",
        entries: [entryState({ status: "running", currentLevel: 37, maxLevel: null })],
        currentEntryId: ENTRY.id,
      },
    });
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    expect(await screen.findByText(/当前 Lv\.37$/)).toBeInTheDocument();
    expect(screen.queryByText(/当前 Lv\.37\//)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
  });

  it("adds servants through the multi-select dialog and persists the queue", async () => {
    mockCommands({
      get_servant_enhancement_queue: [],
      save_servant_enhancement_queue: [ENTRY],
    });
    const user = userEvent.setup();
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    fireEvent.click(screen.getByRole("button", { name: "添加从者" }));
    await user.click(await screen.findByRole("option", { name: /阿尔托莉雅·Caster/ }));
    await user.click(screen.getByRole("button", { name: "确定" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_servant_enhancement_queue", {
        entries: [
          {
            id: undefined,
            variantKey: "100100:1",
            targetLevel: 90,
            twoExp: false,
            faceKey: null,
            enabled: true,
          },
        ],
      });
    });
    expect(await screen.findByText("阿尔托莉雅·Caster")).toBeInTheDocument();
  });

  it("toggles a queue entry off without removing it", async () => {
    mockCommands({
      save_servant_enhancement_queue: [{ ...ENTRY, enabled: false }],
    });
    const user = userEvent.setup();
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    const toggle = await screen.findByRole("switch", {
      name: "启用 阿尔托莉雅·Caster",
    });
    expect(toggle).toBeChecked();
    await user.click(toggle);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_servant_enhancement_queue", {
        entries: [
          {
            id: "entry-1",
            variantKey: "100100:1",
            targetLevel: 90,
            twoExp: false,
            faceKey: null,
            enabled: false,
          },
        ],
      });
    });
    expect(await screen.findByText("阿尔托莉雅·Caster")).toBeInTheDocument();
  });

  it("disables start and shows a warning outside the CN client", async () => {
    mockCommands({ get_server: "JP" });
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    expect(await screen.findByText(/仅支持国服/)).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "开始" })).toBeDisabled(),
    );
  });

  it("stops automation when stop is clicked", async () => {
    mockCommands();
    const user = userEvent.setup();
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "开始" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "开始" }));
    await user.click(screen.getByRole("button", { name: "停止" }));

    expect(invoke).toHaveBeenCalledWith("stop_servant_enhancement_automation");
  });

  it("recovers the start button when a terminal status event arrives", async () => {
    mockCommands();
    let handler:
      | ((event: Event<ServantEnhancementAutomationEvent>) => void)
      | null = null;
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      if (event === "servant-enhancement-automation-status") {
        handler = callback as (event: Event<ServantEnhancementAutomationEvent>) => void;
      }
      return () => {};
    });
    const user = userEvent.setup();
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "开始" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "开始" }));
    await waitFor(() => expect(handler).not.toBeNull());

    act(() => {
      handler?.({
        event: "servant-enhancement-automation-status",
        id: 0,
        payload: {
          state: "Error",
          status: "error",
          currentScreen: "",
          message: "启动失败: no device found",
          level: "warn",
          entries: [entryState({ status: "failed", message: "启动失败" })],
          currentEntryId: ENTRY.id,
        },
      } as Event<ServantEnhancementAutomationEvent>);
    });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "开始" })).toBeEnabled();
    });
  });
});
