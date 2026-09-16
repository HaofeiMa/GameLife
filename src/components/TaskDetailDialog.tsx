import { MoreHorizontal, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  duplicateTask,
  moveTask,
  rescheduleTask,
  toggleTaskDone,
  upsertTask,
  type TaskListView,
  type TaskView,
} from "../lib/api";
import { taskCommandError } from "../lib/taskBoard";
import { notesByteLength, parseTaskNotes, type NotesInline } from "../lib/taskNotesMd";
import { alignRange, unixAt } from "../lib/taskSchedule";
import { categoryColor } from "../lib/theme";
import { TaskActionMenu } from "./TaskActionMenu";
import { TaskCheckbox } from "./TaskCheckbox";
import {
  formFromTask,
  TaskScheduleFields,
  type TaskScheduleForm,
} from "./TaskScheduleFields";
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

function NotesInlineView({ nodes }: { nodes: NotesInline[] }) {
  return (
    <>
      {nodes.map((node, i) => {
        if (node.type === "strong") return <strong key={i}>{node.value}</strong>;
        if (node.type === "link") {
          return (
            <a
              key={i}
              href={node.href}
              target="_blank"
              rel="noreferrer"
              className="text-primary underline"
            >
              {node.text}
            </a>
          );
        }
        return <span key={i}>{node.value}</span>;
      })}
    </>
  );
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
  const [title, setTitle] = useState(task.title);
  const [notes, setNotes] = useState(task.notes);
  const [form, setForm] = useState<TaskScheduleForm>(() =>
    formFromTask(task, Date.now() / 1000),
  );
  const [listId, setListId] = useState(task.listId);
  const [done, setDone] = useState(task.done);
  const [more, setMore] = useState<{ x: number; y: number } | null>(null);
  const moreRef = useRef<HTMLDivElement>(null);
  const notesTimer = useRef<number>(0);
  const scheduleTimer = useRef<number>(0);
  const snapshot = useRef(task);
  snapshot.current = { ...task, title, notes, listId, done };

  useEffect(() => {
    return () => {
      window.clearTimeout(notesTimer.current);
      window.clearTimeout(scheduleTimer.current);
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

  function queueSchedule(next: TaskScheduleForm) {
    setForm(next);
    window.clearTimeout(scheduleTimer.current);
    scheduleTimer.current = window.setTimeout(() => {
      void flushSchedule(next);
    }, 400);
  }

  async function flushSchedule(next: TaskScheduleForm) {
    try {
      const range = alignRange(
        unixAt(next.startDay, next.startHm),
        unixAt(next.endDay, next.endHm),
      );
      await savePatch({
        start: range.start,
        end: range.end,
        repeat: next.repeat,
        remindOffsets: next.remindOffsets,
      });
    } catch (e) {
      onError(taskCommandError(e));
    }
  }

  const preview = parseTaskNotes(notes);

  return (
    <>
      <Dialog
        open
        onClose={onClose}
        title="任务详情"
        className="max-w-lg"
        chrome="plain"
        header={
          <div className="flex items-start gap-3 border-b bg-background px-5 py-3">
            <div className="min-w-0 flex-1 rounded-xl border bg-card p-3">
              <div className="flex items-start gap-3">
                <div className="pt-1">
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
                </div>
                <div className="min-w-0 flex-1">
                  <TaskScheduleFields
                    id="task-detail-schedule"
                    form={form}
                    setForm={queueSchedule}
                    disabled={false}
                  />
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="mt-2"
                    onClick={() => {
                      window.clearTimeout(scheduleTimer.current);
                      void (async () => {
                        try {
                          await rescheduleTask(task.id, null, null);
                          void onSaved();
                        } catch (e) {
                          onError(taskCommandError(e));
                        }
                      })();
                    }}
                  >
                    清除时段
                  </Button>
                </div>
              </div>
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="关闭"
              className="-mr-1 -mt-1 rounded-md p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            >
              <X className="size-4" />
            </button>
          </div>
        }
        footer={
          <>
            <Select
              size="sm"
              className="mr-auto w-auto min-w-36"
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
        <div className="flex flex-col gap-3">
          <div className="rounded-xl border bg-card p-3">
            <Input
              value={title}
              aria-label="任务名称"
              onChange={(e) => setTitle(e.target.value)}
              onBlur={() => void saveTitle()}
            />
          </div>
          <div className="rounded-xl border bg-card p-3">
            <Textarea
              value={notes}
              aria-label="备注"
              rows={6}
              placeholder="指标、会议链接、地点…"
              onChange={(e) => queueNotes(e.target.value)}
              onBlur={() => {
                window.clearTimeout(notesTimer.current);
                void flushNotes(notes);
              }}
            />
            {preview.length > 0 && (
              <div className="mt-3 space-y-2 text-[12.5px] leading-relaxed text-muted-foreground">
                {preview.map((block, i) => {
                  if (block.type === "ul") {
                    return (
                      <ul key={i} className="list-disc pl-5">
                        {block.items.map((item, j) => (
                          <li key={j}>
                            <NotesInlineView nodes={item} />
                          </li>
                        ))}
                      </ul>
                    );
                  }
                  if (block.type === "ol") {
                    return (
                      <ol key={i} className="list-decimal pl-5">
                        {block.items.map((item, j) => (
                          <li key={j}>
                            <NotesInlineView nodes={item} />
                          </li>
                        ))}
                      </ol>
                    );
                  }
                  return (
                    <p key={i}>
                      <NotesInlineView nodes={block.children} />
                    </p>
                  );
                })}
              </div>
            )}
          </div>
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
          document.getElementById("task-detail-schedule")?.focus();
        }}
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
