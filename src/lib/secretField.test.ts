import { describe, expect, it } from "vitest";
import {
  policySignature,
  SECRET_MASK,
  secretToPersist,
  showSecretMask,
} from "./secretField";

describe("secretToPersist", () => {
  it("ignores blank, mask, and whitespace so a save cannot pretend a key was typed", () => {
    expect(secretToPersist("")).toBeNull();
    expect(secretToPersist("   ")).toBeNull();
    expect(secretToPersist(SECRET_MASK)).toBeNull();
    expect(secretToPersist(" sk-live-1 ")).toBe("sk-live-1");
  });
});

describe("showSecretMask", () => {
  it("shows the mask only when the keychain has an entry and the user is not typing a replacement", () => {
    expect(showSecretMask(true, "")).toBe(true);
    expect(showSecretMask(true, "sk-new")).toBe(false);
    expect(showSecretMask(false, "")).toBe(false);
  });
});

describe("policySignature", () => {
  const base = {
    distractionRules: ["bilibili.com"],
    sideProjectRules: ["滴答清单"],
    adminApps: ["微信"],
    neverCaptureApps: [],
    categoryGuides: { mainline: "", side: "", admin: "", entertainment: "" },
  };

  it("changes when a list the judge reads changes", () => {
    expect(policySignature({ ...base, adminApps: ["微信", "Zoom"] })).not.toBe(
      policySignature(base),
    );
  });

  it("ignores whitespace around the guides, because saving trims them", () => {
    const spaced = {
      ...base,
      categoryGuides: { ...base.categoryGuides, mainline: "  主线说明  " },
    };
    const tight = {
      ...base,
      categoryGuides: { ...base.categoryGuides, mainline: "主线说明" },
    };
    expect(policySignature(spaced)).toBe(policySignature(tight));
  });

  it("stays put when nothing the judge reads has changed", () => {
    expect(policySignature({ ...base })).toBe(policySignature(base));
  });
});
