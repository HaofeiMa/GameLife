import { cn } from "../../lib/utils";

export interface ToastItem {
  id: number;
  text: string;
  tone?: "default" | "success" | "warning";
}

const TONE_CLASS = {
  default: "border-border bg-popover",
  success: "border-success/40 bg-success/10",
  warning: "border-warning/40 bg-warning/10",
} as const;

/**
 * Toast host. Deliberately dumb — App owns the queue and the dismissal
 * timer, so the feel-notice cursor logic stays in one place.
 */
export function Toaster({ toasts }: { toasts: ToastItem[] }) {
  return (
    <div
      aria-live="polite"
      className="pointer-events-none fixed left-1/2 top-3 z-[200] flex w-[min(90vw,22rem)] -translate-x-1/2 flex-col items-center gap-2"
    >
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={cn(
            "animate-slide-down w-full rounded-lg border px-4 py-2 text-center text-sm shadow-lg",
            TONE_CLASS[toast.tone ?? "default"],
          )}
        >
          {toast.text}
        </div>
      ))}
    </div>
  );
}
