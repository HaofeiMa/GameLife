import { useEffect, useState } from "react";
import {
  getPermissionStatus,
  requestScreenRecording,
  type PermissionStatus,
} from "../lib/api";

export function PermissionBanner() {
  const [perms, setPerms] = useState<PermissionStatus | null>(null);
  const [busy, setBusy] = useState(false);

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

  const processName = perms.processName || "gamelife";
  const processPath = perms.processPath;

  async function handleRequest() {
    setBusy(true);
    try {
      await requestScreenRecording();
      const next = await getPermissionStatus();
      setPerms(next);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="permission-banner" role="status">
      <strong>缺少系统权限</strong>
      <p className="muted">
        无观测能力时段将记为 <em>unobserved / missing</em>，不会记为 Away。
      </p>
      <p className="muted">
        当前进程是 <code>{processName}</code>
        {processName !== "GameLife" && processName !== "GameLife.app"
          ? "（开发模式，不是 GameLife.app）"
          : ""}
        。系统设置里请允许这个进程，不要只允许 Cursor 或 node。
      </p>
      <ul>
        {!perms.accessibility && (
          <li>
            辅助功能：系统设置 → 隐私与安全性 → 辅助功能 → 允许{" "}
            <code>{processName}</code>
          </li>
        )}
        {!perms.screenRecording && (
          <li>
            屏幕录制：先点下方按钮弹出系统授权。若列表里没有，点 + 选择：
            {processPath ? (
              <>
                <br />
                <code>{processPath}</code>
              </>
            ) : (
              " 当前可执行文件"
            )}
          </li>
        )}
      </ul>
      {!perms.screenRecording && (
        <button type="button" disabled={busy} onClick={() => void handleRequest()}>
          {busy ? "请求中…" : "请求屏幕录制权限"}
        </button>
      )}
    </div>
  );
}
