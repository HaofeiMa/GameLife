export function syncNowDisabled(enabled: boolean, connected: boolean): boolean {
  return !enabled || !connected;
}

export function roleForProject(
  roles: Record<string, string> | undefined,
  projectId: string,
): "ignore" | "mainline" | "side" | "longterm" | "chore" {
  const raw = roles?.[projectId];
  if (
    raw === "mainline" ||
    raw === "side" ||
    raw === "longterm" ||
    raw === "chore" ||
    raw === "ignore"
  ) {
    return raw;
  }
  return "ignore";
}

export function writeTargetHint(projectName: string | null): string | null {
  if (!projectName) return null;
  return `新建任务写入「${projectName}」`;
}

export const EMPTY_COLUMNS_COPY = "这个清单没有分组，任务按整份清单归类。";

const COLUMN_ROLES = ["ignore", "mainline", "side", "longterm", "chore"] as const;

export type ColumnRoleChoice =
  | "inherit"
  | (typeof COLUMN_ROLES)[number];

export function columnRoleChoice(
  roles: Record<string, string> | undefined,
  columnId: string,
): ColumnRoleChoice {
  const raw = roles?.[columnId];
  if (
    raw === "ignore" ||
    raw === "mainline" ||
    raw === "side" ||
    raw === "longterm" ||
    raw === "chore"
  ) {
    return raw;
  }
  return "inherit";
}

export function columnRolePatch(
  roles: Record<string, string> | undefined,
  columnId: string,
  choice: ColumnRoleChoice,
): Record<string, string> {
  const next = { ...(roles ?? {}) };
  if (choice === "inherit") {
    delete next[columnId];
  } else {
    next[columnId] = choice;
  }
  return next;
}

type TicktickRoleMaps = {
  ticktickProjectRoles: Record<string, string>;
  ticktickColumnRoles: Record<string, string>;
};

/** Copy disk role maps onto the open form. Other fields stay as the user left them. */
export function mergeTicktickRolesFromDisk<T extends TicktickRoleMaps>(
  form: T,
  disk: TicktickRoleMaps,
): T {
  return {
    ...form,
    ticktickProjectRoles: { ...disk.ticktickProjectRoles },
    ticktickColumnRoles: { ...disk.ticktickColumnRoles },
  };
}

export function sortedColumns<T extends { id: string; sortOrder: number }>(
  columns: T[],
): T[] {
  return [...columns].sort((a, b) => {
    if (a.sortOrder !== b.sortOrder) return a.sortOrder - b.sortOrder;
    if (a.id < b.id) return -1;
    if (a.id > b.id) return 1;
    return 0;
  });
}
