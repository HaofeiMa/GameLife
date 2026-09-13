import type * as React from "react";
import { cn } from "../../lib/utils";

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  icon?: React.ReactNode;
  title?: string;
}

export interface SegmentedProps<T extends string> {
  value: T;
  onChange: (value: T) => void;
  options: readonly SegmentedOption<T>[];
  size?: "sm" | "default";
  /** Stretch to the container and give every option an equal share. */
  fill?: boolean;
  className?: string;
  "aria-label": string;
}

/** Segmented control used for both view switching and filter chips. */
export function Segmented<T extends string>({
  value,
  onChange,
  options,
  size = "default",
  fill = false,
  className,
  "aria-label": ariaLabel,
}: SegmentedProps<T>) {
  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      className={cn(
        // The mockup's .seg: a warm trough, 11px, no border.
        "items-center gap-[2px] rounded-[11px] bg-seg p-[3px]",
        fill ? "flex w-full" : "inline-flex",
        className,
      )}
    >
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="tab"
            aria-selected={active}
            title={option.title}
            onClick={() => onChange(option.value)}
            className={cn(
              "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-[9px] font-medium transition-all duration-200",
              fill && "flex-1",
              size === "sm" ? "px-[9px] py-[3px] text-[11.5px]" : "px-3 py-1 text-[12.5px]",
              active
                ? "bg-card font-semibold text-foreground shadow-[0_2px_6px_-3px_rgba(120,95,60,0.45)]"
                : "text-ink-dim hover:text-foreground",
            )}
          >
            {option.icon}
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
