import { describe, expect, it } from "vitest";
import type { TaskView } from "./api";
import { sortTasks } from "./taskSort";

function task(partial: Partial<TaskView> & Pick<TaskView, "id" | "title">): TaskView {
  return {
    listId: "l",
    done: false,
    start: null,
    end: null,
    range: null,
    sort: 0,
    repeat: "none",
    remindOffsets: [],
    notes: "",
    ...partial,
  };
}

describe("sortTasks", () => {
  it("puts unscheduled last when sorting by time", () => {
    const tasks = [
      task({ id: "b", title: "乙", start: null, end: null }),
      task({ id: "a", title: "甲", start: 100, end: 200 }),
    ];
    expect(sortTasks(tasks, "time").map((t) => t.id)).toEqual(["a", "b"]);
  });

  it("sorts scheduled tasks by start then end", () => {
    const tasks = [
      task({ id: "late", title: "后", start: 300, end: 400 }),
      task({ id: "early-long", title: "长", start: 100, end: 400 }),
      task({ id: "early-short", title: "短", start: 100, end: 200 }),
    ];
    expect(sortTasks(tasks, "time").map((t) => t.id)).toEqual([
      "early-short",
      "early-long",
      "late",
    ]);
  });

  it("sorts by title in zh order", () => {
    const tasks = [
      task({ id: "b", title: "乙" }),
      task({ id: "a", title: "甲" }),
    ];
    expect(sortTasks(tasks, "title").map((t) => t.id)).toEqual(["a", "b"]);
  });
});
