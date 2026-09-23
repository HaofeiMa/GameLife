import { describe, expect, it } from "vitest";
import {
  columnRoleChoice,
  columnRolePatch,
  EMPTY_COLUMNS_COPY,
  mergeTicktickRolesFromDisk,
  roleForProject,
  sortedColumns,
  syncNowDisabled,
  writeTargetHint,
} from "./ticktickSettings";

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

  it("missing column role follows the list", () => {
    expect(columnRoleChoice({}, "c1")).toBe("inherit");
    expect(columnRoleChoice({ c1: "nope" }, "c1")).toBe("inherit");
    expect(columnRoleChoice({ c1: "mainline" }, "c1")).toBe("mainline");
    expect(columnRoleChoice({ c1: "ignore" }, "c1")).toBe("ignore");
  });

  it("inherit deletes the stored key and a role writes it", () => {
    expect(columnRolePatch({ c1: "side", c2: "chore" }, "c1", "inherit")).toEqual({
      c2: "chore",
    });
    expect(columnRolePatch({}, "c1", "mainline")).toEqual({ c1: "mainline" });
  });

  it("empty columns use the empty-group sentence", () => {
    const columns: { id: string; sortOrder: number }[] = [];
    const copy = columns.length === 0 ? EMPTY_COLUMNS_COPY : "";
    expect(copy).toBe("这个清单没有分组，任务按整份清单归类。");
  });

  it("sorts columns by sortOrder then id", () => {
    expect(
      sortedColumns([
        { id: "b", sortOrder: 1 },
        { id: "a", sortOrder: 1 },
        { id: "c", sortOrder: 0 },
      ]).map((column) => column.id),
    ).toEqual(["c", "a", "b"]);
  });

  it("drops a pruned column role and keeps an unsaved redirect", () => {
    const form = {
      ticktickColumnRoles: { "col-gone": "mainline", "col-kept": "side" },
      ticktickProjectRoles: { "proj-kept": "mainline" },
      ticktickRedirectUri: "http://127.0.0.1:1420/unsaved",
      ticktickClientId: "unsaved-client",
    };
    const disk = {
      ticktickColumnRoles: { "col-kept": "side" },
      ticktickProjectRoles: { "proj-kept": "chore" },
      ticktickRedirectUri: "http://127.0.0.1:1420/saved",
    };
    const merged = mergeTicktickRolesFromDisk(form, disk);
    expect(merged.ticktickColumnRoles).toEqual({ "col-kept": "side" });
    expect(merged.ticktickColumnRoles).not.toHaveProperty("col-gone");
    expect(merged.ticktickProjectRoles).toEqual({ "proj-kept": "chore" });
    expect(merged.ticktickRedirectUri).toBe("http://127.0.0.1:1420/unsaved");
    expect(merged.ticktickClientId).toBe("unsaved-client");
  });
});
