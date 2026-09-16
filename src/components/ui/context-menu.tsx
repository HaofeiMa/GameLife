import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/utils";

export function ContextMenu({
  x,
  y,
  open,
  onClose,
  children,
}: {
  x: number;
  y: number;
  open: boolean;
  onClose: () => void;
  children: ReactNode;
}): ReactNode {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;

    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
    }

    function onPointerDown(event: PointerEvent) {
      const node = menuRef.current;
      if (node && !node.contains(event.target as Node)) {
        onClose();
      }
    }

    document.addEventListener("keydown", onKeyDown, true);
    document.addEventListener("pointerdown", onPointerDown, true);
    return () => {
      document.removeEventListener("keydown", onKeyDown, true);
      document.removeEventListener("pointerdown", onPointerDown, true);
    };
  }, [open, onClose]);

  if (!open) return null;

  const left = Math.min(x, Math.max(8, window.innerWidth - 236));
  const top = Math.min(y, Math.max(8, window.innerHeight - 12));

  return createPortal(
    <div
      ref={menuRef}
      role="menu"
      style={{ left, top }}
      className="fixed z-[110] min-w-[220px] rounded-[11px] border border-border bg-popover py-1 shadow-[0_10px_26px_-18px_rgba(120,95,60,0.7)]"
    >
      {children}
    </div>,
    document.body,
  );
}

export function ContextMenuItem({
  children,
  destructive,
  disabled,
  onSelect,
}: {
  children: ReactNode;
  destructive?: boolean;
  disabled?: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      onClick={() => {
        if (disabled) return;
        onSelect();
      }}
      className={cn(
        "flex w-full items-center px-3 py-1.5 text-left text-[14.5px]",
        disabled
          ? "cursor-default text-muted-foreground"
          : destructive
            ? "text-destructive hover:bg-accent"
            : "text-foreground hover:bg-accent",
      )}
    >
      {children}
    </button>
  );
}

export function ContextMenuSub({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div
      className="relative"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        type="button"
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between gap-3 px-3 py-1.5 text-left text-[14.5px] text-foreground hover:bg-accent"
      >
        {label}
        <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
      </button>
      {open && (
        <div
          role="menu"
          className="absolute top-0 left-full z-10 min-w-[160px] rounded-[11px] border border-border bg-popover py-1 shadow-[0_10px_26px_-18px_rgba(120,95,60,0.7)]"
        >
          {children}
        </div>
      )}
    </div>
  );
}
