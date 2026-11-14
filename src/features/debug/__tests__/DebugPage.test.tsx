import { beforeEach, describe, expect, it, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { DebugPage } from "../DebugPage";
import type { CraftEssence } from "../../../types/craftEssence";
import type { Servant } from "../../../types/servant";
import { invoke } from "@tauri-apps/api/core";

vi.mock("../../team/useCraftEssencePickerData", () => ({
  useCraftEssencePickerData: vi.fn((catalog: CraftEssence[]) => ({
    craftEssences: catalog, recommendedIds: catalog.map((ce) => ce.id),
    bonusDetails: {}, emptyReason: null, loading: false, error: null,
  })),
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: class {
    static getByLabel = vi.fn(async () => null);
    once = vi.fn();
  },
}));

const SERVANTS: Servant[] = [
  {
    id: 150,
    variantKey: "150",
    name_cn: "梅林",
    name_jp: "マーリン",
    name_en: "Merlin",
    class: "Caster",
    rarity: 5,
    noblePhantasmName: "永久关闭的理想乡",
  },
  {
    id: 284,
    variantKey: "284",
    name_cn: "阿尔托莉雅·卡斯特",
    name_jp: "アルトリア・キャスター",
    name_en: "Altria Caster",
    class: "Caster",
    rarity: 5,
    noblePhantasmName: "真圆集",
  },
  {
    id: 418,
    variantKey: "418:1",
    name_cn: "星之希耶尔",
    name_jp: "スターシエル",
    name_en: "Star Ciel",
    class: "Moon Cancer",
    rarity: 5,
    noblePhantasmName: "原理血戒·断头台",
  },
];

const CRAFT_ESSENCES: CraftEssence[] = [
  { id: 1, rarity: 5, category: "normal", name: "万华镜" },
  { id: 2, rarity: 5, category: "normal", name: "黑之圣杯" },
];

describe("DebugPage", () => {
  // The page remembers its last view/inputs in localStorage; clear it so one
  // test switching to the canvas view cannot change another test's start state.
  beforeEach(() => {
    try {
      window.localStorage?.removeItem("mash.debugPagePrefs.v1");
      window.localStorage?.removeItem("mash.debugCanvasMaskOpacity.v1");
    } catch {
      // Some runners do not provide localStorage; nothing to clear.
    }
  });

  function renderDebugPage() {
    renderWithTheme(
      <DebugPage
        onBack={() => {}}
        servants={SERVANTS}
        craftEssences={CRAFT_ESSENCES}
        defaultCardServantIds={[]}
      />
    );
  }

  function mockDebugPageBootstrap(
    handler: (cmd: string, args?: unknown) => Promise<unknown> | unknown
  ) {
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      switch (cmd) {
        case "debug_get_cv_config":
          return { screens: {} };
        case "debug_list_templates":
        case "debug_list_servant_assets":
          return [];
        case "debug_get_runner_coordinates":
          return { groups: [] };
        default:
          return handler(cmd, args);
      }
    });
  }

  it("offers a toggle for digit recognition source regions", () => {
    renderDebugPage();

    expect(
      screen.getByRole("checkbox", { name: "标记数字取值位置" })
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "识别" })).toBeDisabled();
  });

  it("recognizes configured digit regions only after the manual button is clicked", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_capture") {
        return {
          imagePath: "/tmp/debug.png",
          screen: "Battle",
          score: 0.95,
          screenSize: { w: 1920, h: 1080 },
        };
      }
      if (cmd === "debug_read_noble_phantasm_gauges") {
        return [
          {
            slot: 0,
            cardRegion: { x: 0.241, y: 0.097, w: 0.187, h: 0.396 },
            ready: false,
            edgeFrac: 0,
            stdBgr: 0,
            gaugeDigitModelLabels: ["1", "0", "0"],
            turnCountModelLabel: "4",
          },
          {
            slot: 1,
            cardRegion: { x: 0.41, y: 0.097, w: 0.187, h: 0.396 },
            ready: false,
            edgeFrac: 0,
            stdBgr: 0,
            gaugeDigitModelLabels: ["未识别", "6", "0"],
            turnCountModelLabel: "4",
          },
          {
            slot: 2,
            cardRegion: { x: 0.603, y: 0.097, w: 0.187, h: 0.396 },
            ready: false,
            edgeFrac: 0,
            stdBgr: 0,
            gaugeDigitModelLabels: ["1", "9", "0"],
            turnCountModelLabel: "4",
          },
        ];
      }
      return null;
    });

    renderDebugPage();
    await user.click(screen.getByRole("button", { name: "截取画面" }));

    expect(
      vi.mocked(invoke).mock.calls.some(
        ([cmd]) => cmd === "debug_read_noble_phantasm_gauges"
      )
    ).toBe(false);

    await user.click(screen.getByRole("button", { name: "识别" }));

    expect(
      await screen.findByText(
        /数字识别：回合=4；NP1\[百=1 十=0 个=0\]；NP2\[百=未识别 十=6 个=0\]；NP3\[百=1 十=9 个=0\]/
      )
    ).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("debug_read_noble_phantasm_gauges");
  });

  it("renders live scrcpy frames as data URLs for dynamic preview", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_stream_frame") {
        return {
          jpegBase64: "ZmFrZS1qcGVn",
          width: 1920,
          height: 1080,
          screen: null,
          score: null,
          timestampMs: 1,
        };
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "动态读取画面" }));
    const img = await screen.findByRole("img", { name: "screenshot" });
    expect(img).toHaveAttribute("src", "data:image/jpeg;base64,ZmFrZS1qcGVn");
  });

  it("stops live frame polling while dynamic preview is paused", async () => {
    const user = userEvent.setup();
    let frameIndex = 0;
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_stream_frame") {
        frameIndex += 1;
        return {
          jpegBase64: `ZnJhbWUt${frameIndex}`,
          width: 1920,
          height: 1080,
          screen: null,
          score: null,
          timestampMs: frameIndex,
        };
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "动态读取画面" }));
    await screen.findByRole("img", { name: "screenshot" });

    await user.click(screen.getByRole("button", { name: "暂停" }));
    const callsAfterPause = vi
      .mocked(invoke)
      .mock.calls.filter(([cmd]) => cmd === "debug_stream_frame").length;

    await new Promise((resolve) => window.setTimeout(resolve, 250));

    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "debug_stream_frame")
    ).toHaveLength(callsAfterPause);

    await user.click(screen.getByRole("button", { name: "继续" }));
    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(
          ([cmd]) => cmd === "debug_stream_frame"
        ).length
      ).toBeGreaterThan(callsAfterPause)
    );
  });

  it("runs dynamic NP detection through the live command", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_read_noble_phantasm_gauges_live") {
        return [
          {
            slot: 1,
            ready: true,
            edgeFrac: 0,
            stdBgr: 0,
            readySource: "glow",
            gaugeDigitCount: 3,
            cardReady: false,
            gaugeRegion: { x: 0.429, y: 0.913, w: 0.0297, h: 0.0278 },
            npGlowRegion: { x: 0.482, y: 0.94, w: 0.00625, h: 0.01111 },
            npGlowScore: 0.62,
            npGlowReady: true,
            gaugeDigitModelLabels: ["1", "0", "0"],
            turnCountModelLabel: "4",
          },
        ];
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "动态检测宝具" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "debug_read_noble_phantasm_gauges_live"
      )
    );
    expect(await screen.findByText(/宝具卡 not ready/)).toBeInTheDocument();
    expect(screen.getByText(/宝具条端帽 0\.620/)).toBeInTheDocument();
    expect(screen.getByText(/模型当前回合：4/)).toBeInTheDocument();
    expect(screen.getByText(/模型 百\/十\/个 1 \/ 0 \/ 0/)).toBeInTheDocument();
  });

  it("selects command-card candidates by searchable Chinese servant name", async () => {
    const user = userEvent.setup();
    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "添加从者" }));
    await user.type(screen.getByPlaceholderText("搜索从者名称..."), "梅");
    await user.click(screen.getByRole("option", { name: /梅林/ }));

    expect(screen.getByRole("button", { name: "梅林" })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/候选从者 id/)).not.toBeInTheDocument();
  });

  it("selects the optional support craft essence by searchable name", async () => {
    const user = userEvent.setup();
    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "选择礼装 (可选)" }));
    await user.type(screen.getByPlaceholderText("搜索礼装名称..."), "黑");
    await user.click(screen.getByRole("option", { name: /黑之圣杯/ }));

    expect(screen.getByRole("button", { name: "黑之圣杯" })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/礼装 id/)).not.toBeInTheDocument();
  });

  it("uses the selected servant variant for support metadata and OCR", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_capture") {
        return {
          imagePath: "/tmp/debug.png",
          screen: "SupportSelect",
          score: 0.95,
          screenSize: { w: 1920, h: 1080 },
        };
      }
      if (cmd === "get_servant_metadata") {
        return {
          id: 418,
          name: "星之希耶尔",
          names: [
            "谜之代行者C.I.E.L",
            "教教我吧！希耶尔老师",
            "星之希耶尔",
          ],
          npNames: ["第七圣典·断罪死", "原理血戒·断头台"],
        };
      }
      if (cmd === "debug_find_supports") {
        return {
          supports: [
            {
              rowRegion: { x: 0.1, y: 0.2, w: 0.7, h: 0.2 },
              tap: { x: 0.7, y: 0.3 },
              nameText: "星之希耶尔",
              nameScore: 0.98,
              nameRegion: { x: 0.2, y: 0.2, w: 0.2, h: 0.05 },
              npText: "原理血戒·断头台",
              npScore: 0.96,
              npRegion: { x: 0.2, y: 0.27, w: 0.3, h: 0.05 },
              npMatchedName: "原理血戒·断头台",
              servantLevel: 120,
              starMapScore: 62,
              grandStarMapScore: 16,
              scoreText: "62/+16",
              scoreFilterPassed: true,
              elapsedMs: 321,
            },
          ],
          diagnostics: {
            listRegion: { x: 0, y: 0, w: 1, h: 1 },
            nameCandidates: [],
            npCandidates: [],
            fragmentCount: 0,
          },
          scoreFilter: {
            grandMode: true,
            starMapScoreMin: 62,
            grandStarMapScoreMin: 16,
          },
        };
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "选择助战从者" }));
    await user.type(
      screen.getByPlaceholderText("搜索从者名称..."),
      "星之希耶尔"
    );
    await user.click(screen.getByRole("option", { name: /星之希耶尔/ }));
    await user.click(screen.getByRole("button", { name: "截取画面" }));
    await user.type(
      screen.getByRole("spinbutton", { name: "Debug 星图分值" }),
      "62"
    );
    await user.click(screen.getByRole("checkbox", { name: "冠位分值" }));
    await user.type(
      screen.getByRole("spinbutton", { name: "Debug 冠位星图分值" }),
      "16"
    );
    await user.click(screen.getByRole("button", { name: "识别助战" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("get_servant_metadata", {
        id: 418,
        variantKey: "418:1",
      })
    );
    expect(await screen.findByText(/tap \(0\.700, 0\.300\) · 321 ms/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith(
      "debug_find_supports",
      expect.objectContaining({
        servantId: 418,
        servantVariantKey: "418:1",
        supportGrandMode: true,
        supportStarMapScoreMin: 62,
        supportGrandStarMapScoreMin: 16,
      })
    );
    expect(
      await screen.findByText("宝具候选: 第七圣典·断罪死 / 原理血戒·断头台")
    ).toBeInTheDocument();
    expect(await screen.findByText(/星图分值：62 \/ 16/)).toHaveTextContent(
      "✓ 达标"
    );
    expect(await screen.findByText("从者等级：120")).toBeInTheDocument();
  });

  it("includes the support marker in command-card debug summaries", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_capture") {
        return {
          imagePath: "/tmp/debug.png",
          screen: "Attack",
          score: 0.95,
          screenSize: { w: 1080, h: 1920 },
        };
      }
      if (cmd === "debug_find_command_cards") {
        return [
          {
            slot: 1,
            x: 0.3,
            y: 0.6,
            cardRegion: { x: 0.2, y: 0.46, w: 0.2, h: 0.4 },
            faceRegion: { x: 0.24, y: 0.55, w: 0.1, h: 0.14 },
            suit: "a",
            servantId: 150,
            ascension: 1,
            faceScore: 0.88,
            isSupport: true,
            isStunned: true,
          },
        ];
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "截取画面" }));
    await user.click(screen.getByRole("button", { name: "识别指令卡" }));

    expect(
      await screen.findByText(/C2:a 150@1\(0.88\) \[S\] \[无法行动\]/)
    ).toBeInTheDocument();
  });

  it("prints the raw template match center after a successful probe", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_capture") {
        return {
          imagePath: "/tmp/debug.png",
          screen: "CraftEssenceEnhancement",
          score: 0.95,
          screenSize: { w: 1920, h: 1080 },
        };
      }
      if (cmd === "debug_find_element") {
        return {
          found: true,
          x: 0.888,
          y: 0.145,
          score: 0.998,
          region: { x: 0.868, y: 0.129, w: 0.04, h: 0.034 },
        };
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "截取画面" }));
    await user.type(screen.getByPlaceholderText("模板 key"), "shared/test");
    await user.click(screen.getByRole("button", { name: "查找" }));

    expect(
      await screen.findByText(
        "命中: shared/test | score=0.998 | 中心=(0.888, 0.145)"
      )
    ).toBeInTheDocument();
    // 调试构建下，匹配历史会附带这一步的耗时。
    expect(screen.getByText(/阈值 0\.80 · \d+ ms/)).toBeInTheDocument();
  });

  it("switches between the embedded canvas and the option rows", async () => {
    mockDebugPageBootstrap(() => null);
    const user = userEvent.setup();
    renderDebugPage();

    const body = () =>
      document.querySelector(".debug-body") as HTMLElement | null;

    expect(await screen.findByText("原始模板探针")).toBeInTheDocument();
    expect(screen.queryByText("点击 截取画面 开始")).not.toBeInTheDocument();
    expect(body()?.classList.contains("debug-body-canvas")).toBe(false);

    await user.click(screen.getByRole("button", { name: "调试画面" }));
    expect(screen.getByText("点击 截取画面 开始")).toBeInTheDocument();
    expect(body()?.classList.contains("debug-body-canvas")).toBe(true);
    // 选项行保持挂载（折叠状态不丢），由 CSS 在画面视图里隐藏。
    expect(screen.getByText("原始模板探针")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "工具" }));
    expect(body()?.classList.contains("debug-body-canvas")).toBe(false);
    expect(screen.queryByText("点击 截取画面 开始")).not.toBeInTheDocument();
  });

  it("shows per-step timing and the total after a recognition step", async () => {
    mockDebugPageBootstrap((cmd) =>
      cmd === "debug_capture"
        ? { imagePath: "/tmp/shot.jpg", screen: "Unknown", score: 0.1 }
        : null,
    );
    const user = userEvent.setup();
    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "截取画面" }));

    const timingSection = (
      await screen.findByText("识别耗时")
    ).closest(".debug-section") as HTMLElement;
    expect(timingSection).not.toBeNull();
    expect(within(timingSection).getByText("截取画面")).toBeInTheDocument();
    expect(within(timingSection).getByText("合计")).toBeInTheDocument();
    expect(within(timingSection).getAllByText(/\d+ ms/).length).toBeGreaterThan(0);
  });

  it("runs enhancement face matching with the debug-only options", async () => {
    const matchResult = {
      servantId: 150,
      searchRegion: { x: 0.05, y: 0.18, w: 0.78, h: 0.76 },
      templateCrop: { x: 0.195, y: 0.265, w: 0.805, h: 0.438 },
      templateSize: { w: 167, h: 167 },
      threshold: 0.6,
      found: false,
      x: 0,
      y: 0,
      score: 0.42,
      best: null,
      anchors: [],
      referenceAnchor: null,
      gridCells: [],
      matches: [
        {
          template: "face_servant_1.png",
          templatePath: "/tmp/face_servant_1.png",
          row: 0,
          col: 1,
          found: false,
          score: 0.42,
          x: 0,
          y: 0,
          region: { x: 0.2, y: 0.2, w: 0.1, h: 0.1 },
          elapsedMs: 12.5,
        },
      ],
      diagnostics: {
        anchorTemplateKey: "text_servant_avatar_bottom_line",
        anchorEdgeThreshold: 0.5,
        anchorGrayThreshold: 0.85,
        faceThreshold: 0.6,
        anchorCount: 0,
        gridCellCount: 0,
        attempts: 1,
        matchMethod: "direct",
        directSearchThreshold: 0.6,
        gridSkipped: true,
        failReason: "no_anchors",
      },
    };
    mockDebugPageBootstrap((cmd) =>
      cmd === "debug_find_enhancement_servant" ? matchResult : null,
    );
    const user = userEvent.setup();
    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "选择从者" }));
    await user.click(await screen.findByRole("option", { name: /梅林/ }));
    await user.click(
      screen.getByRole("checkbox", { name: "2EXP（左下角裁剪）" }),
    );
    await user.click(screen.getByRole("checkbox", { name: "使用实时帧" }));
    await user.click(screen.getByRole("button", { name: "识别强化从者" }));

    expect(invoke).toHaveBeenCalledWith("debug_find_enhancement_servant", {
      servantId: 150,
      threshold: 0.6,
      twoExp: true,
      useLiveFrame: true,
      directSearchThreshold: 0.6,
    });
    expect(
      await screen.findByText(/method direct（国服已跳过网格，直接搜索头像）/),
    ).toBeInTheDocument();
    expect(screen.getByText(/模板缩放: 167 × 167/)).toBeInTheDocument();
    expect(screen.getByText(/· 12\.5 ms/)).toBeInTheDocument();
  });

  it("explains an option row from its question button", async () => {
    mockDebugPageBootstrap(() => null);
    const user = userEvent.setup();
    renderDebugPage();

    await user.click(
      await screen.findByRole("button", { name: "原始模板探针 功能说明" })
    );
    expect(
      await screen.findByRole("dialog", { name: "原始模板探针" })
    ).toBeInTheDocument();
    expect(screen.getByText(/输入模板 key 与阈值后点「查找」/)).toBeInTheDocument();
  });
});
