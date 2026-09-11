import { useEffect, useState } from "react";
import type {
  EndedEntertainmentView,
  EntertainmentView,
} from "../lib/api";
import { trayEntertainmentMinutes } from "../lib/feel";

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
    const id = window.setInterval(
      () => setNow(Math.floor(Date.now() / 1000)),
      1000,
    );
    return () => window.clearInterval(id);
  }, [active]);

  if (active) {
    const remaining = Math.max(0, active.endsAt - now);
    const mins = trayEntertainmentMinutes(remaining);
    if (mins == null) return null;
    return (
      <div className="entertainment-banner active">
        {active.name} 还剩 {mins}m
      </div>
    );
  }

  if (ended) {
    return (
      <div className="entertainment-banner ended">{ended.name} 已结束</div>
    );
  }

  return null;
}
