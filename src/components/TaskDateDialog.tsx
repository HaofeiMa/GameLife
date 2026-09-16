import { useState } from "react";
import { rescheduleTask, upsertTask, type TaskView } from "../lib/api";
import { taskCommandError } from "../lib/taskBoard";
import { alignRange, unixAt } from "../lib/taskSchedule";
import { formFromTask, TaskScheduleFields } from "./TaskScheduleFields";
import { Button } from "./ui/button";
import { Dialog } from "./ui/dialog";

export function TaskDateDialog({
  task,
  onClose,
  onSaved,
  onError,
}: {
  task: TaskView | null;
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  onError: (message: string) => void;
}) {
  if (!task) {
    return <Dialog open={false} onClose={onClose} title="更改日期" />;
  }
  return (
    <TaskDateDialogBody
      key={task.id}
      task={task}
      onClose={onClose}
      onSaved={onSaved}
      onError={onError}
    />
  );
}

function TaskDateDialogBody({
  task,
  onClose,
  onSaved,
  onError,
}: {
  task: TaskView;
  onClose: () => void;
  onSaved: () => Promise<void> | void;
  onError: (message: string) => void;
}) {
  const [form, setForm] = useState(() => formFromTask(task, Date.now() / 1000));
  const [busy, setBusy] = useState(false);

  async function confirm() {
    setBusy(true);
    try {
      const range = alignRange(
        unixAt(form.startDay, form.startHm),
        unixAt(form.endDay, form.endHm),
      );
      await upsertTask({
        ...task,
        start: range.start,
        end: range.end,
        repeat: form.repeat,
        remindOffsets: form.remindOffsets,
      });
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
    <Dialog
      open
      onClose={onClose}
      title="更改日期"
      className="max-w-lg"
      chrome="plain"
      footer={
        <>
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
        </>
      }
    >
      <div className="rounded-xl border bg-card p-3">
        <TaskScheduleFields form={form} setForm={setForm} disabled={busy} />
      </div>
    </Dialog>
  );
}
