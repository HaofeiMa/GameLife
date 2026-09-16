import { Check, ChevronLeft, ChevronRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { TaskView } from "../lib/api";
import { localDayOf, localTimeOf } from "../lib/taskBoard";
import {
  applyScheduleDay,
  formatDaySlash,
  formatHmMeridiem,
  initialDayLock,
  monthMatrix,
  remindItemLabel,
  remindSummary,
  repeatItemLabel,
  shiftMonth,
} from "../lib/taskDateField";
import {
  ALLOWED_REMIND_OFFSETS,
  TIME_STEPS,
  defaultRange,
  remindToggle,
} from "../lib/taskSchedule";
import { cn } from "../lib/utils";
import { Label } from "./ui/label";
import { FieldOption, FieldPopover, FieldTrigger } from "./ui/field-popover";

const WEEKDAYS = ["一", "二", "三", "四", "五", "六", "日"];
const REPEAT_KEYS = ["none", "daily", "weekly", "monthly"] as const;

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

type OpenField =
  | "startDay"
  | "startHm"
  | "endDay"
  | "endHm"
  | "remind"
  | "repeat"
  | null;

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
  const [open, setOpen] = useState<OpenField>(null);
  const [dayLock, setDayLock] = useState(() =>
    initialDayLock(form.startDay, form.endDay),
  );

  function toggle(field: Exclude<OpenField, null>) {
    setOpen((cur) => (cur === field ? null : field));
  }

  function setDay(which: "start" | "end", day: string) {
    const next = applyScheduleDay(form, dayLock, which, day);
    setDayLock(next.lock);
    setForm({ ...form, startDay: next.startDay, endDay: next.endDay });
  }

  return (
    <div id={id} tabIndex={id ? -1 : undefined} className="flex flex-col gap-3 outline-none">
      <DayTimeRow
        label="开始"
        day={form.startDay}
        hm={form.startHm}
        disabled={disabled}
        dayOpen={open === "startDay"}
        timeOpen={open === "startHm"}
        onToggleDay={() => toggle("startDay")}
        onToggleTime={() => toggle("startHm")}
        onClose={() => setOpen(null)}
        onDay={(startDay) => setDay("start", startDay)}
        onHm={(startHm) => setForm({ ...form, startHm })}
      />
      <DayTimeRow
        label="结束"
        day={form.endDay}
        hm={form.endHm}
        disabled={disabled}
        dayOpen={open === "endDay"}
        timeOpen={open === "endHm"}
        onToggleDay={() => toggle("endDay")}
        onToggleTime={() => toggle("endHm")}
        onClose={() => setOpen(null)}
        onDay={(endDay) => setDay("end", endDay)}
        onHm={(endHm) => setForm({ ...form, endHm })}
      />
      <MenuRow label="提醒">
        <RemindField
          offsets={form.remindOffsets}
          disabled={disabled}
          open={open === "remind"}
          onToggle={() => toggle("remind")}
          onClose={() => setOpen(null)}
          onChange={(remindOffsets) => setForm({ ...form, remindOffsets })}
        />
      </MenuRow>
      <MenuRow label="重复">
        <RepeatField
          value={form.repeat}
          disabled={disabled}
          open={open === "repeat"}
          onToggle={() => toggle("repeat")}
          onClose={() => setOpen(null)}
          onChange={(repeat) => setForm({ ...form, repeat })}
        />
      </MenuRow>
    </div>
  );
}

function MenuRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid grid-cols-[2.75rem_minmax(0,1fr)] items-center gap-2">
      <Label>{label}</Label>
      {children}
    </div>
  );
}

function DayTimeRow({
  label,
  day,
  hm,
  disabled,
  dayOpen,
  timeOpen,
  onToggleDay,
  onToggleTime,
  onClose,
  onDay,
  onHm,
}: {
  label: string;
  day: string;
  hm: string;
  disabled: boolean;
  dayOpen: boolean;
  timeOpen: boolean;
  onToggleDay: () => void;
  onToggleTime: () => void;
  onClose: () => void;
  onDay: (day: string) => void;
  onHm: (hm: string) => void;
}) {
  const dayRef = useRef<HTMLButtonElement>(null);
  const timeRef = useRef<HTMLButtonElement>(null);
  const selectedRef = useRef<HTMLButtonElement>(null);
  const [y, m] = day.split("-").map(Number);
  const [view, setView] = useState({ year: y, month: m });

  useEffect(() => {
    if (dayOpen) setView({ year: y, month: m });
  }, [dayOpen, y, m]);

  useEffect(() => {
    if (timeOpen) selectedRef.current?.scrollIntoView({ block: "center" });
  }, [timeOpen]);

  const todayIso = localDayOf(Date.now() / 1000);
  const cells = monthMatrix(view.year, view.month);

  return (
    <div className="grid grid-cols-[2.75rem_minmax(0,1fr)_8.25rem] items-center gap-2">
      <Label>{label}</Label>
      <FieldTrigger
        label={`${label}日期`}
        disabled={disabled}
        open={dayOpen}
        onClick={onToggleDay}
        triggerRef={dayRef}
      >
        {formatDaySlash(day)}
      </FieldTrigger>
      <FieldPopover open={dayOpen} onClose={onClose} triggerRef={dayRef} width={252}>
        <div className="p-2.5">
          <div className="mb-2 flex items-center justify-between px-1">
            <span className="text-[15px] font-semibold text-foreground">
              {view.year}年{view.month}月
            </span>
            <div className="flex gap-0.5">
              <button
                type="button"
                aria-label="上个月"
                className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={() => setView((v) => shiftMonth(v.year, v.month, -1))}
              >
                <ChevronLeft className="size-4" />
              </button>
              <button
                type="button"
                aria-label="下个月"
                className="rounded-md p-1 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={() => setView((v) => shiftMonth(v.year, v.month, 1))}
              >
                <ChevronRight className="size-4" />
              </button>
            </div>
          </div>
          <div className="grid grid-cols-7 text-center text-[11.5px] text-muted-foreground">
            {WEEKDAYS.map((w) => (
              <div key={w} className="py-1">
                {w}
              </div>
            ))}
          </div>
          <div className="grid grid-cols-7 text-center">
            {cells.map((cell) => {
              const selected = cell.iso === day;
              const today = cell.iso === todayIso;
              return (
                <button
                  key={cell.iso}
                  type="button"
                  onClick={() => {
                    onDay(cell.iso);
                    onClose();
                  }}
                  className={cn(
                    "mx-auto flex size-8 items-center justify-center rounded-full text-[14.5px]",
                    !cell.inMonth && "text-cal-future-ink",
                    cell.inMonth && !selected && "text-foreground hover:bg-accent",
                    today && !selected && "text-primary",
                    selected && "bg-primary font-semibold text-primary-foreground",
                  )}
                >
                  {cell.date}
                </button>
              );
            })}
          </div>
        </div>
      </FieldPopover>
      <FieldTrigger
        label={`${label}时间`}
        disabled={disabled}
        open={timeOpen}
        onClick={onToggleTime}
        triggerRef={timeRef}
      >
        {formatHmMeridiem(hm)}
      </FieldTrigger>
      <FieldPopover open={timeOpen} onClose={onClose} triggerRef={timeRef}>
        <div className="max-h-56 overflow-y-auto py-1">
          {TIME_STEPS.map((step) => (
            <button
              key={step}
              ref={step === hm ? selectedRef : undefined}
              type="button"
              role="option"
              aria-selected={step === hm}
              onClick={() => {
                onHm(step);
                onClose();
              }}
              className={cn(
                "flex w-full items-center justify-between px-3 py-[7px] text-left text-[14.5px] hover:bg-accent",
                step === hm ? "text-primary" : "text-foreground",
              )}
            >
              <span>{formatHmMeridiem(step)}</span>
              {step === hm && <Check className="size-3.5 shrink-0" strokeWidth={2.5} />}
            </button>
          ))}
        </div>
      </FieldPopover>
    </div>
  );
}

function RemindField({
  offsets,
  disabled,
  open,
  onToggle,
  onClose,
  onChange,
}: {
  offsets: number[];
  disabled: boolean;
  open: boolean;
  onToggle: () => void;
  onClose: () => void;
  onChange: (offsets: number[]) => void;
}) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  return (
    <>
      <FieldTrigger
        label="提醒"
        disabled={disabled}
        open={open}
        onClick={onToggle}
        triggerRef={triggerRef}
      >
        {remindSummary(offsets)}
      </FieldTrigger>
      <FieldPopover open={open} onClose={onClose} triggerRef={triggerRef}>
        <div className="py-1">
          {ALLOWED_REMIND_OFFSETS.map((offset) => (
            <FieldOption
              key={offset}
              selected={offsets.includes(offset)}
              onSelect={() => onChange(remindToggle(offsets, offset))}
            >
              {remindItemLabel(offset)}
            </FieldOption>
          ))}
        </div>
      </FieldPopover>
    </>
  );
}

function RepeatField({
  value,
  disabled,
  open,
  onToggle,
  onClose,
  onChange,
}: {
  value: string;
  disabled: boolean;
  open: boolean;
  onToggle: () => void;
  onClose: () => void;
  onChange: (repeat: string) => void;
}) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  return (
    <>
      <FieldTrigger
        label="重复"
        disabled={disabled}
        open={open}
        onClick={onToggle}
        triggerRef={triggerRef}
      >
        {repeatItemLabel(value)}
      </FieldTrigger>
      <FieldPopover open={open} onClose={onClose} triggerRef={triggerRef}>
        <div className="py-1">
          {REPEAT_KEYS.map((key) => (
            <FieldOption
              key={key}
              selected={value === key}
              onSelect={() => {
                onChange(key);
                onClose();
              }}
            >
              {repeatItemLabel(key)}
            </FieldOption>
          ))}
        </div>
      </FieldPopover>
    </>
  );
}
