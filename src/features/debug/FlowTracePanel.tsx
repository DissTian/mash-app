import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button, Checkbox, Flex, Text } from "@radix-ui/themes";
import type { FlowStepDto, FlowTraceSnapshotDto } from "../../types/flowTrace";

const POLL_INTERVAL_MS = 500;

function formatMs(ms: number): string {
  if (!Number.isFinite(ms) || ms < 0) return "0 ms";
  if (ms >= 10_000) return `${(ms / 1000).toFixed(1)} s`;
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)} s`;
  return `${Math.round(ms)} ms`;
}

/**
 * 流程测试：左侧为强化任务的动作步骤列表，右侧展示选中步骤的说明、
 * 使用的工具与耗时。数据由后端 `flow_trace_*` 命令提供，轮询刷新。
 */
export function FlowTracePanel() {
  const [snapshot, setSnapshot] = useState<FlowTraceSnapshotDto | null>(null);
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const [follow, setFollow] = useState(true);
  const listRef = useRef<HTMLDivElement | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [queueCount, setQueueCount] = useState<number | null>(null);

  const refreshQueue = useCallback(async () => {
    try {
      const queue = await invoke<unknown[]>("get_servant_enhancement_queue");
      setQueueCount(Array.isArray(queue) ? queue.length : null);
    } catch {
      setQueueCount(null);
    }
  }, []);

  const poll = useCallback(async () => {
    try {
      const next = await invoke<FlowTraceSnapshotDto>("flow_trace_snapshot");
      setSnapshot(next);
    } catch (err) {
      setMessage(String(err));
    }
  }, []);

  useEffect(() => {
    void poll();
    void refreshQueue();
    const timer = window.setInterval(() => void poll(), POLL_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [poll, refreshQueue]);

  // 跟随当前执行的步骤，直到用户手动选择某一步
  useEffect(() => {
    if (!follow || !snapshot || snapshot.steps.length === 0) return;
    const index =
      snapshot.currentStep ?? snapshot.steps[snapshot.steps.length - 1].index;
    setSelectedIndex(index);
  }, [follow, snapshot]);

  // 新步骤出现时把列表滚到底，避免手动拖动
  useEffect(() => {
    if (!follow) return;
    const element = listRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [follow, snapshot]);

  const steps = useMemo(() => snapshot?.steps ?? [], [snapshot]);
  const selectedStep: FlowStepDto | null = useMemo(() => {
    if (selectedIndex === null) return null;
    return steps.find((step) => step.index === selectedIndex) ?? null;
  }, [selectedIndex, steps]);
  const nextStep: FlowStepDto | null = useMemo(() => {
    if (!selectedStep) return null;
    return steps.find((step) => step.index === selectedStep.index + 1) ?? null;
  }, [selectedStep, steps]);

  const handleStart = async () => {
    if (queueCount === 0) {
      setMessage(
        "从者强化队列为空：请先在「从者强化」页面添加从者并保存队列，再开始流程测试。"
      );
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      await invoke("flow_trace_start");
      await invoke("start_servant_enhancement_automation");
      setFollow(true);
      await poll();
      await refreshQueue();
    } catch (err) {
      await invoke("flow_trace_stop").catch(() => undefined);
      const raw = String(err);
      setMessage(
        raw.includes("队列为空")
          ? "从者强化队列为空：请先在「从者强化」页面添加从者并保存队列，再开始流程测试。"
          : raw
      );
    } finally {
      setBusy(false);
    }
  };

  const handleStop = async () => {
    setBusy(true);
    try {
      await invoke("stop_servant_enhancement_automation").catch(() => undefined);
      await invoke("flow_trace_stop");
      await poll();
      await refreshQueue();
    } catch (err) {
      setMessage(String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleClear = async () => {
    setBusy(true);
    try {
      await invoke("flow_trace_clear");
      setSelectedIndex(null);
      await poll();
    } catch (err) {
      setMessage(String(err));
    } finally {
      setBusy(false);
    }
  };

  const running = snapshot?.enabled === true;

  return (
    <Flex direction="column" className="flow-trace" gap="2">
      <Flex align="center" gap="2" wrap="wrap" className="flow-trace-controls">
        <Button
          type="button"
          size="1"
          onClick={() => void handleStart()}
          disabled={busy || running}
          title="开始记录并启动从者强化队列（需已配置强化队列且停在主页）"
        >
          开始流程测试
        </Button>
        <Button
          type="button"
          size="1"
          variant="soft"
          color="gray"
          onClick={() => void handleStop()}
          disabled={busy || !running}
        >
          停止
        </Button>
        <Button
          type="button"
          size="1"
          variant="ghost"
          color="gray"
          onClick={() => void handleClear()}
          disabled={busy}
        >
          清空
        </Button>
        <label className="debug-coord-toggle" title="自动选中并滚动到最新步骤">
          <Checkbox
            checked={follow}
            onCheckedChange={(checked) => setFollow(checked === true)}
          />
          <Text size="1">跟随</Text>
        </label>
        <Text size="1" color="gray">
          {running ? "记录中" : snapshot && snapshot.steps.length > 0 ? "已停止" : "就绪"}
        </Text>
        <Text size="1" color="gray">
          总耗时 {formatMs(snapshot?.totalMs ?? 0)} · {steps.length} 步
        </Text>
        {queueCount !== null && (
          <Text size="1" color={queueCount === 0 ? "amber" : "gray"}>
            强化队列：{queueCount === 0 ? "空（请先在从者强化页配置）" : `${queueCount} 个从者`}
          </Text>
        )}
      </Flex>

      {message && (
        <Text size="1" color="red" className="flow-trace-message">
          {message}
        </Text>
      )}

      <Flex gap="3" className="flow-trace-body">
        <div className="flow-trace-list" role="list" ref={listRef}>
          {steps.length === 0 && (
            <Text size="1" color="gray">
              还没有记录。点击「开始流程测试」后在游戏主页启动强化队列，
              这里会实时列出每一步动作和工具耗时。
            </Text>
          )}
          {steps.map((step) => (
            <button
              key={step.index}
              type="button"
              role="listitem"
              className={`flow-step${step.index === selectedStep?.index ? " flow-step-selected" : ""}${
                step.status === "running" ? " flow-step-running" : ""
              }`}
              onClick={() => {
                setFollow(false);
                setSelectedIndex(step.index);
              }}
            >
              <span className="flow-step-name">
                {step.index + 1}. {step.name}
              </span>
              <span className="flow-step-meta">
                <span className={`flow-step-kind flow-step-kind-${step.kind}`}>{step.kind}</span>
                <span className="flow-step-duration">{formatMs(step.durationMs)}</span>
              </span>
            </button>
          ))}
        </div>

        <div className="flow-trace-detail">
          {!selectedStep && (
            <Text size="1" color="gray">
              选择左侧步骤查看该步骤做了什么、用了哪些工具、各工具耗时多少。
            </Text>
          )}
          {selectedStep && (
            <Flex direction="column" gap="2">
              <Flex align="center" gap="2" wrap="wrap">
                <Text size="2" weight="bold">
                  {selectedStep.name}
                </Text>
                <Text
                  size="1"
                  className={`flow-step-kind flow-step-kind-${selectedStep.kind}`}
                >
                  {selectedStep.kind}
                </Text>
                <Text size="1" color="gray">
                  开始于 {formatMs(selectedStep.startedAtMs)} · 耗时{" "}
                  {formatMs(selectedStep.durationMs)} · {selectedStep.status}
                </Text>
              </Flex>
              <Text size="1" color="gray">
                下一步骤：
                {nextStep ? nextStep.name : running ? "（等待下一步）" : "（已结束）"}
              </Text>
              <div className="flow-tool-table">
                <div className="flow-tool-header">
                  <span>工具</span>
                  <span>标签</span>
                  <span>耗时</span>
                </div>
                {selectedStep.tools.length === 0 && (
                  <Text size="1" color="gray" className="flow-tool-empty">
                    该步骤没有调用工具
                  </Text>
                )}
                {selectedStep.tools.map((tool, index) => (
                  <div className="flow-tool-row" key={`${tool.name}-${index}`}>
                    <span className="flow-tool-name">{tool.name}</span>
                    <span className="flow-tool-label">{tool.label}</span>
                    <span className="flow-tool-duration">{formatMs(tool.durationMs)}</span>
                  </div>
                ))}
              </div>
            </Flex>
          )}
        </div>
      </Flex>
    </Flex>
  );
}
