import { useCallback, useEffect, useRef, useState } from "react";
import { Shop } from "./pages/Shop";
import { Settings } from "./pages/Settings";
import { Today } from "./pages/Today";
import { Week } from "./pages/Week";
import { getSettings, getToday } from "./lib/api";
import {
  nextFeelNotices,
  noticeText,
  sessionJustEnded,
} from "./lib/feel";
import { railTabs, type RailTabId } from "./lib/rail";

function RailIcon({ id }: { id: RailTabId }) {
  const common = {
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
  };

  switch (id) {
    case "today":
      return (
        <svg {...common}>
          <rect x="3" y="4" width="18" height="18" rx="2" />
          <line x1="16" y1="2" x2="16" y2="6" />
          <line x1="8" y1="2" x2="8" y2="6" />
          <line x1="3" y1="10" x2="21" y2="10" />
        </svg>
      );
    case "week":
      return (
        <svg {...common}>
          <line x1="18" y1="20" x2="18" y2="10" />
          <line x1="12" y1="20" x2="12" y2="4" />
          <line x1="6" y1="20" x2="6" y2="14" />
        </svg>
      );
    case "shop":
      return (
        <svg {...common}>
          <path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4Z" />
          <line x1="3" y1="6" x2="21" y2="6" />
          <path d="M16 10a4 4 0 0 1-8 0" />
        </svg>
      );
    case "settings":
      return (
        <svg {...common}>
          <circle cx="12" cy="12" r="3" />
          <path
            d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42"
          />
        </svg>
      );
  }
}

export function App() {
  const [tab, setTab] = useState<RailTabId>("today");
  const [showRailLabels, setShowRailLabels] = useState(true);
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
    async function refreshRailLabels() {
      try {
        const settings = await getSettings();
        setShowRailLabels(settings.showRailLabels);
      } catch (e) {
        console.error(e);
      }
    }
    refreshRailLabels();
  }, [tab]);

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
        {railTabs().map((t) => (
          <button
            key={t.id}
            type="button"
            className={tab === t.id ? "active" : ""}
            title={t.label}
            aria-label={t.label}
            onClick={() => setTab(t.id)}
          >
            <span className="rail-icon">
              <RailIcon id={t.id} />
            </span>
            {showRailLabels && (
              <span className="rail-label">{t.label}</span>
            )}
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
