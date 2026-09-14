import { invoke as tauriInvoke } from "@tauri-apps/api/core";

/**
 * Tauri IPC, or the in-browser fixture table when `VITE_PREVIEW=1`.
 * The preview branch is dropped from production bundles.
 */
export async function invoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (import.meta.env.VITE_PREVIEW === "1") {
    const { previewInvoke } = await import("./preview/invoke");
    return previewInvoke<T>(cmd, args);
  }
  return tauriInvoke<T>(cmd, args);
}
