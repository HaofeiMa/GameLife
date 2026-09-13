/**
 * The single source of truth for category colour and theme preference.
 *
 * Colours are returned as CSS variable references rather than hex, so a
 * value used in an inline style follows the light/dark tokens in
 * src/index.css with no JavaScript involved. Before this module the
 * same palette was written out three times — in styles.css, in the page
 * component arrays, and inline in chart components.
 */

export type CategoryKey =
  | "mainline"
  | "support"
  | "side"
  | "longterm"
  | "admin"
  | "entertainment"
  | "away"
  | "unobserved"
  | "pending";

/** Full-strength category colour (progress bars, swatches, chart fills). */
export function categoryColor(key: CategoryKey): string {
  return `hsl(var(--cat-${key}))`;
}

/** The same colour at `pct` percent opacity, for heatmaps and stacked bars. */
export function categoryColorAt(key: CategoryKey, pct: number): string {
  const clamped = Math.max(0, Math.min(100, pct));
  return `color-mix(in srgb, hsl(var(--cat-${key})) ${clamped}%, transparent)`;
}

export const CATEGORY_LABELS: Record<CategoryKey, string> = {
  mainline: "主线",
  support: "辅助",
  side: "支线",
  longterm: "长期",
  admin: "杂项",
  entertainment: "娱乐",
  away: "离开",
  unobserved: "未观测",
  pending: "待复核",
};

/**
 * Maps an engine role or dominant string onto a category key. The engine
 * uses two vocabularies — judgment roles (`core_research`, `side_project`)
 * and shorter report keys (`core`, `side`) — and both are accepted here.
 */
export function categoryOf(role: string): CategoryKey {
  switch (role) {
    case "mainline":
    case "core_research":
    case "core":
      return "mainline";
    case "research_support":
    case "support":
      return "support";
    case "side":
    case "side_project":
    case "custom":
      return "side";
    case "longterm":
      return "longterm";
    case "chore":
    case "admin":
      return "admin";
    case "distraction":
    case "entertainment":
      return "entertainment";
    case "break_away":
    case "away":
      return "away";
    case "pending_review":
    case "pending":
      return "pending";
    default:
      return "unobserved";
  }
}

/* ------------------------------------------------------------------ */
/* Theme preference                                                    */
/* ------------------------------------------------------------------ */

export type ThemePreference = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

/** Mirrors AppSettings.theme on the Rust side. */
export function normalizeThemePreference(raw: string | null | undefined): ThemePreference {
  return raw === "light" || raw === "dark" ? raw : "system";
}

export function resolveTheme(
  pref: ThemePreference,
  systemPrefersDark: boolean,
): ResolvedTheme {
  if (pref === "system") return systemPrefersDark ? "dark" : "light";
  return pref;
}

/**
 * The pre-paint script in index.html paints this inline so a dark window
 * never flashes light. It has to be written here too: the script's value
 * would otherwise outrank `--background` forever, and switching to dark
 * while it held a light value left the window looking light.
 */
const PREPAINT_BG: Record<ResolvedTheme, string> = {
  light: "hsl(36 50% 96%)",
  dark: "hsl(24 10% 10%)",
};

export function applyThemeClass(theme: ResolvedTheme, root: HTMLElement): void {
  root.classList.toggle("dark", theme === "dark");
  root.style.backgroundColor = PREPAINT_BG[theme];
}

/**
 * Last applied theme, cached so the pre-paint script in index.html can
 * resolve it without waiting for config.json. Without this, an explicit
 * light/dark override would flash the system default on every launch.
 */
export const THEME_CACHE_KEY = "gl-theme";

export function readCachedTheme(): ResolvedTheme | null {
  try {
    const raw = window.localStorage.getItem(THEME_CACHE_KEY);
    return raw === "light" || raw === "dark" ? raw : null;
  } catch {
    return null;
  }
}

export function writeCachedTheme(theme: ResolvedTheme): void {
  try {
    window.localStorage.setItem(THEME_CACHE_KEY, theme);
  } catch {
    /* storage disabled — the pre-paint script falls back to the system */
  }
}
