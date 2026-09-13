import { useEffect, useState } from "react";
import { getSettings } from "../lib/api";
import { SETTINGS_CHANGED_EVENT } from "../lib/settingsEvents";
import {
  applyThemeClass,
  normalizeThemePreference,
  resolveTheme,
  writeCachedTheme,
  type ThemePreference,
} from "../lib/theme";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * Keeps `<html class="dark">` in step with the saved preference, and keeps
 * following the system appearance while the preference is "system".
 *
 * The pre-paint script in index.html has already resolved the cached theme;
 * this hook is what corrects it once config.json is readable.
 */
export function useTheme(): ThemePreference {
  const [preference, setPreference] = useState<ThemePreference>("system");

  useEffect(() => {
    const media = window.matchMedia(DARK_QUERY);

    function apply() {
      const resolved = resolveTheme(preference, media.matches);
      applyThemeClass(resolved, document.documentElement);
      writeCachedTheme(resolved);
    }

    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [preference]);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      try {
        const settings = await getSettings();
        if (!cancelled) setPreference(normalizeThemePreference(settings.theme));
      } catch (e) {
        console.error(e);
      }
    }

    void load();
    window.addEventListener(SETTINGS_CHANGED_EVENT, load);
    return () => {
      cancelled = true;
      window.removeEventListener(SETTINGS_CHANGED_EVENT, load);
    };
  }, []);

  return preference;
}
