import type { TaskView } from "../lib/api";
import { localDayOf, localTimeOf } from "../lib/taskBoard";
import {
  ALLOWED_REMIND_OFFSETS,
  TIME_STEPS,
  defaultRange,
  remindToggle,
} from "../lib/taskSchedule";
import { cn } from "../lib/utils";
import { Button } from "./ui/button";
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

export type TaskScheduleForm = {
  startDay: string;
  startHm: string;
  endDay: string;
  endHm: string;
  repeat: string;
  remindOffsets: number[];
};

export function formFromTask(task: TaskView, nowSec: number): TaskScheduleForm {
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

export function TaskScheduleFields({
  form,
  setForm,
  disabled,
  id,
}: {
  form: TaskScheduleForm;
  setForm: (form: TaskScheduleForm) => void;
  disabled: boolean;
  id?: string;
}) {
  return (
    <div id={id} tabIndex={id ? -1 : undefined} className="flex flex-col gap-4 outline-none">
      <Row
        label="开始"
        day={form.startDay}
        hm={form.startHm}
        disabled={disabled}
        onDay={(startDay) => setForm({ ...form, startDay })}
        onHm={(startHm) => setForm({ ...form, startHm })}
      />
      <Row
        label="结束"
        day={form.endDay}
        hm={form.endHm}
        disabled={disabled}
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
                disabled={disabled}
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
          disabled={disabled}
          onChange={(e) => setForm({ ...form, repeat: e.target.value })}
        >
          <option value="none">无</option>
          <option value="daily">每天</option>
          <option value="weekly">每周</option>
          <option value="monthly">每月</option>
        </Select>
      </div>
    </div>
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
