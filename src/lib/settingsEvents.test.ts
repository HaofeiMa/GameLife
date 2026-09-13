import { describe, expect, it, vi } from "vitest";
import { notifySettingsChanged, SETTINGS_CHANGED_EVENT } from "./settingsEvents";

describe("notifySettingsChanged", () => {
  it("dispatches a window event the shell can refresh from", () => {
    const target = new EventTarget();
    const listener = vi.fn();
    target.addEventListener(SETTINGS_CHANGED_EVENT, listener);
    notifySettingsChanged(target);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
