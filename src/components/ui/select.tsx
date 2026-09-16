import { ChevronDown } from "lucide-react";
import type * as React from "react";
import { cn } from "../../lib/utils";

/**
 * A styled native <select>. macOS renders the popup itself, which gives
 * correct keyboard, VoiceOver and trackpad behaviour for free — a
 * custom listbox would only be worth it if the options needed markup.
 */
/** `size` is omitted from the native props: it is the HTML row-count
 *  attribute there, and the size ladder here. */
export interface SelectProps
  extends Omit<React.ComponentProps<"select">, "size"> {
  size?: "sm" | "default";
}

export function Select({
  className,
  children,
  size = "default",
  ...props
}: SelectProps) {
  return (
    <div className={cn("relative", className)}>
      <select
        className={cn(
          // The mockup's .fin, with the chevron the mockup paints in --dim2.
          "w-full cursor-default appearance-none rounded-[10px] border border-pip bg-loot text-[14.5px] text-btn-ink transition-colors",
          "disabled:cursor-not-allowed disabled:opacity-50",
          size === "sm"
            ? "h-[30px] pl-2 pr-7 text-[13.5px]"
            : "h-[34px] pl-3 pr-8",
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDown
        className={cn(
          "pointer-events-none absolute top-1/2 -translate-y-1/2 text-dim2",
          size === "sm" ? "right-2 size-3.5" : "right-2.5 size-4",
        )}
        aria-hidden
      />
    </div>
  );
}
