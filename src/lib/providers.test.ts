import { describe, expect, it } from "vitest";
import type { VisionProviderSettings } from "./api";
import {
  addCodexPanel,
  addCustomPanel,
  customPanelTitle,
  KIND_CODEX,
  KIND_CUSTOM,
  migrateVisionProviders,
  moveProvider,
  nextCustomId,
  providerIsReady,
} from "./providers";

function p(
  partial: Partial<VisionProviderSettings> & Pick<VisionProviderSettings, "id">,
): VisionProviderSettings {
  return {
    baseUrl: "",
    model: "",
    kind: "",
    ...partial,
  };
}

describe("migrateVisionProviders", () => {
  it("turns the old three presets into 自定义 API then Codex and drops the empty custom", () => {
    const next = migrateVisionProviders(
      [
        p({
          id: "opencode-go",
          baseUrl: "https://opencode.ai/zen/go/v1",
          model: "deepseek-v4-flash-vision-exp",
        }),
        p({
          id: "openai",
          baseUrl: "https://api.openai.com/v1",
          model: "gpt-4o-mini",
        }),
        p({ id: "custom" }),
      ],
      "opencode-go",
      "openai",
    );
    expect(next.map((x) => x.id)).toEqual(["opencode-go", "codex"]);
    expect(next[0]?.kind).toBe(KIND_CUSTOM);
    expect(next[0]?.baseUrl).toBe("https://opencode.ai/zen/go/v1");
    expect(next[1]?.kind).toBe(KIND_CODEX);
    expect(next[1]?.baseUrl).toBe("");
  });

  it("orders by the old primary then fallback", () => {
    const next = migrateVisionProviders(
      [
        p({ id: "opencode-go", baseUrl: "https://opencode.ai/zen/go/v1" }),
        p({ id: "openai", baseUrl: "https://api.openai.com/v1" }),
      ],
      "openai",
      "opencode-go",
    );
    expect(next.map((x) => x.id)).toEqual(["codex", "opencode-go"]);
  });

  it("keeps a filled custom card after Codex", () => {
    const next = migrateVisionProviders(
      [
        p({ id: "opencode-go", baseUrl: "https://opencode.ai/zen/go/v1" }),
        p({ id: "openai" }),
        p({
          id: "custom",
          baseUrl: "https://proxy.example/v1",
          model: "vision",
        }),
      ],
      "opencode-go",
      "openai",
    );
    expect(next.map((x) => x.id)).toEqual(["opencode-go", "codex", "custom"]);
    expect(next[2]?.kind).toBe(KIND_CUSTOM);
  });

  it("does not reorder a list that already has kinds", () => {
    const already = [
      p({ id: "codex", kind: KIND_CODEX, model: "gpt-5.4" }),
      p({
        id: "opencode-go",
        kind: KIND_CUSTOM,
        baseUrl: "https://opencode.ai/zen/go/v1",
      }),
    ];
    const next = migrateVisionProviders(already, "opencode-go", "codex");
    expect(next.map((x) => x.id)).toEqual(["codex", "opencode-go"]);
  });
});

describe("nextCustomId", () => {
  it("skips taken ids including the legacy opencode-go slot", () => {
    expect(nextCustomId(["opencode-go", "codex"])).toBe("custom-2");
    expect(nextCustomId(["opencode-go", "codex", "custom-2"])).toBe("custom-3");
  });
});

describe("moveProvider", () => {
  it("swaps with the neighbour and clamps the ends", () => {
    expect(moveProvider(["a", "b", "c"], 1, -1)).toEqual(["b", "a", "c"]);
    expect(moveProvider(["a", "b", "c"], 0, -1)).toEqual(["a", "b", "c"]);
    expect(moveProvider(["a", "b", "c"], 2, 1)).toEqual(["a", "b", "c"]);
  });
});

describe("add panels", () => {
  it("appends a blank custom and refuses a second Codex", () => {
    const two = migrateVisionProviders(
      [
        p({ id: "opencode-go", baseUrl: "https://x" }),
        p({ id: "openai" }),
      ],
      "opencode-go",
      "openai",
    );
    const withCustom = addCustomPanel(two);
    expect(withCustom).toHaveLength(3);
    expect(withCustom[2]?.kind).toBe(KIND_CUSTOM);
    expect(addCodexPanel(withCustom)).toEqual(withCustom);
  });
});

describe("customPanelTitle", () => {
  it("numbers extra custom cards from 2", () => {
    const list = [
      p({ id: "opencode-go", kind: KIND_CUSTOM }),
      p({ id: "codex", kind: KIND_CODEX }),
      p({ id: "custom-2", kind: KIND_CUSTOM }),
    ];
    expect(customPanelTitle(list, "opencode-go")).toBe("自定义 API");
    expect(customPanelTitle(list, "custom-2")).toBe("自定义 API 2");
  });
});

describe("providerIsReady", () => {
  it("treats Codex as ready only when the CLI session exists", () => {
    const codex = p({ id: "codex", kind: KIND_CODEX, model: "gpt-5.4" });
    expect(providerIsReady(codex, {}, false)).toBe(false);
    expect(providerIsReady(codex, {}, true)).toBe(true);
  });

  it("requires a key, URL, and model for a custom card", () => {
    const custom = p({
      id: "opencode-go",
      kind: KIND_CUSTOM,
      baseUrl: "https://x",
      model: "m",
    });
    expect(providerIsReady(custom, {}, false)).toBe(false);
    expect(providerIsReady(custom, { "opencode-go": true }, false)).toBe(true);
  });
});
