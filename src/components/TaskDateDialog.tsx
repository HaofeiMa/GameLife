import { useState } from "react";
import { rescheduleTask, upsertTask, type TaskView } from "../lib/api";
import { localDayOf, localTimeOf, taskCommandError } from "../lib/taskBoard";
import {
  ALLOWED_REMIND_OFFSETS,
  TIME_STEPS,
  alignRange,
  defaultRange,
  remindToggle,
  unixAt,
} from "../lib/taskSchedule";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";
import { Dialog } from "./ui/dialog";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { Select } from "./ui/select";

function snapHm(hm: string): string {
  if (TIME_STEPS.includes(hm)) return hm;
  const [h, m] = hm.split(":").map(Number);
  const hours = Number.isFinite(h) ? h : 0;
  const minutes = Number.isFinite(m) ? m : 0;
  const mins = Math.floor((hours * 60 + minutes) / 15) * 15;
  const clamped = Math.min(23 * 60 + 45, Math.max(0, mins));
  const hh = Math.floor(clamped / 60);
  const mm = clamped % 60;
  return `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
}

function remindLabel(offset: number): string {
  return offset === 0 ? "准时" : `${offset} 分钟`;
}

type Form = {
  startDay: string;
  startHm: string;
  endDay: string;
  endHm: string;
  repeat: string;
  remindOffsets: number[];
};

function formFromTask(task: TaskView, nowSec: number): Form {
  const range =
    task.start != null && task.end != null
      ? { start: task.start, end: task.end }
      : defaultRange(nowSec);
  return {
    startDay: localDayOf(range.start),
    startHm: snapHm(localTimeOf(range.start)),
    endDay: localDayOf(range.end),
    endHm: snapHm(localTimeOf(range.end)),
    repeat: task.repeat || "none",
    remindOffsets: [...task.remindOffsets],
  };
}

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
  const [form, setForm] = useState<Form>(() => formFromTask(task, Date.now() / 1000));
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
      <div className="flex flex-col gap-4">
        <Row
          label="开始"
          day={form.startDay}
          hm={form.startHm}
          disabled={busy}
          onDay={(startDay) => setForm({ ...form, startDay })}
          onHm={(startHm) => setForm({ ...form, startHm })}
        />
        <Row
          label="结束"
          day={form.endDay}
          hm={form.endHm}
          disabled={busy}
          onDay={(endDay) => setForm({ ...form, endDay })}
          onHm={(endHm) => setForm({ ...form, endHm })}
        />
        <div className="flex flex-col gap-1.5">
          <Label>提醒</Label>
          <div className="flex flex-wrap gap-1.5">
            {ALLOWED_REMIND_OFFSETS.map((offset) => {
              const on = form.remindOffsets.includes(offset);
              return (
                <Button
                  key={offset}
                  type="button"
                  size="sm"
                  variant={on ? "primary" : "outline"}
                  disabled={busy}
                  aria-pressed={on}
                  onClick={() =>
                    setForm({
                      ...form,
                      remindOffsets: remindToggle(form.remindOffsets, offset),
                    })
                  }
                >
                  {remindLabel(offset)}
                </Button>
              );
            })}
          </div>
        </div>
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="task-date-repeat">重复</Label>
          <Select
            id="task-date-repeat"
            value={form.repeat}
            disabled={busy}
            onChange={(e) => setForm({ ...form, repeat: e.target.value })}
          >
            <option value="none">无</option>
            <option value="daily">每天</option>
            <option value="weekly">每周</option>
            <option value="monthly">每月</option>
          </Select>
        </div>
      </div>
    </Dialog>
  );
}

function Row({
  label,
  day,
  hm,
  disabled,
  onDay,
  onHm,
}: {
  label: string;
  day: string;
  hm: string;
  disabled: boolean;
  onDay: (day: string) => void;
  onHm: (hm: string) => void;
}) {
  const dayId = `task-date-${label}-day`;
  const hmId = `task-date-${label}-hm`;
  return (
    <div className="grid grid-cols-[2.5rem_1fr_7.5rem] items-center gap-2">
      <Label htmlFor={dayId}>{label}</Label>
      <Input
        id={dayId}
        type="date"
        value={day}
        disabled={disabled}
        onChange={(e) => onDay(e.target.value)}
        required
      />
      <Select
        id={hmId}
        value={hm}
        disabled={disabled}
        className={cn("min-w-0")}
        onChange={(e) => onHm(e.target.value)}
      >
        {TIME_STEPS.map((step) => (
          <option key={step} value={step}>
            {step}
          </option>
        ))}
      </Select>
    </div>
  );
}
