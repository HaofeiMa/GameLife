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
