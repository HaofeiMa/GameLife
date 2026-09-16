import { describe, expect, it } from "vitest";
import { notesByteLength, parseTaskNotes } from "./taskNotesMd";

describe("parseTaskNotes", () => {
  it("parses paragraphs, bold, lists, and https links", () => {
    const blocks = parseTaskNotes(
      "见 **指标**\n\n- 地点 A301\n- 带[纪要](https://example.com/a)\n\n1. 先开会",
    );
    expect(blocks).toEqual([
      { type: "p", children: [{ type: "text", value: "见 " }, { type: "strong", value: "指标" }] },
      {
        type: "ul",
        items: [
          [{ type: "text", value: "地点 A301" }],
          [
            { type: "text", value: "带" },
            { type: "link", text: "纪要", href: "https://example.com/a" },
          ],
        ],
      },
      { type: "ol", items: [[{ type: "text", value: "先开会" }]] },
    ]);
  });

  it("keeps javascript URLs as plain text", () => {
    const blocks = parseTaskNotes("[x](javascript:alert(1))");
    expect(blocks).toEqual([
      { type: "p", children: [{ type: "text", value: "[x](javascript:alert(1))" }] },
    ]);
  });

  it("does not emit heading or html nodes", () => {
    const blocks = parseTaskNotes("# 标题\n\n<img src=x>");
    expect(blocks.every((b) => b.type === "p")).toBe(true);
    expect(JSON.stringify(blocks)).not.toMatch(/"type":"html"/);
  });
});

describe("notesByteLength", () => {
  it("counts utf-8 bytes", () => {
    expect(notesByteLength("a".repeat(8192))).toBe(8192);
    expect(notesByteLength("你")).toBe(3);
  });
});
