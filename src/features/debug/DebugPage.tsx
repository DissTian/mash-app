import { Fragment, useState, useEffect, useCallback, useRef, useMemo } from "react";
import {
  Box,
  Button,
  Checkbox,
  Flex,
  Select,
  Text,
  TextField,
} from "@radix-ui/themes";
import {
  ChevronLeftIcon,
  ExternalLinkIcon,
  ReloadIcon,
} from "@radix-ui/react-icons";
import { invoke, convertFileSrc } from "../../tauri";
import type { CvConfig } from "../../types/cv";
import type { CraftEssence } from "../../types/craftEssence";
import type { Servant } from "../../types/servant";
import type {
  Project,
  SupportAppendSkillLevelMins,
  SupportGrandBondCeMode,
  SupportSkillLevelMins,
} from "../../types/project";
import { DebugCanvas, type DebugCanvasState } from "./DebugCanvas";
import { TemplateKeyPickerDialog } from "./TemplateKeyPickerDialog";
import { TemplatePreviewDialog } from "./TemplatePreviewDialog";
import { FlowTracePanel } from "./FlowTracePanel";
import { CraftEssenceSelectDialog } from "../team/CraftEssenceSelectDialog";
import { ServantSelectDialog } from "../team/ServantSelectDialog";
import { SupportSettingsDialog } from "../team/SupportSettingsDialog";
import { useCeCards, usePortraits } from "../team/contentGridAssets";
import { DebugSection } from "./DebugSection";
import { useDebugCanvasPopout } from "./useDebugCanvasPopout";
import { DEBUG_SECTION_HELP } from "./debugSectionHelp";
import { DebugHelpButton } from "./DebugHelpButton";
import { debugTiming, useDebugTimingSnapshot } from "./debugTiming";
import {
  BATTLE_SCENE_FAIL_HINTS,
  EMPTY_DEBUG_SELECT_VALUE,
  asGrandCeIds,
  asGrandMlbRequired,
  asNumberOrNull,
  asSupportAppendSkillLevelMins,
  asSupportSkillLevelMins,
  readDebugPrefs,
  supportCeArtworkChecksText,
  supportCeIconChecksText,
  supportDebugConfigFromProject,
  supportLevelList,
  supportPanelLabel,
  supportSkillDiagnosticsText,
  timestamp,
  writeDebugPrefs,
} from "./debugPageHelpers";
import type {
  AttackButtonResultDto,
  BattleSceneResultDto,
  DebugSkillCooldownsResultDto,
  DebugSkillSlotsResultDto,
  BondLevelUpReadResultDto,
  CommandCardMatchDto,
  DebugCaptureResult,
  DebugStreamFrameResultDto,
  DebugHistoryEntry,
  DebugTemplateFileDto,
  DebugStreamStatusDto,
  ElementMatchDto,
  EnhancementServantMatchResultDto,
  FindSupportsResultDto,
  NoblePhantasmMatchDto,
  ProbeResult,
  RunnerCoordinatesDto,
} from "./debugTypes";

interface ServantMetadataDto {
  id: number;
  name: string;
  names: string[];
  npNames: string[];
}

interface LogEntry {
  time: string;
  level: "info" | "warn" | "error";
  message: string;
}

export interface DebugPageProps {
  onBack: () => void;
  servants: Servant[];
  craftEssences: CraftEssence[];
  /**
   * Front-line servant ids (party slots 0–2 + support, deduped) derived
   * from the currently active project. Used to seed the "候选从者 id"
   * input so the face-matcher has candidates to compare against on first
   * click — without this, the input is empty and every card returns as
   * "未识别从者", which is what tripped users up before. Pass `[]` when
   * no project is active or none of the slots have a servant pinned.
   */
  defaultCardServantIds: number[];
  /**
   * All saved teams ("项目"), used by the debug page's support section to
   * apply one team's support requirements instead of hand-entering them.
   */
  projects?: Project[];
}

type DebugServantPickerTarget = "card" | "support" | "enhancement";

function npCardStatus(slot: NoblePhantasmMatchDto) {
  return slot.cardReady == null ? "unknown" : slot.cardReady ? "ready" : "not ready";
}

function npGaugeStatus(slot: NoblePhantasmMatchDto) {
  return slot.npGlowReady == null ? "unknown" : slot.npGlowReady ? "ready" : "not ready";
}

function npAnyReady(slot: NoblePhantasmMatchDto) {
  return slot.cardReady === true || slot.npGlowReady === true;
}

function npAllUnknown(slot: NoblePhantasmMatchDto) {
  return slot.cardReady == null && slot.npGlowReady == null;
}

function debugPercent(value: number | null | undefined) {
  return typeof value === "number" && Number.isFinite(value)
    ? `${(value * 100).toFixed(1)}%`
    : "—";
}

export function DebugPage({
  onBack,
  servants,
  craftEssences,
  defaultCardServantIds,
  projects = [],
}: DebugPageProps) {
  const initialPrefs = useMemo(() => readDebugPrefs(), []);
  const timingEnabled = debugTiming.enabled;
  const { entries: timingEntries, total: timingTotal } =
    useDebugTimingSnapshot();
  // Timings are a per-visit measurement ("本次合计"), not a cross-session log.
  useEffect(() => {
    debugTiming.clear();
  }, []);
  // 画面 = full-size canvas only; 工具 = the option/result rows without the
  // inline canvas (the canvas was too short to be usable inside the column).
  const [debugView, setDebugView] = useState<"canvas" | "tools" | "flow">(
    initialPrefs.debugView === "canvas"
      ? "canvas"
      : initialPrefs.debugView === "flow"
        ? "flow"
        : "tools"
  );
  const [capture, setCapture] = useState<DebugCaptureResult | null>(null);
  const [liveFrame, setLiveFrame] = useState<DebugStreamFrameResultDto | null>(
    null
  );
  const [streamStatus, setStreamStatus] = useState<DebugStreamStatusDto | null>(
    null
  );
  const [connectingStream, setConnectingStream] = useState(false);
  const [disconnectingStream, setDisconnectingStream] = useState(false);
  const [cacheBuster, setCacheBuster] = useState(0);

  const [cvConfig, setCvConfig] = useState<CvConfig | null>(null);
  const [templateFiles, setTemplateFiles] = useState<DebugTemplateFileDto[]>([]);
  const [selectedScreen, setSelectedScreen] = useState<string>(
    initialPrefs.selectedScreen ?? ""
  );
  const [selectedElement, setSelectedElement] = useState<string>(
    initialPrefs.selectedElement ?? ""
  );
  const [rawTemplateInput, setRawTemplateInput] = useState(
    initialPrefs.rawTemplateInput ?? ""
  );
  const [threshold, setThreshold] = useState(
    typeof initialPrefs.threshold === "number" &&
      Number.isFinite(initialPrefs.threshold)
      ? Math.max(0, Math.min(1, initialPrefs.threshold))
      : 0.8
  );

  const [probes, setProbes] = useState<ProbeResult[]>([]);
  const [history, setHistory] = useState<DebugHistoryEntry[]>([]);
  const [previewTemplateKey, setPreviewTemplateKey] = useState<string | null>(
    null
  );
  const [templatePickerOpen, setTemplatePickerOpen] = useState(false);
  const [supportSettingsOpen, setSupportSettingsOpen] = useState(false);
  const [capturing, setCapturing] = useState(false);
  const [dynamicCapture, setDynamicCapture] = useState(false);
  const [dynamicCapturePaused, setDynamicCapturePaused] = useState(false);
  const [probing, setProbing] = useState(false);
  const [reloading, setReloading] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);

  const [coordinates, setCoordinates] = useState<RunnerCoordinatesDto | null>(
    null
  );
  const [showDigitRecognitionRegions, setShowDigitRecognitionRegions] = useState(
    initialPrefs.showDigitRecognitionRegions === true
  );
  const [visibleCoordGroups, setVisibleCoordGroups] = useState<Set<string>>(
    () => new Set(initialPrefs.visibleCoordGroups ?? [])
  );

  const [availableServantIds, setAvailableServantIds] = useState<number[]>([]);
  // Seed the candidate list with the active project's front-line team.
  // The lazy initializer runs once on mount; users can still add/remove
  // names afterwards, and switching projects remounts the page via the
  // view router so we'll re-seed naturally.
  const [selectedCardServantIds, setSelectedCardServantIds] = useState<
    number[]
  >(() =>
    Array.isArray(initialPrefs.selectedCardServantIds)
      ? initialPrefs.selectedCardServantIds.filter(
          (id): id is number => typeof id === "number" && Number.isFinite(id)
        )
      : defaultCardServantIds
  );
  const [commandCards, setCommandCards] = useState<CommandCardMatchDto[]>([]);
  const [findingCards, setFindingCards] = useState(false);
  const [npGaugeSlots, setNpGaugeSlots] = useState<NoblePhantasmMatchDto[]>([]);
  const [debuggingNpGauges, setDebuggingNpGauges] = useState(false);
  const [dynamicNpGaugeDebug, setDynamicNpGaugeDebug] = useState(false);
  const [battleScene, setBattleScene] =
    useState<BattleSceneResultDto | null>(null);
  const [readingBattleScene, setReadingBattleScene] = useState(false);
  const [bondLevelUpResult, setBondLevelUpResult] =
    useState<BondLevelUpReadResultDto | null>(null);
  const [readingBondLevelUp, setReadingBondLevelUp] = useState(false);
  const [attackButton, setAttackButton] =
    useState<AttackButtonResultDto | null>(null);
  const [findingAttackButton, setFindingAttackButton] = useState(false);
  const [skillSlots, setSkillSlots] = useState<DebugSkillSlotsResultDto | null>(
    null
  );
  const [findingSkillSlots, setFindingSkillSlots] = useState(false);
  const [skillSlotThreshold, setSkillSlotThreshold] = useState(185);
  const [skillSlotMinFraction, setSkillSlotMinFraction] = useState(0.95);
  const [skillCooldowns, setSkillCooldowns] =
    useState<DebugSkillCooldownsResultDto | null>(null);
  const [findingSkillCooldowns, setFindingSkillCooldowns] = useState(false);
  const [cooldownCornerDx, setCooldownCornerDx] = useState(0.012);
  const [cooldownCornerDy, setCooldownCornerDy] = useState(0.009);
  const [cooldownCornerW, setCooldownCornerW] = useState(0.015);
  const [cooldownCornerH, setCooldownCornerH] = useState(0.04);
  const [cooldownScale, setCooldownScale] = useState(1);
  const [cooldownMinConfidence, setCooldownMinConfidence] = useState(0.6);
  const [cooldownSection, setCooldownSection] = useState<"servant" | "equipment">(
    "servant"
  );
  const [cooldownMode, setCooldownMode] = useState<"det" | "rec">("det");
  const [supportServantId, setSupportServantId] = useState<number | null>(
    asNumberOrNull(initialPrefs.supportServantId)
  );
  const [supportServantVariantKey, setSupportServantVariantKey] = useState<
    string | null
  >(() => {
    const stored = initialPrefs.supportServantVariantKey;
    if (typeof stored === "string" && stored.trim()) return stored;
    const servantId = asNumberOrNull(initialPrefs.supportServantId);
    return (
      servants.find((servant) => servant.id === servantId)?.variantKey ?? null
    );
  });
  const [supportTeamProjectId, setSupportTeamProjectId] = useState<
    string | null
  >(
    typeof initialPrefs.supportTeamProjectId === "string"
      ? initialPrefs.supportTeamProjectId
      : null
  );
  const [supportServantLevelMin, setSupportServantLevelMin] = useState<
    number | null
  >(asNumberOrNull(initialPrefs.supportServantLevelMin));
  const [supportNoblePhantasmLevelMin, setSupportNoblePhantasmLevelMin] =
    useState<number | null>(asNumberOrNull(initialPrefs.supportNoblePhantasmLevelMin));
  const [supportSkillLevelMins, setSupportSkillLevelMins] =
    useState<SupportSkillLevelMins>(() =>
      asSupportSkillLevelMins(initialPrefs.supportSkillLevelMins)
    );
  const [supportAppendSkillLevelMins, setSupportAppendSkillLevelMins] =
    useState<SupportAppendSkillLevelMins>(() =>
      asSupportAppendSkillLevelMins(initialPrefs.supportAppendSkillLevelMins)
    );
  const [supportCraftEssenceId, setSupportCraftEssenceId] =
    useState<number | null>(asNumberOrNull(initialPrefs.supportCraftEssenceId));
  const [supportCraftEssenceMlbRequired, setSupportCraftEssenceMlbRequired] =
    useState(initialPrefs.supportCraftEssenceMlbRequired !== false);
  const [supportGrandCraftEssenceIds, setSupportGrandCraftEssenceIds] =
    useState<[number | null, number | null, number | null]>(() =>
      asGrandCeIds(initialPrefs.supportGrandCraftEssenceIds)
    );
  const [
    supportGrandCraftEssenceMlbRequired,
    setSupportGrandCraftEssenceMlbRequired,
  ] = useState<[boolean, boolean, boolean]>(() =>
    asGrandMlbRequired(initialPrefs.supportGrandCraftEssenceMlbRequired)
  );
  const [supportGrandBondCeMode, setSupportGrandBondCeMode] =
    useState<SupportGrandBondCeMode>(initialPrefs.supportGrandBondCeMode ?? "any");
  const [supportScoreGrandMode, setSupportScoreGrandMode] = useState(
    initialPrefs.supportScoreGrandMode === true
  );
  const [supportStarMapScoreMin, setSupportStarMapScoreMin] =
    useState<number | null>(asNumberOrNull(initialPrefs.supportStarMapScoreMin));
  const [supportReleaseOcr, setSupportReleaseOcr] = useState(
    initialPrefs.supportReleaseOcr === true
  );
  const [supportGrandStarMapScoreMin, setSupportGrandStarMapScoreMin] =
    useState<number | null>(
      asNumberOrNull(initialPrefs.supportGrandStarMapScoreMin)
    );
  const [supportMetadata, setSupportMetadata] =
    useState<ServantMetadataDto | null>(null);
  const [supportResult, setSupportResult] =
    useState<FindSupportsResultDto | null>(null);
  const [findingSupports, setFindingSupports] = useState(false);
  const [enhancementServantId, setEnhancementServantId] =
    useState<number | null>(asNumberOrNull(initialPrefs.enhancementServantId));
  const [enhancementServantThreshold, setEnhancementServantThreshold] =
    useState(initialPrefs.enhancementServantThreshold ?? "0.60");
  const [enhancementTwoExp, setEnhancementTwoExp] = useState(
    initialPrefs.enhancementTwoExp === true
  );
  const [enhancementUseLiveFrame, setEnhancementUseLiveFrame] = useState(
    initialPrefs.enhancementUseLiveFrame === true
  );
  const [enhancementServantResult, setEnhancementServantResult] =
    useState<EnhancementServantMatchResultDto | null>(null);
  const [findingEnhancementServant, setFindingEnhancementServant] =
    useState(false);
  const [servantPickerTarget, setServantPickerTarget] =
    useState<DebugServantPickerTarget | null>(null);
  const [craftEssencePickerOpen, setCraftEssencePickerOpen] = useState(false);
  const [craftEssencePickerTarget, setCraftEssencePickerTarget] = useState<
    "single" | 0 | 1 | 2 | null
  >(null);
  const logEndRef = useRef<HTMLDivElement>(null);
  const didShutdown = useRef(false);
  const npGaugePollingInFlight = useRef(false);
  const capturePollingInFlight = useRef(false);

  const log = useCallback(
    (message: string, level: LogEntry["level"] = "info") => {
      setLogs((prev) => [...prev, { time: timestamp(), level, message }]);
      const tag = `[DebugPage ${level}]`;
      if (level === "error") console.error(tag, message);
      else if (level === "warn") console.warn(tag, message);
      else console.log(tag, message);
    },
    []
  );

  const loadConfig = useCallback(async () => {
    try {
      const cfg = await invoke<CvConfig>("debug_get_cv_config");
      setCvConfig(cfg);
      const screenNames = Object.keys(cfg.screens || {});
      log(`已加载 cv.json，screens: ${screenNames.join(", ") || "(空)"}`);
      setSelectedScreen((current) =>
        screenNames.length > 0 && !screenNames.includes(current)
          ? screenNames[0]
          : current
      );
    } catch (err) {
      log(`读取 cv.json 失败: ${err}`, "error");
    }
  }, [log]);

  const loadTemplateList = useCallback(async () => {
    try {
      const files = await invoke<DebugTemplateFileDto[]>(
        "debug_list_template_files"
      );
      if (!Array.isArray(files)) {
        throw new Error("debug_list_template_files 未返回数组");
      }
      setTemplateFiles(files);
      const keys = files.map((file) => file.templateKey);
      log(
        `已加载 ${keys.length} 个模板${keys.length > 0 ? ": " + keys.slice(0, 8).join(", ") + (keys.length > 8 ? " …" : "") : ""}`
      );
    } catch (err) {
      log(`读取模板列表失败: ${err}`, "error");
    }
  }, [log]);

  const loadAvailableServantIds = useCallback(async () => {
    try {
      const ids = await invoke<number[]>("debug_list_servant_assets");
      if (!Array.isArray(ids)) {
        throw new Error("debug_list_servant_assets 未返回数组");
      }
      setAvailableServantIds(ids);
      log(
        `已加载 ${ids.length} 个从者素材${ids.length > 0 ? ": " + ids.slice(0, 8).join(", ") + (ids.length > 8 ? " …" : "") : ""}`
      );
    } catch (err) {
      log(`读取从者素材失败: ${err}`, "error");
    }
  }, [log]);

  const loadCoordinates = useCallback(async () => {
    try {
      const coords = await invoke<RunnerCoordinatesDto>(
        "debug_get_runner_coordinates"
      );
      setCoordinates(coords);
      log(
        `已加载坐标分组: ${coords.groups.map((g) => g.label).join(", ") || "(空)"}`
      );
    } catch (err) {
      log(`读取坐标配置失败: ${err}`, "error");
    }
  }, [log]);

  useEffect(() => {
    log("初始化 CV 调试页面");
    loadConfig();
    loadTemplateList();
    loadCoordinates();
    loadAvailableServantIds();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const templateThumbnails = useMemo(
    () =>
      templateFiles.map((file) => ({
        templateKey: file.templateKey,
        src: convertFileSrc(file.path),
        path: file.path,
      })),
    [templateFiles]
  );

  const visibleCoordGroupIds = useMemo(
    () => Array.from(visibleCoordGroups),
    [visibleCoordGroups]
  );

  useEffect(() => {
    writeDebugPrefs({
      debugView,
      selectedScreen,
      selectedElement,
      rawTemplateInput,
      threshold,
      selectedCardServantIds,
      supportTeamProjectId,
      supportServantId,
      supportServantVariantKey,
      supportServantLevelMin,
      supportNoblePhantasmLevelMin,
      supportSkillLevelMins,
      supportAppendSkillLevelMins,
      supportCraftEssenceId,
      supportCraftEssenceMlbRequired,
      supportGrandCraftEssenceIds,
      supportGrandCraftEssenceMlbRequired,
      supportGrandBondCeMode,
      supportScoreGrandMode,
      supportStarMapScoreMin,
      supportGrandStarMapScoreMin,
      supportReleaseOcr,
      enhancementServantId,
      enhancementServantThreshold,
      enhancementTwoExp,
      enhancementUseLiveFrame,
      showDigitRecognitionRegions,
      visibleCoordGroups: visibleCoordGroupIds,
    });
  }, [
    debugView,
    selectedScreen,
    selectedElement,
    rawTemplateInput,
    threshold,
    selectedCardServantIds,
    supportTeamProjectId,
    supportServantId,
    supportServantVariantKey,
    supportServantLevelMin,
    supportNoblePhantasmLevelMin,
    supportSkillLevelMins,
    supportAppendSkillLevelMins,
    supportCraftEssenceId,
    supportCraftEssenceMlbRequired,
    supportGrandCraftEssenceIds,
    supportGrandCraftEssenceMlbRequired,
    supportGrandBondCeMode,
    supportScoreGrandMode,
    supportStarMapScoreMin,
    supportGrandStarMapScoreMin,
    supportReleaseOcr,
    enhancementServantId,
    enhancementServantThreshold,
    enhancementTwoExp,
    enhancementUseLiveFrame,
    showDigitRecognitionRegions,
    visibleCoordGroupIds,
  ]);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  useEffect(() => {
    return () => {
      if (didShutdown.current) return;
      didShutdown.current = true;
      invoke("debug_shutdown").catch(() => {});
    };
  }, []);

  const collectElementNames = useCallback((screenName: string) => {
    if (!cvConfig) return [];
    const screen = cvConfig.screens[screenName];
    if (!screen) return [];

    const names: string[] = [];
    if (screen.detect) {
      names.push("detect");
    }
    if (screen.elements) {
      names.push(...Object.keys(screen.elements));
    }
    for (const [variantName, variant] of Object.entries(screen.variants || {})) {
      const prefix = `variants.${variantName}`;
      if (variant.detect) {
        names.push(`${prefix}.detect`);
      }
      if (variant.elements) {
        names.push(
          ...Object.keys(variant.elements).map((element) => `${prefix}.elements.${element}`)
        );
      }
    }
    return names;
  }, [cvConfig]);

  const resolveElementSpec = useCallback((screenName: string, elementName: string) => {
    if (!cvConfig) return null;
    const screen = cvConfig.screens[screenName];
    if (!screen) return null;
    if (elementName === "detect") return screen.detect ?? null;
    if (screen.elements?.[elementName]) return screen.elements[elementName];

    const match = /^variants\.([^.]+)\.(.+)$/.exec(elementName);
    if (!match) return null;
    const [, variantName, variantElement] = match;
    const variant = screen.variants?.[variantName];
    if (!variant) return null;
    if (variantElement === "detect") return variant.detect ?? null;
    if (variantElement.startsWith("elements.")) {
      return variant.elements?.[variantElement.slice("elements.".length)] ?? null;
    }
    return variant.elements?.[variantElement] ?? null;
  }, [cvConfig]);

  // Auto-select the first element when screen changes
  const elementNames = useMemo(() => {
    if (!selectedScreen) return [];
    return collectElementNames(selectedScreen);
  }, [collectElementNames, selectedScreen]);

  useEffect(() => {
    if (elementNames.length > 0 && !elementNames.includes(selectedElement)) {
      setSelectedElement(elementNames[0]);
    } else if (elementNames.length === 0) {
      setSelectedElement("");
    }
  }, [elementNames, selectedElement]);

  const screenNames = useMemo(
    () => (cvConfig ? Object.keys(cvConfig.screens) : []),
    [cvConfig]
  );

  const selectedElementSpec = useMemo(() => {
    if (!cvConfig || !selectedScreen || !selectedElement) return null;
    return resolveElementSpec(selectedScreen, selectedElement);
  }, [cvConfig, resolveElementSpec, selectedScreen, selectedElement]);

  const pushHistory = useCallback(
    (entry: Omit<DebugHistoryEntry, "id" | "timestamp">) => {
      setHistory((prev) =>
        [
          {
            ...entry,
            id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
            timestamp: timestamp(),
          },
          ...prev,
        ].slice(0, 60)
      );
    },
    []
  );

  const clearHistory = useCallback(() => {
    setHistory([]);
    log("已清空匹配历史");
  }, [log]);

  const captureFrame = useCallback(
    async (options: { logResult: boolean; clearOverlays: boolean }) => {
      const result = await debugTiming.run("截取画面", () =>
        invoke<DebugCaptureResult>("debug_capture")
      );
      setCapture(result);
      setLiveFrame(null);
      setCacheBuster(Date.now());
      if (options.clearOverlays) {
        setProbes([]);
        setCommandCards([]);
        setNpGaugeSlots([]);
        setBattleScene(null);
        setAttackButton(null);
        setSupportResult(null);
        setEnhancementServantResult(null);
      }
      if (options.clearOverlays) {
        pushHistory({
          label: "截取画面",
          summary: `${result.screen} · score ${result.score.toFixed(3)}`,
          ok: true,
        });
      }
      if (options.logResult) {
        log(
          `截图成功 | 画面 = ${result.screen} (score=${result.score.toFixed(3)})` +
            (result.screenSize
              ? ` | 设备 = ${result.screenSize.w}×${result.screenSize.h}`
              : "")
        );
      }
      if (result.screen !== "Unknown" && screenNames.includes(result.screen)) {
        setSelectedScreen(result.screen);
      }
    },
    [log, pushHistory, screenNames]
  );

  const handleCapture = useCallback(async () => {
    setCapturing(true);
    log("调用 debug_capture…");
    try {
      await captureFrame({ logResult: true, clearOverlays: true });
    } catch (err) {
      log(`debug_capture 失败: ${err}`, "error");
    } finally {
      setCapturing(false);
    }
  }, [captureFrame, log]);

  const handleConnectStream = useCallback(async () => {
    setConnectingStream(true);
    log("连接 scrcpy debug 视频流…");
    try {
      const status = await debugTiming.run("连接视频流", () =>
        invoke<DebugStreamStatusDto>("debug_stream_connect")
      );
      setStreamStatus(status);
      log(
        status.screenSize
          ? `scrcpy 已连接 | ${status.screenSize.w}×${status.screenSize.h}`
          : "scrcpy 已连接"
      );
    } catch (err) {
      log(`连接 scrcpy 失败: ${err}`, "error");
    } finally {
      setConnectingStream(false);
    }
  }, [log]);

  const handleDisconnectStream = useCallback(async () => {
    setDisconnectingStream(true);
    log("断开 scrcpy debug 视频流…");
    try {
      const status = await debugTiming.run("断开视频流", () =>
        invoke<DebugStreamStatusDto>("debug_stream_disconnect")
      );
      setStreamStatus(status);
      setDynamicCapture(false);
      setDynamicNpGaugeDebug(false);
      setLiveFrame(null);
      log("scrcpy 已断开");
    } catch (err) {
      log(`断开 scrcpy 失败: ${err}`, "error");
    } finally {
      setDisconnectingStream(false);
    }
  }, [log]);

  const handleToggleDynamicCapture = useCallback(() => {
    setDynamicCapture((enabled) => {
      const next = !enabled;
      if (next) {
        setDynamicCapturePaused(false);
        // The tools view has no canvas, so show the frames where they render.
        setDebugView("canvas");
      }
      log(next ? "动态画面读取已开启，每 0.2 秒刷新" : "动态画面读取已停止");
      return next;
    });
  }, [log]);

  const handleToggleDynamicCapturePaused = useCallback(() => {
    setDynamicCapturePaused((paused) => {
      const next = !paused;
      log(next ? "动态画面读取已暂停" : "动态画面读取已继续");
      return next;
    });
  }, [log]);

  useEffect(() => {
    if (!dynamicCapture || dynamicCapturePaused) return;

    let cancelled = false;
    const poll = async () => {
      if (capturePollingInFlight.current) return;
      capturePollingInFlight.current = true;
      try {
        const frame = await debugTiming.run(
          "动态取帧",
          () =>
            invoke<DebugStreamFrameResultDto>("debug_stream_frame", {
              detectScreen: false,
            }),
          { replace: true }
        );
        setLiveFrame(frame);
        setStreamStatus({
          connected: true,
          screenSize: { w: frame.width, h: frame.height },
        });
      } catch (err) {
        if (!cancelled) {
          log(`动态画面读取失败: ${err}`, "error");
          setDynamicCapture(false);
        }
      } finally {
        capturePollingInFlight.current = false;
      }
    };

    poll();
    const intervalId = window.setInterval(poll, 200);
    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [dynamicCapture, dynamicCapturePaused, log]);

  const handleProbeByName = useCallback(async () => {
    if (!selectedScreen || !selectedElement) return;
    setProbing(true);
    log(
      `调用 debug_find_element_by_name (screen=${selectedScreen}, element=${selectedElement})`
    );
    try {
      const { value: match, ms: probeMs } = await debugTiming.runMeasured(
        "查找元素（按名称）",
        () =>
          invoke<ElementMatchDto>("debug_find_element_by_name", {
            screen: selectedScreen,
            element: selectedElement,
          })
      );
      const label = `${selectedScreen}.${selectedElement}`;
      const effectiveThreshold = selectedElementSpec?.threshold ?? 0.8;
      pushHistory({
        label,
        summary: `${match.found ? "命中" : "未命中"} · score ${match.score.toFixed(3)} · 阈值 ${effectiveThreshold.toFixed(2)}`,
        ok: match.found,
        elapsedMs: timingEnabled ? probeMs : undefined,
      });
      setProbes((prev) => [
        {
          label,
          threshold: effectiveThreshold,
          match,
          timestamp: timestamp(),
          elapsedMs: timingEnabled ? probeMs : undefined,
        },
        ...prev,
      ]);
      if (match.found) {
        log(
          `命中: ${label} | score=${match.score.toFixed(3)} | 中心=(${match.x.toFixed(3)}, ${match.y.toFixed(3)})`
        );
      } else {
        log(
          `未命中: ${label} | score=${match.score.toFixed(3)} (阈值=${effectiveThreshold})`,
          "warn"
        );
      }
    } catch (err) {
      log(`debug_find_element_by_name 失败: ${err}`, "error");
    } finally {
      setProbing(false);
    }
  }, [
    selectedScreen,
    selectedElement,
    selectedElementSpec,
    log,
    pushHistory,
    timingEnabled,
  ]);

  const handleProbeRaw = useCallback(async () => {
    const key = rawTemplateInput.trim();
    if (!key) return;
    setProbing(true);
    log(`调用 debug_find_element (templateKey=${key}, threshold=${threshold})`);
    try {
      const { value: match, ms: probeMs } = await debugTiming.runMeasured(
        "原始模板查找",
        () =>
          invoke<ElementMatchDto>("debug_find_element", {
            templateKey: key,
            threshold,
          })
      );
      setProbes((prev) => [
        {
          label: key,
          threshold,
          match,
          timestamp: timestamp(),
          elapsedMs: timingEnabled ? probeMs : undefined,
        },
        ...prev,
      ]);
      pushHistory({
        label: key,
        summary: `${match.found ? "命中" : "未命中"} · score ${match.score.toFixed(3)} · 阈值 ${threshold.toFixed(2)}`,
        ok: match.found,
        elapsedMs: timingEnabled ? probeMs : undefined,
      });
      if (match.found) {
        log(
          `命中: ${key} | score=${match.score.toFixed(3)} | 中心=(${match.x.toFixed(3)}, ${match.y.toFixed(3)})`
        );
      } else {
        log(
          `未命中: ${key} | score=${match.score.toFixed(3)} (阈值=${threshold})`,
          "warn"
        );
      }
    } catch (err) {
      log(`debug_find_element 失败: ${err}`, "error");
    } finally {
      setProbing(false);
    }
  }, [rawTemplateInput, threshold, log, pushHistory, timingEnabled]);

  const handleClearOverlays = useCallback(() => {
    setProbes([]);
    setCommandCards([]);
    setNpGaugeSlots([]);
    setBattleScene(null);
    setBondLevelUpResult(null);
    setAttackButton(null);
    setSupportResult(null);
    setEnhancementServantResult(null);
    setSkillSlots(null);
    setSkillCooldowns(null);
    log("已清除标注");
  }, [log]);

  const servantById = useMemo(() => {
    const map = new Map<number, Servant>();
    for (const servant of servants) {
      if (!map.has(servant.id)) {
        map.set(servant.id, servant);
      }
    }
    return map;
  }, [servants]);

  const availableServantIdSet = useMemo(
    () => new Set(availableServantIds),
    [availableServantIds]
  );

  const availableServants = useMemo(() => {
    if (availableServantIdSet.size === 0) return servants;
    return servants.filter((servant) => availableServantIdSet.has(servant.id));
  }, [availableServantIdSet, servants]);

  const selectedSupportServant =
    supportServantId === null
      ? null
      : servants.find(
          (servant) =>
            servant.id === supportServantId &&
            servant.variantKey === supportServantVariantKey
        ) ??
        servantById.get(supportServantId) ??
        null;
  const selectedEnhancementServant = enhancementServantId
    ? servantById.get(enhancementServantId) ?? null
    : null;
  const selectedSupportCraftEssence = supportCraftEssenceId
    ? craftEssences.find((ce) => ce.id === supportCraftEssenceId) ?? null
    : null;
  const selectedSupportGrandCraftEssences = supportGrandCraftEssenceIds.map((id) =>
    id == null ? null : craftEssences.find((ce) => ce.id === id) ?? null
  );

  const displayServantName = useCallback(
    (id: number) => {
      const servant = servantById.get(id);
      if (!servant) return `#${id}`;
      return servant.name_cn_server?.trim() || servant.name_cn;
    },
    [servantById]
  );

  const handleSelectDebugServant = useCallback(
    (servant: Servant) => {
      if (servantPickerTarget === "card") {
        setSelectedCardServantIds((prev) =>
          prev.includes(servant.id) ? prev : [...prev, servant.id]
        );
      } else if (servantPickerTarget === "support") {
        if (servant.id !== supportServantId) {
          setSupportGrandCraftEssenceIds((previous) => [previous[0], null, previous[2]]);
          setSupportGrandBondCeMode("any");
        }
        setSupportServantId(servant.id);
        setSupportServantVariantKey(servant.variantKey);
        setSupportMetadata(null);
        setSupportResult(null);
      } else if (servantPickerTarget === "enhancement") {
        setEnhancementServantId(servant.id);
        setEnhancementServantResult(null);
      }
      setServantPickerTarget(null);
    },
    [servantPickerTarget, supportServantId]
  );

  const handleRemoveCardServant = useCallback((id: number) => {
    setSelectedCardServantIds((prev) => prev.filter((item) => item !== id));
  }, []);

  const handleFindCommandCards = useCallback(async () => {
    if (!capture) return;
    setFindingCards(true);
    log(
      `调用 debug_find_command_cards (servants=[${selectedCardServantIds.map(displayServantName).join(", ")}])`
    );
    try {
      const cards = await debugTiming.run("识别指令卡", () =>
        invoke<CommandCardMatchDto[]>("debug_find_command_cards", {
          servantIds: selectedCardServantIds,
        })
      );
      setCommandCards(cards);
      const identified = cards.filter((card) => card.servantId != null).length;
      pushHistory({
        label: "指令卡识别",
        summary:
          cards.length === 0
            ? "未识别到指令卡"
            : `${cards.length} 张 · 归属 ${identified}/${cards.length}`,
        ok: cards.length > 0,
        elapsedMs: debugTiming.latest("识别指令卡"),
      });
      if (cards.length === 0) {
        log("未识别到指令卡 — 检查截图是否为攻击画面", "warn");
      } else {
        const summary = cards
          .map((c) => {
            const ident = c.servantId
              ? ` ${c.servantId}@${c.ascension ?? "?"}(${(c.faceScore ?? 0).toFixed(2)})`
              : "";
            return `C${c.slot + 1}:${c.suit}${ident}${c.isSupport ? " [S]" : ""}${c.isStunned ? " [无法行动]" : ""}`;
          })
          .join("  ");
        log(`识别到 ${cards.length} 张指令卡 | ${summary}`);
      }
    } catch (err) {
      log(`debug_find_command_cards 失败: ${err}`, "error");
    } finally {
      setFindingCards(false);
    }
  }, [capture, selectedCardServantIds, displayServantName, log, pushHistory]);

  const readNpGauges = useCallback(
    async (options: { logResult: boolean; live: boolean }) => {
      const slots = await debugTiming.run(
        options.live ? "动态宝具检测" : "读取宝具识别",
        () =>
          invoke<NoblePhantasmMatchDto[]>(
            options.live
              ? "debug_read_noble_phantasm_gauges_live"
              : "debug_read_noble_phantasm_gauges"
          ),
        options.live ? { replace: true } : undefined
      );
      setNpGaugeSlots(slots);
      if (options.logResult) {
        const cardReadyCount = slots.filter((s) => s.cardReady === true).length;
        const gaugeReadyCount = slots.filter((s) => s.npGlowReady === true).length;
        const turnCount = slots[0]?.turnCountModelLabel ?? "未识别";
        const summary = slots
          .map(
            (s) =>
              `NP${s.slot + 1}:卡:${npCardStatus(s)} 条:${npGaugeStatus(s)}(${
                s.npGlowScore != null ? ` 端帽:${s.npGlowScore.toFixed(3)}` : ""
              }${s.gaugeDigitCount != null ? ` digit:${s.gaugeDigitCount}位` : ""}${
                s.gaugeHundredsVisible != null
                  ? ` 百位:${s.gaugeHundredsVisible ? "有" : "无"}`
                  : ""
              }${
                s.cardReady != null ? ` edge:${(s.edgeFrac * 100).toFixed(2)}%` : ""
              } 模型:${s.gaugeDigitModelLabels?.join("/") ?? "未识别"})`
          )
          .join("  ");
        const label = options.live ? "动态宝具识别" : "宝具识别";
        const ctcSummary = slots[0]?.turnSequenceValue != null
          ? ` · CNN-CTC 回合 ${slots[0].turnSequenceValue || "未识别"}；${slots.map((slot) =>
              `NP${slot.slot + 1}=${slot.gaugeSequenceValue || "未识别"}(${(slot.gaugeSequenceConfidence ?? 0).toFixed(3)}${slot.gaugeSequenceAccepted ? "" : " 拒绝"})`
            ).join("；")}`
          : "";
        log(`${label}: 回合 ${turnCount} · 卡 ${cardReadyCount}/${slots.length} · 条 ${gaugeReadyCount}/${slots.length} | ${summary}${ctcSummary}`);
        pushHistory({
          label,
          summary: `回合 ${turnCount} · 卡 ${cardReadyCount}/${slots.length} · 条 ${gaugeReadyCount}/${slots.length}`,
          ok: cardReadyCount + gaugeReadyCount > 0,
          elapsedMs: debugTiming.latest(label),
        });
      }
      return slots;
    },
    [log, pushHistory]
  );

  const handleDebugNpGauges = useCallback(async () => {
    if (!capture) return;
    setDebuggingNpGauges(true);
    log("调用 debug_read_noble_phantasm_gauges");
    try {
      await readNpGauges({ logResult: true, live: false });
    } catch (err) {
      log(`debug_read_noble_phantasm_gauges 失败: ${err}`, "error");
    } finally {
      setDebuggingNpGauges(false);
    }
  }, [capture, log, readNpGauges]);

  const handleToggleDynamicNpGaugeDebug = useCallback(() => {
    setDynamicNpGaugeDebug((enabled) => {
      const next = !enabled;
      log(
        next
          ? "动态宝具识别已开启，每 1 秒连续采样"
          : "动态宝具识别已停止"
      );
      return next;
    });
  }, [log]);

  useEffect(() => {
    if (!dynamicNpGaugeDebug) return;

    let cancelled = false;
    const poll = async () => {
      if (npGaugePollingInFlight.current) return;
      npGaugePollingInFlight.current = true;
      try {
        await readNpGauges({ logResult: true, live: true });
      } catch (err) {
        if (!cancelled) {
          log(`动态宝具识别失败: ${err}`, "error");
          setDynamicNpGaugeDebug(false);
        }
      } finally {
        npGaugePollingInFlight.current = false;
      }
    };

    poll();
    const intervalId = window.setInterval(poll, 200);
    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [dynamicNpGaugeDebug, log, readNpGauges]);

  const handleReadBattleScene = useCallback(async () => {
    if (!capture) return;
    setReadingBattleScene(true);
    log("调用 debug_read_battle_scene");
    try {
      const result = await debugTiming.run("战斗场景识别", () =>
        invoke<BattleSceneResultDto>("debug_read_battle_scene")
      );
      setBattleScene(result);
      const anchor = `锚点 ${(result.anchorScore * 100).toFixed(1)}% (阈值 ${(
        result.labelThreshold * 100
      ).toFixed(0)}%)`;
      const digits =
        result.kept.length > 0
          ? `命中数字 [${result.kept
              .map((d) => `${d.value}@${(d.score * 100).toFixed(0)}%`)
              .join(", ")}]`
          : `数字 0 个 (阈值 ${(result.digitThreshold * 100).toFixed(0)}%)`;
      const trimNote =
        result.trimmedLeft + result.trimmedRight > 0
          ? ` | 已剔除 ${result.trimmedLeft + result.trimmedRight} 个噪声数字`
          : "";
      pushHistory({
        label: "战斗场景识别",
        summary:
          result.scene !== null && result.total !== null
            ? `${result.scene}/${result.total} · ${anchor}`
            : `未识别（${BATTLE_SCENE_FAIL_HINTS[result.failReason ?? "unknown"] ?? result.failReason ?? "unknown"}）`,
        ok: result.scene !== null && result.total !== null,
      });
      if (result.scene !== null && result.total !== null) {
        log(
          `战斗场景: ${result.scene}/${result.total} → 第 ${result.scene} 组指令 | ${anchor} | ${digits}${trimNote}`
        );
      } else {
        const reason = result.failReason ?? "unknown";
        const hint = BATTLE_SCENE_FAIL_HINTS[reason] ?? reason;
        log(
          `战斗场景: 未识别 (${hint}) | ${anchor} | ${digits}`,
          "warn"
        );
        if (result.missingDigitTemplates.length > 0) {
          log(
            `缺失数字模板: ${result.missingDigitTemplates.join(", ")}`,
            "warn"
          );
        }
      }
    } catch (err) {
      log(`debug_read_battle_scene 失败: ${err}`, "error");
    } finally {
      setReadingBattleScene(false);
    }
  }, [capture, log, pushHistory]);

  const handleReadBondLevelUp = useCallback(async () => {
    if (!capture) return;
    setReadingBondLevelUp(true);
    log("调用 debug_read_bond_level_up");
    try {
      const result = await debugTiming.run("牵绊升级识别", () =>
        invoke<BondLevelUpReadResultDto>("debug_read_bond_level_up")
      );
      setBondLevelUpResult(result);
      pushHistory({
        label: "牵绊升级识别",
        summary: result.ok
          ? `Lv.${result.bondLevelAfter ?? "?"} · ${result.servantNameMatched ?? result.servantName ?? "未知从者"}`
          : `未识别（${result.reason ?? "unknown"}）`,
        ok: result.ok,
      });
      const confidence = result.confidence;
      const scoreText = [
        `anchor ${debugPercent(confidence?.anchor)}`,
        `level ${debugPercent(confidence?.bondLevelAfter)}`,
        `ocr ${debugPercent(confidence?.servantOcr)}`,
      ].join(" · ");
      if (result.ok) {
        log(
          `牵绊升级: Lv.${result.bondLevelAfter ?? "?"} · ${
            result.servantNameMatched ?? result.servantName ?? "未知从者"
          } | ${scoreText}`
        );
      } else {
        log(
          `牵绊升级: 未识别 (${result.reason ?? "unknown"}) | ${scoreText}`,
          "warn"
        );
      }
    } catch (err) {
      log(`debug_read_bond_level_up 失败: ${err}`, "error");
    } finally {
      setReadingBondLevelUp(false);
    }
  }, [capture, log, pushHistory]);

  const handleFindAttackButton = useCallback(async () => {
    if (!capture) return;
    setFindingAttackButton(true);
    log("调用 debug_find_attack_button");
    try {
      const result = await debugTiming.run("攻击按钮识别", () =>
        invoke<AttackButtonResultDto>("debug_find_attack_button")
      );
      setAttackButton(result);
      const scorePct = (result.score * 100).toFixed(1);
      const thresholdPct = (result.threshold * 100).toFixed(0);
      pushHistory({
        label: "攻击按钮识别",
        summary: `${result.found ? "命中" : "未命中"} · ${scorePct}% / 阈值 ${thresholdPct}%`,
        ok: result.found,
      });
      if (result.found) {
        log(
          `攻击按钮: ✓ ${scorePct}% / ${thresholdPct}% | 中心 (${result.matchX.toFixed(3)}, ${result.matchY.toFixed(3)}) | 点击 (${result.tapPoint.x.toFixed(3)}, ${result.tapPoint.y.toFixed(3)})`
        );
      } else {
        log(
          `攻击按钮: ✗ ${scorePct}% < ${thresholdPct}% | 模板 ${result.template}`,
          "warn"
        );
      }
    } catch (err) {
      log(`debug_find_attack_button 失败: ${err}`, "error");
    } finally {
      setFindingAttackButton(false);
    }
  }, [capture, log, pushHistory]);

  const handleFindSkillSlots = useCallback(async () => {
    if (!capture) return;
    setFindingSkillSlots(true);
    log(
      `调用 debug_find_skill_slots (threshold=${skillSlotThreshold}, minFraction=${skillSlotMinFraction})`
    );
    try {
      const result = await debugTiming.run("技能位识别", () =>
        invoke<DebugSkillSlotsResultDto>("debug_find_skill_slots", {
          threshold: Number(skillSlotThreshold),
          minFraction: Number(skillSlotMinFraction),
        })
      );
      setSkillSlots(result);
      const found = result.slots.filter((slot) => slot.present).length;
      setProbes((previous) => [
        ...previous.filter((probe) => !probe.label.startsWith("技能位")),
        ...result.slots
          .filter((slot) => slot.present && slot.region)
          .map((slot) => {
            const region = slot.region!;
            return {
              label: `技能位 ${slot.index + 1}`,
              threshold: result.threshold,
              match: {
                found: true,
                x: region.x + region.w / 2,
                y: region.y + region.h / 2,
                score: slot.score,
                region,
              },
              timestamp: new Date().toLocaleTimeString(),
            };
          }),
      ]);
      pushHistory({
        label: "技能位识别",
        summary: `${found}/9`,
        ok: found === 9,
      });
      log(`技能位: ${found}/9 存在 · 判定占比 ${result.minFraction.toFixed(2)}`);
    } catch (err) {
      log(`debug_find_skill_slots 失败: ${err}`, "error");
    } finally {
      setFindingSkillSlots(false);
    }
  }, [capture, log, pushHistory, skillSlotThreshold, skillSlotMinFraction]);

  const handleFindSkillCooldowns = useCallback(async () => {
    if (!capture) return;
    setFindingSkillCooldowns(true);
    log(
      `调用 debug_find_skill_cooldowns (${cooldownSection}/${cooldownMode}, dx=${cooldownCornerDx}, dy=${cooldownCornerDy}, w=${cooldownCornerW}, h=${cooldownCornerH}, scale=${cooldownScale}, minConf=${cooldownMinConfidence})`
    );
    try {
      const result = await debugTiming.run("技能冷却识别", () =>
        invoke<DebugSkillCooldownsResultDto>("debug_find_skill_cooldowns", {
          section: cooldownSection,
          mode: cooldownMode,
          cornerDx: Number(cooldownCornerDx),
          cornerDy: Number(cooldownCornerDy),
          cornerW: Number(cooldownCornerW),
          cornerH: Number(cooldownCornerH),
          scale: Number(cooldownScale),
          minConfidence: Number(cooldownMinConfidence),
        })
      );
      setSkillCooldowns(result);
      const cooling = result.slots.filter((slot) => slot.cooling).length;
      setProbes((previous) => [
        ...previous.filter((probe) => !probe.label.includes("冷却位")),
        ...result.slots.map((slot) => ({
          label: `${cooldownSection === "equipment" ? "礼装冷却位" : "冷却位"} ${slot.index + 1}`,
          threshold: result.minConfidence,
          match: {
            found: true,
            x: slot.region.x + slot.region.w / 2,
            y: slot.region.y + slot.region.h / 2,
            score: slot.cooling ? 1 : 0,
            region: slot.region,
          },
          timestamp: new Date().toLocaleTimeString(),
        })),
      ]);
      pushHistory({
        label: "技能冷却识别",
        summary: `${cooling}/${result.slots.length} 冷却 · ${result.elapsedMs.toFixed(0)}ms`,
        ok: true,
      });
      log(
        `技能冷却: ${cooling}/${result.slots.length} · ${result.elapsedMs.toFixed(
          0
        )}ms · ${result.slots.map((slot) => slot.text || "—").join(" | ")}`
      );
    } catch (err) {
      log(`debug_find_skill_cooldowns 失败: ${err}`, "error");
    } finally {
      setFindingSkillCooldowns(false);
    }
  }, [
    capture,
    log,
    pushHistory,
    cooldownCornerDx,
    cooldownCornerDy,
    cooldownCornerW,
    cooldownCornerH,
    cooldownScale,
    cooldownMinConfidence,
    cooldownSection,
    cooldownMode,
  ]);

  const handleUseAllAvailableIds = useCallback(() => {
    setSelectedCardServantIds(availableServantIds);
  }, [availableServantIds]);

  const parsedEnhancementServantThreshold = useMemo<number>(() => {
    const n = Number(enhancementServantThreshold.trim());
    return Number.isFinite(n) && n > 0 && n <= 1 ? n : 0.60;
  }, [enhancementServantThreshold]);

  const handleFindEnhancementServant = useCallback(async () => {
    if ((!capture && !enhancementUseLiveFrame) || enhancementServantId === null) {
      return;
    }
    setFindingEnhancementServant(true);
    log(
      `调用 debug_find_enhancement_servant (servant=${displayServantName(enhancementServantId)}, threshold=${parsedEnhancementServantThreshold})`
    );
    try {
      const result = await debugTiming.run("强化从者头像识别", () =>
        invoke<EnhancementServantMatchResultDto>(
          "debug_find_enhancement_servant",
          {
            servantId: enhancementServantId,
            threshold: parsedEnhancementServantThreshold,
            twoExp: enhancementTwoExp,
            useLiveFrame: enhancementUseLiveFrame,
            directSearchThreshold: parsedEnhancementServantThreshold,
          }
        )
      );
      setEnhancementServantResult(result);
      const sorted = [...result.matches].sort((a, b) => b.score - a.score);
      const best = sorted[0];
      pushHistory({
        label: "强化从者头像识别",
        summary: best
          ? `#${result.servantId} r${best.row}c${best.col} score ${best.score.toFixed(3)} ${
              best.found ? "✓" : `✗ < ${result.threshold.toFixed(2)}`
            }`
          : `#${result.servantId} 没有可用头像模板`,
        ok: best?.found === true,
      });
      log(
        `强化从者网格: size=${result.templateSize.w}×${result.templateSize.h} ` +
          `anchors=${result.anchors.length} cells=${result.gridCells.length} ` +
          `method=${result.diagnostics.matchMethod ?? "grid"} ` +
          `直接阈值=${result.diagnostics.directSearchThreshold?.toFixed(2) ?? "-"} ` +
          `attempts=${result.diagnostics.attempts || 1}`
      );
      if (result.listSevenColumns != null) {
        log(
          result.listSevenColumns
            ? "从者列表密度：7 列（头像模板尺寸匹配）"
            : `从者列表密度：非 7 列（左下角按钮 score ${(result.listScaleButtonScore ?? 0).toFixed(3)}）；请先切换到 7 列再识别`,
          result.listSevenColumns ? undefined : "warn"
        );
      }
      if (!best) {
        log(
          `从者 #${result.servantId}: 没有可用头像模板 (${result.diagnostics.failReason ?? "unknown"})`,
          "warn"
        );
      } else if (best.found) {
        log(
          `强化从者 #${result.servantId}: 最佳 r${best.row}c${best.col} ${best.template} score=${best.score.toFixed(3)} ✓ center=(${best.x.toFixed(3)}, ${best.y.toFixed(3)})`
        );
      } else {
        log(
          `强化从者 #${result.servantId}: 最佳 r${best.row}c${best.col} ${best.template} score=${best.score.toFixed(3)} < ${result.threshold.toFixed(2)} (${result.diagnostics.failReason ?? "unknown"})`,
          "warn"
        );
      }
      for (const m of sorted.slice(0, 6)) {
        log(
          `  r${m.row}c${m.col} ${m.template}: ${m.score.toFixed(3)} ${m.found ? "✓" : "✗"}` +
            (m.region ? ` @ (${m.x.toFixed(3)}, ${m.y.toFixed(3)})` : "")
        );
      }
    } catch (err) {
      log(`debug_find_enhancement_servant 失败: ${err}`, "error");
    } finally {
      setFindingEnhancementServant(false);
    }
  }, [
    capture,
    enhancementServantId,
    parsedEnhancementServantThreshold,
    enhancementTwoExp,
    enhancementUseLiveFrame,
    displayServantName,
    log,
    pushHistory,
  ]);

  const handleSelectSupportTeam = useCallback(
    (value: string) => {
      const projectId = value === EMPTY_DEBUG_SELECT_VALUE ? null : value;
      setSupportTeamProjectId(projectId);
      if (projectId === null) return;
      const project = projects.find((item) => item.id === projectId);
      if (!project) return;
      const config = supportDebugConfigFromProject(project);
      setSupportServantId(config.supportServantId);
      setSupportServantVariantKey(config.supportServantVariantKey);
      setSupportServantLevelMin(config.supportServantLevelMin);
      setSupportNoblePhantasmLevelMin(config.supportNoblePhantasmLevelMin);
      setSupportSkillLevelMins(config.supportSkillLevelMins);
      setSupportAppendSkillLevelMins(config.supportAppendSkillLevelMins);
      setSupportCraftEssenceId(config.supportCraftEssenceId);
      setSupportCraftEssenceMlbRequired(config.supportCraftEssenceMlbRequired);
      setSupportGrandCraftEssenceIds(config.supportGrandCraftEssenceIds);
      setSupportGrandCraftEssenceMlbRequired(
        config.supportGrandCraftEssenceMlbRequired
      );
      setSupportGrandBondCeMode(config.supportGrandBondCeMode);
      setSupportScoreGrandMode(config.supportScoreGrandMode);
      setSupportStarMapScoreMin(config.supportStarMapScoreMin);
      setSupportGrandStarMapScoreMin(config.supportGrandStarMapScoreMin);
      setSupportResult(null);
      setSupportMetadata(null);
      log(
        `套用队伍「${project.name || project.id}」的助战配置` +
          `（从者 ${config.supportServantId ?? "未设置"}）`
      );
    },
    [projects, log]
  );

  const supportPortraitServants = useMemo(
    () => (selectedSupportServant ? [selectedSupportServant] : []),
    [selectedSupportServant]
  );
  const supportPortraitMap = usePortraits(supportPortraitServants);
  const supportCeIds = useMemo(
    () =>
      [supportCraftEssenceId, ...supportGrandCraftEssenceIds].filter(
        (id): id is number => id != null
      ),
    [supportCraftEssenceId, supportGrandCraftEssenceIds]
  );
  const supportCeCardMap = useCeCards(supportCeIds);

  const supportCraftEssenceGroups = useMemo(() => {
    const single = selectedSupportCraftEssence
      ? [supportCeCardMap[selectedSupportCraftEssence.id]]
      : [];
    return [
      { count: single.length, images: single },
      ...selectedSupportGrandCraftEssences.map((ce) => {
        const images = ce ? [supportCeCardMap[ce.id]] : [];
        return { count: images.length, images };
      }),
    ];
  }, [
    selectedSupportCraftEssence,
    selectedSupportGrandCraftEssences,
    supportCeCardMap,
  ]);

  // The shared battle-config panel reads a `Project`; build a throwaway one so
  // both screens edit the exact same requirement set.
  const supportSettingsProject = useMemo<Project>(
    () => ({
      id: "debug-support-settings",
      name: "调试助战",
      slots: [],
      supportGrandMode: supportScoreGrandMode,
      supportServantLevelMin,
      supportNoblePhantasmLevelMin,
      supportStarMapScoreMin,
      supportGrandStarMapScoreMin,
      supportSkillLevelMins,
      supportAppendSkillLevelMins,
    }),
    [
      supportScoreGrandMode,
      supportServantLevelMin,
      supportNoblePhantasmLevelMin,
      supportStarMapScoreMin,
      supportGrandStarMapScoreMin,
      supportSkillLevelMins,
      supportAppendSkillLevelMins,
    ]
  );

  const supportSettingsSummary = useMemo(() => {
    const parts: string[] = [];
    if (supportServantLevelMin !== null) {
      parts.push(`等级≥${supportServantLevelMin}`);
    }
    if (supportNoblePhantasmLevelMin !== null) {
      parts.push(`宝具≥${supportNoblePhantasmLevelMin}`);
    }
    if (supportStarMapScoreMin !== null) {
      parts.push(`星图≥${supportStarMapScoreMin}`);
    }
    if (supportScoreGrandMode) {
      parts.push(
        supportGrandStarMapScoreMin !== null
          ? `冠位星图≥${supportGrandStarMapScoreMin}`
          : "冠位模式"
      );
    }
    const skillParts = supportSkillLevelMins
      .map((min, index) => (min == null ? null : `技${index + 1}≥${min}`))
      .filter((part): part is string => part !== null);
    const appendParts = supportAppendSkillLevelMins
      .map((min, index) => (min == null ? null : `追${index + 1}≥${min}`))
      .filter((part): part is string => part !== null);
    parts.push(...skillParts, ...appendParts);
    if (supportCraftEssenceId !== null) {
      parts.push(supportCraftEssenceMlbRequired ? "礼装满破" : "礼装");
    }
    const grandCeCount = supportGrandCraftEssenceIds.filter(
      (id) => id != null
    ).length;
    if (grandCeCount > 0) {
      parts.push(`冠位礼装 ${grandCeCount} 张`);
    }
    if (supportReleaseOcr) {
      parts.push("释放 OCR");
    }
    return parts.length > 0
      ? parts.join(" · ")
      : "默认参数（不限等级/分值、不校验礼装）";
  }, [
    supportServantLevelMin,
    supportNoblePhantasmLevelMin,
    supportStarMapScoreMin,
    supportScoreGrandMode,
    supportGrandStarMapScoreMin,
    supportSkillLevelMins,
    supportAppendSkillLevelMins,
    supportCraftEssenceId,
    supportCraftEssenceMlbRequired,
    supportGrandCraftEssenceIds,
    supportReleaseOcr,
  ]);

  const handleSupportSettingsConfirm = useCallback(
    (next: {
      grandMode: boolean;
      servantLevel: number | null;
      starMapScore: number | null;
      grandStarMapScore: number | null;
      npLevel: number | null;
      skillLevels: SupportSkillLevelMins;
      appendSkillLevels: SupportAppendSkillLevelMins;
    }) => {
      setSupportScoreGrandMode(next.grandMode);
      setSupportServantLevelMin(next.servantLevel);
      setSupportStarMapScoreMin(next.starMapScore);
      setSupportGrandStarMapScoreMin(next.grandStarMapScore);
      setSupportNoblePhantasmLevelMin(next.npLevel);
      setSupportSkillLevelMins(next.skillLevels);
      setSupportAppendSkillLevelMins(next.appendSkillLevels);
      setSupportResult(null);
      setSupportSettingsOpen(false);
      log("已更新助战识别设置");
    },
    [log]
  );

  const handleFindSupports = useCallback(async () => {
    if (!capture || supportServantId === null) return;
    const servantVariantKey =
      selectedSupportServant?.variantKey ?? supportServantVariantKey;
    setFindingSupports(true);
    log(
      `调用 debug_find_supports (servant=${displayServantName(supportServantId)}, variant=${servantVariantKey ?? "未指定"})`
    );
    try {
      // Fetch metadata first so the log + side panel can show what we
      // actually fed the OCR detector (helpful when a row misses).
      let meta = supportMetadata;
      if (!meta || meta.id !== supportServantId) {
        meta = await invoke<ServantMetadataDto>("get_servant_metadata", {
          id: supportServantId,
          variantKey: servantVariantKey,
        });
        setSupportMetadata(meta);
        log(
          `加载从者元数据: ${meta.name} · 名称 [${meta.names.join(" / ")}] · 宝具 [${meta.npNames.join(" / ")}]`
        );
      }
      const result = await debugTiming.run("识别助战", () =>
        invoke<FindSupportsResultDto>("debug_find_supports", {
          servantId: supportServantId,
          servantVariantKey,
          craftEssenceId: supportCraftEssenceId,
          grandCraftEssenceIds: supportGrandCraftEssenceIds,
          craftEssenceMlbRequired: supportCraftEssenceMlbRequired,
          grandCraftEssenceMlbRequired: supportGrandCraftEssenceMlbRequired,
          grandBondCeMode: supportGrandBondCeMode,
          supportGrandMode: supportScoreGrandMode,
          supportStarMapScoreMin,
          supportGrandStarMapScoreMin: supportScoreGrandMode
            ? supportGrandStarMapScoreMin
            : null,
          supportServantLevelMin,
          supportNoblePhantasmLevelMin,
          supportSkillLevelMins,
          supportAppendSkillLevelMins,
          releaseOcr: supportReleaseOcr,
        })
      );
      setSupportResult(result);
      pushHistory({
        label: "助战识别",
        summary:
          result.supports.length > 0
            ? `${result.supports.length} 行 · OCR ${result.diagnostics.fragmentCount} 片段`
            : "未匹配到助战行",
        ok: result.supports.length > 0,
        elapsedMs: debugTiming.latest("识别助战"),
      });
      const diag = result.diagnostics;
      const scoreFilter = result.scoreFilter;
      const cvInfo =
        diag.cvFingerprint || diag.cvFile
          ? ` · CV ${diag.cvFingerprint || "unknown"} split:${
              diag.supportSkillContourSplit ? "on" : "off"
            }`
          : "";
      log(
        `识别到 ${result.supports.length} 行助战 | OCR ${diag.fragmentCount} 片段` +
          ` · 名称候选 ${diag.nameCandidates.length}` +
          ` · 宝具候选 ${diag.npCandidates.length}` +
          cvInfo
      );
      if (
        scoreFilter &&
        (scoreFilter.starMapScoreMin != null ||
          (scoreFilter.grandMode && scoreFilter.grandStarMapScoreMin != null))
      ) {
        log(
          `分值筛选：星图 ≥ ${scoreFilter.starMapScoreMin ?? "任意"}` +
            (scoreFilter.grandMode
              ? ` · 冠位星图 ≥ ${scoreFilter.grandStarMapScoreMin ?? "任意"}`
              : "")
        );
      }
      const levelFilter = result.levelFilter;
      if (
        levelFilter &&
        (levelFilter.servantLevelMin != null ||
          levelFilter.noblePhantasmLevelMin != null ||
          (levelFilter.skillLevelMins ?? []).some((min) => min != null) ||
          (levelFilter.appendSkillLevelMins ?? []).some((min) => min != null))
      ) {
        log(
          `等级筛选：从者 ≥ ${levelFilter.servantLevelMin ?? "任意"}` +
            ` · 宝具 ≥ ${levelFilter.noblePhantasmLevelMin ?? "任意"}` +
            ` · 持有技能 ${supportLevelList(levelFilter.skillLevelMins)}` +
            ` · 追加技能 ${supportLevelList(levelFilter.appendSkillLevelMins)}`
        );
      }
      // Surface the Grand craft-essence icon probe — info level on a hit
      // so it lines up with the runner's user-facing operation log,
      // muted-gray otherwise. `null`/`undefined` means the active
      // probe result is unavailable, so we
      // skip the line instead of pretending we know. Include the
      // per-anchor scores so a partly-Grand list ("row 1 是冠位, row
      // 2 不是") is obvious from the log alone — the aggregate flag
      // alone reads as "everything matched" on those frames.
      const grandScores = diag.grandIconAnchorScores ?? [];
      const grandThreshold = 0.80;
      const formatGrandScores = () =>
        grandScores.length === 0
          ? ""
          : ` [${grandScores
              .map((s, idx) => {
                if (s == null) return `#${idx + 1}=n/a`;
                const tick = s >= grandThreshold ? "✓" : "✗";
                return `#${idx + 1}=${s.toFixed(2)}${tick}`;
              })
              .join(", ")}]`;
      if (diag.isGrandSectionVisible === true) {
        log(
          `识别到冠位从者：助战编队确认按钮 ${
            (diag.confirmButtonAnchors ?? []).length
          } 个，其中至少一行命中冠位礼装图标${formatGrandScores()}`
        );
      } else if (diag.isGrandSectionVisible === false) {
        log(
          `未识别到冠位从者（${
            (diag.confirmButtonAnchors ?? []).length
          } 个助战编队确认按钮均未命中冠位礼装图标）${formatGrandScores()}`
        );
      }
      if (diag.nameOnlyFallback) {
        // Surface the degraded path immediately instead of waiting for
        // the user to expand the diagnostics panel — when this fires
        // the rows are real but their CE / NP cross-check was skipped.
        const why =
          diag.nameOnlyReason === "noNpExpected"
            ? "目标宝具没有可用的 CN OCR 名称映射"
            : diag.nameOnlyReason === "noNpAboveThreshold"
              ? "OCR 未找到匹配的宝具文本（mooncell 译名与游戏内不一致）"
              : "回退到仅按名称识别";
        log(`仅按名称识别（已跳过宝具核对）: ${why}`, "warn");
      }
      if (result.supports.length === 0) {
        log("未匹配到助战行 — 检查截图是否为助战选择画面", "warn");
        // When OCR ran but nothing matched, dump the closest 5 fragments
        // by best-NP-score so the user can immediately see what OCR
        // actually read in the NP region. Replaces the "lower the
        // threshold and re-run" round-trip.
        const frags = (diag.fragments ?? [])
          .filter((f) => f.text.trim().length > 0)
          .sort((a, b) => b.bestNpScore - a.bestNpScore)
          .slice(0, 5);
        for (const f of frags) {
          log(
            `  片段 '${f.text}' | 名称分 ${f.nameScore.toFixed(2)}` +
              ` | 宝具最佳 ${f.bestNpScore.toFixed(2)}` +
              (f.bestNpName ? ` (vs '${f.bestNpName}')` : "")
          );
        }
      } else {
        for (const s of result.supports) {
          const cePart = s.ce
            ? ` | 礼装 ${s.ce.score.toFixed(2)}/${s.ce.threshold.toFixed(2)} ${s.ce.passed ? "✓" : "✗"}`
            : "";
          const ceArtworkPart = supportCeArtworkChecksText(s.ce?.artworkChecks);
          const grandArtworkPart = (s.grandCes ?? [])
            .map((ce, index) => {
              const text = supportCeArtworkChecksText(ce.artworkChecks);
              return text ? `冠${index + 1} ${text}` : "";
            })
            .filter(Boolean)
            .join(" | ");
          const npPart = s.npText
            ? ` | 宝具='${s.npText}' (${s.npScore.toFixed(2)})`
            : ` | 宝具(未核对)`;
          const npLevelPart =
            s.npLevel != null ? ` | 宝具等级 ${s.npLevel}` : "";
          const servantLevelPart =
            s.servantLevel != null ? ` | 从者等级 ${s.servantLevel}` : "";
          const scorePart = s.starMapScore != null
            ? ` | 星图 ${s.starMapScore}${s.grandStarMapScore != null ? `/${s.grandStarMapScore}` : ""}`
            : "";
          const scoreFilterPart =
            s.scoreFilterPassed == null
              ? ""
              : s.scoreFilterPassed
                ? " | 分值筛选 ✓"
                : ` | 分值筛选 ✗ ${s.scoreFilterReason ?? "未达标"}`;
          const levelFilterPart =
            s.levelFilterPassed == null
              ? ""
              : s.levelFilterPassed
                ? " | 等级筛选 ✓"
                : ` | 等级筛选 ✗ ${s.levelFilterReason ?? "未达标"}`;
          const skillLevels =
            s.skillPanel === "append"
              ? supportLevelList(s.appendSkillLevels)
              : supportLevelList(s.skillLevels);
          const skillPart = s.skillPanel
            ? ` | ${supportPanelLabel(s.skillPanel)} ${skillLevels}`
            : "";
          const skillDiag = supportSkillDiagnosticsText(s.skillLevelDiagnostics);
          const skillDiagPart = skillDiag ? ` | 技能诊断 ${skillDiag}` : "";
          const scoreAnchorPart = s.scoreAnchor
            ? ` | 确认锚点 (${s.scoreAnchor.x.toFixed(3)}, ${s.scoreAnchor.y.toFixed(3)})`
            : " | 确认锚点未识别";
          const iconCheckPart = [
            ...(s.ce?.iconChecks ?? []),
            ...(s.grandCes ?? []).flatMap((ce) => ce.iconChecks ?? []),
          ];
          const iconCheckText = supportCeIconChecksText(iconCheckPart);
          log(
            `  行 y=${s.rowRegion.y.toFixed(3)} | 名称='${s.nameText}' (${s.nameScore.toFixed(2)})` +
              npPart +
              scorePart +
              scoreFilterPart +
              levelFilterPart +
              npLevelPart +
              servantLevelPart +
              skillPart +
              skillDiagPart +
              scoreAnchorPart +
              cePart +
              (ceArtworkPart ? ` | 礼装variants ${ceArtworkPart}` : "") +
              (grandArtworkPart ? ` | 冠位variants ${grandArtworkPart}` : "") +
              (iconCheckText ? ` | 图标 ${iconCheckText}` : "")
          );
        }
      }
    } catch (err) {
      log(`debug_find_supports 失败: ${err}`, "error");
    } finally {
      setFindingSupports(false);
    }
  }, [
    capture,
    supportServantId,
    supportServantVariantKey,
    selectedSupportServant,
    supportCraftEssenceId,
    supportGrandCraftEssenceIds,
    supportCraftEssenceMlbRequired,
    supportGrandCraftEssenceMlbRequired,
    supportGrandBondCeMode,
    supportScoreGrandMode,
    supportStarMapScoreMin,
    supportGrandStarMapScoreMin,
    supportServantLevelMin,
    supportNoblePhantasmLevelMin,
    supportSkillLevelMins,
    supportAppendSkillLevelMins,
    supportReleaseOcr,
    supportMetadata,
    displayServantName,
    log,
    pushHistory,
  ]);

  const handleClearLogs = useCallback(() => setLogs([]), []);

  const handleReloadSidecar = useCallback(async () => {
    setReloading(true);
    log("重启 sidecar 并重新加载模板/配置…");
    try {
      await debugTiming.run("重启 CV/重载配置", () => invoke("debug_reload_sidecar"));
      setStreamStatus(null);
      setDynamicCapture(false);
      setDynamicNpGaugeDebug(false);
      setLiveFrame(null);
      log("sidecar 已重启，模板、cv.json 与 CV 代码已重新读取");
      await loadConfig();
      await loadTemplateList();
    } catch (err) {
      log(`重新加载失败: ${err}`, "error");
    } finally {
      setReloading(false);
    }
  }, [log, loadConfig, loadTemplateList]);

  const imageSrc = liveFrame
    ? `data:image/jpeg;base64,${liveFrame.jpegBase64}`
    : capture
      ? `${convertFileSrc(capture.imagePath)}?t=${cacheBuster}`
      : null;

  const canvasState: DebugCanvasState = useMemo(
    () => ({
      imageSrc,
      probes,
      commandCards,
      npGaugeSlots,
      battleScene,
      attackButton,
      enhancementServantResult,
      supportResult,
      coordinates,
      showDigitRecognitionRegions,
      visibleCoordGroups: Array.from(visibleCoordGroups),
    }),
    [
      imageSrc,
      probes,
      commandCards,
      npGaugeSlots,
      battleScene,
      attackButton,
      enhancementServantResult,
      supportResult,
      coordinates,
      showDigitRecognitionRegions,
      visibleCoordGroups,
    ]
  );
  const { popoutOpen, openPopout: handleOpenPopout } =
    useDebugCanvasPopout(canvasState, log);

  return (
    <Flex direction="column" className="debug-page">
      <Flex
        align="center"
        justify="between"
        gap="3"
        className="debug-header"
      >
        <Flex align="center" gap="3">
          <Button type="button" variant="ghost" color="gray" onClick={onBack}>
            <ChevronLeftIcon width={16} height={16} />
            <Text size="2">返回</Text>
          </Button>
          <Text size="4" weight="bold">
            CV 调试
          </Text>
          {timingEnabled && timingTotal > 0 && (
            <Text size="1" color="gray" className="debug-timing-header">
              本次合计 {timingTotal} ms
            </Text>
          )}
        </Flex>
        <Flex align="center" gap="2" wrap="wrap">
          <Flex align="center" gap="1" className="debug-view-switch">
            <Button
              type="button"
              size="2"
              variant={debugView === "canvas" ? "solid" : "soft"}
              color={debugView === "canvas" ? undefined : "gray"}
              onClick={() => setDebugView("canvas")}
              title="全屏查看截图与识别标注"
            >
              <Text size="1">调试画面</Text>
            </Button>
            <Button
              type="button"
              size="2"
              variant={debugView === "tools" ? "solid" : "soft"}
              color={debugView === "tools" ? undefined : "gray"}
              onClick={() => setDebugView("tools")}
              title="识别选项、结果与日志"
            >
              <Text size="1">工具</Text>
            </Button>
            <Button
              type="button"
              size="2"
              variant={debugView === "flow" ? "solid" : "soft"}
              color={debugView === "flow" ? undefined : "gray"}
              onClick={() => setDebugView("flow")}
              title="记录强化任务整个流程的动作与工具耗时"
            >
              <Text size="1">流程测试</Text>
            </Button>
          </Flex>
          <Button
            type="button"
            size="2"
            variant="surface"
            color="gray"
            disabled={reloading}
            onClick={handleReloadSidecar}
            title="重启 sidecar，并重新加载 CV 代码、cv.json 和模板"
          >
            <ReloadIcon width={14} height={14} />
            <Text size="1">{reloading ? "重启中…" : "重启 CV/重载配置"}</Text>
          </Button>
        </Flex>
      </Flex>

      <Flex
        className={`debug-body${debugView === "canvas" ? " debug-body-canvas" : ""}${
          debugView === "flow" ? " debug-body-flow" : ""
        }`}
        gap="4"
      >
        {debugView === "flow" && <FlowTracePanel />}
        <Flex
          direction="column"
          className={`debug-canvas-col${debugView === "tools" ? " debug-canvas-col-tools" : ""}`}
          gap="2"
        >
          <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
            <Button
              type="button"
              size="3"
              disabled={capturing || dynamicCapture}
              onClick={handleCapture}
            >
              {capturing ? "截取中…" : "截取画面"}
            </Button>

            <Button
              type="button"
              size="1"
              variant={streamStatus?.connected ? "solid" : "soft"}
              color={streamStatus?.connected ? "green" : "gray"}
              disabled={connectingStream || disconnectingStream}
              onClick={
                streamStatus?.connected
                  ? handleDisconnectStream
                  : handleConnectStream
              }
            >
              {connectingStream
                ? "连接中…"
                : disconnectingStream
                  ? "断开中…"
                  : streamStatus?.connected
                    ? "断开 scrcpy"
                    : "连接 scrcpy"}
            </Button>

            <Button
              type="button"
              size="1"
              variant={dynamicCapture ? "solid" : "soft"}
              color={dynamicCapture ? "amber" : "gray"}
              disabled={connectingStream || disconnectingStream}
              onClick={handleToggleDynamicCapture}
            >
              {dynamicCapture ? "停止动态画面" : "动态读取画面"}
            </Button>

            {dynamicCapture && (
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={handleToggleDynamicCapturePaused}
              >
                {dynamicCapturePaused ? "继续" : "暂停"}
              </Button>
            )}

            <Button
              type="button"
              size="1"
              variant="surface"
              onClick={handleOpenPopout}
              title="把画面 + 标注弹到独立窗口，可拖到副屏全屏查看"
            >
              <ExternalLinkIcon width={12} height={12} />
              <Text size="1">{popoutOpen ? "聚焦画面窗口" : "弹出画面"}</Text>
            </Button>

            <Flex align="center" gap="1">
              <Text size="1" color="gray">
                画面
              </Text>
              <Select.Root
                value={selectedScreen || EMPTY_DEBUG_SELECT_VALUE}
                onValueChange={(value) => setSelectedScreen(value)}
                disabled={screenNames.length === 0}
              >
                <Select.Trigger className="debug-select-trigger" aria-label="画面" />
                <Select.Content>
                  {screenNames.length === 0 && (
                    <Select.Item value={EMPTY_DEBUG_SELECT_VALUE} disabled>
                      —
                    </Select.Item>
                  )}
                  {screenNames.map((s) => (
                    <Select.Item key={s} value={s}>
                      {s}
                    </Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </Flex>

            <Flex align="center" gap="1">
              <Text size="1" color="gray">
                元素
              </Text>
              <Select.Root
                value={selectedElement || EMPTY_DEBUG_SELECT_VALUE}
                onValueChange={(value) => setSelectedElement(value)}
                disabled={elementNames.length === 0}
              >
                <Select.Trigger className="debug-select-trigger" aria-label="元素" />
                <Select.Content>
                  {elementNames.length === 0 && (
                    <Select.Item value={EMPTY_DEBUG_SELECT_VALUE} disabled>
                      —
                    </Select.Item>
                  )}
                  {elementNames.map((e) => (
                    <Select.Item key={e} value={e}>
                      {e}
                    </Select.Item>
                  ))}
                </Select.Content>
              </Select.Root>
            </Flex>

            <Button
              type="button"
              disabled={
                probing || !capture || !selectedScreen || !selectedElement
              }
              onClick={handleProbeByName}
            >
              {probing ? "查找中…" : "查找元素"}
            </Button>

            <Button
              type="button"
              color="red"
              disabled={
                probes.length === 0 &&
                commandCards.length === 0 &&
                npGaugeSlots.length === 0 &&
                supportResult === null &&
                enhancementServantResult === null &&
                bondLevelUpResult === null &&
                battleScene === null &&
                attackButton === null &&
                skillSlots === null
              }
              onClick={handleClearOverlays}
            >
              清除标注
            </Button>
            <DebugHelpButton
              title="调试工具栏"
              help={DEBUG_SECTION_HELP.toolbar}
              className="debug-toolbar-help"
            />
          </Flex>

          {debugView === "canvas" && (
            <>
              <DebugCanvas {...canvasState} className="debug-canvas-embedded" />
              <Box className="debug-coord-bar">
                <Flex gap="2" align="center" wrap="wrap">
                  <Text size="1" color="gray">
                    坐标叠层（勾选任意分组即开启）
                  </Text>
                  <label
                    className="debug-coord-toggle"
                    title="标记当前回合数与三个宝具槽的百位、十位、个位取值区域"
                  >
                    <Checkbox
                      checked={showDigitRecognitionRegions}
                      onCheckedChange={(checked) =>
                        setShowDigitRecognitionRegions(checked === true)
                      }
                    />
                    <Text size="1">数字取值位置</Text>
                  </label>
                  {(coordinates?.groups.length ?? 0) === 0 ? (
                    <Text size="1" color="gray">
                      暂无坐标分组
                    </Text>
                  ) : (
                    coordinates?.groups.map((group) => (
                      <label key={group.id} className="debug-coord-toggle">
                        <Checkbox
                          checked={visibleCoordGroups.has(group.id)}
                          onCheckedChange={(checked) => {
                            setVisibleCoordGroups((prev) => {
                              const next = new Set(prev);
                              if (checked === true) next.add(group.id);
                              else next.delete(group.id);
                              return next;
                            });
                          }}
                        />
                        <Text size="1">{group.label}</Text>
                      </label>
                    ))
                  )}
                  <DebugHelpButton
                    title="坐标叠层"
                    help={DEBUG_SECTION_HELP.coordOverlay}
                  />
                </Flex>
              </Box>
            </>
          )}

          <DebugSection title="原始模板探针" help={DEBUG_SECTION_HELP.rawTemplateProbe}>
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Button
                type="button"
                size="1"
                variant="surface"
                className="debug-template-key-button"
                onClick={() => setTemplatePickerOpen(true)}
                title="点击从列表中选择模板 key"
              >
                {rawTemplateInput.trim() || "选择模板 key"}
              </Button>
              <Flex align="center" gap="1">
                <Text size="1" color="gray">
                  阈值
                </Text>
                <TextField.Root
                  className="debug-threshold-field"
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={threshold}
                  onChange={(e) =>
                    setThreshold(
                      Math.max(0, Math.min(1, Number(e.target.value) || 0))
                    )
                  }
                />
              </Flex>
              <Button
                type="button"
                size="1"
                disabled={probing || !capture || !rawTemplateInput.trim()}
                onClick={handleProbeRaw}
              >
                {probing ? "查找中…" : "查找"}
              </Button>
              <Button
                type="button"
                size="1"
                variant="surface"
                disabled={!rawTemplateInput.trim()}
                title="查看该模板 key 对应的原始 PNG"
                onClick={() => setPreviewTemplateKey(rawTemplateInput.trim())}
              >
                预览
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection title="指令卡识别" help={DEBUG_SECTION_HELP.commandCardsInput}>
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Flex align="center" gap="1" wrap="wrap" className="debug-servant-choice-list">
                {selectedCardServantIds.length === 0 ? (
                  <Text size="1" color="gray">
                    未选择候选从者，可留空只定位卡槽
                  </Text>
                ) : (
                  selectedCardServantIds.map((id) => (
                    <Button
                      key={`card-servant-${id}`}
                      type="button"
                      size="1"
                      variant="soft"
                      color="gray"
                      onClick={() => handleRemoveCardServant(id)}
                      title="点击移除"
                    >
                      {displayServantName(id)}
                    </Button>
                  ))
                )}
              </Flex>
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={() => setServantPickerTarget("card")}
              >
                添加从者
              </Button>
              <Button
                type="button"
                size="1"
                variant="surface"
                disabled={availableServantIds.length === 0}
                onClick={handleUseAllAvailableIds}
                title={`填入全部 ${availableServantIds.length} 个有素材的从者 id`}
              >
                全选 ({availableServantIds.length})
              </Button>
              <Button
                type="button"
                size="1"
                disabled={findingCards || !capture}
                onClick={handleFindCommandCards}
              >
                {findingCards ? "识别中…" : "识别指令卡"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection title="宝具调试" help={DEBUG_SECTION_HELP.npDebugInput}>
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Button
                type="button"
                size="1"
                disabled={debuggingNpGauges || dynamicNpGaugeDebug || !capture}
                onClick={handleDebugNpGauges}
              >
                {debuggingNpGauges ? "读取中…" : "读取宝具识别"}
              </Button>
              <Button
                type="button"
                size="1"
                variant={dynamicNpGaugeDebug ? "solid" : "soft"}
                color={dynamicNpGaugeDebug ? "amber" : "gray"}
                disabled={connectingStream || disconnectingStream}
                onClick={handleToggleDynamicNpGaugeDebug}
              >
                {dynamicNpGaugeDebug ? "停止动态检测" : "动态检测宝具"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="技能位识别"
            help={DEBUG_SECTION_HELP.skillSlotsResult}
            timing={timingEnabled ? debugTiming.latest("技能位识别") : undefined}
            badge={
              skillSlots
                ? `${skillSlots.slots.filter((slot) => slot.present).length}/9`
                : undefined
            }
          >
            <Box className="debug-match-list">
              {!skillSlots && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {skillSlots && (
                <>
                  {skillSlots.slots.map((slot) => (
                    <Box
                      key={`skill-slot-${slot.index}`}
                      className={`debug-match-entry ${slot.present ? "found" : "missed"}`}
                    >
                      <Flex justify="between" align="center">
                        <Text size="2" weight="medium">
                          技能位 {slot.index + 1}
                        </Text>
                        <Text size="1" color={slot.present ? "green" : "red"}>
                          {slot.present ? "存在" : "空位"}
                        </Text>
                      </Flex>
                      <Text size="1" color="gray">
                        白边占比 {slot.score.toFixed(2)}
                        {slot.region
                          ? ` · 区域 (${slot.region.x.toFixed(3)}, ${slot.region.y.toFixed(3)}, ${slot.region.w.toFixed(3)}, ${slot.region.h.toFixed(3)})`
                          : ""}
                      </Text>
                    </Box>
                  ))}
                  <Text size="1" color="gray">
                    阈值 {skillSlots.threshold.toFixed(0)} · 判定占比{" "}
                    {skillSlots.minFraction.toFixed(2)} · 扫描 9 个技能位
                  </Text>
                </>
              )}
            </Box>
          </DebugSection>

          <DebugSection
            title="技能冷却识别"
            help={DEBUG_SECTION_HELP.skillCooldownsResult}
            timing={
              timingEnabled ? debugTiming.latest("技能冷却识别") : undefined
            }
            badge={
              skillCooldowns
                ? `${skillCooldowns.slots.filter((slot) => slot.cooling).length}/${skillCooldowns.slots.length}`
                : undefined
            }
          >
            <Box className="debug-match-list">
              {!skillCooldowns && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {skillCooldowns && (
                <>
                  {skillCooldowns.slots.map((slot) => (
                    <Box
                      key={`skill-cooldown-${slot.index}`}
                      className={`debug-match-entry ${slot.cooling ? "found" : "missed"}`}
                    >
                      <Flex justify="between" align="center">
                        <Text size="2" weight="medium">
                          {skillCooldowns.section === "equipment"
                            ? "礼装技能"
                            : "技能位"}{" "}
                          {slot.index + 1}
                        </Text>
                        <Text size="1" color={slot.cooling ? "green" : "gray"}>
                          {slot.cooling ? "冷却中" : "就绪/无数字"}
                        </Text>
                      </Flex>
                      <Text size="1" color="gray">
                        OCR：{slot.text || "（空）"} ·{" "}
                        {slot.elapsedMs.toFixed(0)}ms
                      </Text>
                      <Text size="1" color="gray">
                        区域 ({slot.region.x.toFixed(3)}, {slot.region.y.toFixed(3)},{" "}
                        {slot.region.w.toFixed(3)}, {slot.region.h.toFixed(3)})
                      </Text>
                    </Box>
                  ))}
                  <Text size="1" color="gray">
                    耗时 {skillCooldowns.elapsedMs.toFixed(0)}ms · 模式{" "}
                    {skillCooldowns.mode === "rec" ? "纯识别" : "检测+识别"} ·
                    偏移 dx {skillCooldowns.cornerDx} · dy{" "}
                    {skillCooldowns.cornerDy} · 尺寸 {skillCooldowns.cornerW}×
                    {skillCooldowns.cornerH} · 放大 {skillCooldowns.scale} ·
                    置信度 {skillCooldowns.minConfidence}
                  </Text>
                </>
              )}
            </Box>
          </DebugSection>

          <DebugSection
            title="强化从者头像识别"
            help={DEBUG_SECTION_HELP.enhancementServantInput}
            badge={
              enhancementServantResult
                ? `#${enhancementServantResult.servantId}`
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={() => setServantPickerTarget("enhancement")}
              >
                {selectedEnhancementServant
                  ? displayServantName(selectedEnhancementServant.id)
                  : "选择从者"}
              </Button>
              <TextField.Root
                className="debug-threshold-field"
                type="number"
                step="0.01"
                min="0"
                max="1"
                value={enhancementServantThreshold}
                onChange={(e) => setEnhancementServantThreshold(e.target.value)}
                title="强化从者头像匹配阈值"
              />
              <label className="debug-coord-toggle">
                <Checkbox
                  checked={enhancementTwoExp}
                  onCheckedChange={(checked) =>
                    setEnhancementTwoExp(checked === true)
                  }
                />
                <Text size="1">2EXP（左下角裁剪）</Text>
              </label>
              <label className="debug-coord-toggle">
                <Checkbox
                  checked={enhancementUseLiveFrame}
                  onCheckedChange={(checked) =>
                    setEnhancementUseLiveFrame(checked === true)
                  }
                />
                <Text size="1">使用实时帧</Text>
              </label>
              <Button
                type="button"
                size="1"
                disabled={
                  findingEnhancementServant ||
                  (!capture && !enhancementUseLiveFrame) ||
                  enhancementServantId === null
                }
                onClick={handleFindEnhancementServant}
              >
                {findingEnhancementServant ? "识别中…" : "识别强化从者"}
              </Button>
              <Text size="2" color="gray">
                与国服队列一致：国服列表区域、按截图缩放模板、直接搜索阈值可调。
              </Text>
            </Flex>
          </DebugSection>

          <DebugSection
            title="战斗场景识别"
            help={DEBUG_SECTION_HELP.battleSceneInput}
            badge={
              battleScene && battleScene.scene !== null && battleScene.total !== null
                ? `${battleScene.scene}/${battleScene.total}`
                : battleScene
                  ? "未识别"
                  : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Text size="2" color="gray">
                读取右上角 BATTLE m/n，用于挑选第 m 组指令配置。
              </Text>
              <Button
                type="button"
                size="1"
                disabled={readingBattleScene || !capture}
                onClick={handleReadBattleScene}
              >
                {readingBattleScene ? "识别中…" : "识别战斗场景"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="牵绊升级识别"
            help={DEBUG_SECTION_HELP.bondLevelInput}
            badge={
              bondLevelUpResult
                ? bondLevelUpResult.ok
                  ? `Lv.${bondLevelUpResult.bondLevelAfter ?? "?"}`
                  : "未识别"
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Text size="2" color="gray">
                调用生产逻辑 `read_bond_level_up`，使用配置 anchor、多尺度数字模板和从者币 OCR 行。
              </Text>
              <Button
                type="button"
                size="1"
                disabled={readingBondLevelUp || !capture}
                onClick={handleReadBondLevelUp}
              >
                {readingBondLevelUp ? "识别中…" : "识别牵绊升级"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="攻击按钮识别"
            help={DEBUG_SECTION_HELP.attackButtonInput}
            badge={
              attackButton
                ? `${(attackButton.score * 100).toFixed(0)}% ${attackButton.found ? "✓" : "✗"}`
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Text size="2" color="gray">
                复用 `cv.json` 的 `Battle.variants.main.elements.attack_button` 探针，确认战斗回合开始时是否能命中攻击按钮。
              </Text>
              <Button
                type="button"
                size="1"
                disabled={findingAttackButton || !capture}
                onClick={handleFindAttackButton}
              >
                {findingAttackButton ? "识别中…" : "识别攻击按钮"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="技能位识别"
            help={DEBUG_SECTION_HELP.skillSlotsInput}
            badge={
              skillSlots
                ? `${skillSlots.slots.filter((slot) => slot.present).length}/9`
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Flex align="center" gap="1">
                <Text size="1" color="gray">
                  白框阈值
                </Text>
                <TextField.Root
                  className="debug-threshold-field"
                  type="number"
                  min={0}
                  max={255}
                  step={5}
                  value={skillSlotThreshold}
                  onChange={(event) =>
                    setSkillSlotThreshold(
                      Math.max(0, Math.min(255, Number(event.target.value) || 0))
                    )
                  }
                />
              </Flex>
              <Flex align="center" gap="1">
                <Text size="1" color="gray">
                  判定占比
                </Text>
                <TextField.Root
                  className="debug-threshold-field"
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={skillSlotMinFraction}
                  onChange={(event) =>
                    setSkillSlotMinFraction(
                      Math.max(0, Math.min(1, Number(event.target.value) || 0))
                    )
                  }
                />
              </Flex>
              <Button
                type="button"
                size="1"
                disabled={findingSkillSlots || !capture}
                onClick={handleFindSkillSlots}
              >
                {findingSkillSlots ? "识别中…" : "识别技能位"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="技能冷却识别"
            help={DEBUG_SECTION_HELP.skillCooldownsInput}
            badge={
              skillCooldowns
                ? `${skillCooldowns.slots.filter((slot) => slot.cooling).length}/${skillCooldowns.slots.length}`
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Select.Root
                value={cooldownSection}
                onValueChange={(value) =>
                  setCooldownSection(value as "servant" | "equipment")
                }
              >
                <Select.Trigger aria-label="冷却识别目标" />
                <Select.Content>
                  <Select.Item value="servant">从者技能</Select.Item>
                  <Select.Item value="equipment">御主礼装技能</Select.Item>
                </Select.Content>
              </Select.Root>
              <Select.Root
                value={cooldownMode}
                onValueChange={(value) =>
                  setCooldownMode(value as "det" | "rec")
                }
              >
                <Select.Trigger aria-label="冷却识别模式" />
                <Select.Content>
                  <Select.Item value="det">检测+识别（慢）</Select.Item>
                  <Select.Item value="rec">纯识别（快）</Select.Item>
                </Select.Content>
              </Select.Root>
              {[
                ["dx", cooldownCornerDx, setCooldownCornerDx, -0.1, 0.1],
                ["dy", cooldownCornerDy, setCooldownCornerDy, 0, 0.15],
                ["w", cooldownCornerW, setCooldownCornerW, 0.01, 0.1],
                ["h", cooldownCornerH, setCooldownCornerH, 0.01, 0.1],
              ].map(([label, value, setter, min, max]) => (
                <Flex align="center" gap="1" key={`cooldown-${label}`}>
                  <Text size="1" color="gray">
                    {label as string}
                  </Text>
                  <TextField.Root
                    className="debug-threshold-field"
                    type="number"
                    min={min as number}
                    max={max as number}
                    step={0.002}
                    value={value as number}
                    onChange={(event) =>
                      (setter as (next: number) => void)(
                        Math.max(
                          min as number,
                          Math.min(max as number, Number(event.target.value) || 0)
                        )
                      )
                    }
                  />
                </Flex>
              ))}
              <Flex align="center" gap="1">
                <Text size="1" color="gray">
                  放大
                </Text>
                <TextField.Root
                  className="debug-threshold-field"
                  type="number"
                  min={1}
                  max={6}
                  step={0.5}
                  value={cooldownScale}
                  onChange={(event) =>
                    setCooldownScale(
                      Math.max(1, Math.min(6, Number(event.target.value) || 3))
                    )
                  }
                />
              </Flex>
              <Flex align="center" gap="1">
                <Text size="1" color="gray">
                  置信度
                </Text>
                <TextField.Root
                  className="debug-threshold-field"
                  type="number"
                  min={0}
                  max={1}
                  step={0.05}
                  value={cooldownMinConfidence}
                  onChange={(event) =>
                    setCooldownMinConfidence(
                      Math.max(0, Math.min(1, Number(event.target.value) || 0))
                    )
                  }
                />
              </Flex>
              <Button
                type="button"
                size="1"
                disabled={findingSkillCooldowns || !capture}
                onClick={handleFindSkillCooldowns}
              >
                {findingSkillCooldowns ? "识别中…" : "识别冷却"}
              </Button>
            </Flex>
          </DebugSection>

          <DebugSection
            title="助战识别"
            help={DEBUG_SECTION_HELP.supportInput}
            badge={
              supportMetadata
                ? `${supportMetadata.name} · ${supportMetadata.npNames.length} 宝具`
                : undefined
            }
          >
            <Flex gap="2" align="center" wrap="wrap" className="debug-toolbar">
              <Flex align="center" gap="1">
                <Text size="1" color="gray">
                  队伍配置
                </Text>
                <Select.Root
                  value={
                    supportTeamProjectId !== null &&
                    projects.some(
                      (project) => project.id === supportTeamProjectId
                    )
                      ? supportTeamProjectId
                      : EMPTY_DEBUG_SELECT_VALUE
                  }
                  onValueChange={handleSelectSupportTeam}
                >
                  <Select.Trigger
                    className="debug-select-trigger"
                    aria-label="助战队伍配置"
                  />
                  <Select.Content>
                    <Select.Item value={EMPTY_DEBUG_SELECT_VALUE}>
                      不套用
                    </Select.Item>
                    {projects.map((project) => (
                      <Select.Item key={project.id} value={project.id}>
                        {project.name || project.id}
                      </Select.Item>
                    ))}
                  </Select.Content>
                </Select.Root>
              </Flex>
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={() => setServantPickerTarget("support")}
              >
                {selectedSupportServant
                  ? selectedSupportServant.name_cn_server?.trim() ||
                    selectedSupportServant.name_cn
                  : "选择助战从者"}
              </Button>
              <Button
                type="button"
                size="1"
                variant="surface"
                onClick={() => setSupportSettingsOpen(true)}
                title="星图分、冠位模式、礼装与 OCR 设置"
              >
                设置
              </Button>
              <label
                className="debug-coord-toggle"
                title="默认关闭以保持 OCR 进程热启动；开启后每次识别结束释放 OCR 进程，下次识别需要重新加载模型。"
              >
                <Checkbox
                  checked={supportReleaseOcr}
                  onCheckedChange={(checked) =>
                    setSupportReleaseOcr(checked === true)
                  }
                />
                <Text size="1">识别后释放 OCR</Text>
              </label>
              <Text size="1" color="gray">
                {supportSettingsSummary}
              </Text>
              <Button
                type="button"
                size="1"
                disabled={
                  findingSupports || !capture || supportServantId === null
                }
                onClick={handleFindSupports}
              >
                {findingSupports ? "识别中…" : "识别助战"}
              </Button>
            </Flex>
          </DebugSection>


          <Box className="debug-log-container">
            <Flex justify="between" align="center" style={{ marginBottom: 6 }}>
              <Text size="1" color="gray" className="debug-side-label">
                调试日志
              </Text>
              <Button
                type="button"
                size="1"
                variant="surface"
                color="gray"
                disabled={logs.length === 0}
                onClick={handleClearLogs}
              >
                清空
              </Button>
            </Flex>
            <Box className="debug-log">
              {logs.length === 0 && (
                <Text size="1" color="gray">
                  暂无日志
                </Text>
              )}
              {logs.map((entry, i) => (
                <div
                  key={i}
                  className={`debug-log-entry debug-log-${entry.level}`}
                >
                  <span className="debug-log-time">{entry.time}</span>
                  <span className="debug-log-msg">{entry.message}</span>
                </div>
              ))}
              <div ref={logEndRef} />
            </Box>
          </Box>
        </Flex>

        <Flex direction="column" className="debug-side" gap="3">
          <Box>
            <Text size="1" color="gray" className="debug-side-label">
              识别画面
            </Text>
            <Box className="debug-screen-badge">
              <Text size="3" weight="bold">
                {capture?.screen ?? liveFrame?.screen ?? "—"}
              </Text>
              {capture && (
                <Text size="1" color="gray">
                  score {capture.score.toFixed(3)}
                </Text>
              )}
              {capture?.screenSize && (
                <Text size="1" color="gray">
                  设备 {capture.screenSize.w} × {capture.screenSize.h}
                </Text>
              )}
              {liveFrame && (
                <Text size="1" color="gray">
                  scrcpy {liveFrame.width} × {liveFrame.height}
                  {dynamicCapturePaused ? " · 已暂停" : ""}
                </Text>
              )}
              {streamStatus?.connected && !liveFrame && streamStatus.screenSize && (
                <Text size="1" color="gray">
                  scrcpy {streamStatus.screenSize.w} × {streamStatus.screenSize.h}
                </Text>
              )}
            </Box>
          </Box>

          {timingEnabled && (
            <DebugSection title="识别耗时" badge={`${timingEntries.length} 项`}>
              <Flex justify="between" align="center" gap="2">
                <Text size="1" color="gray">
                  单位毫秒，含 IPC 与 CV 处理
                </Text>
                <Button
                  type="button"
                  size="1"
                  variant="surface"
                  color="gray"
                  disabled={timingEntries.length === 0}
                  onClick={() => debugTiming.clear()}
                >
                  清空
                </Button>
              </Flex>
              <Box className="debug-timing-list">
                {timingEntries.length === 0 ? (
                  <Text size="1" color="gray">
                    暂无记录
                  </Text>
                ) : (
                  <>
                    {timingEntries.map((entry, index) => (
                      <Flex
                        key={`${entry.label}-${index}`}
                        justify="between"
                        gap="3"
                        className="debug-timing-row"
                      >
                        <Text size="1" color="gray">
                          {entry.label}
                        </Text>
                        <Text size="1" className="debug-timing-value">
                          {entry.ms} ms
                        </Text>
                      </Flex>
                    ))}
                    <Flex
                      justify="between"
                      gap="3"
                      className="debug-timing-total"
                    >
                      <Text size="1" weight="medium">
                        合计
                      </Text>
                      <Text size="1" weight="medium">
                        {timingTotal} ms
                      </Text>
                    </Flex>
                  </>
                )}
              </Box>
            </DebugSection>
          )}

          {selectedElementSpec && (
            <DebugSection title="当前元素配置" help={DEBUG_SECTION_HELP.selectedElementConfig}>
              <Box className="debug-screen-badge">
                <Text size="2" weight="medium">
                  {selectedScreen}.{selectedElement}
                </Text>
                <Text size="1" color="gray">
                  模板:{" "}
                  {selectedElementSpec.templates?.join(", ") ??
                    selectedElementSpec.template ??
                    "(未配置)"}
                </Text>
                {selectedElementSpec.region && (
                  <Text size="1" color="gray">
                    区域:{" "}
                    {`(${selectedElementSpec.region.x.toFixed(2)}, ${selectedElementSpec.region.y.toFixed(2)}, ${selectedElementSpec.region.w.toFixed(2)}, ${selectedElementSpec.region.h.toFixed(2)})`}
                  </Text>
                )}
                {selectedElementSpec.threshold !== undefined && (
                  <Text size="1" color="gray">
                    阈值: {selectedElementSpec.threshold}
                  </Text>
                )}
              </Box>
            </DebugSection>
          )}

          <DebugSection
            title="指令卡识别"
            help={DEBUG_SECTION_HELP.commandCardsResult}
            timing={timingEnabled ? debugTiming.latest("识别指令卡") : undefined}
            badge={commandCards.length || undefined}
          >
            <Box className="debug-match-list">
              {commandCards.length === 0 && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {commandCards.map((c) => (
                <Box
                  key={`card-row-${c.slot}`}
                  className={`debug-match-entry ${c.servantId !== undefined ? "found" : "missed"}`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      C{c.slot + 1}
                      {c.suit ? ` · ${c.suit.toUpperCase()}` : " · ?"}
                    </Text>
                    <Text size="1" color="gray">
                      {c.iconScore !== undefined
                        ? `icon ${c.iconScore.toFixed(3)}`
                        : "无图标"}
                    </Text>
                  </Flex>
                  {c.servantId !== undefined ? (
                    <Text size="1" color="green">
                      从者 {c.servantId} · 进阶 {c.ascension ?? "?"} · face{" "}
                      {(c.faceScore ?? 0).toFixed(3)}
                    </Text>
                  ) : (
                    <Text size="1" color="gray">
                      未识别从者
                    </Text>
                  )}
                  <Text size="1" color="gray">
                    中心 ({c.x.toFixed(3)}, {c.y.toFixed(3)})
                    {c.critChance !== undefined
                      ? ` · 暴击 ${c.critChance}%`
                      : ""}
                  </Text>
                  {c.critDigitReads && c.critDigitReads.length > 0 && (
                    <Text size="1" color="gray">
                      暴击位{" "}
                      {c.critDigitReads
                        .map((r) => {
                          const glyph =
                            r.digit !== null && r.digit !== undefined
                              ? String(r.digit)
                              : "-";
                          const score = r.score.toFixed(2);
                          return r.kept
                            ? `${glyph}@${score}`
                            : `(${glyph}@${score})`;
                        })
                        .join(" · ")}
                    </Text>
                  )}
                </Box>
              ))}
            </Box>
          </DebugSection>

          <DebugSection
            title="宝具调试"
            help={DEBUG_SECTION_HELP.npDebugResult}
            timing={timingEnabled ? debugTiming.latest("读取宝具识别") : undefined}
            badge={
              npGaugeSlots.length > 0
                ? `卡 ${npGaugeSlots.filter((s) => s.cardReady === true).length}/${npGaugeSlots.length} · 条 ${npGaugeSlots.filter((s) => s.npGlowReady === true).length}/${npGaugeSlots.length}`
                : undefined
            }
          >
            <Box className="debug-match-list">
              {npGaugeSlots.length === 0 && (
                <Text size="1" color="gray">
                  暂无调试结果
                </Text>
              )}
              {npGaugeSlots[0]?.turnCountModelLabel && (
                <Text size="1" color="gray">
                  模型当前回合：{npGaugeSlots[0].turnCountModelLabel}
                </Text>
              )}
              {npGaugeSlots[0]?.turnSequenceValue != null && (
                <Text size="1" color="gray">
                  CNN-CTC 当前回合：{npGaugeSlots[0].turnSequenceValue || "未识别"}
                  {npGaugeSlots[0].turnSequenceAccepted ? "" : "（置信度不足）"}
                </Text>
              )}
              {npGaugeSlots.map((s) => (
                <Box
                  key={`np-row-${s.slot}`}
                  className={`debug-match-entry ${
                    npAllUnknown(s) ? "missed" : npAnyReady(s) ? "found" : ""
                  }`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      NP{s.slot + 1}
                    </Text>
                    <Text
                      size="1"
                      color={npAllUnknown(s) ? "gray" : npAnyReady(s) ? "green" : "amber"}
                    >
                      卡 {npCardStatus(s)} · 条 {npGaugeStatus(s)}
                    </Text>
                  </Flex>
                  <Text size="1" color="gray">
                    {s.cardReady != null
                      ? `宝具卡 ${s.cardReady ? "ready" : "not ready"} · edge ${(s.edgeFrac * 100).toFixed(2)}%`
                      : "宝具卡 unknown"}
                    {s.npGlowScore != null
                      ? ` · 宝具条端帽 ${s.npGlowScore.toFixed(3)}${
                          s.npGlowReady ? " 亮" : ""
                        }`
                      : " · 宝具条端帽未识别"}
                    {s.gaugeDigitCount != null ? ` · digit ${s.gaugeDigitCount}位` : ""}
                    {s.gaugeHundredsVisible != null
                      ? ` · 百位${s.gaugeHundredsVisible ? "有" : "无"}`
                      : ""}
                    {s.gaugeDigitModelLabels?.length === 3
                      ? ` · 模型 百/十/个 ${s.gaugeDigitModelLabels.join(" / ")}`
                      : ""}
                    {s.gaugeSequenceValue != null
                      ? ` · CNN-CTC ${s.gaugeSequenceValue || "未识别"} (${(s.gaugeSequenceConfidence ?? 0).toFixed(3)}${s.gaugeSequenceAccepted ? "" : " 拒绝"})`
                      : ""}
                  </Text>
                </Box>
              ))}
            </Box>
          </DebugSection>

          <DebugSection
            title="牵绊升级识别"
            help={DEBUG_SECTION_HELP.bondLevelResult}
            timing={timingEnabled ? debugTiming.latest("牵绊升级识别") : undefined}
            badge={
              bondLevelUpResult
                ? bondLevelUpResult.ok
                  ? "ok"
                  : bondLevelUpResult.reason || "failed"
                : undefined
            }
          >
            <Box className="debug-match-list">
              {!bondLevelUpResult && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {bondLevelUpResult && (
                <Box
                  className={`debug-match-entry ${bondLevelUpResult.ok ? "found" : "missed"}`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      {bondLevelUpResult.ok ? "识别成功" : "识别失败"}
                    </Text>
                    <Text size="1" color={bondLevelUpResult.ok ? "green" : "red"}>
                      {bondLevelUpResult.ok
                        ? `Lv.${bondLevelUpResult.bondLevelAfter ?? "?"}`
                        : bondLevelUpResult.reason || "unknown"}
                    </Text>
                  </Flex>
                  <Text size="1" color="gray">
                    matched:{" "}
                    {bondLevelUpResult.servantNameMatched ??
                      bondLevelUpResult.servantName ??
                      "—"}
                    {bondLevelUpResult.servantMatchScore != null
                      ? ` (${debugPercent(bondLevelUpResult.servantMatchScore)})`
                      : ""}
                  </Text>
                  {bondLevelUpResult.servantName &&
                    bondLevelUpResult.servantNameMatched &&
                    bondLevelUpResult.servantName !==
                      bondLevelUpResult.servantNameMatched && (
                      <Text size="1" color="gray">
                        OCR raw: {bondLevelUpResult.servantName}
                      </Text>
                    )}
                  <Text size="1" color="gray">
                    anchor {debugPercent(bondLevelUpResult.confidence?.anchor)} · 数字{" "}
                    {debugPercent(bondLevelUpResult.confidence?.bondLevelAfter)} · OCR{" "}
                    {debugPercent(bondLevelUpResult.confidence?.servantOcr)}
                  </Text>
                  {bondLevelUpResult.diagnostics !== undefined && (
                    <Text size="1" color="gray">
                      diagnostics 已返回，可在 DevTools / Rust 日志中对照查看。
                    </Text>
                  )}
                </Box>
              )}
            </Box>
          </DebugSection>

          <DebugSection
            title="攻击按钮识别"
            help={DEBUG_SECTION_HELP.attackButtonResult}
            timing={timingEnabled ? debugTiming.latest("攻击按钮识别") : undefined}
            badge={
              attackButton
                ? `${(attackButton.score * 100).toFixed(0)}% ${attackButton.found ? "✓" : "✗"}`
                : undefined
            }
          >
            <Box className="debug-match-list">
              {!attackButton && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {attackButton && (
                <Box
                  className={`debug-match-entry ${attackButton.found ? "found" : "missed"}`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      {attackButton.template}
                    </Text>
                    <Text
                      size="1"
                      color={attackButton.found ? "green" : "red"}
                    >
                      {attackButton.found ? "命中" : "未命中"}
                    </Text>
                  </Flex>
                  <Text size="1" color="gray">
                    score {attackButton.score.toFixed(3)} · 阈值{" "}
                    {attackButton.threshold.toFixed(2)}
                  </Text>
                  <Text size="1" color="gray">
                    搜索区域 (
                    {attackButton.region.x.toFixed(3)},{" "}
                    {attackButton.region.y.toFixed(3)},{" "}
                    {attackButton.region.w.toFixed(3)},{" "}
                    {attackButton.region.h.toFixed(3)})
                  </Text>
                  {attackButton.found && (
                    <Text size="1" color="gray">
                      命中中心 ({attackButton.matchX.toFixed(3)},{" "}
                      {attackButton.matchY.toFixed(3)})
                    </Text>
                  )}
                  <Text size="1" color="gray">
                    点击 ({attackButton.tapPoint.x.toFixed(3)},{" "}
                    {attackButton.tapPoint.y.toFixed(3)})
                  </Text>
                </Box>
              )}
            </Box>
          </DebugSection>

          <DebugSection
            title="强化从者头像识别"
            help={DEBUG_SECTION_HELP.enhancementServantResult}
            timing={timingEnabled ? debugTiming.latest("强化从者头像识别") : undefined}
            badge={
              enhancementServantResult
                ? `${enhancementServantResult.matches.filter((m) => m.found).length}/${enhancementServantResult.matches.length}`
                : undefined
            }
          >
            <Box className="debug-match-list">
              {!enhancementServantResult && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {enhancementServantResult && (
                <Box className="debug-match-entry">
                  <Text size="2" weight="medium">
                    目标：#{enhancementServantResult.servantId}
                  </Text>
                  <Text size="1" color="gray">
                    模板缩放: {enhancementServantResult.templateSize.w} ×{" "}
                    {enhancementServantResult.templateSize.h}
                  </Text>
                  <Text size="1" color="gray">
                    crop (
                    {enhancementServantResult.templateCrop.x.toFixed(3)},{" "}
                    {enhancementServantResult.templateCrop.y.toFixed(3)},{" "}
                    {enhancementServantResult.templateCrop.w.toFixed(3)},{" "}
                    {enhancementServantResult.templateCrop.h.toFixed(3)})
                  </Text>
                  <Text size="1" color="gray">
                    搜索区域 (
                    {enhancementServantResult.searchRegion.x.toFixed(3)},{" "}
                    {enhancementServantResult.searchRegion.y.toFixed(3)},{" "}
                    {enhancementServantResult.searchRegion.w.toFixed(3)},{" "}
                    {enhancementServantResult.searchRegion.h.toFixed(3)})
                  </Text>
                  <Text size="1" color="gray">
                    anchors {enhancementServantResult.anchors.length} · cells{" "}
                    {enhancementServantResult.gridCells.length} · attempts{" "}
                    {enhancementServantResult.diagnostics.attempts || 1}
                    {enhancementServantResult.diagnostics.failReason
                      ? ` · ${enhancementServantResult.diagnostics.failReason}`
                      : ""}
                  </Text>
                  <Text size="1" color="gray">
                    method {enhancementServantResult.diagnostics.matchMethod ?? "grid"}
                    {enhancementServantResult.diagnostics.gridSkipped
                      ? "（国服已跳过网格，直接搜索头像）"
                      : enhancementServantResult.diagnostics.gridFailReason
                        ? `（网格失败：${enhancementServantResult.diagnostics.gridFailReason}）`
                        : ""}
                    {" · "}直接搜索阈值{" "}
                    {enhancementServantResult.diagnostics.directSearchThreshold?.toFixed(2) ??
                      "0.85"}
                  </Text>
                  {enhancementServantResult.diagnostics.timings && (
                    <Text size="1" color="gray">
                      耗时：网格{" "}
                      {enhancementServantResult.diagnostics.timings.gridMs ?? 0} ms · 模板{" "}
                      {enhancementServantResult.diagnostics.timings.templateMs ?? 0} ms ·
                      匹配 {enhancementServantResult.diagnostics.timings.matchMs ?? 0} ms ·
                      直接搜索{" "}
                      {enhancementServantResult.diagnostics.timings.directSearchMs ?? 0} ms ·
                      合计 {enhancementServantResult.diagnostics.timings.totalMs ?? 0} ms
                    </Text>
                  )}
                </Box>
              )}
              {enhancementServantResult?.referenceAnchor && (
                <Box className="debug-match-entry found">
                  <Text size="2" weight="medium">
                    参考 anchor
                  </Text>
                  <Text size="1" color="gray">
                    col {enhancementServantResult.referenceAnchor.col ?? "?"} ·
                    edge{" "}
                    {enhancementServantResult.referenceAnchor.edgeScore.toFixed(3)} ·
                    gray{" "}
                    {enhancementServantResult.referenceAnchor.grayScore.toFixed(3)}
                  </Text>
                </Box>
              )}
              {enhancementServantResult?.matches
                .slice()
                .sort((a, b) => b.score - a.score)
                .map((m) => (
                  <Box
                    key={`enhancement-side-${m.row}-${m.col}-${m.template}`}
                    className={`debug-match-entry ${m.found ? "found" : "missed"}`}
                  >
                    <Flex justify="between" align="center">
                      <Text size="2" weight="medium">
                        {m.template}
                      </Text>
                      <Text size="1" color={m.found ? "green" : "red"}>
                        {m.score.toFixed(3)}
                      </Text>
                    </Flex>
                    <Text size="1" color="gray">
                      r{m.row} c{m.col} · 阈值{" "}
                      {enhancementServantResult.threshold.toFixed(2)}
                      {m.elapsedMs != null ? ` · ${m.elapsedMs} ms` : ""}
                      {m.region
                        ? ` · center (${m.x.toFixed(3)}, ${m.y.toFixed(3)})`
                        : ""}
                    </Text>
                  </Box>
                ))}
            </Box>
          </DebugSection>

          <DebugSection
            title="助战识别"
            help={DEBUG_SECTION_HELP.supportResult}
            timing={timingEnabled ? debugTiming.latest("识别助战") : undefined}
            badge={supportResult?.supports.length || undefined}
          >
            <Box className="debug-match-list">
              {!supportResult && (
                <Text size="1" color="gray">
                  暂无识别结果
                </Text>
              )}
              {supportResult && supportMetadata && (
                <Box className="debug-match-entry">
                  <Text size="2" weight="medium">
                    目标：{supportMetadata.name} (#{supportMetadata.id})
                  </Text>
                  <Text size="1" color="gray">
                    名称候选: {supportMetadata.names.join(" / ")}
                  </Text>
                  {supportResult.scoreFilter &&
                    (supportResult.scoreFilter.starMapScoreMin != null ||
                      (supportResult.scoreFilter.grandMode &&
                        supportResult.scoreFilter.grandStarMapScoreMin != null)) && (
                      <Text size="1" color="blue">
                        分值条件：星图 ≥ {supportResult.scoreFilter.starMapScoreMin ?? "任意"}
                        {supportResult.scoreFilter.grandMode
                          ? ` · 冠位星图 ≥ ${supportResult.scoreFilter.grandStarMapScoreMin ?? "任意"}`
                          : ""}
                      </Text>
                    )}
                  <Text size="1" color="gray">
                    宝具候选: {supportMetadata.npNames.join(" / ")}
                  </Text>
                  <Text size="1" color="gray">
                    OCR 片段 {supportResult.diagnostics.fragmentCount} · 名称候选{" "}
                    {supportResult.diagnostics.nameCandidates.length} ·
                    宝具候选 {supportResult.diagnostics.npCandidates.length}
                  </Text>
                  {supportResult.diagnostics.isGrandSectionVisible != null && (
                    <Text
                      size="1"
                      color={
                        supportResult.diagnostics.isGrandSectionVisible
                          ? "amber"
                          : "gray"
                      }
                    >
                      冠位段：
                      {supportResult.diagnostics.isGrandSectionVisible
                        ? "✓ 已识别"
                        : "✗ 未识别"}
                      （
                      {(
                        supportResult.diagnostics.grandIconAnchorScores ?? []
                      )
                        .map((s, idx) => {
                          if (s == null) return `#${idx + 1}=n/a`;
                          const tick = s >= 0.80 ? "✓" : "✗";
                          return `#${idx + 1}=${s.toFixed(2)}${tick}`;
                        })
                        .join("  ") ||
                        `${
                          (
                            supportResult.diagnostics.confirmButtonAnchors ??
                            []
                          ).length
                        } 个 ROI`}
                      ）
                    </Text>
                  )}
                </Box>
              )}
              {supportResult?.supports.map((s, i) => (
                <Box
                  key={`support-side-${i}`}
                  className={`debug-match-entry ${
                    s.scoreFilterPassed === false || s.levelFilterPassed === false
                      ? "missed"
                      : "found"
                  }`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      助战 {i + 1}
                    </Text>
                    <Text size="1" color="gray">
                      tap ({s.tap.x.toFixed(3)}, {s.tap.y.toFixed(3)})
                      {s.elapsedMs != null ? ` · ${s.elapsedMs} ms` : ""}
                    </Text>
                  </Flex>
                  <Text size="1" color="green">
                    名: {s.nameText} ({s.nameScore.toFixed(2)})
                  </Text>
                  <Text size="1" color="green">
                    宝: {s.npText} ({s.npScore.toFixed(2)})
                    {s.npMatchedName ? ` → ${s.npMatchedName}` : ""}
                    {s.npLevel != null ? ` · 等级 ${s.npLevel}` : ""}
                  </Text>
                  <Text size="1" color={s.servantLevel != null ? "green" : "amber"}>
                    从者等级：{s.servantLevel ?? "未识别"}
                  </Text>
                  {s.starMapScore != null && (
                    <Text
                      size="1"
                      color={
                        s.scoreFilterPassed === false
                          ? "red"
                          : s.scoreFilterPassed === true
                            ? "green"
                            : "blue"
                      }
                    >
                      星图分值：{s.starMapScore}
                      {s.grandStarMapScore != null ? ` / ${s.grandStarMapScore}` : ""}
                      {s.scoreText ? `（OCR: ${s.scoreText}）` : ""}
                      {s.scoreFilterPassed === true ? " · ✓ 达标" : ""}
                      {s.scoreFilterPassed === false
                        ? ` · ✗ ${s.scoreFilterReason ?? "未达标"}`
                        : ""}
                    </Text>
                  )}
                  {s.starMapScore == null && s.scoreFilterPassed === false && (
                    <Text size="1" color="red">
                      分值识别失败 · ✗ {s.scoreFilterReason ?? "未达标"}
                      {s.scoreText ? `（OCR: ${s.scoreText}）` : ""}
                    </Text>
                  )}
                  {s.levelFilterPassed === true && (
                    <Text size="1" color="green">
                      等级筛选 · ✓ 达标
                    </Text>
                  )}
                  {s.levelFilterPassed === false && (
                    <Text size="1" color="red">
                      等级筛选 · ✗ {s.levelFilterReason ?? "未达标"}
                    </Text>
                  )}
                  {s.skillPanel && (
                    <Text size="1" color="blue">
                      {supportPanelLabel(s.skillPanel)}：
                      {s.skillPanel === "append"
                        ? supportLevelList(s.appendSkillLevels)
                        : supportLevelList(s.skillLevels)}
                    </Text>
                  )}
                  {s.skillLevelDiagnostics && s.skillLevelDiagnostics.length > 0 && (
                    <Text size="1" color="gray">
                      技能诊断：{supportSkillDiagnosticsText(s.skillLevelDiagnostics)}
                    </Text>
                  )}
                  <Text size="1" color="gray">
                    行 y={s.rowRegion.y.toFixed(3)} h=
                    {s.rowRegion.h.toFixed(3)}
                  </Text>
                  <Text size="1" color={s.scoreAnchor ? "green" : "amber"}>
                    确认锚点：
                    {s.scoreAnchor
                      ? `(${s.scoreAnchor.x.toFixed(3)}, ${s.scoreAnchor.y.toFixed(3)}, ${s.scoreAnchor.w.toFixed(3)}, ${s.scoreAnchor.h.toFixed(3)})`
                      : "未识别"}
                  </Text>
                  {s.ce && (
                    <>
                      <Text
                        size="1"
                        color={s.ce.passed ? "green" : "red"}
                      >
                        礼装 {s.ce.score.toFixed(3)} /{" "}
                        {s.ce.threshold.toFixed(2)}{" "}
                        {s.ce.passed ? "✓ 匹配" : "✗ 未达阈值"}
                        {s.ce.error ? ` · ${s.ce.error}` : ""}
                      </Text>
                      {(s.ce.artworkChecks ?? []).map((check) => (
                        <Text
                          key={`ce-artwork-${check.regionKind}-${check.variant}`}
                          size="1"
                          color={check.passed ? "green" : "red"}
                        >
                          区域 {check.regionKind}:{check.variant}:{" "}
                          {check.score.toFixed(3)} /{" "}
                          {check.threshold.toFixed(2)}{" "}
                          {check.selected ? "*" : check.passed ? "✓" : "✗"}
                          {check.error ? ` · ${check.error}` : ""}
                        </Text>
                      ))}
                      {(s.ce.iconChecks ?? []).map((check) => (
                        <Text
                          key={`ce-icon-${check.kind}-${check.templateKey}`}
                          size="1"
                          color={check.passed ? "green" : "red"}
                        >
                          图标 {check.kind}: {check.score.toFixed(3)} /{" "}
                          {check.threshold.toFixed(2)}{" "}
                          {check.passed ? "✓" : "✗"}
                          {check.error ? ` · ${check.error}` : ""}
                        </Text>
                      ))}
                    </>
                  )}
                  {(s.grandCes ?? []).map((ce, ceIndex) => (
                    <Fragment key={`grand-ce-side-${i}-${ceIndex}`}>
                      <Text
                        size="1"
                        color={ce.passed ? "green" : "red"}
                      >
                        冠{ceIndex + 1} {ce.score.toFixed(3)} /{" "}
                        {ce.threshold.toFixed(2)}{" "}
                        {ce.passed ? "✓" : "✗"}
                        {(ce.iconChecks ?? [])
                          .map(
                            (check) =>
                              ` · ${check.kind} ${check.score.toFixed(2)}${check.passed ? "✓" : "✗"}`
                          )
                          .join("")}
                      </Text>
                      {(ce.artworkChecks ?? []).map((check) => (
                        <Text
                          key={`grand-ce-artwork-${ceIndex}-${check.regionKind}-${check.variant}`}
                          size="1"
                          color={check.passed ? "green" : "red"}
                        >
                          冠{ceIndex + 1} 区域 {check.regionKind}:{check.variant}:{" "}
                          {check.score.toFixed(3)} /{" "}
                          {check.threshold.toFixed(2)}{" "}
                          {check.selected ? "*" : check.passed ? "✓" : "✗"}
                          {check.error ? ` · ${check.error}` : ""}
                        </Text>
                      ))}
                    </Fragment>
                  ))}
                </Box>
              ))}
              {supportResult && supportResult.supports.length === 0 && (
                <>
                  {supportResult.diagnostics.nameCandidates.length > 0 && (
                    <Box className="debug-match-entry missed">
                      <Text size="1" color="gray">
                        名称候选 (未配对):
                      </Text>
                      {supportResult.diagnostics.nameCandidates
                        .slice(0, 8)
                        .map((c, i) => (
                          <Text
                            size="1"
                            color="gray"
                            key={`name-cand-side-${i}`}
                          >
                            · {c.text} ({c.score.toFixed(2)})
                            {c.matchedName ? ` → ${c.matchedName}` : ""} @ y=
                            {c.region.y.toFixed(3)}
                          </Text>
                        ))}
                    </Box>
                  )}
                  {supportResult.diagnostics.npCandidates.length > 0 && (
                    <Box className="debug-match-entry missed">
                      <Text size="1" color="gray">
                        宝具候选 (未配对):
                      </Text>
                      {supportResult.diagnostics.npCandidates
                        .slice(0, 8)
                        .map((c, i) => (
                          <Text size="1" color="gray" key={`np-cand-side-${i}`}>
                            · {c.text} ({c.score.toFixed(2)})
                            {c.matchedName ? ` → ${c.matchedName}` : ""} @ y=
                            {c.region.y.toFixed(3)}
                          </Text>
                        ))}
                    </Box>
                  )}
                </>
              )}
              {supportResult?.diagnostics.timings && (
                <Text size="1" color="gray">
                  耗时：OCR {supportResult.diagnostics.timings.ocrMs ?? 0} ms ·
                  逐行明细 {supportResult.diagnostics.timings.detailsMs ?? 0} ms ·
                  合计 {supportResult.diagnostics.timings.totalMs ?? 0} ms
                </Text>
              )}
            </Box>
          </DebugSection>

          <DebugSection
            title="匹配历史"
            help={DEBUG_SECTION_HELP.matchHistory}
            badge={history.length || undefined}
            defaultOpen
          >
            <Flex justify="between" align="center" mb="2">
              <Text size="1" color="gray">
                最近 {history.length} 条（最多保留 60 条）
              </Text>
              <Button
                type="button"
                size="1"
                variant="surface"
                color="gray"
                disabled={history.length === 0}
                onClick={clearHistory}
              >
                清空历史
              </Button>
            </Flex>
            <Box className="debug-match-list">
              {history.length === 0 && (
                <Text size="1" color="gray">
                  暂无记录：模板查找、指令卡、宝具、助战、战斗场景、攻击按钮、强化从者识别都会记到这里。
                </Text>
              )}
              {history.map((entry) => (
                <Box
                  key={entry.id}
                  className={`debug-match-entry ${
                    entry.ok === false ? "missed" : "found"
                  }`}
                >
                  <Flex justify="between" align="center">
                    <Text size="2" weight="medium">
                      {entry.label}
                    </Text>
                    <Text size="1" color="gray">
                      {entry.timestamp}
                    </Text>
                  </Flex>
                  <Text
                    size="1"
                    color={entry.ok === false ? "red" : entry.ok ? "green" : "gray"}
                  >
                    {entry.summary}
                    {entry.elapsedMs != null ? ` · ${entry.elapsedMs} ms` : ""}
                  </Text>
                </Box>
              ))}
            </Box>
          </DebugSection>
        </Flex>
      </Flex>
      {supportSettingsOpen && (
        <SupportSettingsDialog
          open={supportSettingsOpen}
          project={supportSettingsProject}
          servant={selectedSupportServant ?? null}
          portraitSrc={
            selectedSupportServant
              ? supportPortraitMap[selectedSupportServant.variantKey] ?? null
              : null
          }
          slotNumber={1}
          craftEssenceGroups={supportCraftEssenceGroups}
          onCraftEssenceOpen={(index, grandMode) => {
            if (grandMode) {
              setCraftEssencePickerTarget(index as 0 | 1 | 2);
            } else {
              setCraftEssencePickerTarget("single");
            }
            setCraftEssencePickerOpen(true);
          }}
          onOpenChange={setSupportSettingsOpen}
          onConfirm={handleSupportSettingsConfirm}
        />
      )}
      <TemplateKeyPickerDialog
        open={templatePickerOpen}
        onOpenChange={setTemplatePickerOpen}
        templates={templateThumbnails}
        selectedKey={rawTemplateInput.trim() || null}
        onSelect={(key) => {
          setRawTemplateInput(key);
          log(`选择模板 key: ${key}`);
        }}
      />
      <TemplatePreviewDialog
        templateKey={previewTemplateKey}
        onOpenChange={(open) => {
          if (!open) setPreviewTemplateKey(null);
        }}
      />
      <ServantSelectDialog
        open={servantPickerTarget !== null}
        onOpenChange={(open) => {
          if (!open) setServantPickerTarget(null);
        }}
        onSelect={handleSelectDebugServant}
        servants={availableServants}
        disabledIds={
          servantPickerTarget === "card" ? selectedCardServantIds : undefined
        }
      />
      <CraftEssenceSelectDialog
        open={craftEssencePickerOpen}
        onOpenChange={(open) => {
          setCraftEssencePickerOpen(open);
          if (!open) setCraftEssencePickerTarget(null);
        }}
        onSelect={(ce) => {
          if (craftEssencePickerTarget === "single") {
            setSupportCraftEssenceId(ce.id);
          } else if (craftEssencePickerTarget != null) {
            setSupportGrandCraftEssenceIds((prev) => {
              const next = [...prev] as [number | null, number | null, number | null];
              next[craftEssencePickerTarget] = ce.id;
              return next;
            });
          }
          setSupportResult(null);
        }}
        craftEssences={craftEssences}
        grandBondOnly={craftEssencePickerTarget === 1}
        servantId={supportServantId}
        mlbRequired={
          craftEssencePickerTarget === "single"
            ? supportCraftEssenceMlbRequired
            : craftEssencePickerTarget != null
              ? supportGrandCraftEssenceMlbRequired[craftEssencePickerTarget]
              : true
        }
        onMlbRequiredChange={
          craftEssencePickerTarget === "single"
            ? setSupportCraftEssenceMlbRequired
            : craftEssencePickerTarget != null
              ? (required) =>
                  setSupportGrandCraftEssenceMlbRequired((prev) => {
                    const next = [...prev] as [boolean, boolean, boolean];
                    next[craftEssencePickerTarget] = required;
                    return next;
                  })
              : undefined
        }
        grandBondCeMode={supportGrandBondCeMode}
        onGrandBondCeModeChange={
          craftEssencePickerTarget === 1 ? setSupportGrandBondCeMode : undefined
        }
      />
    </Flex>
  );
}
