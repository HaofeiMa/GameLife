import type * as React from "react";
import { cn } from "../../lib/utils";

export interface EmptyStateProps {
  icon?: React.ReactNode;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}

export function EmptyState({
  icon,
  title,
  description,
  action,
  className,
}: EmptyStateProps) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center rounded-xl border border-dashed px-6 py-10 text-center",
        className,
      )}
    >
      {icon && (
        <div className="mb-3 flex size-11 items-center justify-center rounded-full bg-muted text-muted-foreground">
          {icon}
        </div>
      )}
      <p className="text-sm font-medium">{title}</p>
      {description && (
        <p className="mt-1 max-w-md text-xs leading-relaxed text-muted-foreground">
          {description}
        </p>
      )}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

/** One-line empty state for a panel that is only occasionally populated. */
export function EmptyLine({ children, className }: React.ComponentProps<"p">) {
  return (
    <p
      className={cn(
        "rounded-lg border border-dashed px-4 py-6 text-center text-xs text-muted-foreground",
        className,
      )}
    >
      {children}
    </p>
  );
}
