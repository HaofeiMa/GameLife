import { Info } from "lucide-react";
import { useEffect, useState } from "react";
import { getObservationStatus, type ObservationStatus } from "../lib/api";

/**
 * Shown when the running platform — or, on Linux, the session — has no
 * observation backend. It is the difference between "no work today" and "this
 * build cannot see anything", and it is answered by the backend rather than
 * sniffed from the user agent: a Wayland session keeps an XWayland display
 * around and looks exactly like a working X11 session from the frontend.
 */
export function PlatformNotice() {
  const [status, setStatus] = useState<ObservationStatus | null>(null);

  useEffect(() => {
    let cancelled = false;
    getObservationStatus()
      .then((next) => {
        if (!cancelled) setStatus(next);
      })
      .catch(() => {
        if (!cancelled) setStatus(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Silent until the backend answers: a flash of "cannot observe" on a machine
  // that can would be worse than a moment of quiet.
  if (!status || status.supported) return null;

  return (
    <div
      role="status"
      className="rounded-lg border border-border bg-muted/40 px-4 py-3 text-sm"
    >
      <div className="flex items-center gap-2 font-medium">
        <Info className="size-4 shrink-0" aria-hidden />
        当前平台暂无观测能力
      </div>
      <p className="mt-1 text-xs leading-relaxed text-muted-foreground">
        {status.backend === "Wayland" ? (
          <>
            检测到 Wayland 会话。Wayland 不提供「前台是哪个窗口」的查询，本应用目前只支持
            Xorg 会话——登录时在齿轮里选「Ubuntu on Xorg」即可。
          </>
        ) : (
          <>
            读不到前台窗口（观测后端：{status.backend}）。采样照常运行，但无法分类，
            因此不会产生有效时间，也不会计入硬币与能量。
          </>
        )}
      </p>
    </div>
  );
}
