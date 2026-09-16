import { Coins, Target, Zap } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import appIcon from "../src-tauri/icons/128x128@2x.png";
import { BRAND_ROW, TRAFFIC_LIGHT_STRIP } from "./components/PageHeader";
import { Toaster, type ToastItem } from "./components/ui/toaster";
import { Progress } from "./components/ui/progress";
import {
  SIDEBAR_COLLAPSED,
  useSidebarWidth,
} from "./hooks/useSidebarWidth";
import { useTheme } from "./hooks/useTheme";
import { getSettings, getToday, type TodayView } from "./lib/api";
import { CAL_DAY_KEY } from "./lib/calendar";
import { nextFeelNotices, noticeText, sessionJustEnded } from "./lib/feel";
import { railTabs, type RailTabId } from "./lib/rail";
import { SETTINGS_CHANGED_EVENT } from "./lib/settingsEvents";
import { cn } from "./lib/utils";
import { Shop } from "./pages/Shop";
import { Settings } from "./pages/Settings";
import { Tasks } from "./pages/Tasks";
import { Today } from "./pages/Today";
import { Stats } from "./pages/Stats";

const GOAL_SECONDS = 28800;

/** Icon-over-number stack shown when the sidebar is collapsed. */
function CollapsedStat({
  icon,
  value,
  title,
}: {
  icon: React.ReactNode;
  value: React.ReactNode;
  title: string;
}) {
  return (
    <div
      title={title}
      className="flex w-full flex-col items-center gap-0.5 rounded-lg px-1 py-1.5 transition-colors hover:bg-background/60"
    >
      {icon}
      <span className="text-[11px] font-medium leading-none tabular-nums">
        {value}
      </span>
    </div>
  );
}

function initialTab(): RailTabId {
  if (import.meta.env.VITE_PREVIEW === "1") {
    const tab = new URLSearchParams(window.location.search).get("tab");
    if (
      tab === "today" ||
      tab === "tasks" ||
      tab === "week" ||
      tab === "shop" ||
      tab === "settings"
    ) {
      return tab;
    }
  }
  return "today";
}

export function App() {
  const [tab, setTab] = useState<RailTabId>(initialTab);
  const [showRailLabels, setShowRailLabels] = useState(true);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const [today, setToday] = useState<TodayView | null>(null);
  const prevMaxTsRef = useRef<number | null>(null);
  const prevRemainingRef = useRef<number | null>(null);
  const toastIdRef = useRef(0);

  useTheme();

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
    window.addEventListener(SETTINGS_CHANGED_EVENT, refreshRailLabels);
    return () => window.removeEventListener(SETTINGS_CHANGED_EVENT, refreshRailLabels);
  }, [tab]);

  // Drives both the toast pipeline and the sidebar's today summary. A
  // failure here must never take a page down, so everything is caught.
  useEffect(() => {
    async function poll() {
      try {
        const view = await getToday();
        setToday(view);

        const { notices, maxTs } = nextFeelNotices(
          prevMaxTsRef.current,
          view.ledgerTail,
        );
        prevMaxTsRef.current = maxTs;
        for (const notice of notices) {
          addToast(noticeText(notice));
        }

        const active = view.activeEntertainment;
        if (sessionJustEnded(prevRemainingRef.current, active)) {
          const name = view.endedEntertainment?.name ?? "娱乐";
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

  const expanded = showRailLabels;
  const { width: sidebarWidth, dragging, handleProps } = useSidebarWidth();

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      <aside
        aria-label="主导航"
        style={{
          width: expanded ? sidebarWidth : SIDEBAR_COLLAPSED,
          paddingTop: TRAFFIC_LIGHT_STRIP,
        }}
        className={cn(
          "relative flex shrink-0 flex-col border-r border-border bg-rail px-3.5 pb-4",
          !dragging && "transition-[width] duration-200",
        )}
      >
        {/* Clears the traffic lights, then the design's own 54px brand row. */}
        <div
          data-tauri-drag-region="deep"
          style={{ height: BRAND_ROW }}
          className={cn(
            "flex shrink-0 items-center gap-2.5",
            !expanded && "justify-center",
          )}
        >
          <img
            src={appIcon}
            alt=""
            className="size-7 shrink-0 rounded-[9px] shadow-[0_3px_8px_-2px_rgba(224,144,47,0.55)]"
            draggable={false}
          />
          {expanded && (
            <div className="min-w-0">
              <div className="truncate text-[16.5px] font-semibold tracking-[-0.01em]">
                GameLife
              </div>
              <div className="mt-px truncate text-[11.5px] text-muted-foreground">
                今天也在存档
              </div>
            </div>
          )}
        </div>

        <nav className="mt-2.5 flex flex-col gap-[3px]">
          {railTabs().map((item) => {
            const Icon = item.icon;
            const active = item.id === tab;
            return (
              <button
                key={item.id}
                type="button"
                onClick={() => setTab(item.id)}
                aria-label={item.label}
                aria-current={active ? "page" : undefined}
                title={item.label}
                className={cn(
                  "relative flex h-9 items-center gap-2.5 rounded-[11px] text-[15.5px] transition-all duration-200",
                  expanded ? "justify-start px-3" : "justify-center px-0",
                  active
                    ? "bg-card font-semibold text-foreground shadow-[0_2px_8px_-3px_rgba(120,95,60,0.28)]"
                    : "text-ink-dim hover:bg-card/60 hover:text-foreground",
                )}
              >
                <Icon className="size-[17px] shrink-0" aria-hidden />
                {expanded && <span className="truncate">{item.label}</span>}
              </button>
            );
          })}
        </nav>

        {!expanded && today && (
          <div className="mt-auto flex flex-col items-center gap-2 px-1.5 pb-4">
            <CollapsedStat
              icon={<Coins className="size-4 text-warning" aria-hidden />}
              value={today.coinBalance}
              title={`硬币 ${today.coinBalance}`}
            />
            <CollapsedStat
              icon={<Zap className="size-4 text-primary" aria-hidden />}
              value={today.xpToday}
              title={`能量 ${today.xpToday}`}
            />
            <CollapsedStat
              icon={<Target className="size-4 text-muted-foreground" aria-hidden />}
              value={today.creditedLabel}
              title={`今日主线 ${today.creditedLabel}`}
            />
          </div>
        )}

        {expanded && today && (
          <div className="mt-auto rounded-2xl bg-card px-3.5 py-[13px] shadow-[0_4px_14px_-10px_rgba(120,95,60,0.6)]">
            <div className="text-[13px] text-muted-foreground">今日主线</div>
            <div className="mt-[3px] mb-2 text-[25px] font-bold leading-none tracking-[-0.03em] tabular-nums">
              {today.creditedLabel}
              <small className="ml-1.5 text-[13.5px] font-medium text-muted-foreground">
                / 8h
              </small>
            </div>
            <Progress
              value={(today.creditedSeconds / GOAL_SECONDS) * 100}
              aria-label="今日主线进度"
              className="h-[7px] rounded bg-track-rail"
              barClassName="rounded surface-track"
            />
            <div className="mt-[9px] flex justify-between text-[13.5px] tabular-nums text-muted-foreground">
              <span>◉ {today.coinBalance}</span>
              <span>▲ {today.streak} 天</span>
            </div>
          </div>
        )}
        {expanded && (
          <div
            role="separator"
            aria-orientation="vertical"
            aria-label="调整侧栏宽度"
            tabIndex={0}
            {...handleProps}
            className={cn(
              // Overlay the sidebar's border-r so the header's border-b meets
              // it. A flex sibling here used to insert an 8px gap.
              "absolute inset-y-0 right-0 z-20 w-2 translate-x-1/2 cursor-col-resize touch-none",
              "before:absolute before:inset-y-0 before:left-1/2 before:w-px before:-translate-x-1/2 before:transition-colors",
              dragging
                ? "before:bg-primary"
                : "before:bg-transparent hover:before:bg-primary/40 focus-visible:before:bg-primary",
            )}
          />
        )}
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        {tab === "today" && <Today />}
        {tab === "tasks" && <Tasks />}
          {tab === "week" && (
            <Stats
              onPickDay={(day) => {
                try {
                  window.localStorage.setItem(CAL_DAY_KEY, day);
                } catch {
                  /* private mode / quota */
                }
                setTab("today");
              }}
            />
          )}
          {tab === "shop" && <Shop />}
          {tab === "settings" && <Settings />}
      </main>

      <Toaster toasts={toasts} />
    </div>
  );
}
