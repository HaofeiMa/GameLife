import type * as React from "react";
import { cn } from "../../lib/utils";

export function Label({ className, ...props }: React.ComponentProps<"label">) {
  return (
    <label
      className={cn(
        // The mockup's .flab.
        "text-[11.5px] font-semibold leading-none text-btn-ink",
        className,
      )}
      {...props}
    />
  );
}
