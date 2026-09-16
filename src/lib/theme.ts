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

/**
 * The mockup's stacked-bar fill: the category colour under a hand-picked
 * lighter top (`.bars .bstack i`). Only the three stackable categories have
 * one — everything else falls back to the flat colour.
 */
const BAR_TOPS: Partial<Record<CategoryKey, string>> = {
  mainline: "--bar-mainline",
  side: "--bar-side",
  admin: "--bar-admin",
};

export function categoryBarFill(key: CategoryKey): string {
  const top = BAR_TOPS[key];
  if (!top) return categoryColor(key);
  return `linear-gradient(180deg, hsl(var(${top})), ${categoryColor(key)})`;
}

/**
 * The shop's tile palette. The mockup paints `.gicon` from a per-item pastel
 * in its own SHELF data, so a shelf is a mix of tints rather than one colour
 * per kind; the position of a wish inside its shelf picks the entry.
 */
const GIFT_TONES: Record<"coin" | "xp", readonly string[]> = {
  coin: ["1", "2", "3", "4", "5"],
  xp: ["x1", "x2", "x3"],
};

export function giftTone(
  kind: "coin" | "xp",
  index: number,
): { background: string; color: string } {
  const keys = GIFT_TONES[kind];
  const key = keys[((index % keys.length) + keys.length) % keys.length]!;
  return {
    background: `hsl(var(--gift-${key}-bg))`,
    color: `hsl(var(--gift-${key}-ink))`,
  };
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
/* Color themes                                                        */
/* ------------------------------------------------------------------ */

export const COLOR_THEMES = [
  { id: "default", label: "默认" },
  { id: "nuanhuang", label: "暖黄" },
  { id: "qinglan", label: "晴蓝" },
  { id: "songshi", label: "松石" },
  { id: "miqing", label: "秘青" },
  { id: "jianjia", label: "蒹葭" },
  { id: "xinghuang", label: "杏黄" },
  { id: "taotian", label: "桃天" },
  { id: "mushanzi", label: "暮山紫" },
  { id: "chenxiang", label: "沉香" },
  { id: "zanglan", label: "藏蓝" },
] as const;

export type ColorTheme = (typeof COLOR_THEMES)[number]["id"];

const COLOR_THEME_IDS: ReadonlySet<string> = new Set(COLOR_THEMES.map((t) => t.id));

/** Mirrors AppSettings.colorTheme on the Rust side. */
export function normalizeColorTheme(raw: string | null | undefined): ColorTheme {
  return raw && COLOR_THEME_IDS.has(raw) ? (raw as ColorTheme) : "default";
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
 *
 * Non-default accents copy `--background` from src/index.css so the inline
 * colour matches the washed page instead of flashing parchment.
 */
const ACCENT_PREPAINT_BG: Record<ColorTheme, Record<ResolvedTheme, string>> = {
  default: { light: "hsl(220.0 2.9% 98.0%)", dark: "hsl(220.0 1.4% 10.0%)" },
  nuanhuang: { light: "hsl(36.0 50.0% 96.1%)", dark: "hsl(24.0 9.8% 10.0%)" },
  qinglan: { light: "hsl(214.0 50.0% 96.1%)", dark: "hsl(214.0 9.8% 10.0%)" },
  songshi: { light: "hsl(166.0 50.0% 96.1%)", dark: "hsl(166.0 9.8% 10.0%)" },
  miqing: { light: "hsl(188.0 50.0% 96.1%)", dark: "hsl(188.0 9.8% 10.0%)" },
  jianjia: { light: "hsl(98.0 36.0% 96.1%)", dark: "hsl(98.0 7.1% 10.0%)" },
  xinghuang: { light: "hsl(26.0 67.5% 94.7%)", dark: "hsl(26.0 13.2% 8.6%)" },
  taotian: { light: "hsl(348.0 50.0% 96.1%)", dark: "hsl(348.0 9.8% 10.0%)" },
  mushanzi: { light: "hsl(265.0 50.0% 96.1%)", dark: "hsl(265.0 9.8% 10.0%)" },
  chenxiang: { light: "hsl(22.0 46.0% 94.9%)", dark: "hsl(22.0 9.0% 8.8%)" },
  zanglan: { light: "hsl(222.0 54.0% 94.3%)", dark: "hsl(222.0 10.6% 8.2%)" },
};

export function prepaintBackground(theme: ResolvedTheme, accent: ColorTheme): string {
  return ACCENT_PREPAINT_BG[accent][theme];
}

export function applyThemeClass(
  theme: ResolvedTheme,
  root: HTMLElement,
  accent: string = "default",
): void {
  const color = normalizeColorTheme(accent);
  root.classList.toggle("dark", theme === "dark");
  root.setAttribute("data-accent", color);
  root.style.backgroundColor = prepaintBackground(theme, color);
}

/**
 * Last applied theme, cached so the pre-paint script in index.html can
 * resolve it without waiting for config.json. Without this, an explicit
 * light/dark override would flash the system default on every launch.
 */
export const THEME_CACHE_KEY = "gl-theme";
export const ACCENT_CACHE_KEY = "gl-accent";
export const PREPAINT_BG_CACHE_KEY = "gl-prepaint-bg";

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

export function writeCachedAccent(accent: ColorTheme): void {
  try {
    window.localStorage.setItem(ACCENT_CACHE_KEY, accent);
  } catch {
    /* storage disabled */
  }
}

export function writeCachedPrepaintBg(color: string): void {
  try {
    window.localStorage.setItem(PREPAINT_BG_CACHE_KEY, color);
  } catch {
    /* storage disabled */
  }
}
