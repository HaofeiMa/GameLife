import type * as React from "react";
import { cn } from "../../lib/utils";

const TONES = {
  default: "from-muted to-muted/50 text-muted-foreground",
  primary: "from-primary/25 to-primary/5 text-primary",
  success: "from-success/25 to-success/5 text-success",
  warning: "from-warning/30 to-warning/5 text-warning",
  entertainment: "from-entertainment/25 to-entertainment/5 text-entertainment",
} as const;

export interface StatTileProps {
  label: string;
  value: React.ReactNode;
  sub?: React.ReactNode;
  icon?: React.ReactNode;
  tone?: keyof typeof TONES;
  className?: string;
  title?: string;
}

/** Compact KPI tile shared by 今日 and 统计. */
export function StatTile({
  label,
  value,
  sub,
  icon,
  tone = "default",
  className,
  title,
}: StatTileProps) {
  return (
    <div
      title={title}
      className={cn(
        "flex items-start gap-3 rounded-xl border bg-card p-4 transition-all duration-200",
        "hover:-translate-y-0.5 hover:border-primary/25 hover:shadow-md hover:shadow-primary/5",
        className,
      )}
    >
      {icon && (
        <div
          className={cn(
            "flex size-8 shrink-0 items-center justify-center rounded-lg bg-gradient-to-br ring-1 ring-inset ring-current/10",
            TONES[tone],
          )}
          aria-hidden
        >
          {icon}
        </div>
      )}
      <div className="min-w-0 space-y-0.5">
        <p className="text-xs text-muted-foreground">{label}</p>
        <p className="text-xl font-semibold leading-tight tabular-nums tracking-tight">
          {value}
        </p>
        {sub && (
          <p className="truncate text-xs text-muted-foreground">{sub}</p>
        )}
      </div>
    </div>
  );
}
