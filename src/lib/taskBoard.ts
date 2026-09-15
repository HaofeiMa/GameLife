export function listRoleLabel(role: string): string {
  switch (role) {
    case "mainline":
      return "主线";
    case "side":
      return "支线";
    case "longterm":
      return "长期";
    case "chore":
      return "杂项";
    default:
      return "支线";
  }
}

export function taskTimeLabel(start: number, end: number): string {
  const fmt = (ts: number) =>
    new Date(ts * 1000).toLocaleTimeString("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  return `${fmt(start)}–${fmt(end)}`;
}
