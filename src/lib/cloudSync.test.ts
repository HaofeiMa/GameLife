import { describe, expect, it } from "vitest";
import {
  defaultSyncSettings,
  deviceName,
  formatBytes,
  formatCloudStatus,
  formatLastSeen,
  normalizeScope,
  normalizeSettleGraceHours,
  scopeLabel,
} from "./cloudSync";
import type { SyncStatus } from "./api";

function status(overrides: Partial<SyncStatus> = {}): SyncStatus {
  return {
    enabled: true,
    target: "webdav",
    url: "https://dav.example.com",
    scope: "aggregate",
    intervalMinutes: 60,
    lastAt: null,
    lastOk: false,
    lastError: "",
    snapshotBytes: 0,
    deviceId: "abcdef123456",
    devices: [],
    ...overrides,
  };
}

describe("normalizeScope", () => {
  it("keeps samples and narrows everything else to aggregate", () => {
    expect(normalizeScope("samples")).toBe("samples");
    expect(normalizeScope("aggregate")).toBe("aggregate");
    expect(normalizeScope("everything")).toBe("aggregate");
    expect(normalizeScope("")).toBe("aggregate");
  });
});

describe("scopeLabel", () => {
  it("warns about window titles only for the samples scope", () => {
    expect(scopeLabel("aggregate")).toContain("不含窗口标题");
    expect(scopeLabel("samples")).toContain("含窗口标题");
  });
});

describe("defaultSyncSettings", () => {
  it("starts disabled, on webdav, with the aggregate scope", () => {
    const s = defaultSyncSettings();
    expect(s.enabled).toBe(false);
    expect(s.target).toBe("webdav");
    expect(s.scope).toBe("aggregate");
    expect(s.intervalMinutes).toBe(60);
    expect(s.keepSnapshots).toBe(7);
    expect(s.remotePath).toBe("gamelife");
    expect(s.region).toBe("auto");
    expect(s.settleGraceHours).toBe(36);
  });
});

describe("normalizeSettleGraceHours", () => {
  it("keeps a positive value and falls back to 36 otherwise", () => {
    expect(normalizeSettleGraceHours(12)).toBe(12);
    expect(normalizeSettleGraceHours(0)).toBe(36);
    expect(normalizeSettleGraceHours(-1)).toBe(36);
    expect(normalizeSettleGraceHours(Number.NaN)).toBe(36);
  });
});

describe("formatLastSeen", () => {
  it("renders a placeholder when the device has never reported", () => {
    expect(formatLastSeen(0)).toBe("从未上报");
    expect(formatLastSeen(-1)).toBe("从未上报");
  });

  it("formats a unix timestamp in the local locale", () => {
    expect(formatLastSeen(1_789_000_000)).toContain("2026");
  });
});

describe("formatBytes", () => {
  it("renders a placeholder for nothing uploaded", () => {
    expect(formatBytes(0)).toBe("—");
    expect(formatBytes(-1)).toBe("—");
    expect(formatBytes(Number.NaN)).toBe("—");
  });

  it("scales through B, KB and MB", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(3 * 1024 * 1024)).toBe("3.0 MB");
  });
});

describe("formatCloudStatus", () => {
  it("reports the disabled state before anything else", () => {
    expect(formatCloudStatus(status({ enabled: false, lastError: "boom" }))).toBe(
      "未开启",
    );
  });

  it("reports a failure ahead of a stale success", () => {
    expect(
      formatCloudStatus(status({ lastAt: 1_789_000_000, lastError: "远端返回 503" })),
    ).toBe("上次失败：远端返回 503");
  });

  it("reports never-synced and then the timestamp", () => {
    expect(formatCloudStatus(status())).toBe("尚未同步");
    expect(formatCloudStatus(status({ lastAt: 1_789_000_000 }))).toContain(
      "上次成功：",
    );
  });
});

describe("deviceName", () => {
  it("prefers the label and falls back to the id prefix", () => {
    expect(deviceName({ label: "书房 Mac", deviceId: "abcdef123456" })).toBe(
      "书房 Mac",
    );
    expect(deviceName({ label: "   ", deviceId: "abcdef123456" })).toBe("设备 abcdef");
  });
});
