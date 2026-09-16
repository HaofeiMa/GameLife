import { X } from "lucide-react";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/utils";

const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "textarea:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

export interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  /** Extra classes for the panel — use to widen a specific dialog. */
  className?: string;
  /** Replace the default title row. `title` remains the accessible name. */
  header?: ReactNode;
  chrome?: "muted" | "plain" | "float";
  /** Backdrop click. Defaults to `onClose`. Escape still calls `onClose`. */
  onDismiss?: () => void;
  /** Sibling overlay inside this dialog (not a nested Dialog). */
  layer?: ReactNode;
}

/**
 * Modal dialog. Hand-rolled rather than pulled from Radix: a focus trap,
 * Esc handling and a scroll lock are the whole requirement. Do not nest
 * dialogs.
 */
export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  className,
  header,
  chrome = "muted",
  onDismiss,
  layer,
}: DialogProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    if (!open) return;

    restoreRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;

    const panel = panelRef.current;
    const firstField = panel?.querySelector<HTMLElement>(FOCUSABLE);
    (firstField ?? panel)?.focus();

    function onEscape(event: KeyboardEvent) {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      event.stopPropagation();
      onCloseRef.current();
    }

    function onTab(event: KeyboardEvent) {
      if (event.key !== "Tab") return;

      const node = panelRef.current;
      if (!node) return;
      const items = Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
        (el) => el.offsetParent !== null,
      );
      if (items.length === 0) {
        event.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      if (event.shiftKey) {
        if (active === first || !node.contains(active)) {
          event.preventDefault();
          last.focus();
        }
      } else if (active === last || !node.contains(active)) {
        event.preventDefault();
        first.focus();
      }
    }

    document.addEventListener("keydown", onEscape);
    document.addEventListener("keydown", onTab, true);
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    return () => {
      document.removeEventListener("keydown", onEscape);
      document.removeEventListener("keydown", onTab, true);
      document.body.style.overflow = previousOverflow;
      restoreRef.current?.focus();
    };
  }, [open]);

  if (!open) return null;

  return createPortal(
    <div className="fixed inset-0 z-[100] flex items-center justify-center p-6">
      <div
        className="animate-fade-in fixed inset-0 bg-black/45 backdrop-blur-[2px]"
        onClick={onDismiss ?? onClose}
        aria-hidden
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={description ? descriptionId : undefined}
        tabIndex={-1}
        className="relative z-10 outline-none"
      >
      <div
        className={cn(
          "animate-rise flex max-h-[85vh] w-full max-w-md flex-col outline-none",
          chrome === "float"
            ? "overflow-visible border-0 bg-transparent p-0 shadow-none"
            : cn(
                "overflow-hidden rounded-xl border shadow-2xl",
                chrome === "plain" ? "bg-card" : "bg-background",
              ),
          className,
        )}
      >
        {header ? (
          <>
            <h2 id={titleId} className="sr-only">
              {title}
            </h2>
            {header}
          </>
        ) : chrome === "float" ? (
          <h2 id={titleId} className="sr-only">
            {title}
          </h2>
        ) : (
          <div
            className={cn(
              "flex items-start justify-between gap-4 px-5 py-4",
              chrome === "plain" ? "bg-card" : "border-b bg-muted/30",
            )}
          >
            <div className="space-y-1">
              <h2 id={titleId} className="text-sm font-semibold leading-tight">
                {title}
              </h2>
              {description && (
                <div
                  id={descriptionId}
                  className="text-xs leading-relaxed text-muted-foreground"
                >
                  {description}
                </div>
              )}
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="关闭"
              className="-mr-1 -mt-1 rounded-md p-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
            >
              <X className="size-4" />
            </button>
          </div>
        )}
        {children && (
          <div
            className={
              chrome === "float"
                ? "contents"
                : "min-h-0 flex-1 overflow-y-auto px-5 py-4"
            }
          >
            {children}
          </div>
        )}
        {footer && (
          <div
            className={cn(
              "flex items-center justify-end gap-2 px-5 py-3",
              chrome === "plain" ? "rounded-b-xl bg-card" : "border-t bg-muted/30",
            )}
          >
            {footer}
          </div>
        )}
      </div>
        {layer && (
          <div className="absolute left-1/2 top-1/2 z-20 -translate-x-1/2 -translate-y-1/2">
            {layer}
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
