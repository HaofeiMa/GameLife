import type { PermissionStatus } from "./api";

export function privacySettingsUrl(kind: "accessibility" | "screen"): string {
  return kind === "accessibility"
    ? "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
    : "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
}

export function permissionRows(status: PermissionStatus): {
  id: "accessibility" | "screen";
  label: string;
  granted: boolean;
  hint: string;
}[] {
  const processName = status.processName || "gamelife";
  return [
    {
      id: "accessibility",
      label: "辅助功能",
      granted: status.accessibility,
      hint: `系统设置 → 隐私与安全性 → 辅助功能 → 允许 ${processName}`,
    },
    {
      id: "screen",
      label: "屏幕录制",
      granted: status.screenRecording,
      hint: `系统设置 → 隐私与安全性 → 屏幕录制 → 允许 ${processName}`,
    },
  ];
}
