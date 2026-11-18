import { useCallback, useEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import {
  Badge,
  Box,
  Button,
  Flex,
  IconButton,
  Select,
  Switch,
  Text,
  TextField,
  Tooltip,
} from "@radix-ui/themes";
import {
  ChevronLeftIcon,
  DragHandleDots2Icon,
  ExclamationTriangleIcon,
  PlusIcon,
  TrashIcon,
} from "@radix-ui/react-icons";
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { convertFileSrc, invoke, listen } from "../../tauri";
import { ServantSelectDialog } from "../team/ServantSelectDialog";
import { moveQueueEntry } from "./queueOrder";
import type { Servant } from "../../types/servant";
import type { Server } from "../../types/server";
import { SERVER_LABELS } from "../../types/server";
import { isAutomationTerminal, type AutomationStatus } from "../../types/automation";
import type {
  ServantEnhancementAutomationEvent,
  ServantEnhancementEntryInput,
  ServantEnhancementEntryState,
  ServantEnhancementEntryStatus,
  ServantEnhancementQueueEntry,
  ServantEnhancementStatePayload,
  ServantFaceOption,
} from "../../types/servantEnhancement";

interface LogEntry {
  time: string;
  message: string;
}

interface EnhancementPageProps {
  servants: Servant[];
  onBack: () => void;
  onAutomationStart?: () => void;
  onLogEntry?: (message: string) => void;
}

const EVENT_NAME = "servant-enhancement-automation-status";
const ALL_FACES = "__all__";

function timestamp(): string {
  const d = new Date();
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}

function naturalMaxForRarity(rarity: number): number {
  if (rarity >= 5) return 90;
  if (rarity === 4) return 80;
  if (rarity === 3) return 70;
  if (rarity === 2) return 65;
  return 60;
}

function displayName(servant: Servant): string {
  return servant.name_cn_server?.trim() || servant.name_cn;
}

function statusLabel(status: ServantEnhancementEntryStatus): string {
  switch (status) {
    case "running":
      return "进行中";
    case "completed":
      return "已完成";
    case "skipped":
      return "已跳过";
    case "failed":
      return "失败";
    default:
      return "等待";
  }
}

function statusColor(
  status: ServantEnhancementEntryStatus,
): "gray" | "blue" | "green" | "amber" | "red" {
  switch (status) {
    case "running":
      return "blue";
    case "completed":
      return "green";
    case "skipped":
      return "amber";
    case "failed":
      return "red";
    default:
      return "gray";
  }
}

function toInput(entry: ServantEnhancementQueueEntry): ServantEnhancementEntryInput {
  return {
    id: entry.id || undefined,
    variantKey: entry.variantKey,
    targetLevel: entry.targetLevel,
    twoExp: entry.twoExp,
    faceKey: entry.faceKey,
    enabled: entry.enabled,
  };
}

interface SortableEnhancementRowProps {
  entry: ServantEnhancementQueueEntry;
  state: ServantEnhancementEntryState | undefined;
  status: ServantEnhancementEntryStatus;
  facePath: string | null;
  faceOptions: ServantFaceOption[];
  editable: boolean;
  draftLevel: string | undefined;
  onDraftLevelChange: (value: string) => void;
  onCommitLevel: () => void;
  onUpdate: (patch: Partial<ServantEnhancementQueueEntry>) => void;
  onRemove: () => void;
}

function SortableEnhancementRow({
  entry,
  state,
  status,
  facePath,
  faceOptions,
  editable,
  draftLevel,
  onDraftLevelChange,
  onCommitLevel,
  onUpdate,
  onRemove,
}: SortableEnhancementRowProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } =
    useSortable({ id: entry.id || entry.variantKey, disabled: !editable });
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  } as CSSProperties;
  const currentLevel = state?.currentLevel;
  const maxLevel = state?.maxLevel;
  return (
    <Flex
      ref={setNodeRef}
      style={style}
      align="center"
      gap="3"
      data-dragging={isDragging}
      className={`servant-enhancement-row ${status === "running" ? "running" : ""}${
        entry.enabled ? "" : " disabled"
      }`}
    >
      <IconButton
        type="button"
        variant="ghost"
        color="gray"
        className="servant-enhancement-drag"
        aria-label={`拖动排序 ${entry.nameCn}`}
        disabled={!editable}
        {...attributes}
        {...listeners}
      >
        <DragHandleDots2Icon />
      </IconButton>

      <Box className="servant-enhancement-face">
        {facePath ? (
          <img src={convertFileSrc(facePath)} alt="" />
        ) : (
          <Text size="1" color="gray">{entry.className.slice(0, 2)}</Text>
        )}
      </Box>

      <Box className="servant-enhancement-info">
        <Flex align="center" gap="2" wrap="wrap">
          <Text size="2" weight="medium">{entry.nameCn}</Text>
          <Badge color={status === "pending" && !entry.enabled ? "gray" : statusColor(status)} variant="soft" size="1">
            {status === "pending" && !entry.enabled ? "已禁用" : statusLabel(status)}
          </Badge>
        </Flex>
        <Text size="1" color="gray">
          {entry.className} / {entry.rarity}★
          {currentLevel != null
            ? ` · 当前 Lv.${currentLevel}${maxLevel != null ? `/${maxLevel}` : ""}`
            : ""}
        </Text>
        {state?.message && (
          <Text size="1" color={status === "failed" ? "red" : "gray"}>
            {state.message}
          </Text>
        )}
      </Box>

      <Box className="servant-enhancement-level">
        <Text size="1" color="gray">目标等级</Text>
        <TextField.Root
          type="number"
          min={1}
          max={120}
          value={draftLevel ?? String(entry.targetLevel)}
          disabled={!editable}
          onChange={(event) => onDraftLevelChange(event.target.value)}
          onBlur={onCommitLevel}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
          }}
        />
      </Box>

      <Flex direction="column" gap="1" className="servant-enhancement-toggle">
        <Text size="1" color="gray">2EXP</Text>
        <Switch
          checked={entry.twoExp}
          disabled={!editable}
          onCheckedChange={(checked) => onUpdate({ twoExp: checked })}
        />
      </Flex>

      <Flex direction="column" gap="1" className="servant-enhancement-toggle">
        <Text size="1" color="gray">启用</Text>
        <Switch
          checked={entry.enabled}
          disabled={!editable}
          aria-label={`启用 ${entry.nameCn}`}
          onCheckedChange={(checked) => onUpdate({ enabled: checked })}
        />
      </Flex>

      <Box className="servant-enhancement-face-select">
        <Text size="1" color="gray">头像</Text>
        <Select.Root
          value={entry.faceKey ?? ALL_FACES}
          disabled={!editable || faceOptions.length === 0}
          onValueChange={(value) =>
            onUpdate({ faceKey: value === ALL_FACES ? null : value })
          }
        >
          <Select.Trigger variant="soft" />
          <Select.Content>
            <Select.Item value={ALL_FACES}>自动（全部）</Select.Item>
            {faceOptions.map((option) => (
              <Select.Item key={option.key} value={option.key}>
                <Flex align="center" gap="2">
                  <img
                    className="servant-enhancement-face-option"
                    src={convertFileSrc(option.path)}
                    alt=""
                  />
                  <Text size="1">第 {option.stage} 再临</Text>
                </Flex>
              </Select.Item>
            ))}
          </Select.Content>
        </Select.Root>
      </Box>

      <Tooltip content={editable ? "移出队列" : "运行中不可删除"}>
        <IconButton
          variant="soft"
          color="red"
          disabled={!editable}
          aria-label={`移出 ${entry.nameCn}`}
          onClick={onRemove}
        >
          <TrashIcon />
        </IconButton>
      </Tooltip>
    </Flex>
  );
}

export function EnhancementPage({
  servants,
  onBack,
  onAutomationStart,
  onLogEntry,
}: EnhancementPageProps) {
  const [queue, setQueue] = useState<ServantEnhancementQueueEntry[]>([]);
  const [live, setLive] = useState<
    Record<string, ServantEnhancementEntryState>
  >({});
  const [completed, setCompleted] = useState<ServantEnhancementEntryState[]>([]);
  const [status, setStatus] = useState<AutomationStatus>("idle");
  const [running, setRunning] = useState(false);
  const [currentScreen, setCurrentScreen] = useState("");
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [server, setServer] = useState<Server | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [draftLevels, setDraftLevels] = useState<Record<string, string>>({});
  const [faces, setFaces] = useState<Record<string, ServantFaceOption[]>>({});
  const logEndRef = useRef<HTMLDivElement>(null);
  const fetchingFaces = useRef(new Set<string>());

  const appendLog = useCallback(
    (message: string) => {
      setLogs((prev) => [...prev.slice(-199), { time: timestamp(), message }]);
    },
    [],
  );

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [queueResponse, statusResponse, serverResponse] = await Promise.all([
          invoke<ServantEnhancementQueueEntry[]>("get_servant_enhancement_queue"),
          invoke<ServantEnhancementStatePayload>(
            "get_servant_enhancement_automation_status",
          ),
          invoke<Server>("get_server"),
        ]);
        if (cancelled) return;
        setQueue(queueResponse ?? []);
        if (serverResponse) setServer(serverResponse);
        if (statusResponse) {
          setStatus(statusResponse.status);
          setRunning(
            statusResponse.status === "starting" || statusResponse.status === "running",
          );
          const map: Record<string, ServantEnhancementEntryState> = {};
          for (const entry of statusResponse.entries ?? []) map[entry.id] = entry;
          setLive(map);
        }
      } catch (err) {
        if (!cancelled) setError(`读取强化队列失败：${String(err)}`);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!running) return;
    const unlisten = listen<ServantEnhancementAutomationEvent>(EVENT_NAME, (event) => {
      const payload = event.payload;
      setStatus(payload.status);
      setCurrentScreen(payload.currentScreen);
      const nextRunning =
        payload.status === "starting" || payload.status === "running";
      setRunning(nextRunning);
      if (payload.message) appendLog(payload.message);
      const map: Record<string, ServantEnhancementEntryState> = {};
      for (const entry of payload.entries ?? []) map[entry.id] = entry;
      setLive(map);
      if (isAutomationTerminal(payload)) {
        setCompleted(
          (payload.entries ?? []).filter((entry) => entry.status === "completed"),
        );
        invoke<ServantEnhancementQueueEntry[]>("get_servant_enhancement_queue")
          .then((next) => {
            if (next) setQueue(next);
          })
          .catch(() => {});
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [appendLog, running]);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  useEffect(() => {
    const missing = queue
      .map((entry) => entry.variantKey)
      .filter(
        (variantKey) =>
          !(variantKey in faces) && !fetchingFaces.current.has(variantKey),
      );
    if (missing.length === 0) return;
    for (const key of missing) fetchingFaces.current.add(key);
    let cancelled = false;
    void Promise.all(
      missing.map((variantKey) =>
        invoke<ServantFaceOption[]>("list_servant_faces", { variantKey })
          .then((options) => [variantKey, options ?? []] as const)
          .catch(() => [variantKey, [] as ServantFaceOption[]] as const),
      ),
    ).then((results) => {
      if (cancelled) return;
      setFaces((prev) => {
        const next = { ...prev };
        for (const [key, options] of results) next[key] = options;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // `faces` is only read to find missing keys; including it would loop.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [queue]);

  const persistQueue = useCallback(
    async (next: ServantEnhancementQueueEntry[]) => {
      const saved = await invoke<ServantEnhancementQueueEntry[]>(
        "save_servant_enhancement_queue",
        { entries: next.map(toInput) },
      );
      if (saved) setQueue(saved);
      return saved ?? next;
    },
    [],
  );

  const updateEntry = useCallback(
    (id: string, patch: Partial<ServantEnhancementQueueEntry>) => {
      const next = queue.map((entry) =>
        entry.id === id ? { ...entry, ...patch } : entry,
      );
      setQueue(next);
      void persistQueue(next).catch((err) => setError(String(err)));
    },
    [persistQueue, queue],
  );

  const handleConfirmServants = useCallback(
    (selected: Servant[]) => {
      const selectedKeys = new Set(selected.map((servant) => servant.variantKey));
      const existingKeys = new Set(queue.map((entry) => entry.variantKey));
      const next = queue.filter((entry) => selectedKeys.has(entry.variantKey));
      for (const servant of selected) {
        if (existingKeys.has(servant.variantKey)) continue;
        next.push({
          id: "",
          servantId: servant.id,
          variantKey: servant.variantKey,
          nameCn: displayName(servant),
          className: servant.class,
          rarity: servant.rarity,
          targetLevel: naturalMaxForRarity(servant.rarity),
          twoExp: false,
          faceKey: null,
          enabled: true,
        });
      }
      void persistQueue(next).catch((err) => setError(String(err)));
    },
    [persistQueue, queue],
  );

  const handleRemove = useCallback(
    (id: string) => {
      const next = queue.filter((entry) => entry.id !== id);
      void persistQueue(next).catch((err) => setError(String(err)));
    },
    [persistQueue, queue],
  );

  const handleStart = useCallback(() => {
    if (running || queue.length === 0 || server !== "CN") return;
    setLogs([]);
    setCompleted([]);
    setLive({});
    setError(null);
    setRunning(true);
    setStatus("starting");
    onAutomationStart?.();
    invoke("start_servant_enhancement_automation").catch((err) => {
      const message = `启动失败: ${String(err)}`;
      appendLog(message);
      onLogEntry?.(message);
      setRunning(false);
      setStatus("idle");
    });
  }, [appendLog, onAutomationStart, onLogEntry, queue.length, running, server]);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, {
      coordinateGetter: sortableKeyboardCoordinates,
    }),
  );

  const handleReorder = useCallback(
    (activeId: string, overId: string) => {
      const next = moveQueueEntry(queue, activeId, overId);
      if (next === queue) return;
      setQueue(next);
      void persistQueue(next).catch((err) => setError(String(err)));
    },
    [persistQueue, queue],
  );

  const handleDragEnd = useCallback(
    (event: DragEndEvent) => {
      const { active, over } = event;
      if (!over || active.id === over.id) return;
      handleReorder(String(active.id), String(over.id));
    },
    [handleReorder],
  );

  const handleStop = useCallback(() => {
    invoke("stop_servant_enhancement_automation").catch((err) =>
      setError(String(err)),
    );
  }, []);

  const commitLevel = useCallback(
    (id: string) => {
      const entry = queue.find((candidate) => candidate.id === id);
      if (!entry) return;
      const raw = draftLevels[id];
      if (raw === undefined) return;
      const parsed = Number(raw);
      const nextLevel = Number.isFinite(parsed)
        ? Math.max(1, Math.min(120, Math.round(parsed)))
        : entry.targetLevel;
      setDraftLevels((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      if (nextLevel !== entry.targetLevel) {
        updateEntry(id, { targetLevel: nextLevel });
      }
    },
    [draftLevels, queue, updateEntry],
  );

  const faceOptions = useCallback(
    (variantKey: string) => faces[variantKey] ?? [],
    [faces],
  );

  const faceFor = useCallback(
    (entry: ServantEnhancementQueueEntry): ServantFaceOption | null => {
      const options = faceOptions(entry.variantKey);
      if (entry.faceKey) {
        return options.find((option) => option.key === entry.faceKey) ?? null;
      }
      return options[0] ?? null;
    },
    [faceOptions],
  );

  return (
    <Flex direction="column" className="battle-page servant-enhancement-page">
      <Flex align="center" gap="3" className="servant-enhancement-header">
        <Button variant="soft" color="gray" onClick={onBack}>
          <ChevronLeftIcon width={16} height={16} />
          <Text size="2">返回</Text>
        </Button>
        <Text size="4" weight="bold">
          从者强化
        </Text>
        <Badge color="gray" variant="soft">
          国服
        </Badge>
      </Flex>

      <Flex direction="column" gap="4" className="battle-body">
        {server && server !== "CN" && (
          <Flex align="center" gap="2" className="servant-enhancement-warning" role="alert">
            <ExclamationTriangleIcon />
            <Text size="2">
              当前为{SERVER_LABELS[server]}客户端，从者强化仅支持国服，无法开始。
            </Text>
          </Flex>
        )}

        <Box className="servant-enhancement-panel">
          <Flex align="center" justify="between" gap="3">
            <Flex align="center" gap="2">
              <Text size="2" weight="medium">强化队列</Text>
              <Text size="1" color="gray">{queue.length} 位从者</Text>
              {running && <Badge color="blue" variant="soft">运行中，暂不可编辑</Badge>}
            </Flex>
            <Button
              size="1"
              variant="soft"
              onClick={() => setDialogOpen(true)}
              disabled={running}
            >
              <PlusIcon /> 添加从者
            </Button>
          </Flex>

          {queue.length === 0 ? (
            <Flex align="center" justify="center" py="6">
              <Text size="2" color="gray">
                {loading ? "读取队列…" : "尚未添加从者，点击右上角添加。"}
              </Text>
            </Flex>
          ) : (
            <DndContext
              sensors={sensors}
              collisionDetection={closestCenter}
              onDragEnd={handleDragEnd}
            >
              <SortableContext
                items={queue.map((entry) => entry.id || entry.variantKey)}
                strategy={verticalListSortingStrategy}
              >
                <Flex direction="column" gap="2" mt="3">
                  {queue.map((entry) => (
                    <SortableEnhancementRow
                      key={entry.id || entry.variantKey}
                      entry={entry}
                      state={live[entry.id]}
                      status={live[entry.id]?.status ?? "pending"}
                      facePath={faceFor(entry)?.path ?? null}
                      faceOptions={faceOptions(entry.variantKey)}
                      editable={!running}
                      draftLevel={draftLevels[entry.id]}
                      onDraftLevelChange={(value) =>
                        setDraftLevels((prev) => ({ ...prev, [entry.id]: value }))
                      }
                      onCommitLevel={() => commitLevel(entry.id)}
                      onUpdate={(patch) => updateEntry(entry.id, patch)}
                      onRemove={() => handleRemove(entry.id)}
                    />
                  ))}
                </Flex>
              </SortableContext>
            </DndContext>
          )}
        </Box>

        {completed.length > 0 && (
          <Box className="servant-enhancement-completed">
            <Text size="2" weight="medium">本次完成</Text>
            <Flex gap="2" wrap="wrap" mt="2">
              {completed.map((entry) => (
                <Badge key={entry.id} color="green" variant="soft">
                  {entry.nameCn} · Lv.{entry.currentLevel ?? entry.targetLevel}
                </Badge>
              ))}
            </Flex>
          </Box>
        )}

        {error && <Text size="2" color="red">{error}</Text>}

        <Flex gap="3" className="servant-enhancement-controls">
          <Button
            disabled={running || queue.length === 0 || server !== "CN"}
            onClick={handleStart}
          >
            开始
          </Button>
          <Button color="red" variant="soft" disabled={!running} onClick={handleStop}>
            停止
          </Button>
          <Text size="1" color="gray">
            {status === "idle" ? "等待启动" : `状态：${status}`}
            {currentScreen ? ` · ${currentScreen}` : ""}
          </Text>
        </Flex>

        <Box className="battle-log-container">
          <Text size="2" weight="medium" style={{ marginBottom: 6, display: "block" }}>
            运行日志
          </Text>
          <Box className="battle-log">
            {logs.length === 0 && (
              <Text size="1" className="battle-log-placeholder">
                等待启动…
              </Text>
            )}
            {logs.map((entry, index) => (
              <div key={index} className="battle-log-entry">
                <span className="battle-log-time">{entry.time}</span>
                <span className="battle-log-msg">{entry.message}</span>
              </div>
            ))}
            <div ref={logEndRef} />
          </Box>
        </Box>
      </Flex>

      <ServantSelectDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        mode="multiple"
        servants={servants}
        selectedVariantKeys={queue.map((entry) => entry.variantKey)}
        onConfirm={handleConfirmServants}
      />
    </Flex>
  );
}
