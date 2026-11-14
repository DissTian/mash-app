export type BenchCategory = "device" | "capture" | "ocr" | "template";

export type BenchSource = "device" | "liveFrame" | "lastCapture";

export interface BenchCaseInfo {
  id: string;
  category: BenchCategory;
  name: string;
  description: string;
  source: BenchSource;
  deviceInput: boolean;
  iterations: number;
  budgetMs: number | null;
}

export interface BenchPresetInfo {
  id: string;
  name: string;
  description: string;
  caseIds: string[];
  deviceInput: boolean;
  estimatedSeconds: number;
}

export interface BenchCaseReport {
  caseId: string;
  name: string;
  source: BenchSource;
  samplesMs: number[];
  minMs: number;
  medianMs: number;
  p95Ms: number;
  budgetMs: number | null;
  passed: boolean | null;
  skipped: string | null;
  note: string;
}

export interface BenchRunSummary {
  runId: string;
  presetId: string | null;
  presetName: string;
  startedAtMs: number;
  finishedAtMs: number;
  deviceLabel: string;
  server: string;
  reports: BenchCaseReport[];
}

export interface BenchStatus {
  running: boolean;
  currentCase: string | null;
  completed: number;
  total: number;
}

export interface BenchEvent {
  status: BenchStatus;
  report: BenchCaseReport | null;
  summary: BenchRunSummary | null;
  error: string | null;
}

export const BENCH_EVENT_NAME = "bench-run-event";

export const EMPTY_BENCH_STATUS: BenchStatus = {
  running: false,
  currentCase: null,
  completed: 0,
  total: 0,
};

export function benchSourceLabel(source: BenchSource): string {
  switch (source) {
    case "device":
      return "设备";
    case "liveFrame":
      return "实时帧";
    case "lastCapture":
      return "上一张截图";
  }
}

export function formatBenchMs(value: number): string {
  if (!Number.isFinite(value) || value <= 0) {
    return "—";
  }
  return value >= 1000 ? `${(value / 1000).toFixed(2)} s` : `${Math.round(value)} ms`;
}
