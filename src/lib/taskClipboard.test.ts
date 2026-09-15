import { describe, expect, it } from "vitest";
import { parseTaskCopy, serializeTaskCopy } from "./taskClipboard";
import type { TaskView } from "./api";

const sample: TaskView = {
  id: "a",
  listId: "list-mainline",
  title: "x",
  done: false,
  start: 0,
  end: 1800,
  range: null,
  sort: 0,
  repeat: "none",
  remindOffsets: [],
};

describe("taskClipboard", () => {
  it("roundtrips a task copy payload", () => {
    const raw = serializeTaskCopy(sample);
    const back = parseTaskCopy(raw);
    expect(back?.title).toBe("x");
    expect(parseTaskCopy("not-ours")).toBeNull();
  });
});
