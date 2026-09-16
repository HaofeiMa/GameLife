import { Check } from "lucide-react";
import {
  useEffect,
  useLayoutEffect,
  useState,
  type ReactNode,
  type RefObject,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/utils";

export function FieldPopover({
  open,
  onClose,
  triggerRef,
  width,
  className,
  children,
}: {
  open: boolean;
  onClose: () => void;
  triggerRef: RefObject<HTMLElement | null>;
  width?: number;
  className?: string;
  children: ReactNode;
}) {
  const [box, setBox] = useState({ left: 0, top: 0, width: 200, flip: false });

  useLayoutEffect(() => {
    if (!open || !triggerRef.current) return;
    const r = triggerRef.current.getBoundingClientRect();
    const w = Math.max(width ?? 0, r.width);
    const left = Math.min(Math.max(8, r.left), window.innerWidth - w - 8);
    const spaceBelow = window.innerHeight - r.bottom - 8;
    const flip = spaceBelow < 260 && r.top > spaceBelow;
    setBox({ left, top: flip ? r.top - 6 : r.bottom + 6, width: w, flip });
  }, [open, triggerRef, width]);

  useEffect(() => {
    if (!open) return;

    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopImmediatePropagation();
      onClose();
    }

    function onPointerDown(event: PointerEvent) {
      const t = event.target as Node;
      if (triggerRef.current?.contains(t)) return;
      const pop = document.getElementById("task-field-popover");
      if (pop?.contains(t)) return;
      onClose();
    }

    document.addEventListener("keydown", onKeyDown, true);
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => {
      document.removeEventListener("keydown", onKeyDown, true);
      document.removeEventListener("pointerdown", onPointerDown, true);
    };
  }, [open, onClose, triggerRef]);

  if (!open) return null;

  return createPortal(
    <div
      id="task-field-popover"
      role="listbox"
      style={{
        left: box.left,
        top: box.flip ? undefined : box.top,
        bottom: box.flip ? window.innerHeight - box.top : undefined,
        width: box.width,
      }}
      className={cn(
        "fixed z-[120] overflow-hidden rounded-[11px] border border-border bg-popover shadow-[0_10px_26px_-18px_rgba(120,95,60,0.7)]",
        className,
      )}
    >
      {children}
    </div>,
    document.body,
  );
}

export function FieldTrigger({
  id,
  label,
  disabled,
  open,
  onClick,
  triggerRef,
  children,
}: {
  id?: string;
  label: string;
  disabled: boolean;
  open: boolean;
  onClick: () => void;
  triggerRef: RefObject<HTMLButtonElement | null>;
  children: ReactNode;
}) {
  return (
    <button
      ref={triggerRef}
      id={id}
      type="button"
      disabled={disabled}
      aria-label={label}
      aria-expanded={open}
      aria-haspopup="listbox"
      onClick={onClick}
      className={cn(
        "flex h-[34px] w-full min-w-0 items-center rounded-[10px] border border-pip bg-loot px-2.5 text-left text-[14.5px] text-btn-ink",
        "hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50",
      )}
    >
      <span className="min-w-0 flex-1 truncate tabular-nums">{children}</span>
    </button>
  );
}

export function FieldOption({
  selected,
  onSelect,
  children,
}: {
  selected: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      onClick={onSelect}
      className={cn(
        "flex w-full items-center justify-between gap-3 px-3 py-[7px] text-left text-[14.5px] hover:bg-accent",
        selected ? "text-primary" : "text-foreground",
      )}
    >
      <span className="min-w-0 flex-1">{children}</span>
      {selected && <Check className="size-3.5 shrink-0" strokeWidth={2.5} />}
    </button>
  );
}
