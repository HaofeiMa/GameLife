import { describe, expect, it } from "vitest";
import { isMacOsUserAgent } from "./platform";

const MAC_SAFARI =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
const WIN_CHROMIUM =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const LINUX_FIREFOX =
  "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0";

describe("isMacOsUserAgent", () => {
  it("recognises a macOS webview", () => {
    expect(isMacOsUserAgent(MAC_SAFARI)).toBe(true);
  });

  it("does not mistake Windows or Linux for macOS", () => {
    expect(isMacOsUserAgent(WIN_CHROMIUM)).toBe(false);
    expect(isMacOsUserAgent(LINUX_FIREFOX)).toBe(false);
  });

  it("treats a missing user agent as not macOS", () => {
    for (const ua of [undefined, null, ""]) {
      expect(isMacOsUserAgent(ua)).toBe(false);
    }
  });
});
