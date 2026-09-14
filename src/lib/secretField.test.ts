import { describe, expect, it } from "vitest";
import {
  callbackPasteKind,
  isTicktickAuthorizeUrl,
  oauthErrorMessage,
  oauthWaitingHint,
  primaryProviderHasKey,
  policySignature,
  SECRET_MASK,
  secretToPersist,
  showSecretMask,
  ticktickSecretReady,
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

describe("ticktickSecretReady", () => {
  it("requires a typed secret when the keychain has none", () => {
    expect(ticktickSecretReady("", false)).toEqual({
      ok: false,
      error: "请填写 Client Secret 后再连接",
    });
    expect(ticktickSecretReady("  abc  ", false)).toEqual({
      ok: true,
      toWrite: "abc",
    });
  });

  it("lets connect proceed with a stored secret and no new draft", () => {
    expect(ticktickSecretReady("", true)).toEqual({ ok: true, toWrite: null });
  });
});

describe("primaryProviderHasKey", () => {
  it("checks the selected provider, not a sibling", () => {
    expect(
      primaryProviderHasKey("opencode-go", {
        opencodeGo: true,
        openai: false,
        custom: false,
      }),
    ).toBe(true);
    expect(
      primaryProviderHasKey("opencode-go", {
        opencodeGo: false,
        openai: true,
        custom: false,
      }),
    ).toBe(false);
  });
});

describe("oauthErrorMessage", () => {
  it("translates the missing-secret preflight into Chinese", () => {
    expect(oauthErrorMessage("missing ticktick client secret")).toBe(
      "缺少 TickTick Client Secret，请重新填写后连接",
    );
  });

  it("explains oauth listen as a port/callback problem, not a save failure", () => {
    expect(oauthErrorMessage("oauth listen")).toContain("18789");
    expect(oauthErrorMessage("oauth listen")).toContain("粘贴");
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

describe("callbackPasteKind", () => {
  it("rejects the developer-console redirect URI that has no code", () => {
    expect(callbackPasteKind("http://localhost:3000/callback")).toBe("setting");
    expect(
      callbackPasteKind("http://127.0.0.1:18789/callback?code=abc"),
    ).toBe("code");
  });
});

describe("isTicktickAuthorizeUrl", () => {
  it("accepts TickTick authorize URLs and rejects the loopback redirect", () => {
    expect(
      isTicktickAuthorizeUrl(
        "https://ticktick.com/oauth/authorize?client_id=abc&response_type=code",
      ),
    ).toBe(true);
    expect(isTicktickAuthorizeUrl("http://127.0.0.1:18789/callback")).toBe(
      false,
    );
    expect(isTicktickAuthorizeUrl("http://localhost:3000/callback")).toBe(
      false,
    );
  });
});

describe("oauthWaitingHint", () => {
  it("tells the user to open the authorize URL, not the redirect URI", () => {
    const hint = oauthWaitingHint();
    expect(hint).toContain("ticktick.com/oauth/authorize");
    expect(hint).toContain("不要");
    expect(hint).toContain("127.0.0.1");
  });
});
