import { useEffect, useState } from "react";
import type {
  EndedEntertainmentView,
  EntertainmentView,
} from "../lib/api";
import {
  showEndedFromActive,
  trayEntertainmentMinutes,
} from "../lib/feel";

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
    const remaining = active.endsAt - now;
    const endedText = showEndedFromActive(remaining, active.name);
    if (endedText) {
      return (
        <div className="entertainment-banner ended">{endedText}</div>
      );
    }
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
