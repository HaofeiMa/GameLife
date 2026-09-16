import type * as React from "react";
import { cn } from "../../lib/utils";

export function Input({ className, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      className={cn(
        // The mockup's .fin: a 34px sunken field on the warm loot surface.
        "flex h-[34px] w-full rounded-[10px] border border-pip bg-loot px-3 text-[14.5px] text-btn-ink transition-colors",
        "placeholder:text-dim2 disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  );
}
