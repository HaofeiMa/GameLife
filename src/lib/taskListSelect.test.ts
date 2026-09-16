import { describe, expect, it } from "vitest";
import { rangeSelect, toggleSelect, visibleTaskIds } from "./taskListSelect";

describe("visibleTaskIds", () => {
  it("walks expanded groups in list order and skips collapsed", () => {
    const map = new Map([
      ["g1", [{ id: "a" }, { id: "b" }]],
      ["g2", [{ id: "c" }]],
      ["g3", [{ id: "d" }]],
    ]);
    expect(visibleTaskIds(["g1", "g2", "g3"], map, ["g2"])).toEqual(["a", "b", "d"]);
  });
});

describe("rangeSelect", () => {
  const vis = ["a", "b", "d"];
  it("selects the closed visible range", () => {
    expect(rangeSelect(vis, "a", "d")).toEqual(["a", "b", "d"]);
  });
  it("selects only the target when there is no anchor", () => {
    expect(rangeSelect(vis, null, "d")).toEqual(["d"]);
  });
});

describe("toggleSelect", () => {
  it("adds and removes", () => {
    expect(toggleSelect(["a"], "b")).toEqual(["a", "b"]);
    expect(toggleSelect(["a", "b"], "a")).toEqual(["b"]);
  });
});
