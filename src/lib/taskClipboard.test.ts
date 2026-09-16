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
  notes: "会 A301",
};

describe("taskClipboard", () => {
  it("roundtrips a task copy payload", () => {
    const raw = serializeTaskCopy(sample);
    const back = parseTaskCopy(raw);
    expect(back?.title).toBe("x");
    expect(back?.notes).toBe("会 A301");
    expect(parseTaskCopy("not-ours")).toBeNull();
  });

  it("treats a missing notes field as empty", () => {
    const raw = serializeTaskCopy(sample);
    const stripped = raw.replace(/,"notes":"会 A301"/, "");
    expect(parseTaskCopy(stripped)?.notes).toBe("");
  });
});
