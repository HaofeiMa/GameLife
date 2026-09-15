import type { TaskView } from "./api";

export type TaskSort = "time" | "title";

export function sortTasks(tasks: TaskView[], sort: TaskSort): TaskView[] {
  const copy = tasks.slice();
  if (sort === "title") {
    copy.sort((a, b) => a.title.localeCompare(b.title, "zh"));
    return copy;
  }
  copy.sort((a, b) => {
    const as = a.start;
    const bs = b.start;
    if (as == null && bs == null) return a.title.localeCompare(b.title, "zh");
    if (as == null) return 1;
    if (bs == null) return -1;
    return as - bs || (a.end ?? 0) - (b.end ?? 0);
  });
  return copy;
}
