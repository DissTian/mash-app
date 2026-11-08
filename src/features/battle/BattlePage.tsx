import { PageHeader } from "../../components/common/PageHeader";
import { TaskActionButton } from "../../components/common/TaskActionButton";
import { StopAfterRoundButton } from "../../components/common/StopAfterRoundButton";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertDialog,
  Box,
  Button,
  Flex,
  Text,
  TextField,
} from "@radix-ui/themes";
import { invoke, listen } from "../../tauri";
import {
  MinusIcon,
  PlusIcon,
} from "@radix-ui/react-icons";
import { ProjectBar } from "../projects/ProjectBar";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { SectionHeading } from "../../components/common/SectionHeading";
import type {
  BattleApRecoveryItem,
  BattleApRecoveryLimits,
  BattleRepeatMode,
  GrandClass,
  GrandClassDefinition,
  Project,
  ProjectCatalog,
} from "../../types/project";
import type { Servant } from "../../types/servant";
import {
  grandClassDefinition,
  validateGrandServants as grandServantsAreValid,
} from "../advanced/grandClassModel";
import { isAutomationTerminal, type AutomationStatus } from "../../types/automation";
import type { BattleRunStatus } from "../../types/battleRunStatus";
import { buildProjectRunConfig } from "./projectRunConfig";
import { ApRecoveryPanel } from "./ApRecoveryPanel";
import {
  AP_RECOVERY_OPTIONS,
  EMPTY_AP_RECOVERY_ITEMS,
  UNLIMITED_AP_RECOVERY_LIMITS,
  normalizeApRecoveryLimit,
  orderedApRecoveryItems,
  projectApRecoveryLimits,
} from "./apRecoveryModel";

interface AutomationEvent {
  state: string;
  status: AutomationStatus;
  currentScreen: string;
  message: string;
}

interface BattlePageProps {
  projects: Project[];
  projectCatalog: ProjectCatalog;
  grandClassDefinitions: GrandClassDefinition[];
  servants: Servant[];
  activeProjectId: string | null;
  onProjectSelect: (id: string) => void;
  onCreateProject: (
    name: string,
    advancedMode?: boolean,
    grandClass?: GrandClass,
    groupId?: string | null,
  ) => void;
  onRenameProject: (id: string, name: string) => void;
  onDuplicateProject: (id: string, name: string) => void;
  onDeleteProject: (id: string) => void;
  onCreateProjectGroup: (name: string) => Promise<void>;
  onRenameProjectGroup: (groupId: string, name: string) => Promise<void>;
  onDeleteProjectGroup: (groupId: string) => Promise<void>;
  onMoveProjectToGroup: (projectId: string, groupId: string | null) => Promise<void>;
  onReorderProjectGroups: (groupIds: string[]) => Promise<void>;
  onReorderProjectsInGroup: (groupId: string | null, projectIds: string[]) => Promise<void>;
  onOpenProjectSettings: () => void;
  beforeShare?: (projectId: string) => Promise<void>;
  onFormationImported?: (project: Project) => void;
  onUpdateProject: (project: Project) => Promise<void>;
  onBack: () => void;
  onOpenHome?: () => void;
  onAutomationStart?: (maxRuns: number | null) => void;
  onAutomationStartFailed?: () => void;
  onLogEntry?: (message: string) => void;
  battleRunStatus?: BattleRunStatus | null;
}

interface BattleProjectDraft {
  projectId: string;
  repeatMode: BattleRepeatMode;
  repeatCount: number | null;
  apRecoveryItems: BattleApRecoveryItem[];
  apRecoveryLimits: BattleApRecoveryLimits;
}

function projectRepeatMode(project: Project | null | undefined): BattleRepeatMode {
  if (project?.repeatMode === "infinite" || project?.repeatMode === "count") {
    return project.repeatMode;
  }
  if (project?.repeatMission) {
    return "infinite";
  }
  return "single";
}

function projectRepeatCount(project: Project | null | undefined): number | null {
  const count = project?.repeatCount;
  return typeof count === "number" && Number.isInteger(count) && count > 0 ? count : null;
}

function projectDraftFromProject(project: Project): BattleProjectDraft {
  return {
    projectId: project.id,
    repeatMode: projectRepeatMode(project),
    repeatCount: projectRepeatCount(project),
    apRecoveryItems: orderedApRecoveryItems(project),
    apRecoveryLimits: projectApRecoveryLimits(project),
  };
}

export function BattlePage({
  projects,
  projectCatalog,
  grandClassDefinitions,
  activeProjectId,
  onProjectSelect,
  onCreateProject,
  onRenameProject,
  onDuplicateProject,
  onDeleteProject,
  onCreateProjectGroup,
  onRenameProjectGroup,
  onDeleteProjectGroup,
  onMoveProjectToGroup,
  onReorderProjectGroups,
  onReorderProjectsInGroup,
  onOpenProjectSettings,
  beforeShare,
  onFormationImported,
  onUpdateProject,
  onBack,
  onOpenHome,
  onAutomationStart,
  onAutomationStartFailed,
  onLogEntry,
  battleRunStatus = null,
}: BattlePageProps) {
  const [running, setRunning] = useState(false);
  const [rainbowConfirmOpen, setRainbowConfirmOpen] = useState(false);
  const [stopAfterCurrentRequested, setStopAfterCurrentRequested] = useState(false);
  const [draft, setDraft] = useState<BattleProjectDraft | null>(null);
  const [startError, setStartError] = useState<string | null>(null);
  const draftRef = useRef<BattleProjectDraft | null>(null);

  useEffect(() => {
    const unlisten = listen<AutomationEvent>("automation-status", (event) => {
      if (isAutomationTerminal(event.payload)) {
        setRunning(false);
        setStopAfterCurrentRequested(false);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const selectedProject = projects.find((project) => project.id === activeProjectId) ?? null;
  const projectDraft = useMemo(
    () => (selectedProject ? projectDraftFromProject(selectedProject) : null),
    [selectedProject]
  );
  const activeDraft =
    selectedProject && draft?.projectId === selectedProject.id
      ? draft
      : projectDraft;
  const repeatMode = activeDraft?.repeatMode ?? "single";
  const repeatCount = activeDraft?.repeatCount ?? null;
  const apRecoveryItems = activeDraft?.apRecoveryItems ?? EMPTY_AP_RECOVERY_ITEMS;
  const apRecoveryLimits =
    activeDraft?.apRecoveryLimits ?? UNLIMITED_AP_RECOVERY_LIMITS;

  const saveProject = useCallback(
    (nextProject: Project) => {
      onUpdateProject(nextProject).catch(console.error);
    },
    [onUpdateProject]
  );

  const validateGrandServants = useCallback(() => {
    if (selectedProject?.advancedMode !== true) return true;
    const definition = grandClassDefinition(grandClassDefinitions, selectedProject.grandClass);
    if (!definition || !grandServantsAreValid(selectedProject.grandServants ?? [], definition)) {
      setStartError(definition?.validationMessage ?? "无法读取冠位职阶定义");
      return false;
    }
    return true;
  }, [grandClassDefinitions, selectedProject]);

  const updateSelectedProject = useCallback(
    (updater: (project: Project) => Project) => {
      if (!selectedProject) return;
      const nextProject = updater(selectedProject);
      const nextDraft = projectDraftFromProject(nextProject);
      draftRef.current = nextDraft;
      setDraft(nextDraft);
      saveProject(nextProject);
    },
    [saveProject, selectedProject]
  );

  const startAutomation = useCallback(() => {
    if (!selectedProject) return;
    if (!validateGrandServants()) return;
    const latestDraft =
      draftRef.current?.projectId === selectedProject.id
        ? draftRef.current
        : projectDraftFromProject(selectedProject);
    const maxMissionRuns =
      latestDraft.repeatMode === "count" ? latestDraft.repeatCount : null;
    setRunning(true);
    setStopAfterCurrentRequested(false);
    onAutomationStart?.(maxMissionRuns);

    const config = buildProjectRunConfig(selectedProject, {
      repeatMission: latestDraft.repeatMode === "infinite",
      maxMissionRuns,
      apRecoveryItems: latestDraft.apRecoveryItems,
      apRecoveryLimits: latestDraft.apRecoveryLimits,
    });

    invoke("start_automation", { config }).catch((err) => {
      onLogEntry?.(`启动失败: ${String(err)}`);
      onAutomationStartFailed?.();
      setRunning(false);
      setStopAfterCurrentRequested(false);
    });
  }, [
    onAutomationStart,
    onAutomationStartFailed,
    onLogEntry,
    selectedProject,
    validateGrandServants,
  ]);

  const handleStart = useCallback(() => {
    if (!selectedProject) return;
    if (!validateGrandServants()) return;
    if (apRecoveryItems.includes("rainbow")) {
      setRainbowConfirmOpen(true);
      return;
    }
    startAutomation();
  }, [apRecoveryItems, selectedProject, startAutomation, validateGrandServants]);

  const handleStop = useCallback(() => {
    invoke("stop_automation")
      .then(() => {
        setRunning(false);
        setStopAfterCurrentRequested(false);
        onLogEntry?.("已请求停止自动化");
      })
      .catch(console.error);
  }, [onLogEntry]);

  const handleStopAfterCurrent = useCallback(() => {
    invoke("stop_automation_after_current")
      .then(() => {
        setStopAfterCurrentRequested(true);
        onLogEntry?.("已设置：运行完当前轮次后停止");
      })
      .catch(console.error);
  }, [onLogEntry]);

  const handleRepeatModeChange = useCallback(
    (nextMode: BattleRepeatMode) => {
      if (!selectedProject || running) return;
      if (nextMode === "count") {
        updateSelectedProject((project) => ({
          ...project,
          repeatMission: true,
          repeatMode: "count",
          repeatCount: repeatCount ?? 1,
        }));
        return;
      }
      updateSelectedProject((project) => ({
        ...project,
        repeatMission: nextMode !== "single",
        repeatMode: nextMode,
        repeatCount: null,
      }));
    },
    [repeatCount, running, selectedProject, updateSelectedProject]
  );

  const setRepeatCountValue = useCallback(
    (nextCount: number) => {
      if (!selectedProject || running) return;
      const normalized = Math.max(1, Math.floor(nextCount));
      updateSelectedProject((project) => ({
        ...project,
        repeatMission: true,
        repeatMode: "count",
        repeatCount: normalized,
      }));
    },
    [running, selectedProject, updateSelectedProject]
  );

  const handleRepeatCountStep = useCallback(
    (delta: number) => {
      setRepeatCountValue((repeatCount ?? 1) + delta);
    },
    [repeatCount, setRepeatCountValue]
  );

  const handleRepeatCountInput = useCallback(
    (value: string) => {
      if (!value.trim()) return;
      const parsed = Number(value);
      if (!Number.isInteger(parsed) || parsed <= 0) return;
      setRepeatCountValue(parsed);
    },
    [setRepeatCountValue]
  );

  const enableRepeatCount = useCallback(() => {
    if (!selectedProject || running) return;
    updateSelectedProject((project) => ({
      ...project,
      repeatMission: true,
      repeatMode: "count",
      repeatCount: repeatCount ?? 1,
    }));
  }, [repeatCount, running, selectedProject, updateSelectedProject]);

  const setApRecoveryItems = useCallback(
    (items: BattleApRecoveryItem[]) => {
      updateSelectedProject((project) => ({
        ...project,
        apRecoveryItems: AP_RECOVERY_OPTIONS.map((option) => option.value).filter((value) =>
          items.includes(value)
        ),
        apRecoveryLimits,
      }));
    },
    [apRecoveryLimits, updateSelectedProject]
  );

  const setApRecoveryLimit = useCallback(
    (item: BattleApRecoveryItem, limit: number | null) => {
      if (!selectedProject || running) return;
      const normalized = normalizeApRecoveryLimit(limit);
      updateSelectedProject((project) => ({
        ...project,
        apRecoveryItems,
        apRecoveryLimits: {
          ...apRecoveryLimits,
          [item]: normalized,
        },
      }));
    },
    [apRecoveryItems, apRecoveryLimits, running, selectedProject, updateSelectedProject]
  );

  const displayedRepeatCount = repeatCount ?? 1;
  const runProgressText =
    running && repeatMode === "count"
      ? `${battleRunStatus?.completedRuns ?? 0}/${battleRunStatus?.maxRuns ?? displayedRepeatCount}`
      : null;

  return (
    <Flex direction="column" className="battle-page workspace-page">
      <Box className="battle-topbar">
        <PageHeader title="运行设置" english="RUN SETTINGS" onBack={onBack} backDisabled={running}><ProjectBar headerStyle
          beforeShare={beforeShare}
          onFormationImported={onFormationImported}
          projects={projects}
          projectCatalog={projectCatalog}
          grandClassDefinitions={grandClassDefinitions}
          activeProjectId={activeProjectId}
          disabled={running}
          onProjectSelect={onProjectSelect}
          onCreateProject={onCreateProject}
          onRenameProject={onRenameProject}
          onDuplicateProject={onDuplicateProject}
          onDeleteProject={onDeleteProject}
          onCreateProjectGroup={onCreateProjectGroup}
          onRenameProjectGroup={onRenameProjectGroup}
          onDeleteProjectGroup={onDeleteProjectGroup}
          onMoveProjectToGroup={onMoveProjectToGroup}
          onReorderProjectGroups={onReorderProjectGroups}
          onReorderProjectsInGroup={onReorderProjectsInGroup}
          onOpenProjectSettings={onOpenProjectSettings}
          onOpenHome={onOpenHome}
        /></PageHeader>
      </Box>

      <Flex direction="column" className="battle-body battle-body-scroll">
        <Box className="battle-panel">
          <SectionHeading rail stacked english="REPEAT">重复任务</SectionHeading>
          <OptionCardRadioGroup
            indicator="diamond"
            value={repeatMode}
            className="battle-repeat-cards"
            disabled={running || !selectedProject}
            onValueChange={(value) => handleRepeatModeChange(value as BattleRepeatMode)}
            options={[
              {
                value: "single",
                title: "不重复",
                description: "任务执行完毕后停止",
              },
              {
                value: "infinite",
                title: "无限",
                description: "无限重复执行任务",
                accessory: <span className="battle-repeat-symbol">∞</span>,
              },
              {
                value: "count",
                title: "设置次数",
                description: "按指定次数执行任务",
                className: "battle-repeat-card-count",
                onSelect: enableRepeatCount,
                accessory: (
                  <div className="battle-repeat-counter" aria-label="重复次数">
                    <Button
                      color="indigo"
                      disabled={running || !selectedProject || displayedRepeatCount <= 1}
                      aria-label="减少重复次数"
                      onClick={(event) => {
                        event.stopPropagation();
                        handleRepeatCountStep(-1);
                      }}
                    >
                      <MinusIcon width={15} height={15} />
                    </Button>
                    {runProgressText != null ? (
                      <Text
                        className="battle-counter-progress"
                        aria-label={`运行进度 ${runProgressText}`}
                      >
                        {runProgressText}
                      </Text>
                    ) : (
                      <TextField.Root
                        className="battle-counter-value"
                        type="number"
                        min="1"
                        step="1"
                        variant="soft"
                        radius="none"
                        inputMode="numeric"
                        aria-label="重复次数"
                        value={displayedRepeatCount}
                        disabled={running || !selectedProject}
                        onClick={(event) => event.stopPropagation()}
                        onChange={(event) => handleRepeatCountInput(event.target.value)}
                      />
                    )}
                    <Button
                      disabled={running || !selectedProject}
                      color="indigo"
                      aria-label="增加重复次数"
                      onClick={(event) => {
                        event.stopPropagation();
                        handleRepeatCountStep(1);
                      }}
                    >
                      <PlusIcon width={15} height={15} />
                    </Button>
                  </div>
                ),
              },
            ]}
          />
        </Box>

        <ApRecoveryPanel
          items={apRecoveryItems}
          limits={apRecoveryLimits}
          disabled={running || !selectedProject}
          onChangeItems={setApRecoveryItems}
          onChangeLimit={setApRecoveryLimit}
        />
      </Flex>

      <Flex justify="end" align="center" className="battle-footer" gap="3">
        <Flex align="center" justify="end" className="battle-task-actions">
          {running && (
            <>
              <StopAfterRoundButton selected={stopAfterCurrentRequested} onClick={handleStopAfterCurrent} />
              <span className="battle-task-action-divider" aria-hidden="true" />
            </>
          )}
          <TaskActionButton running={running} disabled={!selectedProject} onClick={running ? handleStop : handleStart} />
        </Flex>
      </Flex>

      <AlertDialog.Root open={rainbowConfirmOpen} onOpenChange={setRainbowConfirmOpen}>
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>确认开始任务</AlertDialog.Title>
          <AlertDialog.Description size="2">
            当前已勾选圣晶石。开始后若行动力不足，自动化可能会消耗圣晶石补充行动力。确认继续开始吗？
          </AlertDialog.Description>
          <Flex justify="end" gap="3" mt="4">
            <AlertDialog.Cancel>
              <Button type="button" variant="soft" color="gray">
                取消
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button
                type="button"
                color="red"
                onClick={() => {
                  setRainbowConfirmOpen(false);
                  startAutomation();
                }}
              >
                确认开始
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>

      <AlertDialog.Root open={startError != null} onOpenChange={(open) => {
        if (!open) setStartError(null);
      }}>
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>无法开始战斗</AlertDialog.Title>
          <AlertDialog.Description size="2">
            {startError}
          </AlertDialog.Description>
          <Flex justify="end" gap="3" mt="4">
            <AlertDialog.Action>
              <Button type="button" onClick={() => setStartError(null)}>
                知道了
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Flex>
  );
}
