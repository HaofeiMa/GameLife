import { useCallback, useEffect, useRef, useState } from "react";
import { Shop } from "./pages/Shop";
import { Settings } from "./pages/Settings";
import { Today } from "./pages/Today";
import { Timeline } from "./pages/Timeline";
import { Week } from "./pages/Week";
import { getToday } from "./lib/api";
import {
  nextFeelNotices,
  sessionJustEnded,
  type FeelNotice,
} from "./lib/feel";

type Tab = "today" | "timeline" | "week" | "shop" | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "today", label: "今日" },
  { id: "timeline", label: "时间轴" },
  { id: "week", label: "本周" },
  { id: "shop", label: "商店" },
  { id: "settings", label: "设置" },
];

function noticeText(notice: FeelNotice): string {
  switch (notice.type) {
    case "coins":
      return `+${notice.n} Coins`;
    case "xp":
      return `+${notice.n} XP`;
    case "chest":
      return "Chest";
    case "goldDay":
      return "Gold Day";
    case "earlyStart":
      return `Early start +${notice.coins}`;
    case "streak":
      return `连胜 ${notice.n}`;
    case "redeem":
      return "已兑换";
    case "entertainmentOver":
      return `${notice.name} 时间到`;
  }
}

export function App() {
  const [tab, setTab] = useState<Tab>("today");
  const [toasts, setToasts] = useState<{ id: number; text: string }[]>([]);
  const prevMaxTsRef = useRef<number | null>(null);
  const prevRemainingRef = useRef<number | null>(null);
  const toastIdRef = useRef(0);

  const addToast = useCallback((text: string) => {
    const id = ++toastIdRef.current;
    setToasts((prev) => [...prev, { id, text }]);
    window.setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 3000);
  }, []);

  useEffect(() => {
    async function poll() {
      try {
        const today = await getToday();
        const { notices, maxTs } = nextFeelNotices(
          prevMaxTsRef.current,
          today.ledgerTail,
        );
        prevMaxTsRef.current = maxTs;
        for (const notice of notices) {
          addToast(noticeText(notice));
        }

        const active = today.activeEntertainment;
        if (sessionJustEnded(prevRemainingRef.current, active)) {
          const name = today.endedEntertainment?.name ?? "娱乐";
          addToast(`${name} 时间到`);
        }
        prevRemainingRef.current = active?.remainingSecs ?? null;
      } catch (e) {
        console.error(e);
      }
    }

    poll();
    const id = window.setInterval(() => {
      if (document.visibilityState === "visible") {
        poll();
      }
    }, 5000);
    return () => window.clearInterval(id);
  }, [addToast]);

  return (
    <div className="app">
      <div className="toast-host" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className="toast">
            {t.text}
          </div>
        ))}
      </div>
      <header>
        <h1>GameLife</h1>
        <nav>
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              className={tab === t.id ? "active" : ""}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>
      <main>
        {tab === "today" && <Today />}
        {tab === "timeline" && <Timeline />}
        {tab === "week" && <Week />}
        {tab === "shop" && <Shop />}
        {tab === "settings" && <Settings />}
      </main>
    </div>
  );
}
