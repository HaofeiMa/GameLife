import { describe, expect, it } from "vitest";
import {
  LIST_DEFAULT,
  LIST_MAX,
  LIST_MIN,
  TASK_SPLIT_KEY,
  clampListWidth,
  readStoredListWidth,
  writeStoredListWidth,
} from "./taskSplit";

function memoryStorage(initial: Record<string, string> = {}) {
  const store = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => {
      store.set(k, v);
    },
  };
}

describe("clampListWidth", () => {
  it("clamps to the list pane bounds and defaults illegal values", () => {
    expect(clampListWidth(0)).toBe(LIST_MIN);
    expect(clampListWidth(9_999)).toBe(LIST_MAX);
    expect(clampListWidth(LIST_DEFAULT)).toBe(LIST_DEFAULT);
    expect(clampListWidth(Number.NaN)).toBe(LIST_DEFAULT);
  });

  it("remembers a dragged list width", () => {
    const storage = memoryStorage();
    expect(readStoredListWidth(storage)).toBe(LIST_DEFAULT);
    writeStoredListWidth(storage, 400);
    expect(readStoredListWidth(storage)).toBe(400);
  });

  it("falls back when storage holds garbage", () => {
    const storage = memoryStorage({ [TASK_SPLIT_KEY]: "nope" });
    expect(readStoredListWidth(storage)).toBe(LIST_DEFAULT);
  });
});
