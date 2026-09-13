import { describe, expect, it } from "vitest";
import { applyThemeClass, categoryColorAt, categoryOf } from "./theme";

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

describe("applyThemeClass", () => {
  /** Stands in for document.documentElement. */
  function fakeRoot() {
    const classes = new Set<string>();
    return {
      classList: {
        toggle: (name: string, on: boolean) => {
          if (on) classes.add(name);
          else classes.delete(name);
        },
      },
      style: { backgroundColor: "" },
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
    expect(root.style.backgroundColor).toBe("hsl(24 10% 10%)");

    applyThemeClass("light", root as unknown as HTMLElement);
    expect(root.style.backgroundColor).toBe("hsl(36 50% 96%)");
  });
});
