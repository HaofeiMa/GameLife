import { describe, expect, it } from "vitest";
import {
  committedTag,
  deleteTagIfBackspace,
  filterHashLists,
  hashQuery,
  paintSegments,
} from "./taskComposer";

const lists = [
  { id: "list-mainline", name: "主线任务", role: "mainline" },
  { id: "list-side", name: "支线任务", role: "side" },
  { id: "list-chore", name: "杂项", role: "chore" },
  { id: "mine", name: "论文", role: "mainline" },
];

describe("paintSegments", () => {
  it("marks datetime spans as time and a committed hashtag as tag", () => {
    const text = "明天十点 写 #主线";
    expect(paintSegments(text, [{ start: 0, end: 4 }], { start: 7, end: 10 })).toEqual([
      { text: "明天十点", tone: "time" },
      { text: " 写 ", tone: "plain" },
      { text: "#主线", tone: "tag" },
    ]);
  });
});

describe("hashQuery", () => {
  it("reads the open # fragment at the caret", () => {
    expect(hashQuery("写论文 #杂", 6, null)).toEqual({ start: 4, query: "杂" });
  });

  it("ignores a committed tag", () => {
    expect(hashQuery("写 #杂项 好", 8, { start: 2, end: 5 })).toBeNull();
  });
});

describe("filterHashLists", () => {
  it("keeps list order and matches name or role alias", () => {
    expect(filterHashLists(lists, "").map((l) => l.id)).toEqual([
      "list-mainline",
      "list-side",
      "list-chore",
      "mine",
    ]);
    expect(filterHashLists(lists, "主").map((l) => l.id)).toEqual(["list-mainline"]);
    expect(filterHashLists(lists, "论文").map((l) => l.id)).toEqual(["mine"]);
  });
});

describe("committedTag", () => {
  it("treats a complete #alias as one chip", () => {
    expect(committedTag("写方法节 #主线", lists)).toEqual({
      start: 5,
      end: 8,
      name: "主线",
    });
  });
});

describe("deleteTagIfBackspace", () => {
  it("removes the whole tag when the caret is on or just after it", () => {
    const tag = { start: 5, end: 8 };
    expect(deleteTagIfBackspace("写方法节 #主线", 8, tag)).toEqual({
      text: "写方法节 ",
      caret: 5,
    });
  });
});
