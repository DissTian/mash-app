export interface FlowToolCallDto {
  name: string;
  label: string;
  durationMs: number;
  status: string;
}

export interface FlowStepDto {
  index: number;
  name: string;
  kind: string;
  startedAtMs: number;
  durationMs: number;
  status: string;
  tools: FlowToolCallDto[];
}

export interface FlowTraceSnapshotDto {
  enabled: boolean;
  totalMs: number;
  steps: FlowStepDto[];
  currentStep?: number | null;
}
