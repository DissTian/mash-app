import { describe, expect, it } from "vitest";
import { moveQueueEntry } from "../queueOrder";

const entries = [{ id: "a" }, { id: "b" }, { id: "c" }];

describe("moveQueueEntry", () => {
  it("moves the dragged entry to the drop position", () => {
    expect(moveQueueEntry(entries, "a", "c").map((entry) => entry.id)).toEqual([
      "b",
      "c",
      "a",
    ]);
    expect(moveQueueEntry(entries, "c", "a").map((entry) => entry.id)).toEqual([
      "c",
      "a",
      "b",
    ]);
  });

  it("keeps the original array for no-op or unknown ids", () => {
    expect(moveQueueEntry(entries, "a", "a")).toBe(entries);
    expect(moveQueueEntry(entries, "a", "missing")).toBe(entries);
    expect(moveQueueEntry(entries, "missing", "a")).toBe(entries);
  });
});
