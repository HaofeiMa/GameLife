import { useEffect, useRef, type ReactNode } from "react";
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

  const left = Math.min(x, Math.max(8, window.innerWidth - 208));
  const top = Math.min(y, Math.max(8, window.innerHeight - 12));

  return createPortal(
    <div
      ref={menuRef}
      role="menu"
      style={{ left, top }}
      className="fixed z-[110] min-w-[180px] rounded-[11px] border border-border bg-popover py-1 shadow-[0_10px_26px_-18px_rgba(120,95,60,0.7)]"
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
        "flex w-full items-center px-3 py-1.5 text-left text-[12.5px]",
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
