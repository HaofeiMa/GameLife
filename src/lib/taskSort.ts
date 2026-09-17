import type { TaskView } from "./api";

export type TaskSort = "time" | "title";

export function hasSchedule(task: { start: number | null; end: number | null }): boolean {
  return task.start != null && task.end != null;
}

export function sortTasks(tasks: TaskView[], sort: TaskSort): TaskView[] {
  const copy = tasks.slice();
  if (sort === "title") {
    copy.sort((a, b) => a.title.localeCompare(b.title, "zh"));
    return copy;
  }
  copy.sort((a, b) => {
    const aOn = hasSchedule(a);
    const bOn = hasSchedule(b);
    if (!aOn && !bOn) return a.sort - b.sort || a.id.localeCompare(b.id);
    if (!aOn) return 1;
    if (!bOn) return -1;
    return (
      (a.start ?? 0) - (b.start ?? 0) ||
      (a.end ?? 0) - (b.end ?? 0) ||
      a.sort - b.sort ||
      a.id.localeCompare(b.id)
    );
  });
  return copy;
}
