import { describe, expect, it } from "vitest";
import { permissionRows, privacySettingsUrl } from "./permissions";

describe("privacySettingsUrl", () => {
  it("points at the Accessibility and Screen Recording panes", () => {
    expect(privacySettingsUrl("accessibility")).toContain("Privacy_Accessibility");
    expect(privacySettingsUrl("screen")).toContain("Privacy_ScreenCapture");
  });
});

describe("permissionRows", () => {
  it("always lists accessibility and screen recording with the current process", () => {
    const rows = permissionRows({
      accessibility: true,
      screenRecording: false,
      processName: "gamelife",
      processPath: "/tmp/gamelife",
    });
    expect(rows.map((r) => r.id)).toEqual(["accessibility", "screen"]);
    expect(rows[0]?.granted).toBe(true);
    expect(rows[1]?.granted).toBe(false);
    expect(rows[0]?.hint).toContain("gamelife");
  });
});
