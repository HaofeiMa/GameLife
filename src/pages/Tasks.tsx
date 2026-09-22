import { ChevronDown, ChevronRight } from "lucide-react";
import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import { PageHeader } from "../components/PageHeader";
import { TaskActionMenu, TaskBulkMenu } from "../components/TaskActionMenu";
import { TaskCheckbox } from "../components/TaskCheckbox";
import { TaskComposer } from "../components/TaskComposer";
import { TaskDateDialog } from "../components/TaskDateDialog";
import { TaskDetailDialog } from "../components/TaskDetailDialog";
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
  duplicateTask,
  getDayView,
  getToday,
  listTaskBoard,
  moveTask,
  parseTaskLine,
  renameList,
  reorderList,
  reorderTask,
  rescheduleTask,
  toggleTaskDone,
  upsertTask,
  type ParsedTaskView,
  type TaskListView,
  type TaskView,
  type TodaySlot,
} from "../lib/api";
import { dayStartUnix, planBlocks } from "../lib/calendar";
import {
  COLLAPSED_KEY,
  isPresetListId,
  listRoleLabel,
  parseCollapsed,
  taskCommandError,
  taskTimeLabel,
  toggleCollapsed,
} from "../lib/taskBoard";
import {
  CAL_BLOCK_INSET_LEFT,
  CAL_BLOCK_INSET_RIGHT,
  CAL_DAY_GAP,
  CAL_GUTTER,
  CAL_HOUR_H,
  CAL_LANE_GAP,
  calendarDays,
  dayColumnLabel,
  dropRange,
  hitCalendarTs,
  nowLineTop,
  showNowLine,
  moveRangeToDrop,
  resizeRange,
  type CalEdge,
} from "../lib/taskCalendar";
import { lastCopiedPayload, parseTaskCopy, serializeTaskCopy } from "../lib/taskClipboard";
import {
  formatScheduledDuration,
  scheduledSecondsToday,
  unfinishedCount,
} from "../lib/taskHeaderStats";
import { rangeSelect, toggleSelect, visibleTaskIds } from "../lib/taskListSelect";
import { hourWashCategory, ribbonCells } from "../lib/slotRibbon";
import { listScheduleChip } from "../lib/taskScheduleLabel";
import { withinClickSlop } from "../lib/taskPointer";
import { listDragShown, ranksAfterDrag, unscheduledBeforeId } from "../lib/taskReorder";
import { hasSchedule, sortTasks } from "../lib/taskSort";
import { LIST_MAX, LIST_MIN } from "../lib/taskSplit";
import { assignPlanLanes, planLaneSpan } from "../lib/timelinePlan";
import { categoryColor, categoryColorAt, categoryOf, type CategoryKey } from "../lib/theme";
import { cn } from "../lib/utils";

const CAL_DAYS_KEY = "gl-task-cal-days";

type CalDays = "3" | "7";

type MenuState =
  | { kind: "task"; task: TaskView; x: number; y: number }
  | { kind: "bulk"; x: number; y: number }
  | { kind: "list"; list: TaskListView; x: number; y: number }
  | { kind: "board"; x: number; y: number };

type CalDrag = {
  task: TaskView;
  mode: "move" | "place" | "resize";
  edge?: CalEdge;
  grabOffset: number;
  start: number;
  end: number;
  originX: number;
  originY: number;
};

function nextCalRange(drag: CalDrag, ts: number): { start: number; end: number } {
  if (drag.mode === "place") return dropRange(ts);
  if (drag.mode === "resize" && drag.edge) {
    return resizeRange(
      drag.task.start ?? ts,
      drag.task.end ?? ts + 1800,
      drag.edge,
      ts,
    );
  }
  return moveRangeToDrop(
    drag.task.start ?? ts,
    drag.task.end ?? ts + 1800,
    ts,
    drag.grabOffset,
  );
}

function nextListSort(tasks: TaskView[], listId: string): number {
  let max = -1;
  for (const task of tasks) {
    if (task.listId === listId && task.sort > max) max = task.sort;
  }
  return max + 1;
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
  const [calDays, setCalDays] = useState<CalDays>(readCalDays);
  const [showDone, setShowDone] = useState(false);
  const [collapsed, setCollapsed] = useState<string[]>(readCollapsed);
  const [line, setLine] = useState("");
  const [parsed, setParsed] = useState<ParsedTaskView | null>(null);
  const [focusedListId, setFocusedListId] = useState<string | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [dateTask, setDateTask] = useState<TaskView | null>(null);
  const [detailTask, setDetailTask] = useState<TaskView | null>(null);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [selectAnchor, setSelectAnchor] = useState<string | null>(null);
  const [abandonIds, setAbandonIds] = useState<string[]>([]);
  const [renameTarget, setRenameTarget] = useState<TaskListView | null>(null);
  const [renameName, setRenameName] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<TaskListView | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [addName, setAddName] = useState("");
  const [addRole, setAddRole] = useState("mainline");
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const toastIdRef = useRef(0);
  const parseGen = useRef(0);
  const gridRef = useRef<HTMLDivElement>(null);
  const [calDrag, setCalDrag] = useState<CalDrag | null>(null);
  const [pendingCalClick, setPendingCalClick] = useState<{
    task: TaskView;
    originX: number;
    originY: number;
  } | null>(null);
  const [listDrag, setListDrag] = useState<{
    id: string;
    overListId: string;
    beforeId: string | null;
    timed: boolean;
    rowHeight: number;
    pointerX: number;
    pointerY: number;
    title: string;
  } | null>(null);
  const [groupDrag, setGroupDrag] = useState<{
    id: string;
    beforeId: string | null;
    rowHeight: number;
    pointerX: number;
    pointerY: number;
    title: string;
  } | null>(null);
  const calDragRef = useRef<CalDrag | null>(null);
  calDragRef.current = calDrag;
  const pendingCalClickRef = useRef(pendingCalClick);
  pendingCalClickRef.current = pendingCalClick;
  const { listWidth, dragging, splitRef, handleProps } = useTaskSplit();
  const today = todayIso();
  const days = useMemo(
    () => calendarDays(today, calDays === "3" ? 3 : 7),
    [calDays, today],
  );
  const daysRef = useRef(days);
  daysRef.current = days;
  const [ribbons, setRibbons] = useState<Record<string, TodaySlot[]>>({});
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  const [wallet, setWallet] = useState<{
    coinBalance: number;
    xpToday: number;
  } | null>(null);

  useEffect(() => {
    let cancelled = false;
    void Promise.all(
      days.map((day) =>
        getDayView(day)
          .then((view) => [day, view.slots] as const)
          .catch(() => [day, [] as TodaySlot[]] as const),
      ),
    ).then((rows) => {
      if (cancelled) return;
      const next: Record<string, TodaySlot[]> = {};
      for (const [day, slots] of rows) next[day] = slots;
      setRibbons(next);
    });
    return () => {
      cancelled = true;
    };
  }, [days]);

  useEffect(() => {
    const id = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 15_000);
    return () => clearInterval(id);
  }, []);

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
      try {
        const t = await getToday();
        setWallet({ coinBalance: t.coinBalance, xpToday: t.xpToday });
      } catch {
        setWallet(null);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const hitTs = useCallback((e: { clientX: number; clientY: number }) => {
    const grid = gridRef.current;
    if (!grid) return null;
    const box = grid.getBoundingClientRect();
    return hitCalendarTs(
      daysRef.current,
      e.clientX,
      e.clientY,
      {
        left: box.left,
        top: box.top,
        width: box.width,
        scrollTop: grid.scrollTop,
      },
      32,
    );
  }, []);

  const updateCalDrag = useCallback(
    (e: { clientX: number; clientY: number }) => {
      const drag = calDragRef.current;
      if (!drag) return;
      const ts = hitTs(e);
      if (ts == null) return;
      const next = nextCalRange(drag, ts);
      setCalDrag({ ...drag, start: next.start, end: next.end });
    },
    [hitTs],
  );

  const beginCalDragAt = useCallback(
    (
      task: TaskView,
      clientX: number,
      clientY: number,
      fromBlock: boolean,
      edge?: CalEdge | null,
    ) => {
      if (fromBlock && edge && task.start != null && task.end != null) {
        const range = resizeRange(task.start, task.end, edge, hitTs({ clientX, clientY }) ?? task.start);
        setCalDrag({
          task,
          mode: "resize",
          edge,
          grabOffset: 0,
          start: range.start,
          end: range.end,
          originX: clientX,
          originY: clientY,
        });
        return;
      }
      const ts = hitTs({ clientX, clientY });
      if (task.start != null && task.end != null) {
        const grabOffset = fromBlock && ts != null ? ts - task.start : 0;
        setCalDrag({
          task,
          mode: "move",
          grabOffset,
          start: task.start,
          end: task.end,
          originX: clientX,
          originY: clientY,
        });
      } else {
        const placed = ts != null ? dropRange(ts) : { start: 0, end: 1800 };
        setCalDrag({
          task,
          mode: "place",
          grabOffset: 0,
          start: placed.start,
          end: placed.end,
          originX: clientX,
          originY: clientY,
        });
      }
    },
    [hitTs],
  );

  const beginCalDrag = useCallback(
    (
      task: TaskView,
      e: ReactPointerEvent<HTMLElement>,
      fromBlock: boolean,
      edge?: CalEdge | null,
    ) => {
      if (e.button !== 0) return;
      if ((e.target as HTMLElement).closest("input")) return;
      e.preventDefault();
      e.stopPropagation();
      if (fromBlock && !edge) {
        setPendingCalClick({ task, originX: e.clientX, originY: e.clientY });
        return;
      }
      setPendingCalClick(null);
      beginCalDragAt(task, e.clientX, e.clientY, fromBlock, edge);
    },
    [beginCalDragAt],
  );

  useEffect(() => {
    if (!pendingCalClick) return;
    function onMove(e: PointerEvent) {
      const pending = pendingCalClickRef.current;
      if (!pending) return;
      if (withinClickSlop(e.clientX - pending.originX, e.clientY - pending.originY)) {
        return;
      }
      setPendingCalClick(null);
      beginCalDragAt(pending.task, e.clientX, e.clientY, true, null);
    }
    function onUp(e: PointerEvent) {
      const pending = pendingCalClickRef.current;
      setPendingCalClick(null);
      if (!pending) return;
      if (e.shiftKey || e.metaKey || e.ctrlKey) return;
      setDateTask(null);
      setDetailTask(pending.task);
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [beginCalDragAt, pendingCalClick]);

  const draggingId = calDrag?.task.id ?? null;
  useEffect(() => {
    if (!draggingId) return;
    function onMove(e: PointerEvent) {
      updateCalDrag(e);
    }
    function onUp(e: PointerEvent) {
      const drag = calDragRef.current;
      if (!drag) return;
      const ts = hitTs(e);
      setCalDrag(null);
      if (ts == null) return;
      const next = nextCalRange(drag, ts);
      if (drag.task.start === next.start && drag.task.end === next.end) return;
      void (async () => {
        try {
          await rescheduleTask(drag.task.id, next.start, next.end);
          await refresh();
        } catch (err) {
          addToast(taskCommandError(err));
        }
      })();
    }
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onUp);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
    };
  }, [addToast, draggingId, hitTs, refresh, updateCalDrag]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const lists = useMemo(
    () => (board?.lists ?? []).slice().sort((a, b) => a.sort - b.sort),
    [board],
  );
  const listsRef = useRef(lists);
  listsRef.current = lists;

  const currentListId =
    (focusedListId && lists.some((list) => list.id === focusedListId)
      ? focusedListId
      : null) ??
    lists.find((list) => list.role === "mainline")?.id ??
    lists[0]?.id ??
    null;

  useEffect(() => {
    function typingInField(el: EventTarget | null): boolean {
      return (
        el instanceof HTMLInputElement ||
        el instanceof HTMLTextAreaElement ||
        (el instanceof HTMLElement && el.isContentEditable)
      );
    }

    function onKeyDown(event: KeyboardEvent) {
      if (!(event.metaKey || event.ctrlKey)) return;
      if (event.key !== "c" && event.key !== "C" && event.key !== "v" && event.key !== "V") {
        return;
      }
      if (typingInField(event.target) || typingInField(document.activeElement)) return;
      const tasks = board?.tasks ?? [];
      if (event.key === "c" || event.key === "C") {
        const row =
          document.activeElement instanceof HTMLElement
            ? document.activeElement.closest("[data-task-id]")
            : null;
        const id = row?.getAttribute("data-task-id");
        const task = tasks.find((item) => item.id === id);
        if (!task) return;
        event.preventDefault();
        const raw = serializeTaskCopy(task);
        void navigator.clipboard.writeText(raw).catch(() => undefined);
        return;
      }
      const parsed = lastCopiedPayload() ? parseTaskCopy(lastCopiedPayload() ?? "") : null;
      if (!parsed) return;
      event.preventDefault();
      const listId = currentListId ?? parsed.listId;
      const id = crypto.randomUUID();
      const sort = nextListSort(tasks, listId);
      void (async () => {
        try {
          const result = await upsertTask({
            ...parsed,
            id,
            listId,
            done: false,
            sort: 0,
          });
          if (result.warning) addToast(result.warning);
          await reorderTask(id, listId, sort);
          await refresh();
        } catch (e) {
          addToast(taskCommandError(e));
        }
      })();
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [addToast, board, currentListId, refresh]);

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
      map.set(
        id,
        sortTasks(tasks, "time"),
      );
    }
    return map;
  }, [board, lists, showDone]);

  const tasksByListRef = useRef(tasksByList);
  tasksByListRef.current = tasksByList;
  const visibleIds = useMemo(
    () => visibleTaskIds(lists.map((list) => list.id), tasksByList, collapsed),
    [lists, tasksByList, collapsed],
  );
  const visibleIdsRef = useRef(visibleIds);
  visibleIdsRef.current = visibleIds;
  const selectedIdsRef = useRef(selectedIds);
  selectedIdsRef.current = selectedIds;
  const selectAnchorRef = useRef(selectAnchor);
  selectAnchorRef.current = selectAnchor;

  function clearListSelection() {
    setSelectedIds([]);
    setSelectAnchor(null);
    setDetailTask(null);
  }

  const beginListDrag = useCallback(
    (task: TaskView, listId: string, e: ReactPointerEvent<HTMLElement>) => {
      if (e.button !== 0) return;
      if ((e.target as HTMLElement).closest("[role=checkbox]")) return;
      e.preventDefault();
      const originX = e.clientX;
      const originY = e.clientY;
      const pointerId = e.pointerId;
      const row = e.currentTarget;
      const rowHeight = row.getBoundingClientRect().height;
      let armed = false;
      let switched = false;

      function overCalendar(clientX: number, clientY: number) {
        const grid = gridRef.current;
        if (!grid) return false;
        const box = grid.getBoundingClientRect();
        return (
          clientX >= box.left &&
          clientX <= box.right &&
          clientY >= box.top &&
          clientY <= box.bottom
        );
      }

      function hitDrop(clientX: number, clientY: number) {
        const stack = document.elementsFromPoint(clientX, clientY);
        let listHit: string | null = null;
        let beforeId: string | null | undefined;
        for (const node of stack) {
          if (!(node instanceof HTMLElement)) continue;
          if (!listHit && node.dataset.listId) listHit = node.dataset.listId;
          if (beforeId === undefined && node.dataset.insertBefore !== undefined) {
            beforeId = node.dataset.insertBefore === "" ? null : node.dataset.insertBefore;
            continue;
          }
          if (
            beforeId === undefined &&
            node.dataset.taskId &&
            node.dataset.taskId !== task.id
          ) {
            const owner = node.closest("[data-list-id]")?.getAttribute("data-list-id");
            if (!listHit || owner === listHit) beforeId = node.dataset.taskId;
          }
        }
        return { listId: listHit, beforeId: beforeId ?? null };
      }

      function onMove(ev: PointerEvent) {
        if (switched) return;
        if (!armed) {
          if (Math.hypot(ev.clientX - originX, ev.clientY - originY) < 4) return;
          armed = true;
          setSelectedIds([]);
          setDetailTask(null);
          try {
            row.setPointerCapture(pointerId);
          } catch {
            /* already captured */
          }
        }
        if (overCalendar(ev.clientX, ev.clientY)) {
          switched = true;
          try {
            row.releasePointerCapture(pointerId);
          } catch {
            /* ignore */
          }
          setListDrag(null);
          beginCalDragAt(task, ev.clientX, ev.clientY, false);
          return;
        }
        const drop = hitDrop(ev.clientX, ev.clientY);
        const dest = drop.listId ?? listId;
        const destTasks = tasksByListRef.current.get(dest) ?? [];
        const timed = hasSchedule(task);
        setListDrag({
          id: task.id,
          overListId: dest,
          beforeId: timed
            ? drop.beforeId
            : unscheduledBeforeId(destTasks, task.id, drop.beforeId),
          timed,
          rowHeight,
          pointerX: ev.clientX,
          pointerY: ev.clientY,
          title: task.title,
        });
      }

      function onUp(ev: PointerEvent) {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("pointercancel", onUp);
        if (switched) return;
        if (!armed) {
          setListDrag(null);
          if ((ev.target as HTMLElement).closest("[role=checkbox]")) return;
          if (ev.shiftKey) {
            const next = rangeSelect(
              visibleIdsRef.current,
              selectAnchorRef.current,
              task.id,
            );
            setSelectedIds(next);
            if (!selectAnchorRef.current) setSelectAnchor(task.id);
            setDetailTask(null);
            return;
          }
          if (ev.metaKey || ev.ctrlKey) {
            const next = toggleSelect(selectedIdsRef.current, task.id);
            setSelectedIds(next);
            setSelectAnchor(task.id);
            if (next.length !== 1) setDetailTask(null);
            return;
          }
          setSelectedIds([task.id]);
          setSelectAnchor(task.id);
          setDateTask(null);
          setDetailTask(task);
          return;
        }
        const drop = hitDrop(ev.clientX, ev.clientY);
        const dest = drop.listId ?? listId;
        setListDrag(null);
        void (async () => {
          try {
            if (hasSchedule(task)) {
              if (dest !== listId) {
                await reorderTask(task.id, dest, task.sort);
                await refresh();
              }
              return;
            }
            const destTasks = tasksByListRef.current.get(dest) ?? [];
            const beforeId = unscheduledBeforeId(destTasks, task.id, drop.beforeId);
            const ids = destTasks
              .filter((item) => !hasSchedule(item) || item.id === task.id)
              .map((item) => item.id);
            const ranks = ranksAfterDrag(
              ids.includes(task.id) ? ids : [...ids, task.id],
              task.id,
              beforeId,
            );
            for (const rank of ranks) {
              await reorderTask(rank.id, dest, rank.sort);
            }
            await refresh();
          } catch (err) {
            addToast(taskCommandError(err));
          }
        })();
      }

      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("pointercancel", onUp);
    },
    [addToast, beginCalDragAt, refresh],
  );

  const beginGroupDrag = useCallback(
    (list: TaskListView, e: ReactPointerEvent<HTMLElement>) => {
      if (e.button !== 0) return;
      e.preventDefault();
      const originX = e.clientX;
      const originY = e.clientY;
      const pointerId = e.pointerId;
      const row = e.currentTarget;
      const section = row.closest("section");
      const rowHeight = (section ?? row).getBoundingClientRect().height;
      let armed = false;

      function hitBefore(clientX: number, clientY: number): string | null {
        const stack = document.elementsFromPoint(clientX, clientY);
        for (const node of stack) {
          if (!(node instanceof HTMLElement)) continue;
          if (node.dataset.groupInsertBefore !== undefined) {
            return node.dataset.groupInsertBefore === ""
              ? null
              : node.dataset.groupInsertBefore;
          }
          const owner = node.closest("[data-list-id]")?.getAttribute("data-list-id");
          if (owner && owner !== list.id) return owner;
        }
        return null;
      }

      function onMove(ev: PointerEvent) {
        if (!armed) {
          if (Math.hypot(ev.clientX - originX, ev.clientY - originY) < 4) return;
          armed = true;
          try {
            row.setPointerCapture(pointerId);
          } catch {
            /* already captured */
          }
        }
        setGroupDrag({
          id: list.id,
          beforeId: hitBefore(ev.clientX, ev.clientY),
          rowHeight,
          pointerX: ev.clientX,
          pointerY: ev.clientY,
          title: list.name,
        });
      }

      function onUp(ev: PointerEvent) {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("pointercancel", onUp);
        if (!armed) {
          setGroupDrag(null);
          const next = toggleCollapsed(collapsed, list.id);
          setCollapsed(next);
          writeCollapsed(next);
          return;
        }
        const beforeId = hitBefore(ev.clientX, ev.clientY);
        const ids = listsRef.current.map((item) => item.id);
        const ranks = ranksAfterDrag(ids, list.id, beforeId);
        setGroupDrag(null);
        void (async () => {
          try {
            for (const rank of ranks) {
              await reorderList(rank.id, rank.sort);
            }
            await refresh();
          } catch (err) {
            addToast(taskCommandError(err));
          }
        })();
      }

      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("pointercancel", onUp);
    },
    [addToast, collapsed, refresh],
  );

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
      const result = await upsertTask({
        id: crypto.randomUUID(),
        listId,
        title,
        done: false,
        start: scheduled ? parsed!.start : null,
        end: scheduled ? parsed!.end : null,
        range: null,
        sort: 0,
        repeat: "none",
        remindOffsets: [],
        notes: "",
      });
      if (result.warning) addToast(result.warning);
      await refresh();
      setLine("");
      setParsed(null);
    } catch (e) {
      addToast(taskCommandError(e));
    }
  }

  function openDateDialog(task: TaskView) {
    setMenu(null);
    if (detailTask?.id === task.id) {
      document.getElementById("task-detail-schedule")?.focus();
      return;
    }
    setDetailTask(null);
    setDateTask(task);
  }

  const parsedListName =
    parsed && lists.find((list) => list.id === parsed.listId)?.name;
  const boardTasks = board?.tasks ?? [];
  const subtitle = `未完成 ${unfinishedCount(boardTasks)} · 今日已排期 ${formatScheduledDuration(
    scheduledSecondsToday(boardTasks, dayStartUnix(today)),
  )}`;

  const header = (
    <PageHeader
      title={<h1 className="text-[21px] font-bold tracking-[-0.02em]">任务</h1>}
      subtitle={subtitle}
      actions={
        <div className="flex items-center gap-2">
          {wallet && (
            <>
              <span className="flex items-center gap-[6px] text-[14.5px] font-semibold tabular-nums text-btn-ink">
                ◉ {wallet.coinBalance}
              </span>
              <span className="flex items-center gap-[6px] text-[14.5px] font-semibold tabular-nums text-energy">
                ⚡ {wallet.xpToday}
              </span>
            </>
          )}
          <Segmented
            aria-label="日历跨度"
            size="sm"
            value={calDays}
            onChange={(next) => {
              setCalDays(next);
              writeCalDays(next);
              clearListSelection();
            }}
            options={[
              { value: "3", label: "3 天" },
              { value: "7", label: "7 天" },
            ]}
          />
        </div>
      }
    />
  );

  const dialogs = (
    <>
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
      <TaskDateDialog
        task={dateTask}
        onClose={() => setDateTask(null)}
        onSaved={refresh}
        onError={addToast}
        onWarning={addToast}
      />
      <TaskDetailDialog
        task={detailTask}
        lists={lists}
        onClose={() => setDetailTask(null)}
        onSaved={refresh}
        onError={addToast}
        onWarning={addToast}
        onAbandon={(task) => {
          setDetailTask(null);
          setAbandonIds([task.id]);
        }}
      />
      <Dialog
        open={abandonIds.length > 0}
        onClose={() => setAbandonIds([])}
        title="放弃任务"
        footer={
          <>
            <Button variant="outline" size="sm" onClick={() => setAbandonIds([])}>
              取消
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => {
                if (abandonIds.length === 0) return;
                const ids = [...abandonIds];
                void run(async () => {
                  for (const id of ids) await deleteTask(id);
                  setAbandonIds([]);
                  clearListSelection();
                });
              }}
            >
              放弃
            </Button>
          </>
        }
      >
        <p className="text-[14.5px] text-muted-foreground">
          {abandonIds.length <= 1
            ? `放弃后不可恢复。确定放弃「${board?.tasks.find((t) => t.id === abandonIds[0])?.title ?? ""}」？`
            : `放弃后不可恢复。确定放弃 ${abandonIds.length} 条任务？`}
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
        <p className="text-[14.5px] text-muted-foreground">
          确定删除「{deleteTarget?.name}」？
        </p>
      </Dialog>
      <TaskActionMenu
        open={menu?.kind === "task"}
        x={menu?.kind === "task" ? menu.x : 0}
        y={menu?.kind === "task" ? menu.y : 0}
        task={menu?.kind === "task" ? menu.task : null}
        lists={lists}
        onClose={() => setMenu(null)}
        onDate={openDateDialog}
        onSaved={refresh}
        onError={(message) => addToast(message)}
        onMove={(task, listId) => {
          setMenu(null);
          void run(() => moveTask(task.id, listId));
        }}
        onDuplicate={(task) => {
          setMenu(null);
          void run(async () => {
            const result = await duplicateTask(task.id);
            if (result.warning) addToast(result.warning);
          });
        }}
        onAbandon={(task) => {
          setMenu(null);
          setAbandonIds([task.id]);
        }}
      />
      <TaskBulkMenu
        open={menu?.kind === "bulk"}
        x={menu?.kind === "bulk" ? menu.x : 0}
        y={menu?.kind === "bulk" ? menu.y : 0}
        lists={lists}
        onClose={() => setMenu(null)}
        onMove={(listId) => {
          const ids = [...selectedIds];
          setMenu(null);
          void run(async () => {
            for (const id of ids) {
              const row = (board?.tasks ?? []).find((t) => t.id === id);
              if (!row || row.listId === listId) continue;
              await moveTask(id, listId);
            }
          });
        }}
        onAbandon={() => {
          setMenu(null);
          setAbandonIds([...selectedIds]);
        }}
      />
      <ContextMenu
        open={menu?.kind === "list"}
        x={menu?.kind === "list" ? menu.x : 0}
        y={menu?.kind === "list" ? menu.y : 0}
        onClose={() => setMenu(null)}
      >
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
      <ContextMenu
        open={menu?.kind === "board"}
        x={menu?.kind === "board" ? menu.x : 0}
        y={menu?.kind === "board" ? menu.y : 0}
        onClose={() => setMenu(null)}
      >
        {menu?.kind === "board" && (
          <>
            <ContextMenuItem
              onSelect={() => {
                setMenu(null);
                setAddName("");
                setAddRole("mainline");
                setAddOpen(true);
              }}
            >
              添加分组
            </ContextMenuItem>
            <ContextMenuItem
              onSelect={() => {
                setShowDone((v) => !v);
                setMenu(null);
              }}
            >
              {showDone ? "隐藏已完成" : "显示已完成"}
            </ContextMenuItem>
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
              <TaskComposer
                value={line}
                lists={lists}
                spans={parsed?.spans ?? []}
                onChange={(value) => {
                  setLine(value);
                  void runParse(value, currentListId);
                }}
                onSubmit={() => void submitLine()}
              />
              {parsed && line.trim() && (
                <div className="mt-1.5 flex flex-wrap gap-1.5 text-[13px] text-muted-foreground">
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
            <div
              className={cn(
                "min-h-0 flex-1 overflow-y-auto px-2 pb-3",
                listDrag && "select-none",
                groupDrag && "select-none",
              )}
              onPointerDown={(e) => {
                if (e.target === e.currentTarget) clearListSelection();
              }}
              onContextMenu={(e) => {
                const el = e.target;
                if (!(el instanceof HTMLElement)) return;
                if (el.closest("[data-task-id], button, [role=checkbox], input, textarea")) {
                  return;
                }
                e.preventDefault();
                setMenu({ kind: "board", x: e.clientX, y: e.clientY });
              }}
            >
              {(() => {
                const listIds = lists.map((item) => item.id);
                const groupLayout = groupDrag
                  ? listDragShown(listIds, groupDrag.id, groupDrag.beforeId)
                  : { shown: listIds, gapIndex: -1 };
                const listById = new Map(lists.map((item) => [item.id, item]));
                const groupGap = (before: string) =>
                  groupDrag && groupLayout.gapIndex >= 0 ? (
                    <div
                      key="group-gap"
                      data-group-insert-before={before}
                      className="mx-1 rounded-[9px] bg-muted shadow-inner"
                      style={{ height: groupDrag.rowHeight }}
                    />
                  ) : null;
                return (
                  <>
                    {groupLayout.shown.map((id, gi) => {
                const list = listById.get(id);
                if (!list) return null;
                const tasks = tasksByList.get(list.id) ?? [];
                const openCount = (board.tasks ?? []).filter(
                  (t) => t.listId === list.id && !t.done,
                ).length;
                const folded = collapsed.includes(list.id);
                const Chevron = folded ? ChevronRight : ChevronDown;
                return (
                  <Fragment key={list.id}>
                    {groupLayout.gapIndex === gi ? groupGap(id) : null}
                  <section
                    key={list.id}
                    data-list-id={list.id}
                    className="pt-1"
                  >
                    <button
                      type="button"
                      tabIndex={0}
                      onFocus={() => setFocusedListId(list.id)}
                      onPointerDown={(e) => beginGroupDrag(list, e)}
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
                      <span className="min-w-0 flex-1 truncate text-[15px] font-semibold">
                        {list.name}
                      </span>
                      <span className="tabular-nums text-[13px] text-muted-foreground">
                        {openCount}
                      </span>
                    </button>
                    {!folded &&
                      (() => {
                        const rawIds = tasks.map((t) => t.id);
                        const layout = listDrag
                          ? listDrag.overListId === list.id
                            ? listDrag.timed
                              ? {
                                  shown: rawIds.filter((id) => id !== listDrag.id),
                                  gapIndex: -1,
                                }
                              : listDragShown(rawIds, listDrag.id, listDrag.beforeId)
                            : {
                                shown: rawIds.filter((id) => id !== listDrag.id),
                                gapIndex: -1,
                              }
                          : { shown: rawIds, gapIndex: -1 };
                        const byId = new Map(tasks.map((t) => [t.id, t]));
                        const gap = (before: string) =>
                          listDrag && layout.gapIndex >= 0 ? (
                            <div
                              key="gap"
                              data-insert-before={before}
                              className="mx-1 rounded-[9px] bg-muted shadow-inner transition-[height] duration-150"
                              style={{ height: listDrag.rowHeight }}
                            />
                          ) : null;
                        return (
                          <>
                            {layout.shown.map((id, i) => {
                              const task = byId.get(id);
                              if (!task) return null;
                              const chip = listScheduleChip(
                                task.start,
                                task.end,
                                Date.now() / 1000,
                                task.done,
                              );
                              return (
                                <Fragment key={id}>
                                  {layout.gapIndex === i ? gap(id) : null}
                                  <div
                                    data-task-id={task.id}
                                    tabIndex={0}
                                    onFocus={() => setFocusedListId(list.id)}
                                    onPointerDown={(e) => beginListDrag(task, list.id, e)}
                                    onContextMenu={(e) => {
                                      e.preventDefault();
                                      setFocusedListId(list.id);
                                      if (
                                        selectedIds.length > 1 &&
                                        selectedIds.includes(task.id)
                                      ) {
                                        setMenu({
                                          kind: "bulk",
                                          x: e.clientX,
                                          y: e.clientY,
                                        });
                                        return;
                                      }
                                      setSelectedIds([task.id]);
                                      setSelectAnchor(task.id);
                                      setMenu({
                                        kind: "task",
                                        task,
                                        x: e.clientX,
                                        y: e.clientY,
                                      });
                                    }}
                                    className={cn(
                                      "flex cursor-grab items-center gap-2 rounded-[9px] py-1 pr-1.5 text-[14.5px] hover:bg-accent/40 active:cursor-grabbing",
                                      "pl-[calc(0.375rem+0.875rem+0.5rem)]",
                                      selectedIds.includes(task.id) && "bg-accent/40",
                                    )}
                                  >
                                    <TaskCheckbox
                                      checked={task.done}
                                      color={roleDot(list.role)}
                                      label={`完成 ${task.title}`}
                                      onToggle={(next) => {
                                        void run(() => toggleTaskDone(task.id, next));
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
                                    <span
                                      className={cn(
                                        "shrink-0 text-[13px] tabular-nums",
                                        chip.tone === "overdue" && "text-destructive",
                                        chip.tone === "upcoming" && "text-energy",
                                        chip.tone === "muted" && "text-muted-foreground",
                                      )}
                                    >
                                      {chip.label}
                                    </span>
                                  </div>
                                </Fragment>
                              );
                            })}
                            {layout.gapIndex === layout.shown.length ? gap("") : null}
                          </>
                        );
                      })()}
                  </section>
                  </Fragment>
                );
                })}
                    {groupLayout.gapIndex === groupLayout.shown.length
                      ? groupGap("")
                      : null}
                  </>
                );
              })()}
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
            <TaskCalendar
              days={days}
              today={today}
              tasks={board.tasks}
              lists={lists}
              preview={
                calDrag
                  ? { id: calDrag.task.id, start: calDrag.start, end: calDrag.end }
                  : null
              }
              gridRef={gridRef}
              onBlockDown={beginCalDrag}
              onBlockMenu={(task, x, y) =>
                setMenu({ kind: "task", task, x, y })
              }
              ribbons={ribbons}
              now={now}
            />
          </Card>
        </div>
      </div>
      {listDrag && (
        <div
          className="pointer-events-none fixed z-50 max-w-xs rounded-[9px] border bg-card px-2 py-1.5 text-[15px] font-semibold shadow-lg"
          style={{
            left: listDrag.pointerX + 8,
            top: listDrag.pointerY + 8,
          }}
        >
          {listDrag.title}
        </div>
      )}
      {groupDrag && (
        <div
          className="pointer-events-none fixed z-50 max-w-xs rounded-[9px] border bg-card px-2 py-1.5 text-[15px] font-semibold shadow-lg"
          style={{
            left: groupDrag.pointerX + 8,
            top: groupDrag.pointerY + 8,
          }}
        >
          {groupDrag.title}
        </div>
      )}
      {dialogs}
    </>
  );
}

function DayColumnGap() {
  return (
    <div className="relative shrink-0 self-stretch" style={{ width: CAL_DAY_GAP }}>
      <div className="pointer-events-none absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-hour-line" />
    </div>
  );
}

function TaskCalendar({
  days,
  today,
  now,
  tasks,
  lists,
  preview,
  gridRef,
  onBlockDown,
  onBlockMenu,
  ribbons,
}: {
  days: string[];
  today: string;
  now: number;
  tasks: TaskView[];
  lists: TaskListView[];
  preview: { id: string; start: number; end: number } | null;
  gridRef: RefObject<HTMLDivElement | null>;
  onBlockDown: (
    task: TaskView,
    e: ReactPointerEvent<HTMLElement>,
    fromBlock: boolean,
    edge?: CalEdge | null,
  ) => void;
  onBlockMenu: (task: TaskView, x: number, y: number) => void;
  ribbons: Record<string, TodaySlot[]>;
}) {
  const hours = Array.from({ length: 24 }, (_, h) => h);
  const height = 24 * CAL_HOUR_H;
  const scrolledForRef = useRef<string | null>(null);
  useEffect(() => {
    if (!days.includes(today) || scrolledForRef.current === today) return;
    const el = gridRef.current;
    if (!el || el.clientHeight === 0) return;
    scrolledForRef.current = today;
    const dayStart = dayStartUnix(today);
    const top = nowLineTop(now, dayStart);
    el.scrollTop = Math.max(0, 32 + top - el.clientHeight / 3);
  }, [days, today, now, gridRef]);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <div ref={gridRef} className="min-h-0 flex-1 overflow-auto">
        <div className="sticky top-0 z-20 flex h-8 bg-card">
          <div className="shrink-0" style={{ width: CAL_GUTTER }} />
          {days.map((day, i) => (
            <Fragment key={day}>
              {i > 0 ? <DayColumnGap /> : null}
              <div
                className="min-w-0 flex-1 py-1.5 text-center text-[13px] font-medium"
                style={
                  day === today
                    ? { background: categoryColorAt("mainline", 16) }
                    : undefined
                }
              >
                {dayColumnLabel(day)}
              </div>
            </Fragment>
          ))}
        </div>
        <div className="relative flex" style={{ height }}>
          <div className="relative shrink-0" style={{ width: CAL_GUTTER }}>
            {hours.map((h) => (
              <div
                key={h}
                className="absolute right-1 text-[11px] tabular-nums text-muted-foreground"
                style={{ top: h * CAL_HOUR_H + 2 }}
              >
                {String(h).padStart(2, "0")}
              </div>
            ))}
          </div>
          {days.map((day, i) => (
            <Fragment key={day}>
              {i > 0 ? <DayColumnGap /> : null}
              <CalendarDayColumn
                day={day}
                today={today}
                now={now}
                tasks={tasks}
                lists={lists}
                preview={preview}
                onBlockDown={onBlockDown}
                onBlockMenu={onBlockMenu}
                slots={ribbons[day] ?? []}
              />
            </Fragment>
          ))}
        </div>
      </div>
    </div>
  );
}

function CalendarDayColumn({
  day,
  today,
  now,
  tasks,
  lists,
  preview,
  onBlockDown,
  onBlockMenu,
  slots,
}: {
  day: string;
  today: string;
  now: number;
  tasks: TaskView[];
  lists: TaskListView[];
  preview: { id: string; start: number; end: number } | null;
  onBlockDown: (
    task: TaskView,
    e: ReactPointerEvent<HTMLElement>,
    fromBlock: boolean,
    edge?: CalEdge | null,
  ) => void;
  onBlockMenu: (task: TaskView, x: number, y: number) => void;
  slots: TodaySlot[];
}) {
  const dayStart = dayStartUnix(day);
  const dayEnd = dayStart + 86400;
  const hours = Array.from({ length: 24 }, (_, h) => h);
  const visible = tasks.filter(
    (task) =>
      !task.done &&
      task.id !== preview?.id &&
      task.start != null &&
      task.end != null &&
      task.end > task.start &&
      task.start < dayEnd &&
      task.end > dayStart,
  );
  const previewTask =
    preview && preview.start < dayEnd && preview.end > dayStart
      ? {
          id: preview.id,
          listId: tasks.find((t) => t.id === preview.id)?.listId ?? "",
          title: tasks.find((t) => t.id === preview.id)?.title ?? "",
          done: false,
          start: preview.start,
          end: preview.end,
          range: null,
          sort: 0,
          repeat: "none",
          remindOffsets: [],
          notes: tasks.find((t) => t.id === preview.id)?.notes ?? "",
        }
      : null;
  const shown = previewTask ? [...visible, previewTask] : visible;
  const blocks = planBlocks(
    shown.map((task) => ({
      start: task.start ?? dayStart,
      end: task.end ?? dayStart + 1800,
      role: lists.find((list) => list.id === task.listId)?.role ?? "side",
      title: task.title,
    })),
    dayStart,
  );
  const { items, laneCount } = assignPlanLanes(blocks);
  const lanes = Math.max(1, laneCount);
  const idOf = (item: (typeof items)[number]) => {
    const hit = shown.find(
      (task) =>
        task.start === item.start &&
        task.end === item.end &&
        task.title === item.title,
    );
    return hit?.id ?? `${item.title}-${item.rowStart}`;
  };

  const cells = ribbonCells(slots, dayStart);

  return (
    <div className="relative min-w-0 flex-1">
      {hours.map((h) => {
        const wash = hourWashCategory(cells, h);
        return (
          <div
            key={`wash-${h}`}
            className="pointer-events-none absolute inset-x-0"
            style={{
              top: h * CAL_HOUR_H,
              height: CAL_HOUR_H,
              background: wash ? categoryColorAt(wash, 10) : undefined,
            }}
          />
        );
      })}
      {hours.slice(1).map((h) => (
        <div
          key={`line-${h}`}
          className="absolute inset-x-0 border-t border-hour-line"
          style={{ top: h * CAL_HOUR_H }}
        />
      ))}
      {items.map((mark) => {
        const cat = categoryOf(mark.role);
        const span = planLaneSpan(mark, items, lanes);
        const id = idOf(mark);
        const task = shown.find((row) => row.id === id);
        const isPreview = preview?.id === id;
        return (
          <div
            key={`${id}-${mark.rowStart}-${mark.lane}`}
            title={mark.title}
            data-task-id={task?.id}
            onPointerDown={(e) => {
              if (!task || isPreview) return;
              onBlockDown(task, e, true);
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              if (!task || isPreview) return;
              onBlockMenu(task, e.clientX, e.clientY);
            }}
            className={cn(
              "absolute overflow-hidden rounded-[5px] px-1.5 py-0.5 text-[13px] leading-tight",
              !isPreview && "cursor-grab active:cursor-grabbing",
              isPreview && "pointer-events-none opacity-80",
            )}
            style={{
              left: `calc(${(mark.lane / lanes) * 100}% + ${CAL_BLOCK_INSET_LEFT}px)`,
              width: `calc(${(span / lanes) * 100}% - ${CAL_BLOCK_INSET_LEFT + CAL_BLOCK_INSET_RIGHT + CAL_LANE_GAP}px)`,
              top: mark.rowStart * (CAL_HOUR_H / 4) + 1,
              height: mark.rowSpan * (CAL_HOUR_H / 4) - 2,
              background: `color-mix(in srgb, ${categoryColor(cat)} 28%, hsl(var(--card)))`,
              borderLeft: `3px solid ${categoryColor(cat)}`,
            }}
          >
            {!isPreview && task && (
              <>
                <div
                  className="absolute inset-x-0 top-0 h-1.5 cursor-ns-resize"
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    onBlockDown(task, e, true, "start");
                  }}
                />
                <div
                  className="absolute inset-x-0 bottom-0 h-1.5 cursor-ns-resize"
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    onBlockDown(task, e, true, "end");
                  }}
                />
              </>
            )}
            <span className="line-clamp-6 font-semibold text-foreground/85">
              {mark.title}
            </span>
          </div>
        );
      })}
      {showNowLine(day, today, now, dayStart) && (
        <div
          className="pointer-events-none absolute inset-x-0 z-20 flex items-center"
          style={{ top: nowLineTop(now, dayStart) }}
          aria-hidden
        >
          <span className="absolute -left-[9px] -top-[4px] size-[9px] rounded-full border-2 border-card bg-destructive" />
          <span className="h-0 w-full border-t-2 border-destructive" />
        </div>
      )}
    </div>
  );
}
