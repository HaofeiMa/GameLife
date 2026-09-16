import { Bell, Calendar, MoreHorizontal, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  duplicateTask,
  moveTask,
  toggleTaskDone,
  upsertTask,
  type TaskListView,
  type TaskView,
} from "../lib/api";
import { taskCommandError } from "../lib/taskBoard";
import { notesByteLength } from "../lib/taskNotesMd";
import {
  repeatShortLabel,
  scheduleOverdue,
  scheduleSummary,
} from "../lib/taskScheduleLabel";
import { categoryColor } from "../lib/theme";
import { cn } from "../lib/utils";
import { TaskActionMenu } from "./TaskActionMenu";
import { TaskCheckbox } from "./TaskCheckbox";
import { TaskDateCard } from "./TaskDateCard";
import { Button } from "./ui/button";
import { Dialog } from "./ui/dialog";
import { Input } from "./ui/input";
import { Select } from "./ui/select";
import { Textarea } from "./ui/textarea";

function listRoleColor(role: string): string {
  const key =
    role === "mainline"
      ? "mainline"
      : role === "longterm"
        ? "longterm"
        : role === "chore"
          ? "admin"
          : "side";
  return categoryColor(key);
}

export function TaskDetailDialog({
  task,
  lists,
  onClose,
  onSaved,
  onError,
  onAbandon,
}: {
  task: TaskView | null;
  lists: TaskListView[];
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  onError: (message: string) => void;
  onAbandon: (task: TaskView) => void;
}) {
  if (!task) {
    return <Dialog open={false} onClose={onClose} title="任务详情" />;
  }
  return (
    <TaskDetailDialogBody
      key={task.id}
      task={task}
      lists={lists}
      onClose={onClose}
      onSaved={onSaved}
      onError={onError}
      onAbandon={onAbandon}
    />
  );
}

function TaskDetailDialogBody({
  task,
  lists,
  onClose,
  onSaved,
  onError,
  onAbandon,
}: {
  task: TaskView;
  lists: TaskListView[];
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  onError: (message: string) => void;
  onAbandon: (task: TaskView) => void;
}) {
  const role = lists.find((list) => list.id === task.listId)?.role ?? "side";
  const color = listRoleColor(role);
  const nowSec = Date.now() / 1000;
  const [title, setTitle] = useState(task.title);
  const [notes, setNotes] = useState(task.notes);
  const [listId, setListId] = useState(task.listId);
  const [done, setDone] = useState(task.done);
  const [dateOpen, setDateOpen] = useState(false);
  const [more, setMore] = useState<{ x: number; y: number } | null>(null);
  const moreRef = useRef<HTMLDivElement>(null);
  const notesTimer = useRef<number>(0);
  const snapshot = useRef(task);
  snapshot.current = { ...task, title, notes, listId, done };
  const overdue = scheduleOverdue(task.end, nowSec, done);
  const summary = scheduleSummary(task.start, task.end, nowSec);
  const repeat = repeatShortLabel(task.repeat);
  const hasRemind = task.remindOffsets.length > 0;

  useEffect(() => {
    return () => {
      window.clearTimeout(notesTimer.current);
    };
  }, []);

  async function savePatch(patch: Partial<TaskView>) {
    await upsertTask({ ...snapshot.current, ...patch });
    void onSaved();
  }

  async function saveTitle() {
    const next = title.trim();
    if (!next) {
      onError(taskCommandError("empty_title"));
      setTitle(task.title);
      return;
    }
    if (next === task.title) return;
    try {
      await savePatch({ title: next });
    } catch (e) {
      onError(taskCommandError(e));
    }
  }

  function queueNotes(next: string) {
    setNotes(next);
    window.clearTimeout(notesTimer.current);
    notesTimer.current = window.setTimeout(() => {
      void flushNotes(next);
    }, 400);
  }

  async function flushNotes(next: string) {
    if (notesByteLength(next) > 8192) {
      onError(taskCommandError("notes_too_long"));
      return;
    }
    if (next === task.notes) return;
    try {
      await savePatch({ notes: next });
    } catch (e) {
      onError(taskCommandError(e));
    }
  }

  return (
    <>
      <Dialog
        open
        onClose={() => {
          if (dateOpen) setDateOpen(false);
          else onClose();
        }}
        onDismiss={onClose}
        title="任务详情"
        className="max-w-lg"
        chrome="plain"
        header={
          <div className="relative flex items-center gap-2 px-5 pt-3 pb-1">
            <TaskCheckbox
              checked={done}
              color={color}
              label={`完成 ${title}`}
              onToggle={(next) => {
                setDone(next);
                void (async () => {
                  try {
                    await toggleTaskDone(task.id, next);
                    void onSaved();
                  } catch (e) {
                    setDone(!next);
                    onError(taskCommandError(e));
                  }
                })();
              }}
            />
            <span className="h-3 w-px shrink-0 bg-border" aria-hidden />
            <button
              type="button"
              className={cn(
                "flex min-w-0 flex-1 items-center gap-1.5 rounded-md px-1 py-0.5 text-left text-[14.5px] leading-snug",
                overdue ? "text-destructive" : "text-muted-foreground",
                "hover:bg-accent",
              )}
              onClick={() => setDateOpen((open) => !open)}
            >
              <Calendar className="size-3.5 shrink-0" />
              <span className="truncate">{summary}</span>
            </button>
            <button
              type="button"
              onClick={onClose}
              aria-label="关闭"
              className="-mr-1 rounded-md p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            >
              <X className="size-4" />
            </button>
          </div>
        }
        layer={
          dateOpen ? (
            <TaskDateCard
              key={`${task.id}-${task.start ?? "none"}-${task.end ?? "none"}`}
              task={task}
              onClose={() => setDateOpen(false)}
              onSaved={onSaved}
              onError={onError}
            />
          ) : null
        }
        footer={
          <>
            <Select
              size="sm"
              className="mr-auto w-auto min-w-28"
              value={listId}
              onChange={(e) => {
                const next = e.target.value;
                setListId(next);
                void (async () => {
                  try {
                    await moveTask(task.id, next);
                    void onSaved();
                  } catch (err) {
                    setListId(task.listId);
                    onError(taskCommandError(err));
                  }
                })();
              }}
            >
              {lists.map((list) => (
                <option key={list.id} value={list.id}>
                  {list.name}
                </option>
              ))}
            </Select>
            {repeat && (
              <span className="text-[13.5px] text-muted-foreground">{repeat}</span>
            )}
            {hasRemind && (
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="提醒"
                onClick={() => setDateOpen(true)}
              >
                <Bell className="size-3.5" />
              </Button>
            )}
            <div ref={moreRef}>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label="更多"
                onClick={() => {
                  const box = moreRef.current?.getBoundingClientRect();
                  setMore({ x: box?.right ?? 0, y: box?.bottom ?? 0 });
                }}
              >
                <MoreHorizontal className="size-4" />
              </Button>
            </div>
          </>
        }
      >
        <div className="flex flex-col gap-1 px-0">
          <Input
            value={title}
            aria-label="任务名称"
            className="h-auto border-0 bg-transparent px-0 text-[18px] font-semibold shadow-none"
            onChange={(e) => setTitle(e.target.value)}
            onBlur={() => void saveTitle()}
          />
          <Textarea
            value={notes}
            aria-label="备注"
            rows={7}
            placeholder="指标、会议链接、地点…"
            className="min-h-40 resize-none border-0 bg-transparent px-0 shadow-none"
            onChange={(e) => queueNotes(e.target.value)}
            onBlur={() => {
              window.clearTimeout(notesTimer.current);
              void flushNotes(notes);
            }}
          />
        </div>
      </Dialog>
      <TaskActionMenu
        open={more != null}
        x={more?.x ?? 0}
        y={more?.y ?? 0}
        task={task}
        lists={lists}
        onClose={() => setMore(null)}
        onDate={() => {
          setMore(null);
          setDateOpen(true);
        }}
        onSaved={onSaved}
        onError={onError}
        onMove={(_, nextList) => {
          setMore(null);
          setListId(nextList);
          void (async () => {
            try {
              await moveTask(task.id, nextList);
              void onSaved();
            } catch (e) {
              onError(taskCommandError(e));
            }
          })();
        }}
        onDuplicate={() => {
          setMore(null);
          void (async () => {
            try {
              await duplicateTask(task.id);
              void onSaved();
            } catch (e) {
              onError(taskCommandError(e));
            }
          })();
        }}
        onAbandon={() => {
          setMore(null);
          onClose();
          onAbandon(task);
        }}
      />
    </>
  );
}
