import { useEffect, useRef, useState } from "react";
import { rescheduleTask, upsertTask, type TaskView } from "../lib/api";
import { taskCommandError } from "../lib/taskBoard";
import { alignRange, unixAt } from "../lib/taskSchedule";
import { cn } from "../lib/utils";
import { formFromTask, TaskScheduleFields } from "./TaskScheduleFields";
import { Button } from "./ui/button";

export function TaskDateCard({
  task,
  onClose,
  onSaved,
  onError,
  onWarning,
  className,
}: {
  task: TaskView;
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  onError: (message: string) => void;
  onWarning?: (text: string) => void;
  className?: string;
}) {
  const [form, setForm] = useState(() => formFromTask(task, Date.now() / 1000));
  const [busy, setBusy] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const first = rootRef.current?.querySelector<HTMLElement>(
      "input:not([disabled]), select:not([disabled]), button:not([disabled])",
    );
    first?.focus();
  }, []);

  async function confirm() {
    setBusy(true);
    try {
      const range = alignRange(
        unixAt(form.startDay, form.startHm),
        unixAt(form.endDay, form.endHm),
      );
      const result = await upsertTask({
        ...task,
        start: range.start,
        end: range.end,
        repeat: form.repeat,
        remindOffsets: form.remindOffsets,
      });
      if (result.warning) onWarning?.(result.warning);
      onClose();
      void onSaved();
    } catch (e) {
      onError(taskCommandError(e));
      setBusy(false);
    }
  }

  async function clearSchedule() {
    if (task.start == null && task.end == null) {
      onClose();
      return;
    }
    setBusy(true);
    try {
      await rescheduleTask(task.id, null, null);
      onClose();
      void onSaved();
    } catch (e) {
      onError(taskCommandError(e));
      setBusy(false);
    }
  }

  return (
    <div
      ref={rootRef}
      className={cn(
        "w-[22rem] max-w-[calc(100vw-2rem)] rounded-xl border bg-card p-4 shadow-[0_16px_40px_-12px_rgba(80,55,30,0.45)]",
        className,
      )}
      onPointerDown={(event) => event.stopPropagation()}
    >
      <TaskScheduleFields form={form} setForm={setForm} disabled={busy} />
      <div className="mt-4 flex items-center justify-end gap-2">
        <Button
          variant="outline"
          size="sm"
          className="mr-auto"
          disabled={busy}
          onClick={() => void clearSchedule()}
        >
          清除
        </Button>
        <Button size="sm" disabled={busy} onClick={() => void confirm()}>
          确定
        </Button>
      </div>
    </div>
  );
}
