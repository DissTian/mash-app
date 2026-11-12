export type ServantEnhancementEntryStatus =
  | "pending"
  | "running"
  | "completed"
  | "skipped"
  | "failed";

export interface ServantEnhancementQueueEntry {
  id: string;
  servantId: number;
  variantKey: string;
  nameCn: string;
  className: string;
  rarity: number;
  targetLevel: number;
  twoExp: boolean;
  faceKey: string | null;
}

export interface ServantEnhancementEntryInput {
  id?: string;
  variantKey: string;
  targetLevel?: number;
  twoExp: boolean;
  faceKey?: string | null;
}

export interface ServantEnhancementEntryState extends ServantEnhancementQueueEntry {
  status: ServantEnhancementEntryStatus;
  currentLevel: number | null;
  maxLevel: number | null;
  message: string;
}

export interface ServantEnhancementAutomationEvent {
  state: string;
  status: "idle" | "starting" | "running" | "finished" | "error";
  currentScreen: string;
  message: string;
  level: string;
  entries: ServantEnhancementEntryState[];
  currentEntryId: string | null;
}

export interface ServantEnhancementStatePayload {
  status: "idle" | "starting" | "running" | "finished" | "error";
  entries: ServantEnhancementEntryState[];
  currentEntryId: string | null;
}

export interface ServantFaceOption {
  key: string;
  path: string;
  stage: number;
}
