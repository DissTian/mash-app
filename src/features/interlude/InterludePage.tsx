import { useCallback, useEffect, useMemo, useState } from "react";
import { Box, Button, Checkbox, Flex, Select, Text, TextField } from "@radix-ui/themes";
import { ChevronLeftIcon, ReloadIcon } from "@radix-ui/react-icons";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { SectionHeading } from "../../components/common/SectionHeading";
import { convertFileSrc, invoke, listen } from "../../tauri";
import type { AutoBattleProfile } from "../../types/autoBattle";
import type { AutomationStatus } from "../../types/automation";
import type { BattleApRecoveryItem, BattleApRecoveryLimits, Project } from "../../types/project";
import type { CraftEssence } from "../../types/craftEssence";
import type { Servant } from "../../types/servant";
import type { Server } from "../../types/server";
import {
  DEFAULT_INTERLUDE_TASK,
  type InterludeCaptureResult,
  type InterludeQuestMode,
  type InterludeQuestRow,
  type InterludeTaskConfig,
} from "../../types/interlude";
import { isAutomationTerminal } from "../../types/automation";
import { ApRecoveryPanel } from "../battle/ApRecoveryPanel";
import { normalizeApRecoveryLimit } from "../battle/apRecoveryModel";
import { ServantSelectDialog } from "../team/ServantSelectDialog";
import { CraftEssenceSelectDialog } from "../team/CraftEssenceSelectDialog";
import { SupportSettingsDialog } from "../team/SupportSettingsDialog";

interface InterludePageProps {
  servants: Servant[];
  craftEssences: CraftEssence[];
  onBack: () => void;
  onAutomationStart?: () => void;
  onAutomationStartFailed?: () => void;
  onLogEntry?: (message: string) => void;
}

interface AutomationEvent {
  status: AutomationStatus;
  currentScreen: string;
  message: string;
}

const AP_ORDER: BattleApRecoveryItem[] = ["gold", "silver", "bronze", "copper", "rainbow"];

function orderedApItems(config: InterludeTaskConfig): BattleApRecoveryItem[] {
  return AP_ORDER.filter((item) => config.apRecoveryItems.includes(item));
}

function normalizeLimits(limits: BattleApRecoveryLimits): BattleApRecoveryLimits {
  return AP_ORDER.reduce(
    (next, item) => ({ ...next, [item]: normalizeApRecoveryLimit(limits?.[item] ?? null) }),
    {} as BattleApRecoveryLimits,
  );
}

function rowStyle(row: InterludeQuestRow) {
  return {
    left: `${row.region.x * 100}%`,
    top: `${row.region.y * 100}%`,
    width: `${row.region.w * 100}%`,
    height: `${row.region.h * 100}%`,
  };
}

function supportProject(config: InterludeTaskConfig): Project {
  return {
    id: "interlude",
    name: "幕间物语",
    supportServantId: config.support.servantId,
    supportServantVariantKey: config.support.servantVariantKey,
    supportServantLevelMin: config.support.servantLevelMin,
    supportNoblePhantasmLevelMin: config.support.noblePhantasmLevelMin,
    supportStarMapScoreMin: config.support.starMapScoreMin,
    supportSkillLevelMins: config.support.skillLevelMins,
    supportAppendSkillLevelMins: config.support.appendSkillLevelMins,
    supportGrandMode: false,
    supportGrandCraftEssenceIds: [null, null, null],
    supportGrandCraftEssenceIdLists: [[], [], []],
    supportGrandCraftEssenceMlbRequired: [true, true, true],
    supportGrandBondCeMode: "any",
    supportGrandStarMapScoreMin: null,
  } as unknown as Project;
}


export function InterludePage({
  servants,
  craftEssences,
  onBack,
  onAutomationStart,
  onAutomationStartFailed,
  onLogEntry,
}: InterludePageProps) {
  const [config, setConfig] = useState<InterludeTaskConfig>(DEFAULT_INTERLUDE_TASK);
  const [profiles, setProfiles] = useState<AutoBattleProfile[]>([]);
  const [server, setServer] = useState<Server | null>(null);
  const [capture, setCapture] = useState<InterludeCaptureResult | null>(null);
  const [selectedCandidateId, setSelectedCandidateId] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [running, setRunning] = useState(false);
  const [currentScreen, setCurrentScreen] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [servantPickerOpen, setServantPickerOpen] = useState(false);
  const [supportSettingsOpen, setSupportSettingsOpen] = useState(false);
  const [cePickerOpen, setCePickerOpen] = useState(false);

  const selectedServant = useMemo(
    () =>
      servants.find((servant) => servant.id === config.support.servantId) ?? null,
    [config.support.servantId, servants],
  );

  const saveConfig = useCallback(async (next: InterludeTaskConfig) => {
    setConfig(next);
    try {
      await invoke("save_interlude_task_config", { task: next });
    } catch (value) {
      setError(String(value));
    }
  }, []);

  useEffect(() => {
    invoke<InterludeTaskConfig>("get_interlude_task_config")
      .then((value) => setConfig({ ...DEFAULT_INTERLUDE_TASK, ...value }))
      .catch((value) => setError(String(value)));
    invoke<AutoBattleProfile[]>("list_auto_battle_profiles")
      .then(setProfiles)
      .catch(() => setProfiles([]));
    invoke<Server>("get_server")
      .then(setServer)
      .catch(() => setServer(null));
  }, []);

  useEffect(() => {
    const unlisten = listen<AutomationEvent>("automation-status", (event) => {
      setCurrentScreen(event.payload.currentScreen);
      if (isAutomationTerminal(event.payload)) {
        setRunning(false);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const capturePage = useCallback(() => {
    setCapturing(true);
    setError(null);
    setSelectedCandidateId(null);
    invoke<InterludeCaptureResult>("capture_interlude_page")
      .then((value) => {
        setCapture(value);
        setConfig((previous) => ({ ...previous, mode: "single" }));
      })
      .catch((value) => {
        setCapture(null);
        setError(String(value));
      })
      .finally(() => setCapturing(false));
  }, []);

  const handleStart = useCallback(() => {
    if (config.mode === "single" && (!capture || !selectedCandidateId)) return;
    setError(null);
    setRunning(true);
    setCurrentScreen("");
    onAutomationStart?.();
    void invoke("start_interlude_automation", {
      task: { ...config, mode: config.mode },
      workflow: {
        mode: config.mode,
        captureId: config.mode === "single" ? capture?.captureId ?? null : null,
        candidateId: config.mode === "single" ? selectedCandidateId : null,
      },
    }).catch((value) => {
      const message = `启动失败: ${String(value)}`;
      setError(message);
      setRunning(false);
      onLogEntry?.(message);
      onAutomationStartFailed?.();
    });
  }, [
    capture,
    config,
    onAutomationStart,
    onAutomationStartFailed,
    onLogEntry,
    selectedCandidateId,
  ]);

  const support = config.support;
  const canStart =
    !running &&
    !capturing &&
    server === "CN" &&
    (config.mode === "all" || (capture != null && selectedCandidateId != null));

  return (
    <Flex direction="column" className="battle-page interlude-page">
      <Flex align="center" gap="3" className="battle-header">
        <Button variant="soft" color="gray" onClick={onBack} disabled={running}>
          <ChevronLeftIcon width={16} height={16} />
          <Text size="2">返回</Text>
        </Button>
        <Text size="4" weight="bold">幕间物语</Text>
      </Flex>

      <Flex direction="column" gap="4" className="battle-body battle-body-scroll">
        {server === "JP" && (
          <Box className="rank-up-quest-notice">
            <Text size="2" color="red">幕间物语自动化首版仅支持国服。</Text>
          </Box>
        )}
        <Box className="battle-panel">
          <SectionHeading>完成方式</SectionHeading>
          <OptionCardRadioGroup
            value={config.mode}
            disabled={running}
            onValueChange={(value) =>
              void saveConfig({ ...config, mode: value as InterludeQuestMode })
            }
            options={[
              {
                value: "all",
                title: "完成所有可打幕间",
                description: "从当前列表开始，逐个完成可点击任务",
              },
              {
                value: "single",
                title: "完成指定幕间任务",
                description: "截图后选择一个任务，只完成它",
              },
            ]}
          />
        </Box>

        <Box className="battle-panel">
          <SectionHeading>支援角色</SectionHeading>
          <Flex gap="3" align="center" wrap="wrap">
            <Button
              variant="soft"
              color="gray"
              onClick={() => setServantPickerOpen(true)}
              disabled={running}
            >
              {selectedServant ? selectedServant.name_cn : "选择支援从者"}
            </Button>
            {selectedServant && (
              <Button
                variant="ghost"
                color="gray"
                onClick={() =>
                  void saveConfig({
                    ...config,
                    support: {
                      ...support,
                      servantId: null,
                      servantVariantKey: null,
                    },
                  })
                }
                disabled={running}
              >
                清除
              </Button>
            )}
            <Button
              variant="soft"
              color="gray"
              onClick={() => setSupportSettingsOpen(true)}
              disabled={running}
            >
              筛选设置
            </Button>
          </Flex>
          <Text size="1" color="gray">
            NPC 助战优先；没有 NPC 时按这里的支援条件搜索，未指定则取列表首行。
          </Text>
        </Box>

        <Box className="battle-panel">
          <SectionHeading>概念礼装</SectionHeading>
          <Flex gap="2" align="center" wrap="wrap">
            <Button
              variant="soft"
              color="gray"
              onClick={() => setCePickerOpen(true)}
              disabled={running}
            >
              添加礼装
            </Button>
            <Checkbox
              checked={support.craftEssenceMlbRequired}
              disabled={running || support.craftEssenceIds.length === 0}
              onCheckedChange={(checked) =>
                void saveConfig({
                  ...config,
                  support: { ...support, craftEssenceMlbRequired: checked === true },
                })
              }
            />
            <Text size="1" color="gray">要求满破</Text>
          </Flex>
          <Flex gap="2" wrap="wrap">
            {support.craftEssenceIds.map((id) => {
              const ce = craftEssences.find((item) => item.id === id);
              return (
                <Button
                  key={id}
                  size="1"
                  variant="surface"
                  color="gray"
                  disabled={running}
                  onClick={() =>
                    void saveConfig({
                      ...config,
                      support: {
                        ...support,
                        craftEssenceIds: support.craftEssenceIds.filter((item) => item !== id),
                      },
                    })
                  }
                >
                  {ce?.name ?? `礼装 ${id}`} ×
                </Button>
              );
            })}
          </Flex>
        </Box>

        <Box className="battle-panel">
          <SectionHeading>自动战斗方案</SectionHeading>
          <Select.Root
            value={config.autoBattleProfileId ?? "builtin-basic"}
            onValueChange={(value) =>
              void saveConfig({
                ...config,
                autoBattleProfileId: value === "builtin-basic" ? null : value,
              })
            }
            disabled={running}
          >
            <Select.Trigger />
            <Select.Content>
              <Select.Item value="builtin-basic">内置基础方案</Select.Item>
              {profiles
                .filter((profile) => !profile.builtin)
                .map((profile) => (
                  <Select.Item key={profile.id} value={profile.id}>
                    {profile.name}
                  </Select.Item>
                ))}
            </Select.Content>
          </Select.Root>
          <Text size="1" color="gray">
            战斗全程交给自动战斗接管，不读取任何队伍的技能/出卡配置。
          </Text>
        </Box>

        <ApRecoveryPanel
          items={orderedApItems(config)}
          limits={normalizeLimits(config.apRecoveryLimits)}
          disabled={running}
          onChangeItems={(items) =>
            void saveConfig({
              ...config,
              apRecoveryItems: items,
              apRecoveryLimits: normalizeLimits(config.apRecoveryLimits),
            })
          }
          onChangeLimit={(item, limit) =>
            void saveConfig({
              ...config,
              apRecoveryItems: orderedApItems(config),
              apRecoveryLimits: {
                ...normalizeLimits(config.apRecoveryLimits),
                [item]: normalizeApRecoveryLimit(limit),
              },
            })
          }
        />

        <Box className="battle-panel">
          <SectionHeading>任务选项</SectionHeading>
          <Flex direction="column" gap="3">
            <Flex align="center" gap="2">
              <Checkbox
                checked={config.autoFormation}
                disabled={running}
                onCheckedChange={(checked) =>
                  void saveConfig({ ...config, autoFormation: checked === true })
                }
              />
              <Text size="2">特殊关卡优先自动编成；队伍有空位时自动补齐</Text>
            </Flex>
            <Flex align="center" gap="2">
              <Checkbox
                checked={config.allowTurnLimitQuest}
                disabled={running}
                onCheckedChange={(checked) =>
                  void saveConfig({ ...config, allowTurnLimitQuest: checked === true })
                }
              />
              <Text size="2">允许挑战规定回合数任务（默认关闭，遇到会取消并跳过）</Text>
            </Flex>
            <Box>
              <Text size="2" weight="medium">战斗失败处理</Text>
              <OptionCardRadioGroup
                value={config.defeatStrategy}
                disabled={running}
                onValueChange={(value) =>
                  void saveConfig({
                    ...config,
                    defeatStrategy: value as InterludeTaskConfig["defeatStrategy"],
                  })
                }
                options={[
                  { value: "retreat", title: "撤退", description: "跳过该任务，继续下一个" },
                  {
                    value: "commandSpell",
                    title: "使用令咒",
                    description: "令咒不可用时自动改为撤退",
                  },
                ]}
              />
            </Box>
            <Flex gap="4" wrap="wrap">
              <label>
                <Text as="div" size="1" color="gray">连续失败停止次数</Text>
                <TextField.Root
                  type="number"
                  min={1}
                  value={config.maxConsecutiveFailures}
                  disabled={running}
                  onChange={(event) =>
                    void saveConfig({
                      ...config,
                      maxConsecutiveFailures: Math.max(1, event.currentTarget.valueAsNumber || 3),
                    })
                  }
                />
              </label>
              <label>
                <Text as="div" size="1" color="gray">同任务最大重试</Text>
                <TextField.Root
                  type="number"
                  min={1}
                  value={config.maxRepeatAttempts}
                  disabled={running}
                  onChange={(event) =>
                    void saveConfig({
                      ...config,
                      maxRepeatAttempts: Math.max(1, event.currentTarget.valueAsNumber || 10),
                    })
                  }
                />
              </label>
              <label>
                <Text as="div" size="1" color="gray">剧情宽限秒数</Text>
                <TextField.Root
                  type="number"
                  min={5}
                  value={config.dialogueGraceSeconds}
                  disabled={running}
                  onChange={(event) =>
                    void saveConfig({
                      ...config,
                      dialogueGraceSeconds: Math.max(5, event.currentTarget.valueAsNumber || 15),
                    })
                  }
                />
              </label>
            </Flex>
          </Flex>
        </Box>

        {config.mode === "single" && (
          <Box className="battle-panel">
            <Flex align="center" justify="between" gap="3">
              <SectionHeading>选择幕间任务</SectionHeading>
              <Button
                variant="soft"
                color="gray"
                onClick={capturePage}
                disabled={running || capturing}
              >
                <ReloadIcon />
                {capturing ? "正在截图…" : capture ? "重新截图" : "截取游戏画面"}
              </Button>
            </Flex>
            {capture ? (
              <Box className="interlude-capture">
                <img src={convertFileSrc(capture.imagePath)} alt="幕间任务截图" />
                {capture.rows.map((row) => {
                  const selected = row.candidateId === selectedCandidateId;
                  return (
                    <button
                      key={row.candidateId}
                      type="button"
                      style={rowStyle(row)}
                      className={`interlude-row${row.actionable ? " is-actionable" : " is-disabled"}${selected ? " is-selected" : ""}`}
                      disabled={!row.actionable || running}
                      onClick={() => setSelectedCandidateId(row.candidateId)}
                      aria-label={row.actionable ? "选择幕间任务" : "不可点击的幕间任务"}
                    >
                      <span>{row.actionable ? (selected ? "已选择" : "可完成") : "不可点击"}</span>
                    </button>
                  );
                })}
              </Box>
            ) : (
              <Text size="2" color="gray">
                请先在游戏中打开幕间物语列表或场景任务列表，再截取画面。
              </Text>
            )}
          </Box>
        )}

        {error && <Text size="2" color="red">{error}</Text>}
        {running && (
          <Text size="2" color="gray">当前页面：{currentScreen || "正在启动"}</Text>
        )}
      </Flex>

      <Flex justify="end" align="center" className="page-footer" gap="3">
        {running ? (
          <Button color="red" variant="soft" onClick={() => void invoke("stop_automation")}>
            停止
          </Button>
        ) : (
          <Button disabled={!canStart} onClick={handleStart}>
            开始幕间物语
          </Button>
        )}
      </Flex>

      <ServantSelectDialog
        open={servantPickerOpen}
        onOpenChange={setServantPickerOpen}
        servants={servants}
        onSelect={(servant) => {
          void saveConfig({
            ...config,
            support: {
              ...support,
              servantId: servant.id,
              servantVariantKey: servant.variantKey,
            },
          });
          setServantPickerOpen(false);
        }}
      />

      <SupportSettingsDialog
        open={supportSettingsOpen}
        project={supportProject(config)}
        servant={selectedServant}
        portraitSrc={null}
        slotNumber={1}
        craftEssenceGroups={[]}
        onCraftEssenceOpen={() => setCePickerOpen(true)}
        onOpenChange={setSupportSettingsOpen}
        onConfirm={(next) => {
          void saveConfig({
            ...config,
            support: {
              ...support,
              servantLevelMin: next.servantLevel,
              starMapScoreMin: next.starMapScore,
              noblePhantasmLevelMin: next.npLevel,
              skillLevelMins: next.skillLevels,
              appendSkillLevelMins: next.appendSkillLevels,
            },
          });
          setSupportSettingsOpen(false);
        }}
      />

      <CraftEssenceSelectDialog
        open={cePickerOpen}
        onOpenChange={setCePickerOpen}
        craftEssences={craftEssences}
        mlbRequired={support.craftEssenceMlbRequired}
        onMlbRequiredChange={(required) =>
          void saveConfig({
            ...config,
            support: { ...support, craftEssenceMlbRequired: required },
          })
        }
        onSelect={(ce) => {
          if (support.craftEssenceIds.includes(ce.id)) {
            setCePickerOpen(false);
            return;
          }
          void saveConfig({
            ...config,
            support: {
              ...support,
              craftEssenceIds: [...support.craftEssenceIds, ce.id].slice(0, 10),
            },
          });
          setCePickerOpen(false);
        }}
      />
    </Flex>
  );
}
