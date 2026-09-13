import { Settings, Sun } from "lucide-react";
import { describe, expect, it } from "vitest";
import { railTabs } from "./rail";

describe("railTabs", () => {
  it("labels week tab as 统计", () => {
    expect(railTabs()[1].label).toBe("统计");
  });

  it("gives every tab an icon", () => {
    for (const tab of railTabs()) {
      expect(tab.icon).toBeTruthy();
    }
  });
});

describe("settings icon", () => {
  it("uses a toothed gear, not sun rays", () => {
    const settings = railTabs().find((tab) => tab.id === "settings");
    expect(settings?.icon).toBe(Settings);
    expect(settings?.icon).not.toBe(Sun);
  });
});
