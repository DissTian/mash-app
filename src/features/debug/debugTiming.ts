import { useSyncExternalStore } from "react";
import { featureToggles } from "../../featureToggles";

/**
 * Per-step timing for the CV debug page.
 *
 * Instrumentation only exists for debug builds: `import.meta.env.DEV` is
 * statically replaced at build time (so the measurement branches are dropped
 * from a release bundle) and `featureToggles.cvDebug` keeps it available in a
 * "test feature" package. In a plain release build `run`/`runMeasured` just
 * await the action — no `performance.now()`, no snapshot rebuild, no listeners.
 *
 * The store is module-level (not a hook) so async handlers can time calls
 * without threading callback dependencies through the page.
 */
export function debugTimingEnabled(): boolean {
  return import.meta.env.DEV || featureToggles.cvDebug;
}

export interface DebugTimingEntry {
  label: string;
  ms: number;
}

export interface DebugTimingSnapshot {
  entries: DebugTimingEntry[];
  total: number;
}

const MAX_ENTRIES = 40;

function roundMs(ms: number): number {
  return Math.max(0, Math.round(ms));
}

class DebugTimingStore {
  readonly enabled = debugTimingEnabled();
  private entries: DebugTimingEntry[] = [];
  private snapshot: DebugTimingSnapshot = { entries: [], total: 0 };
  private latestMs = new Map<string, number>();
  private listeners = new Set<() => void>();

  /** Record one step. `replace` keeps a rolling entry (live frame polling). */
  record(label: string, ms: number, options?: { replace?: boolean }): void {
    if (!this.enabled) return;
    const rounded = roundMs(ms);
    this.latestMs.set(label, rounded);
    if (options?.replace) {
      const index = this.entries.findIndex((entry) => entry.label === label);
      if (index >= 0) {
        const next = this.entries.slice();
        next[index] = { label, ms: rounded };
        this.entries = next;
        this.publish();
        return;
      }
    }
    this.entries = [...this.entries, { label, ms: rounded }].slice(-MAX_ENTRIES);
    this.publish();
  }

  /** Run one step, record its duration, return the value unchanged. */
  async run<T>(
    label: string,
    action: () => Promise<T>,
    options?: { replace?: boolean },
  ): Promise<T> {
    if (!this.enabled) return action();
    const started = performance.now();
    try {
      const value = await action();
      this.record(label, performance.now() - started, options);
      return value;
    } catch (error) {
      this.record(`${label}（失败）`, performance.now() - started, options);
      throw error;
    }
  }

  /** Like `run`, but also yields the milliseconds for the caller's own row. */
  async runMeasured<T>(
    label: string,
    action: () => Promise<T>,
  ): Promise<{ value: T; ms: number }> {
    if (!this.enabled) {
      return { value: await action(), ms: 0 };
    }
    const started = performance.now();
    try {
      const value = await action();
      const ms = roundMs(performance.now() - started);
      this.record(label, ms);
      return { value, ms };
    } catch (error) {
      this.record(`${label}（失败）`, performance.now() - started);
      throw error;
    }
  }

  latest(label: string): number | undefined {
    return this.latestMs.get(label);
  }

  clear(): void {
    if (!this.enabled) return;
    this.latestMs.clear();
    this.entries = [];
    this.publish();
  }

  private publish(): void {
    this.snapshot = {
      entries: this.entries,
      total: this.entries.reduce((sum, entry) => sum + entry.ms, 0),
    };
    for (const listener of this.listeners) listener();
  }

  getSnapshot = (): DebugTimingSnapshot => this.snapshot;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };
}

export const debugTiming = new DebugTimingStore();

/** Subscribe the page to the latest timing snapshot (debug builds only). */
export function useDebugTimingSnapshot(): DebugTimingSnapshot {
  return useSyncExternalStore(debugTiming.subscribe, debugTiming.getSnapshot);
}
