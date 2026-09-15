import { MoreHorizontal } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { PageHeader } from "../components/PageHeader";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { Dialog } from "../components/ui/dialog";
import { Input } from "../components/ui/input";
import { Segmented } from "../components/ui/segmented";
import { SkeletonPanel } from "../components/ui/skeleton";
import { useTaskSplit } from "../hooks/useTaskSplit";
import {
  listTaskBoard,
  type TaskListView,
  type TaskView,
} from "../lib/api";
import { LIST_MAX, LIST_MIN } from "../lib/taskSplit";
import { listRoleLabel, taskTimeLabel } from "../lib/taskBoard";
import { categoryColor, type CategoryKey } from "../lib/theme";
import { cn } from "../lib/utils";

const SORT_KEY = "gl-task-sort";
const CAL_DAYS_KEY = "gl-task-cal-days";

type TaskSort = "time" | "title";
type CalDays = "3" | "7";

function readSort(): TaskSort {
  try {
    return window.localStorage.getItem(SORT_KEY) === "title" ? "title" : "time";
  } catch {
    return "time";
  }
}

function writeSort(sort: TaskSort): void {
  try {
    window.localStorage.setItem(SORT_KEY, sort);
  } catch {
    /* private mode / quota */
  }
}

function readCalDays(): CalDays {
  try {
    return window.localStorage.getItem(CAL_DAYS_KEY) === "3" ? "3" : "7";
  } catch {
    return "7";
  }
}

function writeCalDays(days: CalDays): void {
  try {
    window.localStorage.setItem(CAL_DAYS_KEY, days);
  } catch {
    /* private mode / quota */
  }
}

function roleDot(role: string): string {
  const key: CategoryKey =
    role === "mainline"
      ? "mainline"
      : role === "longterm"
        ? "longterm"
        : role === "chore"
          ? "admin"
          : "side";
  return categoryColor(key);
}

function orderTasks(tasks: TaskView[], sort: TaskSort): TaskView[] {
  const copy = tasks.slice();
  if (sort === "title") {
    copy.sort((a, b) => a.title.localeCompare(b.title, "zh"));
    return copy;
  }
  copy.sort((a, b) => {
    if (a.start == null && b.start == null) {
      return a.title.localeCompare(b.title, "zh");
    }
    if (a.start == null) return 1;
    if (b.start == null) return -1;
    return a.start - b.start || (a.end ?? 0) - (b.end ?? 0);
  });
  return copy;
}

export function Tasks() {
  const [board, setBoard] = useState<{
    lists: TaskListView[];
    tasks: TaskView[];
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sort, setSort] = useState<TaskSort>(readSort);
  const [calDays, setCalDays] = useState<CalDays>(readCalDays);
  const [moreOpen, setMoreOpen] = useState(false);
  const [showDone, setShowDone] = useState(false);
  const { listWidth, dragging, splitRef, handleProps } = useTaskSplit();

  const refresh = useCallback(async () => {
    try {
      const next = await listTaskBoard();
      setBoard(next);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const lists = useMemo(
    () => (board?.lists ?? []).slice().sort((a, b) => a.sort - b.sort),
    [board],
  );

  const tasksByList = useMemo(() => {
    const map = new Map<string, TaskView[]>();
    for (const list of lists) map.set(list.id, []);
    for (const task of board?.tasks ?? []) {
      if (!showDone && task.done) continue;
      const bucket = map.get(task.listId);
      if (bucket) bucket.push(task);
      else map.set(task.listId, [task]);
    }
    for (const [id, tasks] of map) {
      map.set(id, orderTasks(tasks, sort));
    }
    return map;
  }, [board, lists, showDone, sort]);

  const header = (
    <PageHeader
      title={<h1 className="text-[19px] font-bold tracking-[-0.02em]">任务</h1>}
      center={
        <div className="flex items-center gap-1.5">
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              const next = sort === "time" ? "title" : "time";
              setSort(next);
              writeSort(next);
            }}
          >
            排序
          </Button>
          <Button
            variant="outline"
            size="icon"
            className="text-ink-dim"
            aria-label="更多"
            onClick={() => setMoreOpen(true)}
          >
            <MoreHorizontal className="size-4" />
          </Button>
        </div>
      }
      actions={
        <Segmented
          aria-label="日历跨度"
          size="sm"
          value={calDays}
          onChange={(next) => {
            setCalDays(next);
            writeCalDays(next);
          }}
          options={[
            { value: "3", label: "3 天" },
            { value: "7", label: "7 天" },
          ]}
        />
      }
    />
  );

  if (error) {
    return (
      <>
        {header}
        <div className="flex min-h-0 flex-1 flex-col px-[22px] pb-4">
          <Card className="flex flex-col items-center gap-3 p-8 text-center">
            <p className="text-sm text-destructive">{error}</p>
            <Button variant="outline" size="sm" onClick={() => void refresh()}>
              重试
            </Button>
          </Card>
        </div>
        <MoreDialog
          open={moreOpen}
          showDone={showDone}
          onClose={() => setMoreOpen(false)}
          onToggleDone={() => {
            setShowDone((v) => !v);
            setMoreOpen(false);
          }}
        />
      </>
    );
  }

  if (!board) {
    return (
      <>
        {header}
        <div className="flex min-h-0 flex-1 flex-col px-[22px] pb-4">
          <SkeletonPanel rows={4} />
        </div>
      </>
    );
  }

  return (
    <>
      {header}
      <div className="flex min-h-0 flex-1 flex-col px-[22px] pb-4">
        <div
          ref={splitRef}
          className={cn("flex min-h-0 flex-1", dragging && "select-none")}
        >
          <Card
            className="flex min-h-0 flex-col overflow-hidden"
            style={{ width: listWidth }}
          >
            <div className="shrink-0 px-3.5 pt-3 pb-2">
              <Input
                readOnly
                placeholder="明天上午十点到十二点，写方法节 #主线"
                aria-label="添加任务"
              />
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
              {lists.map((list) => {
                const tasks = tasksByList.get(list.id) ?? [];
                const openCount = (board.tasks ?? []).filter(
                  (t) => t.listId === list.id && !t.done,
                ).length;
                return (
                  <section key={list.id} className="pt-1">
                    <div className="flex items-center gap-2 px-1.5 py-1.5">
                      <span
                        className="size-2 shrink-0 rounded-full"
                        style={{ background: roleDot(list.role) }}
                        title={listRoleLabel(list.role)}
                      />
                      <span className="min-w-0 flex-1 truncate text-[13px] font-semibold">
                        {list.name}
                      </span>
                      <span className="tabular-nums text-[11px] text-muted-foreground">
                        {openCount}
                      </span>
                    </div>
                    {tasks.map((task) => (
                      <div
                        key={task.id}
                        className="flex items-baseline gap-2 rounded-[9px] px-2 py-1 text-[12.5px]"
                      >
                        <span
                          className={cn(
                            "min-w-0 flex-1 truncate",
                            task.done && "text-muted-foreground line-through",
                          )}
                        >
                          {task.title}
                        </span>
                        <span className="shrink-0 text-[11px] tabular-nums text-muted-foreground">
                          {task.start != null && task.end != null
                            ? taskTimeLabel(task.start, task.end)
                            : "未排期"}
                        </span>
                      </div>
                    ))}
                  </section>
                );
              })}
            </div>
          </Card>
          <div
            role="separator"
            aria-orientation="vertical"
            aria-valuemin={LIST_MIN}
            aria-valuemax={LIST_MAX}
            aria-valuenow={listWidth}
            aria-label="调整分组栏宽度"
            tabIndex={0}
            {...handleProps}
            className={cn(
              "relative z-20 w-2 shrink-0 cursor-col-resize touch-none",
              "before:absolute before:inset-y-0 before:left-1/2 before:w-px before:-translate-x-1/2 before:transition-colors",
              dragging
                ? "before:bg-primary"
                : "before:bg-transparent hover:before:bg-primary/40 focus-visible:before:bg-primary",
            )}
          />
          <Card className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
            <p className="px-4 py-3 text-[12.5px] text-muted-foreground">
              日历（{calDays === "3" ? "3 天" : "7 天"}）
            </p>
          </Card>
        </div>
      </div>
      <MoreDialog
        open={moreOpen}
        showDone={showDone}
        onClose={() => setMoreOpen(false)}
        onToggleDone={() => {
          setShowDone((v) => !v);
          setMoreOpen(false);
        }}
      />
    </>
  );
}

function MoreDialog({
  open,
  showDone,
  onClose,
  onToggleDone,
}: {
  open: boolean;
  showDone: boolean;
  onClose: () => void;
  onToggleDone: () => void;
}) {
  return (
    <Dialog open={open} onClose={onClose} title="更多">
      <div className="flex flex-col gap-2">
        <Button variant="outline" size="sm" disabled>
          添加分组
        </Button>
        <Button variant="outline" size="sm" onClick={onToggleDone}>
          {showDone ? "隐藏已完成" : "显示已完成"}
        </Button>
      </div>
    </Dialog>
  );
}
