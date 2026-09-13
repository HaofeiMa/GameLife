import {
  BarChart3,
  CalendarDays,
  Settings,
  Store,
  type LucideIcon,
} from "lucide-react";

export type RailTabId = "today" | "week" | "shop" | "settings";

export interface RailTab {
  id: RailTabId;
  label: string;
  icon: LucideIcon;
}

export function railTabs(): RailTab[] {
  return [
    { id: "today", label: "今日", icon: CalendarDays },
    { id: "week", label: "统计", icon: BarChart3 },
    { id: "shop", label: "商店", icon: Store },
    { id: "settings", label: "设置", icon: Settings },
  ];
}
