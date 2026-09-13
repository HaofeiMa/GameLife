export type RailTabId = "today" | "week" | "shop" | "settings";

export function railTabs(): { id: RailTabId; label: string }[] {
  return [
    { id: "today", label: "今日" },
    { id: "week", label: "统计" },
    { id: "shop", label: "商店" },
    { id: "settings", label: "设置" },
  ];
}
