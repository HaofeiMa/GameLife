/**
 * The webview reports the host OS only through its user agent, and the app
 * ships no `plugin-os`, so this is the check the UI has to work with.
 */
export function isMacOsUserAgent(ua: string | null | undefined): boolean {
  return typeof ua === "string" && ua.includes("Macintosh");
}

export const IS_MACOS = isMacOsUserAgent(
  typeof navigator === "undefined" ? "" : navigator.userAgent,
);

/** macOS is the only platform that reserves room for traffic lights. */
