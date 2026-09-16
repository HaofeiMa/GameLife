import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const srcRoot = fileURLToPath(new URL("..", import.meta.url));
const css = readFileSync(join(srcRoot, "index.css"), "utf8");

const TEXT_PX = /text-\[(\d+(?:\.\d+)?)px\]/g;

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    if (ent.name === "node_modules") continue;
    const path = join(dir, ent.name);
    if (ent.isDirectory()) {
      out.push(...walk(path));
      continue;
    }
    if (/\.(tsx|css)$/.test(ent.name)) out.push(path);
  }
  return out;
}

/** Sizes the pre-bump mockup used that the mapping moves off their old value. */
const PRE_BUMP_GONE = [9, 10.5, 12, 12.5, 16, 23, 26];

describe("type scale", () => {
  it("sets body to 16px and lifts Tailwind xs/sm two sizes", () => {
    expect(css).toMatch(/body \{[^}]*font-size: 1rem;/);
    expect(css).toMatch(/--text-xs:\s*0\.875rem;/);
    expect(css).toMatch(/--text-sm:\s*1rem;/);
  });

  it("moves arbitrary text-[Npx] off the pre-bump reading sizes", () => {
    const leftover: { file: string; size: number }[] = [];
    for (const file of walk(srcRoot)) {
      const text = readFileSync(file, "utf8");
      for (const match of text.matchAll(TEXT_PX)) {
        const size = Number(match[1]);
        if (PRE_BUMP_GONE.includes(size)) {
          leftover.push({
            file: file.slice(srcRoot.replace(/\/$/, "").length + 1),
            size,
          });
        }
      }
    }
    expect(leftover).toEqual([]);
  });
});
