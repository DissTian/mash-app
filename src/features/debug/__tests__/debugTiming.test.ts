import { beforeEach, describe, expect, it } from "vitest";
import { debugTiming } from "../debugTiming";

describe("debugTiming", () => {
  beforeEach(() => {
    debugTiming.clear();
  });

  it("records each step and reports the total", async () => {
    await debugTiming.run("截取画面", async () => "ok");
    await debugTiming.run("识别指令卡", async () => "ok");

    const snapshot = debugTiming.getSnapshot();
    expect(snapshot.entries.map((entry) => entry.label)).toEqual([
      "截取画面",
      "识别指令卡",
    ]);
    expect(snapshot.total).toBe(
      snapshot.entries.reduce((sum, entry) => sum + entry.ms, 0),
    );
    expect(debugTiming.latest("识别指令卡")).toBe(
      snapshot.entries[1].ms,
    );
  });

  it("keeps one rolling entry for polling-style steps", async () => {
    await debugTiming.run("动态取帧", async () => "ok", { replace: true });
    await debugTiming.run("动态取帧", async () => "ok", { replace: true });

    const entries = debugTiming.getSnapshot().entries;
    expect(entries.filter((entry) => entry.label === "动态取帧")).toHaveLength(1);
  });

  it("returns the measured milliseconds to the caller", async () => {
    const { value, ms } = await debugTiming.runMeasured("原始模板查找", async () => 42);
    expect(value).toBe(42);
    expect(ms).toBeGreaterThanOrEqual(0);
    expect(debugTiming.latest("原始模板查找")).toBe(ms);
  });

  it("marks failed steps and rethrows", async () => {
    await expect(
      debugTiming.run("识别助战", async () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");

    const labels = debugTiming.getSnapshot().entries.map((entry) => entry.label);
    expect(labels).toContain("识别助战（失败）");
  });

  it("clears every recorded step", async () => {
    await debugTiming.run("截取画面", async () => "ok");
    debugTiming.clear();
    expect(debugTiming.getSnapshot().entries).toHaveLength(0);
    expect(debugTiming.getSnapshot().total).toBe(0);
    expect(debugTiming.latest("截取画面")).toBeUndefined();
  });
});
