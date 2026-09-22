import { describe, expect, it } from "vitest";
import { roleForProject, syncNowDisabled, writeTargetHint } from "./ticktickSettings";

describe("ticktick settings", () => {
  it("disables immediate sync until the switch is on and the account is connected", () => {
    expect(syncNowDisabled(false, true)).toBe(true);
    expect(syncNowDisabled(true, false)).toBe(true);
    expect(syncNowDisabled(true, true)).toBe(false);
  });

  it("defaults an unseen list to ignore", () => {
    expect(roleForProject({ p: "mainline" }, "p")).toBe("mainline");
    expect(roleForProject({}, "new")).toBe("ignore");
  });

  it("names the write target", () => {
    expect(writeTargetHint("论文")).toBe("新建任务写入「论文」");
    expect(writeTargetHint(null)).toBeNull();
  });
});
