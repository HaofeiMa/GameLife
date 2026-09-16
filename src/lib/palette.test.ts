import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { prepaintBackground } from "./theme";

const css = readFileSync(
  fileURLToPath(new URL("../index.css", import.meta.url)),
  "utf8",
);

/** `hsl(H S% L%)` back to the 8-bit hex a browser would actually paint. */
function hslToHex(triplet: string): string {
  const [h, s, l] = triplet
    .trim()
    .split(/\s+/)
    .map((p) => Number.parseFloat(p.replace("%", "")));
  const c = (1 - Math.abs((2 * l) / 100 - 1)) * (s / 100);
  const hp = h / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  const [r1, g1, b1] =
    hp < 1 ? [c, x, 0]
    : hp < 2 ? [x, c, 0]
    : hp < 3 ? [0, c, x]
    : hp < 4 ? [0, x, c]
    : hp < 5 ? [x, 0, c]
    : [c, 0, x];
  const m = l / 100 - c / 2;
  return (
    "#" +
    [r1, g1, b1]
      .map((v) => Math.round((v + m) * 255).toString(16).padStart(2, "0"))
      .join("")
      .toUpperCase()
  );
}

function tokensIn(block: string) {
  const out: { name: string; triplet: string; hex: string }[] = [];
  const re = /--([a-z0-9-]+):\s*([\d.]+ [\d.]+% [\d.]+%);\s*\/\* (#[0-9A-Fa-f]{6}) \*\//g;
  for (const m of block.matchAll(re)) {
    out.push({ name: m[1]!, triplet: m[2]!, hex: m[3]!.toUpperCase() });
  }
  return out;
}

const lightBlock = /:root \{(.*?)\n\}/s.exec(css)?.[1] ?? "";
const darkBlock = /\.dark \{(.*?)\n\}/s.exec(css)?.[1] ?? "";

describe("light palette", () => {
  const tokens = tokensIn(lightBlock);

  it("is written down at all", () => {
    expect(tokens.length).toBeGreaterThan(40);
  });

  /**
   * The design's hex values are the source of truth. They are stored as bare
   * HSL triplets for the theme system, so the only way to keep them identical
   * is to store enough decimal places that the round-trip is lossless —
   * integer triplets are a full 8-bit step off for most of the palette.
   */
  it("round-trips to the design's hex exactly", () => {
    const drifted = tokens
      .map((t) => ({ ...t, got: hslToHex(t.triplet) }))
      .filter((t) => t.got !== t.hex);
    expect(drifted).toEqual([]);
  });

  it("keeps every category colour", () => {
    const names = tokens.map((t) => t.name);
    for (const cat of [
      "cat-mainline",
      "cat-support",
      "cat-side",
      "cat-longterm",
      "cat-admin",
      "cat-entertainment",
      "cat-away",
      "cat-unobserved",
      "cat-pending",
    ]) {
      expect(names).toContain(cat);
    }
  });
});

describe("dark palette", () => {
  it("round-trips to the design's hex exactly", () => {
    const drifted = tokensIn(darkBlock)
      .map((t) => ({ ...t, got: hslToHex(t.triplet) }))
      .filter((t) => t.got !== t.hex);
    expect(drifted).toEqual([]);
  });

  it("is cool near-black for 默认, not the 暖黄 charcoal", () => {
    const bg = tokensIn(darkBlock).find((t) => t.name === "background");
    expect(bg?.hex).not.toBe("#1C1917");
  });
});

const ACCENT_IDS = [
  "nuanhuang",
  "qinglan",
  "songshi",
  "miqing",
  "jianjia",
  "xinghuang",
  "taotian",
  "mushanzi",
  "chenxiang",
  "zanglan",
] as const;

function accentBlock(id: string, dark: boolean): string {
  const re = dark
    ? new RegExp(`\\.dark\\[data-accent="${id}"\\] \\{(.*?)\\n\\}`, "s")
    : new RegExp(`\\[data-accent="${id}"\\] \\{(.*?)\\n\\}`, "s");
  return re.exec(css)?.[1] ?? "";
}

describe("color-theme palettes", () => {
  it("washes the ground tokens without touching category colours", () => {
    for (const id of ACCENT_IDS) {
      const light = tokensIn(accentBlock(id, false));
      const dark = tokensIn(accentBlock(id, true));
      expect(light.map((t) => t.name), id).toEqual(
        expect.arrayContaining(["background", "primary", "rail", "loot", "card", "swatch"]),
      );
      expect(dark.map((t) => t.name), id).toEqual(
        expect.arrayContaining(["background", "primary", "rail", "loot", "card", "swatch"]),
      );
      expect(light.map((t) => t.name)).not.toContain("cat-mainline");
      expect(dark.map((t) => t.name)).not.toContain("cat-mainline");
    }
  });

  it("round-trips every accent hex comment", () => {
    const drifted = ACCENT_IDS.flatMap((id) =>
      [...tokensIn(accentBlock(id, false)), ...tokensIn(accentBlock(id, true))].map(
        (t) => ({ id, ...t, got: hslToHex(t.triplet) }),
      ),
    ).filter((t) => t.got !== t.hex);
    expect(drifted).toEqual([]);
  });

  it("keeps JS prepaint colours in lockstep with CSS --background", () => {
    for (const id of ACCENT_IDS) {
      const light = tokensIn(accentBlock(id, false)).find((t) => t.name === "background");
      const dark = tokensIn(accentBlock(id, true)).find((t) => t.name === "background");
      expect(prepaintBackground("light", id), id).toBe(`hsl(${light?.triplet})`);
      expect(prepaintBackground("dark", id), id).toBe(`hsl(${dark?.triplet})`);
    }
  });

  it("keeps 暖黄 as the mockup parchment", () => {
    const light = tokensIn(accentBlock("nuanhuang", false)).find((t) => t.name === "background");
    const dark = tokensIn(accentBlock("nuanhuang", true)).find((t) => t.name === "background");
    expect(light?.hex).toBe("#FAF6F0");
    expect(dark?.hex).toBe("#1C1917");
  });

  it("paints 默认 from :root, slightly off pure parchment", () => {
    const bg = tokensIn(lightBlock).find((t) => t.name === "background");
    const darkBg = tokensIn(darkBlock).find((t) => t.name === "background");
    expect(prepaintBackground("light", "default")).toBe(`hsl(${bg?.triplet})`);
    expect(prepaintBackground("dark", "default")).toBe(`hsl(${darkBg?.triplet})`);
    expect(bg?.hex).toBe("#FAFAFA");
    expect(bg?.hex).not.toBe("#FAF6F0");
  });

  it("paints the 默认 chip white", () => {
    const swatch = tokensIn(lightBlock).find((t) => t.name === "swatch");
    expect(swatch?.hex).toBe("#FFFFFF");
  });
});
