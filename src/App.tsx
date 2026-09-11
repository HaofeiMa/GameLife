import { useCallback, useEffect, useRef, useState } from "react";
import { Shop } from "./pages/Shop";
import { Settings } from "./pages/Settings";
import { Today } from "./pages/Today";
import { Week } from "./pages/Week";
import { getToday } from "./lib/api";
import {
  nextFeelNotices,
  noticeText,
  sessionJustEnded,
} from "./lib/feel";

type Tab = "today" | "week" | "shop" | "settings";

const TABS: { id: Tab; label: string; icon: string }[] = [
  { id: "today", label: "今日", icon: "今" },
  { id: "week", label: "本周", icon: "周" },
  { id: "shop", label: "商店", icon: "店" },
  { id: "settings", label: "设置", icon: "设" },
];

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
      <aside className="app-rail" aria-label="主导航">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            className={tab === t.id ? "active" : ""}
            title={t.label}
            aria-label={t.label}
            onClick={() => setTab(t.id)}
          >
            <span aria-hidden="true">{t.icon}</span>
            <span className="rail-label">{t.label}</span>
          </button>
        ))}
      </aside>
      <div className="app-body">
        <div className="toast-host" aria-live="polite">
          {toasts.map((t) => (
            <div key={t.id} className="toast">
              {t.text}
            </div>
          ))}
        </div>
        <main>
          {tab === "today" && <Today />}
          {tab === "week" && <Week />}
          {tab === "shop" && <Shop />}
          {tab === "settings" && <Settings />}
        </main>
      </div>
    </div>
  );
}
