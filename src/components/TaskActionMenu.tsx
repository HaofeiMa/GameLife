import { Calendar, CalendarPlus, Sun, Sunrise, X } from "lucide-react";
import type { ReactNode } from "react";
import { rescheduleTask, type TaskListView, type TaskView } from "../lib/api";
import { taskCommandError } from "../lib/taskBoard";
import { quickDateRange, type QuickDateKind } from "../lib/taskQuickDate";
import { cn } from "../lib/utils";
import { ContextMenu, ContextMenuItem, ContextMenuSub } from "./ui/context-menu";

const QUICK_DATES: { kind: QuickDateKind; label: string; icon: typeof Sun }[] = [
  { kind: "today", label: "今天", icon: Sun },
  { kind: "tomorrow", label: "明天", icon: Sunrise },
  { kind: "nextWeek", label: "下周", icon: CalendarPlus },
];

function DateIconButton({
  label,
  onSelect,
  children,
}: {
  label: string;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      title={label}
      aria-label={label}
      onClick={onSelect}
      className={cn(
        "flex size-7 items-center justify-center rounded-md text-muted-foreground",
        "hover:bg-accent hover:text-foreground",
      )}
    >
      {children}
    </button>
  );
}

export function TaskActionMenu({
  open,
  x,
  y,
  task,
  lists,
  onClose,
  onDate,
  onSaved,
  onError,
  onMove,
  onDuplicate,
  onAbandon,
}: {
  open: boolean;
  x: number;
  y: number;
  task: TaskView | null;
  lists: TaskListView[];
  onClose: () => void;
  onDate: (task: TaskView) => void;
  onSaved: () => Promise<void> | void;
  onError: (message: string) => void;
  onMove: (task: TaskView, listId: string) => void;
  onDuplicate: (task: TaskView) => void;
  onAbandon: (task: TaskView) => void;
}) {
  const others = task ? lists.filter((list) => list.id !== task.listId) : [];

  async function applyQuick(kind: QuickDateKind) {
    if (!task) return;
    onClose();
    const range = quickDateRange(task, Date.now() / 1000, kind);
    try {
      await rescheduleTask(task.id, range.start, range.end);
      await onSaved();
    } catch (e) {
      onError(taskCommandError(e));
    }
  }

  async function applyClear() {
    if (!task) return;
    onClose();
    if (task.start == null && task.end == null) return;
    try {
      await rescheduleTask(task.id, null, null);
      await onSaved();
    } catch (e) {
      onError(taskCommandError(e));
    }
  }

  return (
    <ContextMenu open={open && task != null} x={x} y={y} onClose={onClose}>
      {task && (
        <>
          <div className="flex items-center gap-0.5 px-2 py-1">
            <span className="mr-auto pl-0.5 text-[14px] text-muted-foreground">
              日期
            </span>
            {QUICK_DATES.map((item) => {
              const Icon = item.icon;
              return (
                <DateIconButton
                  key={item.kind}
                  label={item.label}
                  onSelect={() => void applyQuick(item.kind)}
                >
                  <Icon className="size-4" />
                </DateIconButton>
              );
            })}
            <DateIconButton label="自定义" onSelect={() => onDate(task)}>
              <Calendar className="size-4" />
            </DateIconButton>
            <DateIconButton label="清除" onSelect={() => void applyClear()}>
              <X className="size-4" />
            </DateIconButton>
          </div>
          <div className="mx-2 my-1 h-px bg-border" role="separator" />
          {others.length > 0 && (
            <ContextMenuSub label="移动到">
              {others.map((list) => (
                <ContextMenuItem
                  key={list.id}
                  onSelect={() => onMove(task, list.id)}
                >
                  {list.name}
                </ContextMenuItem>
              ))}
            </ContextMenuSub>
          )}
          <ContextMenuItem onSelect={() => onDuplicate(task)}>创建副本</ContextMenuItem>
          <ContextMenuItem destructive onSelect={() => onAbandon(task)}>
            放弃任务
          </ContextMenuItem>
        </>
      )}
    </ContextMenu>
  );
}

export function TaskBulkMenu({
  open,
  x,
  y,
  lists,
  onClose,
  onMove,
  onAbandon,
}: {
  open: boolean;
  x: number;
  y: number;
  lists: TaskListView[];
  onClose: () => void;
  onMove: (listId: string) => void;
  onAbandon: () => void;
}) {
  return (
    <ContextMenu open={open} x={x} y={y} onClose={onClose}>
      {lists.length > 0 && (
        <ContextMenuSub label="移动到">
          {lists.map((list) => (
            <ContextMenuItem key={list.id} onSelect={() => onMove(list.id)}>
              {list.name}
            </ContextMenuItem>
          ))}
        </ContextMenuSub>
      )}
      <ContextMenuItem destructive onSelect={onAbandon}>
        放弃任务
      </ContextMenuItem>
    </ContextMenu>
  );
}
