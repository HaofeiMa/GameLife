import { describe, expect, it } from "vitest";
import { listDragShown, ranksAfterDrag, unscheduledBeforeId } from "./taskReorder";

const timed = (id: string) => ({ id, start: 1, end: 2 });
const open = (id: string) => ({ id, start: null, end: null });

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

describe("unscheduledBeforeId", () => {
  const dest = [timed("t1"), timed("t2"), open("u1"), open("u2")];

  it("keeps a drop before another unscheduled row", () => {
    expect(unscheduledBeforeId(dest, "u2", "u1")).toBe("u1");
  });

  it("clamps a drop onto a timed row to the top of the unscheduled block", () => {
    expect(unscheduledBeforeId(dest, "u2", "t1")).toBe("u1");
  });

  it("appends when dropping at the end", () => {
    expect(unscheduledBeforeId(dest, "u1", null)).toBeNull();
  });
});
