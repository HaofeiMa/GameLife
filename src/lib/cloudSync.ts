import type { SyncSettings, SyncStatus } from "./api";

/** Mirrors `SyncSettings::default()` in src-tauri/src/config.rs. */
export function defaultSyncSettings(): SyncSettings {
  return {
    enabled: false,
    target: "webdav",
    url: "",
    username: "",
    bucket: "",
    region: "auto",
    remotePath: "gamelife",
    intervalMinutes: 60,
    scope: "aggregate",
    keepSnapshots: 7,
    deviceLabel: "",
  };
}

/**
 * Mirrors `normalize_scope` on the Rust side: anything unrecognised narrows the
 * upload to `aggregate` rather than widening it to `samples`.
 */
export function normalizeScope(scope: string): "aggregate" | "samples" {
  return scope === "samples" ? "samples" : "aggregate";
}

export function scopeLabel(scope: string): string {
  return normalizeScope(scope) === "samples"
    ? "含原始采样（含窗口标题与路径）"
    : "仅判定与汇总（不含窗口标题）";
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatCloudStatus(status: SyncStatus): string {
  if (!status.enabled) return "未开启";
  if (status.lastError) return `上次失败：${status.lastError}`;
  if (status.lastAt == null) return "尚未同步";
  return `上次成功：${new Date(status.lastAt * 1000).toLocaleString()}`;
}

/** Falls back to the id prefix so two unlabelled devices are still tellable apart. */
export function deviceName(device: { label: string; deviceId: string }): string {
  const label = device.label.trim();
  return label || `设备 ${device.deviceId.slice(0, 6)}`;
}
