import { useState } from "react";
import { Shop } from "./pages/Shop";
import { Settings } from "./pages/Settings";
import { Today } from "./pages/Today";
import { Timeline } from "./pages/Timeline";
import { Week } from "./pages/Week";

type Tab = "today" | "timeline" | "week" | "shop" | "settings";

const TABS: { id: Tab; label: string }[] = [
  { id: "today", label: "今日" },
  { id: "timeline", label: "时间轴" },
  { id: "week", label: "本周" },
  { id: "shop", label: "商店" },
  { id: "settings", label: "设置" },
];

export function App() {
  const [tab, setTab] = useState<Tab>("today");

  return (
    <div className="app">
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
