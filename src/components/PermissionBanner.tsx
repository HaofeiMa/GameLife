import { useEffect, useState } from "react";
import { getPermissionStatus, type PermissionStatus } from "../lib/api";

export function PermissionBanner() {
  const [perms, setPerms] = useState<PermissionStatus | null>(null);

  useEffect(() => {
    let cancelled = false;
    const refresh = () => {
      getPermissionStatus()
        .then((p) => {
          if (!cancelled) setPerms(p);
        })
        .catch(() => {
          if (!cancelled) setPerms(null);
        });
    };
    refresh();
    const id = setInterval(refresh, 3000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  if (!perms) return null;
  if (perms.accessibility && perms.screenRecording) return null;

  return (
    <div className="permission-banner" role="status">
      <strong>缺少系统权限</strong>
      <p className="muted">
        无观测能力时段将记为 <em>unobserved / missing</em>，不会记为 Away。
      </p>
      <ul>
        {!perms.accessibility && (
          <li>辅助功能：系统设置 → 隐私与安全性 → 辅助功能 → 允许 GameLife</li>
        )}
        {!perms.screenRecording && (
          <li>屏幕录制：系统设置 → 隐私与安全性 → 屏幕录制 → 允许 GameLife</li>
        )}
      </ul>
    </div>
  );
}
