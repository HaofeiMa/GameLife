import { describe, expect, it } from "vitest";
import { ranksAfterDrag } from "./taskReorder";

describe("ranksAfterDrag", () => {
  it("moves an id before another", () => {
    const next = ranksAfterDrag(["a", "b", "c"], "c", "a");
    expect(next.map((x) => x.id)).toEqual(["c", "a", "b"]);
  });
});
