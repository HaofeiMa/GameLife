import { cn } from "../../lib/utils";

export interface ProgressProps {
  /** 0–100. Values outside the range are clamped. */
  value: number;
  className?: string;
  barClassName?: string;
  /** Any CSS `background` value — a category token, or a gradient. */
  color?: string;
  "aria-label"?: string;
}

export function Progress({
  value,
  className,
  barClassName,
  color,
  ...aria
}: ProgressProps) {
  const pct = Math.max(0, Math.min(100, Number.isFinite(value) ? value : 0));
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(pct)}
      aria-label={aria["aria-label"]}
      className={cn("h-1.5 w-full overflow-hidden rounded-full bg-track", className)}
    >
      <div
        className={cn(
          "h-full rounded-full bg-primary transition-[width] duration-500 ease-out",
          barClassName,
        )}
        style={{ width: `${pct}%`, background: color }}
      />
    </div>
  );
}
