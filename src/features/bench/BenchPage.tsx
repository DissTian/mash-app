import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Badge,
  Box,
  Button,
  Checkbox,
  Flex,
  Progress,
  Text,
} from "@radix-ui/themes";
import {
  ChevronLeftIcon,
  CopyIcon,
  PlayIcon,
  TrashIcon,
} from "@radix-ui/react-icons";
import { invoke, listen } from "../../tauri";
import {
  BENCH_EVENT_NAME,
  EMPTY_BENCH_STATUS,
  benchSourceLabel,
  formatBenchMs,
  type BenchCaseReport,
  type BenchEvent,
  type BenchPresetInfo,
  type BenchRunSummary,
  type BenchStatus,
} from "../../types/bench";

interface BenchPageProps {
  onBack: () => void;
}

function verdictBadge(report: BenchCaseReport) {
  if (report.skipped) {
    return <Badge color="gray" size="1">跳过</Badge>;
  }
  if (report.passed === true) {
    return <Badge color="green" size="1">达标</Badge>;
  }
  if (report.passed === false) {
    return <Badge color="red" size="1">超预算</Badge>;
  }
  return <Badge color="gray" size="1">—</Badge>;
}

function runTimeLabel(run: BenchRunSummary) {
  const date = new Date(run.startedAtMs);
  const time = Number.isNaN(date.getTime())
    ? "未知时间"
    : `${date.getMonth() + 1}/${date.getDate()} ${date.getHours().toString().padStart(2, "0")}:${date
        .getMinutes()
        .toString()
        .padStart(2, "0")}`;
  return `${run.presetName} · ${time}`;
}

export function BenchPage({ onBack }: BenchPageProps) {
  const [presets, setPresets] = useState<BenchPresetInfo[]>([]);
  const [status, setStatus] = useState<BenchStatus>(EMPTY_BENCH_STATUS);
  const [runs, setRuns] = useState<BenchRunSummary[]>([]);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [liveReports, setLiveReports] = useState<BenchCaseReport[]>([]);
  const [runningPresetId, setRunningPresetId] = useState<string | null>(null);
  const [allowDeviceInput, setAllowDeviceInput] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    (async () => {
      try {
        const [presetList, runList, currentStatus] = await Promise.all([
          invoke<BenchPresetInfo[]>("bench_list_presets"),
          invoke<BenchRunSummary[]>("bench_get_runs"),
          invoke<BenchStatus>("bench_get_status"),
        ]);
        if (disposed) return;
        setPresets(presetList ?? []);
        setRuns(runList ?? []);
        setStatus(currentStatus ?? EMPTY_BENCH_STATUS);
        setSelectedRunId(runList?.[0]?.runId ?? null);
      } catch (value) {
        if (!disposed) setError(String(value));
      }
    })();
    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<BenchEvent>(BENCH_EVENT_NAME, (event) => {
      const payload = event.payload;
      setStatus(payload.status);
      if (payload.error) {
        setError(payload.error);
        setRunningPresetId(null);
      }
      if (payload.report) {
        const report = payload.report;
        setLiveReports((previous) => [...previous, report]);
      }
      if (payload.summary) {
        const summary = payload.summary;
        setLiveReports([]);
        setRuns((previous) => [
          summary,
          ...previous.filter((run) => run.runId !== summary.runId),
        ].slice(0, 10));
        setSelectedRunId(summary.runId);
        setRunningPresetId(null);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const running = status.running;

  const runPreset = useCallback(
    async (presetId: string) => {
      setError(null);
      setNotice(null);
      setLiveReports([]);
      setSelectedRunId(null);
      setRunningPresetId(presetId);
      try {
        await invoke("bench_run_preset", { presetId, allowDeviceInput });
      } catch (value) {
        setRunningPresetId(null);
        setError(String(value));
      }
    },
    [allowDeviceInput],
  );

  const clearRuns = useCallback(async () => {
    setNotice(null);
    try {
      await invoke("bench_clear_runs");
      setRuns([]);
      setSelectedRunId(null);
    } catch (value) {
      setError(String(value));
    }
  }, []);

  const displayedRun = useMemo(() => {
    if (liveReports.length > 0) return null;
    return runs.find((run) => run.runId === selectedRunId) ?? runs[0] ?? null;
  }, [liveReports, runs, selectedRunId]);

  const reports = useMemo(() => {
    if (liveReports.length > 0) return liveReports;
    if (running) return [];
    return displayedRun?.reports ?? [];
  }, [displayedRun, liveReports, running]);
  const percent = status.total > 0 ? Math.min(100, (status.completed / status.total) * 100) : 0;

  const copyRun = useCallback(async () => {
    const payload = displayedRun ?? (reports.length > 0 ? { reports } : null);
    if (!payload) return;
    if (!navigator.clipboard) {
      setNotice("当前环境不支持复制");
      return;
    }
    try {
      await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
      setNotice("已复制 JSON");
    } catch (value) {
      setNotice(`复制失败：${String(value)}`);
    }
  }, [displayedRun, reports]);

  return (
    <Flex direction="column" className="battle-page bench-page">
      <Flex align="center" gap="3" className="battle-header">
        <Button variant="soft" color="gray" onClick={onBack} disabled={running}>
          <ChevronLeftIcon width={16} height={16} />
          <Text size="2">返回</Text>
        </Button>
        <Text size="4" weight="bold">测试</Text>
        <Text size="1" color="gray">预设任务 · 耗时与预算判定</Text>
        {running && (
          <Button
            size="1"
            variant="soft"
            color="red"
            onClick={() => {
              void invoke("bench_cancel").catch((value) => setError(String(value)));
            }}
          >
            停止
          </Button>
        )}
      </Flex>

      <Flex direction="column" gap="4" className="battle-body battle-body-scroll">
        {error && (
          <Box className="bench-notice bench-notice-error">
            <Text size="2" color="red">{error}</Text>
          </Box>
        )}

        <Box className="battle-panel bench-panel">
          <Text size="2" weight="medium">预设任务</Text>
          <Flex direction="column" gap="2" className="bench-preset-list">
            {presets.map((preset) => (
              <Flex key={preset.id} align="center" justify="between" gap="4" className="bench-preset">
                <Flex direction="column" gap="1" className="bench-preset-copy">
                  <Flex align="center" gap="2" wrap="wrap">
                    <Text size="2" weight="medium">{preset.name}</Text>
                    <Text size="1" color="gray">
                      {preset.caseIds.length} 项 · 约 {preset.estimatedSeconds} 秒
                    </Text>
                    {preset.deviceInput && <Badge color="orange" size="1">会点击设备</Badge>}
                  </Flex>
                  <Text size="1" color="gray">{preset.description}</Text>
                </Flex>
                <Button
                  size="2"
                  variant={runningPresetId === preset.id ? "soft" : "solid"}
                  disabled={running}
                  onClick={() => void runPreset(preset.id)}
                >
                  <PlayIcon width={14} height={14} />
                  {runningPresetId === preset.id ? "运行中" : "运行"}
                </Button>
              </Flex>
            ))}
            {presets.length === 0 && (
              <Text size="1" color="gray">暂无预设任务</Text>
            )}
          </Flex>

          {presets.some((preset) => preset.deviceInput) && (
            <label className="bench-allow-input">
              <Checkbox
                checked={allowDeviceInput}
                onCheckedChange={(checked) => setAllowDeviceInput(checked === true)}
                disabled={running}
              />
              <Text size="1" color="gray">允许会真实点击设备的用例（默认关闭）</Text>
            </label>
          )}

          {running && (
            <Flex direction="column" gap="1" className="bench-progress">
              <Progress value={percent} />
              <Text size="1" color="gray">
                正在运行：{status.currentCase ?? "准备中"}（{status.completed}/{status.total}）
              </Text>
            </Flex>
          )}
        </Box>

        <Box className="battle-panel bench-panel">
          <Flex align="center" justify="between" gap="3" wrap="wrap">
            <Flex align="center" gap="2">
              <Text size="2" weight="medium">结果</Text>
              {displayedRun && (
                <Text size="1" color="gray">
                  {displayedRun.deviceLabel} · {displayedRun.server}
                </Text>
              )}
            </Flex>
            <Flex align="center" gap="2">
              <Button size="1" variant="soft" color="gray" onClick={() => void copyRun()} disabled={reports.length === 0}>
                <CopyIcon width={14} height={14} />
                复制 JSON
              </Button>
              <Button size="1" variant="soft" color="gray" onClick={() => void clearRuns()} disabled={runs.length === 0 || running}>
                <TrashIcon width={14} height={14} />
                清空历史
              </Button>
            </Flex>
          </Flex>

          {notice && <Text size="1" color="gray">{notice}</Text>}

          {reports.length === 0 ? (
            <Text size="1" color="gray">尚无结果，点击上方预设的「运行」开始。</Text>
          ) : (
            <Box className="bench-results-scroll">
              <table className="bench-table">
                <thead>
                  <tr>
                    <th>用例</th>
                    <th>数据源</th>
                    <th>样本</th>
                    <th>min</th>
                    <th>中位</th>
                    <th>p95</th>
                    <th>预算</th>
                    <th>判定</th>
                    <th>备注</th>
                  </tr>
                </thead>
                <tbody>
                  {reports.map((report) => (
                    <tr key={report.caseId} className={report.skipped ? "bench-row-skipped" : undefined}>
                      <td>{report.name}</td>
                      <td>{benchSourceLabel(report.source)}</td>
                      <td>{report.samplesMs.length}</td>
                      <td>{formatBenchMs(report.minMs)}</td>
                      <td>{formatBenchMs(report.medianMs)}</td>
                      <td>{formatBenchMs(report.p95Ms)}</td>
                      <td>{report.budgetMs == null ? "—" : formatBenchMs(report.budgetMs)}</td>
                      <td>{verdictBadge(report)}</td>
                      <td className="bench-note">{report.skipped ?? report.note}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Box>
          )}

          {runs.length > 0 && (
            <Flex direction="column" gap="1" className="bench-history">
              <Text size="1" color="gray">历史（最近 {runs.length} 次）</Text>
              <Flex gap="2" wrap="wrap">
                {runs.map((run) => (
                  <Button
                    key={run.runId}
                    size="1"
                    variant={run.runId === selectedRunId && liveReports.length === 0 ? "solid" : "soft"}
                    color="gray"
                    onClick={() => {
                      setLiveReports([]);
                      setSelectedRunId(run.runId);
                    }}
                    disabled={running}
                  >
                    {runTimeLabel(run)}
                  </Button>
                ))}
              </Flex>
            </Flex>
          )}
        </Box>
      </Flex>
    </Flex>
  );
}
