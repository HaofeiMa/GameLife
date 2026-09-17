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

  it("sorts unscheduled tasks by add order, not title", () => {
    const tasks = [
      task({ id: "later", title: "甲", sort: 20 }),
      task({ id: "earlier", title: "乙", sort: 10 }),
    ];
    expect(sortTasks(tasks, "time").map((t) => t.id)).toEqual(["earlier", "later"]);
  });

  it("inserts a newly scheduled task among timed rows by start", () => {
    const tasks = [
      task({ id: "inbox", title: "未排", sort: 0 }),
      task({ id: "late", title: "后", start: 300, end: 400, sort: 10 }),
      task({ id: "mid", title: "中", start: 200, end: 260, sort: 30 }),
      task({ id: "early", title: "早", start: 100, end: 160, sort: 20 }),
    ];
    expect(sortTasks(tasks, "time").map((t) => t.id)).toEqual([
      "early",
      "mid",
      "late",
      "inbox",
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
