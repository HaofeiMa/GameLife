import { Play, Square } from "lucide-react";
import { useEffect, useState } from "react";
import type { EndedEntertainmentView, EntertainmentView } from "../lib/api";
import { showEndedFromActive, trayEntertainmentMinutes } from "../lib/feel";
import { cn } from "../lib/utils";

export function EntertainmentBanner({
  active,
  ended,
}: {
  active: EntertainmentView | null;
  ended: EndedEntertainmentView | null;
}) {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));

  useEffect(() => {
    if (!active) return;
    const id = window.setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => window.clearInterval(id);
  }, [active]);

  const base =
    "flex items-center gap-2 rounded-lg border px-4 py-2.5 text-sm";

  if (active) {
    const remaining = active.endsAt - now;
    const endedText = showEndedFromActive(remaining, active.name);
    if (endedText) {
      return (
        <div className={cn(base, "border-border bg-muted text-muted-foreground")}>
          <Square className="size-4 shrink-0" aria-hidden />
          {endedText}
        </div>
      );
    }
    const mins = trayEntertainmentMinutes(remaining);
    if (mins == null) return null;
    return (
      <div className={cn(base, "border-primary/30 bg-primary/10 text-primary")}>
        <Play className="size-4 shrink-0" aria-hidden />
        <span className="font-medium">{active.name}</span>
        <span className="opacity-80">还剩 {mins}m</span>
      </div>
    );
  }

  if (ended) {
    return (
      <div className={cn(base, "border-border bg-muted text-muted-foreground")}>
        <Square className="size-4 shrink-0" aria-hidden />
        {ended.name} 已结束
      </div>
    );
  }

  return null;
}
