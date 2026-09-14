import { describe, expect, it } from "vitest";
import { PREVIEW_TODAY, PREVIEW_WEEK, PREVIEW_SETTINGS } from "./fixtures";

describe("preview fixtures", () => {
  it("does not embed a real home directory or developer id", () => {
    const blob = JSON.stringify({
      today: PREVIEW_TODAY,
      week: PREVIEW_WEEK,
      settings: PREVIEW_SETTINGS,
    });
    expect(blob).not.toMatch(/mahaofei/i);
    expect(blob).not.toMatch(/MobileSSD/);
    expect(blob).not.toMatch(/3762LQR944/);
    expect(blob).not.toMatch(/sk-[a-zA-Z0-9]/);
  });
});
