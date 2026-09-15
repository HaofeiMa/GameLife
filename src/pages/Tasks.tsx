import { ChevronDown, ChevronRight, MoreHorizontal } from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
} from "react";
import { PageHeader } from "../components/PageHeader";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { ContextMenu, ContextMenuItem } from "../components/ui/context-menu";
import { Dialog } from "../components/ui/dialog";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { Segmented } from "../components/ui/segmented";
import { Select } from "../components/ui/select";
import { SkeletonPanel } from "../components/ui/skeleton";
import { Toaster, type ToastItem } from "../components/ui/toaster";
import { useTaskSplit } from "../hooks/useTaskSplit";
import {
  createList,
  deleteList,
  deleteTask,
  listTaskBoard,
  moveTask,
  parseTaskLine,
  renameList,
  rescheduleTask,
  toggleTaskDone,
  upsertTask,
  type ParsedTaskView,
  type TaskListView,
  type TaskView,
} from "../lib/api";
import { dayStartUnix } from "../lib/calendar";
import {
  COLLAPSED_KEY,
  isPresetListId,
  listRoleLabel,
  localDayOf,
  localTimeOf,
  parseCollapsed,
  shiftRangeToDay,
  taskCommandError,
  taskTimeLabel,
  toggleCollapsed,
  unixAt,
} from "../lib/taskBoard";
import { LIST_MAX, LIST_MIN } from "../lib/taskSplit";
import { sortTasks, type TaskSort } from "../lib/taskSort";
import { categoryColor, type CategoryKey } from "../lib/theme";
import { cn } from "../lib/utils";

const SORT_KEY = "gl-task-sort";
const CAL_DAYS_KEY = "gl-task-cal-days";

type CalDays = "3" | "7";

type MenuState =
  | { kind: "task"; task: TaskView; x: number; y: number }
  | { kind: "list"; list: TaskListView; x: number; y: number };

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

function readCollapsed(): string[] {
  try {
    return parseCollapsed(window.localStorage.getItem(COLLAPSED_KEY));
  } catch {
    return [];
  }
}

function writeCollapsed(ids: string[]): void {
  try {
    window.localStorage.setItem(COLLAPSED_KEY, JSON.stringify(ids));
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

function todayIso(): string {
  const d = new Date();
  const mm = String(d.getMonth() + 1).padStart(2, "0");
  const dd = String(d.getDate()).padStart(2, "0");
  return `${d.getFullYear()}-${mm}-${dd}`;
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
  const [collapsed, setCollapsed] = useState<string[]>(readCollapsed);
  const [line, setLine] = useState("");
  const [parsed, setParsed] = useState<ParsedTaskView | null>(null);
  const [focusedListId, setFocusedListId] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [dateTask, setDateTask] = useState<TaskView | null>(null);
  const [dateDay, setDateDay] = useState(todayIso());
  const [dateStart, setDateStart] = useState("");
  const [dateEnd, setDateEnd] = useState("");
  const [abandonTask, setAbandonTask] = useState<TaskView | null>(null);
  const [renameTarget, setRenameTarget] = useState<TaskListView | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<TaskListView | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [addName, setAddName] = useState("");
  const [addRole, setAddRole] = useState("mainline");
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const toastIdRef = useRef(0);
  const parseGen = useRef(0);
  const { listWidth, dragging, splitRef, handleProps } = useTaskSplit();

  const addToast = useCallback((text: string) => {
    const id = ++toastIdRef.current;
    setToasts((prev) => [...prev, { id, text }]);
    window.setTimeout(() => {
      setToasts((prev) => prev.filter((item) => item.id !== id));
    }, 3000);
  }, []);

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

  const currentListId =
    (focusedListId && lists.some((list) => list.id === focusedListId)
      ? focusedListId
      : null) ??
    lists.find((list) => list.role === "mainline")?.id ??
    lists[0]?.id ??
    null;

  const runParse = useCallback(
    async (value: string, listId: string | null) => {
      if (!value.trim()) {
        setParsed(null);
        return;
      }
      const gen = ++parseGen.current;
      try {
        const next = await parseTaskLine(value, listId);
        if (gen === parseGen.current) setParsed(next);
      } catch {
        /* live preview is best-effort */
      }
    },
    [],
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
      map.set(id, sortTasks(tasks, sort));
    }
    return map;
  }, [board, lists, showDone, sort]);

  async function run(action: () => Promise<void>) {
    try {
      await action();
      await refresh();
    } catch (e) {
      addToast(taskCommandError(e));
    }
  }

  async function submitLine() {
    const title = (parsed?.title ?? line).trim();
    if (!title) {
      addToast("标题不能为空。");
      return;
    }
    const listId = parsed?.listId || currentListId;
    if (!listId) {
      addToast("没有可用的分组。");
      return;
    }
    const scheduled = Boolean(parsed?.parseOk && parsed.start != null && parsed.end != null);
    try {
      await upsertTask({
        id: crypto.randomUUID(),
        listId,
        title,
        done: false,
        start: scheduled ? parsed!.start : null,
        end: scheduled ? parsed!.end : null,
        range: null,
      });
      await refresh();
      setLine("");
      setParsed(null);
    } catch (e) {
      addToast(taskCommandError(e));
    }
  }

  function openDateDialog(task: TaskView) {
    setMenu(null);
    setDateTask(task);
    if (task.start != null && task.end != null) {
      setDateDay(localDayOf(task.start));
      setDateStart(localTimeOf(task.start));
      setDateEnd(localTimeOf(task.end));
    } else {
      setDateDay(todayIso());
      setDateStart("");
      setDateEnd("");
    }
  }

  async function confirmDate(event: FormEvent) {
    event.preventDefault();
    if (!dateTask) return;
    const hasStart = dateStart.trim().length > 0;
    const hasEnd = dateEnd.trim().length > 0;
    if (hasStart !== hasEnd) {
      addToast("开始和结束时间要一起填。");
      return;
    }
    try {
      if (hasStart && hasEnd) {
        const start = unixAt(dateDay, dateStart);
        const end = unixAt(dateDay, dateEnd);
        await rescheduleTask(dateTask.id, start, end);
      } else if (dateTask.start != null && dateTask.end != null) {
        const shifted = shiftRangeToDay(
          dateTask.start,
          dateTask.end,
          dayStartUnix(localDayOf(dateTask.start)),
          dayStartUnix(dateDay),
        );
        await rescheduleTask(dateTask.id, shifted.start, shifted.end);
      }
      await refresh();
      setDateTask(null);
    } catch (e) {
      addToast(taskCommandError(e));
    }
  }

  const parsedListName =
    parsed && lists.find((list) => list.id === parsed.listId)?.name;

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

  const dialogs = (
    <>
      <Dialog
        open={moreOpen}
        onClose={() => setMoreOpen(false)}
        title="更多"
      >
        <div className="flex flex-col gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setMoreOpen(false);
              setAddName("");
              setAddRole("mainline");
              setAddOpen(true);
            }}
          >
            添加分组
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setShowDone((v) => !v);
              setMoreOpen(false);
            }}
          >
            {showDone ? "隐藏已完成" : "显示已完成"}
          </Button>
        </div>
      </Dialog>
      <Dialog
        open={addOpen}
        onClose={() => setAddOpen(false)}
        title="添加分组"
        footer={
          <>
            <Button variant="outline" size="sm" onClick={() => setAddOpen(false)}>
              取消
            </Button>
            <Button
              size="sm"
              onClick={() => {
                const name = addName.trim();
                if (!name) {
                  addToast("名称不能为空。");
                  return;
                }
                void run(async () => {
                  await createList(name, addRole);
                  setAddOpen(false);
                });
              }}
            >
              添加
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-3">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="task-add-name">名称</Label>
            <Input
              id="task-add-name"
              value={addName}
              onChange={(e) => setAddName(e.target.value)}
            />
          </div>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="task-add-role">角色</Label>
            <Select
              id="task-add-role"
              value={addRole}
              onChange={(e) => setAddRole(e.target.value)}
            >
              <option value="mainline">主线</option>
              <option value="side">支线</option>
              <option value="chore">杂项</option>
              <option value="longterm">长期规划</option>
            </Select>
          </div>
        </div>
      </Dialog>
      <Dialog
        open={dateTask != null}
        onClose={() => setDateTask(null)}
        title="更改日期"
        footer={
          <>
            <Button variant="outline" size="sm" onClick={() => setDateTask(null)}>
              取消
            </Button>
            <Button size="sm" form="task-date-form">
              保存
            </Button>
          </>
        }
      >
        <form id="task-date-form" className="flex flex-col gap-3" onSubmit={confirmDate}>
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="task-date-day">日期</Label>
            <Input
              id="task-date-day"
              type="date"
              value={dateDay}
              onChange={(e) => setDateDay(e.target.value)}
              required
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="task-date-start">开始</Label>
              <Input
                id="task-date-start"
                type="time"
                value={dateStart}
                onChange={(e) => setDateStart(e.target.value)}
              />
            </div>
            <div className="flex flex-col gap-1.5">
              <Label htmlFor="task-date-end">结束</Label>
              <Input
                id="task-date-end"
                type="time"
                value={dateEnd}
                onChange={(e) => setDateEnd(e.target.value)}
              />
            </div>
          </div>
        </form>
      </Dialog>
      <Dialog
        open={abandonTask != null}
        onClose={() => setAbandonTask(null)}
        title="放弃任务"
        footer={
          <>
            <Button variant="outline" size="sm" onClick={() => setAbandonTask(null)}>
              取消
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => {
                if (!abandonTask) return;
                void run(async () => {
                  await deleteTask(abandonTask.id);
                  setAbandonTask(null);
                });
              }}
            >
              放弃
            </Button>
          </>
        }
      >
        <p className="text-[12.5px] text-muted-foreground">
          放弃后不可恢复。确定放弃「{abandonTask?.title}」？
        </p>
      </Dialog>
      <Dialog
        open={renameTarget != null}
        onClose={() => setRenameTarget(null)}
        title="重命名分组"
        footer={
          <>
            <Button variant="outline" size="sm" onClick={() => setRenameTarget(null)}>
              取消
            </Button>
            <Button
              size="sm"
              onClick={() => {
                const name = renameName.trim();
                if (!name) {
                  addToast("名称不能为空。");
                  return;
                }
                if (!renameTarget) return;
                void run(async () => {
                  await renameList(renameTarget.id, name);
                  setRenameTarget(null);
                });
              }}
            >
              保存
            </Button>
          </>
        }
      >
        <Input
          value={renameName}
          onChange={(e) => setRenameName(e.target.value)}
          aria-label="分组名称"
        />
      </Dialog>
      <Dialog
        open={deleteTarget != null}
        onClose={() => setDeleteTarget(null)}
        title="删除分组"
        footer={
          <>
            <Button variant="outline" size="sm" onClick={() => setDeleteTarget(null)}>
              取消
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => {
                if (!deleteTarget) return;
                void run(async () => {
                  await deleteList(deleteTarget.id);
                  setDeleteTarget(null);
                });
              }}
            >
              删除
            </Button>
          </>
        }
      >
        <p className="text-[12.5px] text-muted-foreground">
          确定删除「{deleteTarget?.name}」？
        </p>
      </Dialog>
      <ContextMenu
        open={menu != null}
        x={menu?.x ?? 0}
        y={menu?.y ?? 0}
        onClose={() => setMenu(null)}
      >
        {menu?.kind === "task" && (
          <>
            <ContextMenuItem onSelect={() => openDateDialog(menu.task)}>
              更改日期…
            </ContextMenuItem>
            {lists
              .filter((list) => list.id !== menu.task.listId)
              .map((list) => (
                <ContextMenuItem
                  key={list.id}
                  onSelect={() => {
                    setMenu(null);
                    void run(() => moveTask(menu.task.id, list.id));
                  }}
                >
                  移动到{list.name}
                </ContextMenuItem>
              ))}
            <ContextMenuItem
              destructive
              onSelect={() => {
                setMenu(null);
                setAbandonTask(menu.task);
              }}
            >
              放弃任务
            </ContextMenuItem>
          </>
        )}
        {menu?.kind === "list" && (
          <>
            <ContextMenuItem
              onSelect={() => {
                setRenameName(menu.list.name);
                setRenameTarget(menu.list);
                setMenu(null);
              }}
            >
              重命名
            </ContextMenuItem>
            {!isPresetListId(menu.list.id) &&
              (board?.tasks ?? []).every((task) => task.listId !== menu.list.id) && (
                <ContextMenuItem
                  destructive
                  onSelect={() => {
                    setDeleteTarget(menu.list);
                    setMenu(null);
                  }}
                >
                  删除分组
                </ContextMenuItem>
              )}
          </>
        )}
      </ContextMenu>
      <Toaster toasts={toasts} />
    </>
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
        {dialogs}
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
        {dialogs}
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
                value={line}
                placeholder="明天上午十点到十二点，写方法节 #主线"
                aria-label="添加任务"
                onChange={(e) => {
                  const value = e.target.value;
                  setLine(value);
                  void runParse(value, currentListId);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void submitLine();
                  }
                }}
              />
              {parsed && line.trim() && (
                <div className="mt-1.5 flex flex-wrap gap-1.5 text-[11px] text-muted-foreground">
                  <span>{parsedListName ?? listRoleLabel("mainline")}</span>
                  <span>
                    {parsed.parseOk && parsed.start != null && parsed.end != null
                      ? taskTimeLabel(parsed.start, parsed.end)
                      : "未排期"}
                  </span>
                  <span>{parsed.title || "（无标题）"}</span>
                </div>
              )}
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-3">
              {lists.map((list) => {
                const tasks = tasksByList.get(list.id) ?? [];
                const openCount = (board.tasks ?? []).filter(
                  (t) => t.listId === list.id && !t.done,
                ).length;
                const folded = collapsed.includes(list.id);
                const Chevron = folded ? ChevronRight : ChevronDown;
                return (
                  <section key={list.id} className="pt-1">
                    <button
                      type="button"
                      tabIndex={0}
                      onFocus={() => setFocusedListId(list.id)}
                      onClick={() => {
                        const next = toggleCollapsed(collapsed, list.id);
                        setCollapsed(next);
                        writeCollapsed(next);
                      }}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        setFocusedListId(list.id);
                        setMenu({
                          kind: "list",
                          list,
                          x: e.clientX,
                          y: e.clientY,
                        });
                      }}
                      className="flex w-full items-center gap-2 rounded-[9px] px-1.5 py-1.5 text-left hover:bg-accent/60"
                    >
                      <Chevron className="size-3.5 shrink-0 text-muted-foreground" />
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
                    </button>
                    {!folded &&
                      tasks.map((task) => (
                        <div
                          key={task.id}
                          tabIndex={0}
                          onFocus={() => setFocusedListId(list.id)}
                          onContextMenu={(e) => {
                            e.preventDefault();
                            setFocusedListId(list.id);
                            setMenu({
                              kind: "task",
                              task,
                              x: e.clientX,
                              y: e.clientY,
                            });
                          }}
                          className="flex items-center gap-2 rounded-[9px] px-2 py-1 text-[12.5px] hover:bg-accent/40"
                        >
                          <input
                            type="checkbox"
                            checked={task.done}
                            aria-label={`完成 ${task.title}`}
                            className="size-3.5 shrink-0"
                            style={{ accentColor: "hsl(var(--primary))" }}
                            onChange={(e) => {
                              void run(() =>
                                toggleTaskDone(task.id, e.target.checked),
                              );
                            }}
                          />
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
      {dialogs}
    </>
  );
}
