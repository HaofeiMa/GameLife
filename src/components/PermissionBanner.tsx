import { AlertTriangle, Check, ExternalLink, ShieldAlert } from "lucide-react";
import { useEffect, useState } from "react";
import {
  getPermissionStatus,
  openPrivacySettings,
  requestScreenRecording,
  type PermissionStatus,
} from "../lib/api";
import { permissionRows } from "../lib/permissions";
import { Button } from "./ui/button";
import { Card } from "./ui/card";
import { cn } from "../lib/utils";

function usePermissionPoll(): [
  PermissionStatus | null,
  (next: PermissionStatus) => void,
] {
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

  return [perms, setPerms];
}

function processNote(perms: PermissionStatus) {
  const name = perms.processName || "gamelife";
  const devMode = name !== "GameLife" && name !== "GameLife.app";
  return (
    <>
      当前进程是 <code className="rounded bg-muted px-1 font-mono text-[13px]">{name}</code>
      {devMode ? "（开发模式，不是 GameLife.app）" : ""}
      。系统设置里请允许这个进程，不要只允许 Cursor 或 node。
    </>
  );
}

/** Inline warning shown at the top of 今日 when a permission is missing. */
export function PermissionBanner() {
  const [perms, setPerms] = usePermissionPoll();
  const [busy, setBusy] = useState(false);

  if (!perms) return null;
  if (perms.accessibility && perms.screenRecording) return null;

  const name = perms.processName || "gamelife";

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
    <div
      role="status"
      className="rounded-lg border border-warning/30 bg-warning/10 px-4 py-3 text-sm text-warning"
    >
      <div className="flex items-center gap-2 font-medium">
        <AlertTriangle className="size-4 shrink-0" aria-hidden />
        缺少系统权限
      </div>
      <p className="mt-1 text-xs leading-relaxed opacity-90">
        无观测能力时段将记为 <em>unobserved / missing</em>，不会记为 Away。
      </p>
      <p className="mt-2 text-xs leading-relaxed opacity-90">{processNote(perms)}</p>
      <ul className="mt-2 space-y-1 text-xs leading-relaxed">
        {!perms.accessibility && (
          <li>
            辅助功能：系统设置 → 隐私与安全性 → 辅助功能 → 允许{" "}
            <code className="rounded bg-warning/20 px-1 font-mono text-[13px]">{name}</code>
          </li>
        )}
        {!perms.screenRecording && (
          <li>
            屏幕录制：先点下方按钮弹出系统授权。若列表里没有，点 + 选择：
            {perms.processPath ? (
              <code className="mt-0.5 block break-all rounded bg-warning/20 px-1 font-mono text-[13px]">
                {perms.processPath}
              </code>
            ) : (
              " 当前可执行文件"
            )}
          </li>
        )}
      </ul>
      {!perms.screenRecording && (
        <Button
          variant="outline"
          size="sm"
          disabled={busy}
          onClick={() => void handleRequest()}
          className="mt-3 border-warning/40 text-warning hover:bg-warning/15 hover:text-warning"
        >
          {busy ? "请求中…" : "请求屏幕录制权限"}
        </Button>
      )}
    </div>
  );
}

/** Full permission report, shown on 设置 → 权限. */
export function PermissionPanel() {
  const [perms, setPerms] = usePermissionPoll();
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  if (!perms) {
    return (
      <Card className="px-5 py-4 text-sm text-muted-foreground">正在检查权限…</Card>
    );
  }

  const rows = permissionRows(perms);
  const allOk = perms.accessibility && perms.screenRecording;

  async function handleRequest() {
    setBusy(true);
    try {
      await requestScreenRecording();
      setPerms(await getPermissionStatus());
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className="overflow-hidden">
      <div className="border-b px-5 py-3">
        <h2 className="flex items-center gap-2 text-sm font-medium">
          <ShieldAlert className="size-4 text-muted-foreground" aria-hidden />
          权限检查
        </h2>
      </div>
      <div className="space-y-3 px-5 py-4">
        <p className="text-xs leading-relaxed text-muted-foreground">
          {processNote(perms)}
        </p>
        {perms.processPath && (
          <p className="break-all text-xs leading-relaxed text-muted-foreground">
            路径：
            <code className="rounded bg-muted px-1 font-mono text-[13px]">
              {perms.processPath}
            </code>
          </p>
        )}
        {allOk ? (
          <p className="flex items-center gap-2 text-xs text-success">
            <Check className="size-3.5" aria-hidden />
            辅助功能和屏幕录制都已允许。
          </p>
        ) : (
          <p className="text-xs leading-relaxed text-muted-foreground">
            无观测能力时段将记为 <em>unobserved / missing</em>，不会记为 Away。
          </p>
        )}

        <ul className="space-y-2">
          {rows.map((row) => (
            <li
              key={row.id}
              className={cn(
                "flex items-center justify-between gap-3 rounded-lg border px-3 py-2.5",
                row.granted ? "border-success/30 bg-success/5" : "border-warning/30 bg-warning/5",
              )}
            >
              <div className="min-w-0 space-y-0.5">
                <p
                  className={cn(
                    "flex items-center gap-1.5 text-xs font-medium",
                    row.granted ? "text-success" : "text-warning",
                  )}
                >
                  {row.granted ? (
                    <Check className="size-3.5 shrink-0" aria-hidden />
                  ) : (
                    <AlertTriangle className="size-3.5 shrink-0" aria-hidden />
                  )}
                  {row.granted ? "已允许" : "未允许"} · {row.label}
                </p>
                <p className="text-[13px] leading-relaxed text-muted-foreground">
                  {row.hint}
                </p>
              </div>
              <Button
                variant="outline"
                size="sm"
                disabled={busy}
                className="shrink-0"
                onClick={() => {
                  setBusy(true);
                  setMsg(null);
                  void openPrivacySettings(row.id)
                    .then(() => getPermissionStatus().then(setPerms))
                    .catch((e) => setMsg(String(e)))
                    .finally(() => setBusy(false));
                }}
              >
                <ExternalLink className="size-3.5" aria-hidden />
                打开系统设置
              </Button>
            </li>
          ))}
        </ul>

        {!perms.screenRecording && (
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onClick={() => void handleRequest()}
          >
            {busy ? "请求中…" : "请求屏幕录制权限"}
          </Button>
        )}

        <p className="text-[13px] leading-relaxed text-muted-foreground">
          Chrome / Safari / Arc 的当前标签 URL 需要「自动化」权限；拒绝则 URL 为空，不影响采样与截图。
        </p>
        {msg && (
          <p className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {msg}
          </p>
        )}
      </div>
    </Card>
  );
}
