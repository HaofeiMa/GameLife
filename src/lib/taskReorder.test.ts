import { describe, expect, it } from "vitest";
import { listDragShown, ranksAfterDrag } from "./taskReorder";

describe("ranksAfterDrag", () => {
  it("moves an id before another", () => {
    const next = ranksAfterDrag(["a", "b", "c"], "c", "a");
    expect(next.map((x) => x.id)).toEqual(["c", "a", "b"]);
  });
});

describe("listDragShown", () => {
  it("drops the dragged id and inserts a gap before the target", () => {
    expect(listDragShown(["a", "b", "c"], "a", "c")).toEqual({
      shown: ["b", "c"],
      gapIndex: 1,
    });
  });

  it("puts the gap at the end when beforeId is null", () => {
    expect(listDragShown(["x", "y"], "z", null)).toEqual({
      shown: ["x", "y"],
      gapIndex: 2,
    });
  });

  it("treats a missing beforeId as append", () => {
    expect(listDragShown(["a", "b"], "c", "missing")).toEqual({
      shown: ["a", "b"],
      gapIndex: 2,
    });
  });
});
