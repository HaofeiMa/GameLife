import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

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

  it("is warm charcoal, not cool grey", () => {
    // #1C1917 — R >= G >= B, the mockup's named value for the dark ground.
    const bg = tokensIn(darkBlock).find((t) => t.name === "background");
    expect(bg?.hex).toBe("#1C1917");
  });
});
