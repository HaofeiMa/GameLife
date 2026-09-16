import { describe, expect, it } from "vitest";
import {
  applyThemeClass,
  categoryColorAt,
  categoryOf,
  COLOR_THEMES,
  normalizeColorTheme,
} from "./theme";

describe("categoryColorAt", () => {
  it("mixes the category variable at the given percentage", () => {
    expect(categoryColorAt("mainline", 28)).toBe(
      "color-mix(in srgb, hsl(var(--cat-mainline)) 28%, transparent)",
    );
  });

  it("clamps out-of-range percentages", () => {
    expect(categoryColorAt("side", -5)).toContain("0%");
    expect(categoryColorAt("side", 400)).toContain("100%");
  });
});

describe("categoryOf", () => {
  it("accepts both the role and the short report vocabulary", () => {
    expect(categoryOf("core_research")).toBe("mainline");
    expect(categoryOf("core")).toBe("mainline");
    expect(categoryOf("chore")).toBe("admin");
    expect(categoryOf("distraction")).toBe("entertainment");
  });
});

describe("color themes", () => {
  it("lists TickTick-style labels in a stable order", () => {
    expect(COLOR_THEMES.map((t) => t.label)).toEqual([
      "默认",
      "暖黄",
      "晴蓝",
      "松石",
      "秘青",
      "蒹葭",
      "杏黄",
      "桃天",
      "暮山紫",
      "沉香",
      "藏蓝",
    ]);
  });

  it("normalises unknown or empty values to default", () => {
    expect(normalizeColorTheme("qinglan")).toBe("qinglan");
    expect(normalizeColorTheme("default")).toBe("default");
    expect(normalizeColorTheme("nuanhuang")).toBe("nuanhuang");
    expect(normalizeColorTheme("huise")).toBe("default");
    expect(normalizeColorTheme("night")).toBe("default");
    expect(normalizeColorTheme("")).toBe("default");
    expect(normalizeColorTheme(undefined)).toBe("default");
  });
});

describe("applyThemeClass", () => {
  /** Stands in for document.documentElement. */
  function fakeRoot() {
    const classes = new Set<string>();
    const attrs = new Map<string, string>();
    return {
      classList: {
        toggle: (name: string, on: boolean) => {
          if (on) classes.add(name);
          else classes.delete(name);
        },
      },
      style: { backgroundColor: "" },
      setAttribute: (name: string, value: string) => {
        attrs.set(name, value);
      },
      getAttribute: (name: string) => attrs.get(name) ?? null,
      has: (name: string) => classes.has(name),
    };
  }

  it("toggles the dark class", () => {
    const root = fakeRoot();
    applyThemeClass("dark", root as unknown as HTMLElement);
    expect(root.has("dark")).toBe(true);
    applyThemeClass("light", root as unknown as HTMLElement);
    expect(root.has("dark")).toBe(false);
  });

  it("stamps data-accent so CSS palettes can attach", () => {
    const root = fakeRoot();
    applyThemeClass("light", root as unknown as HTMLElement, "qinglan");
    expect(root.getAttribute("data-accent")).toBe("qinglan");
    applyThemeClass("light", root as unknown as HTMLElement, "night");
    expect(root.getAttribute("data-accent")).toBe("default");
  });

  it("paints a themed prepaint background instead of the default ground", () => {
    const themed = fakeRoot();
    const fallback = fakeRoot();
    applyThemeClass("light", themed as unknown as HTMLElement, "qinglan");
    applyThemeClass("light", fallback as unknown as HTMLElement);
    expect(themed.style.backgroundColor).not.toBe(fallback.style.backgroundColor);
    expect(themed.style.backgroundColor.startsWith("hsl(")).toBe(true);
  });

  /**
   * The pre-paint script in index.html sets an inline backgroundColor so the
   * first frame is not a flash. Inline style outranks `--background`, so if
   * this function only toggled the class, switching to dark would leave the
   * warm-white inline value painting over the dark token.
   */
  it("rewrites the inline background so the prepaint value cannot win", () => {
    const root = fakeRoot();
    root.style.backgroundColor = "hsl(36 50% 96%)";

    applyThemeClass("dark", root as unknown as HTMLElement);
    expect(root.style.backgroundColor).not.toBe("hsl(36 50% 96%)");

    const light = fakeRoot();
    applyThemeClass("light", light as unknown as HTMLElement);
    applyThemeClass("dark", root as unknown as HTMLElement);
    expect(root.style.backgroundColor).not.toBe(light.style.backgroundColor);

    applyThemeClass("light", root as unknown as HTMLElement);
    expect(root.style.backgroundColor).toBe(light.style.backgroundColor);
  });
});
