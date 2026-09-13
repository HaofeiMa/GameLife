import { useCallback, useRef, useState } from "react";

export const SIDEBAR_MIN = 184;
export const SIDEBAR_MAX = 340;
export const SIDEBAR_DEFAULT = 208;
export const SIDEBAR_COLLAPSED = 64;

const STORAGE_KEY = "gl-sidebar-width";

function clamp(value: number): number {
  return Math.min(SIDEBAR_MAX, Math.max(SIDEBAR_MIN, Math.round(value)));
}

function readStored(): number {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    const parsed = raw == null ? NaN : Number(raw);
    return Number.isFinite(parsed) ? clamp(parsed) : SIDEBAR_DEFAULT;
  } catch {
    return SIDEBAR_DEFAULT;
  }
}

/**
 * Width of the navigation column, draggable at the divider.
 *
 * A window-geometry preference rather than a product setting, so it lives in
 * localStorage next to the theme cache instead of config.json — no IPC round
 * trip while dragging, and nothing to migrate.
 */
export function useSidebarWidth() {
  const [width, setWidth] = useState(readStored);
  const [dragging, setDragging] = useState(false);
  const draggingRef = useRef(false);
  const widthRef = useRef(width);
  widthRef.current = width;

  const onPointerDown = useCallback((e: React.PointerEvent<HTMLElement>) => {
    e.preventDefault();
    draggingRef.current = true;
    setDragging(true);
    e.currentTarget.setPointerCapture(e.pointerId);
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent<HTMLElement>) => {
    if (!draggingRef.current) return;
    // The column starts at the window's left edge, so clientX is the width.
    setWidth(clamp(e.clientX));
  }, []);

  const endDrag = useCallback((e: React.PointerEvent<HTMLElement>) => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    setDragging(false);
    try {
      e.currentTarget.releasePointerCapture(e.pointerId);
    } catch {
      /* pointer already released */
    }
    try {
      window.localStorage.setItem(STORAGE_KEY, String(widthRef.current));
    } catch {
      /* storage disabled — width still applies for this session */
    }
  }, []);

  const onKeyDown = useCallback((e: React.KeyboardEvent<HTMLElement>) => {
    const step = e.shiftKey ? 32 : 8;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      setWidth((w) => clamp(w - step));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      setWidth((w) => clamp(w + step));
    } else if (e.key === "Home") {
      e.preventDefault();
      setWidth(SIDEBAR_MIN);
    } else if (e.key === "End") {
      e.preventDefault();
      setWidth(SIDEBAR_MAX);
    } else {
      return;
    }
    try {
      window.localStorage.setItem(STORAGE_KEY, String(widthRef.current));
    } catch {
      /* ignore */
    }
  }, []);

  return {
    width,
    dragging,
    handleProps: {
      onPointerDown,
      onPointerMove,
      onPointerUp: endDrag,
      onPointerCancel: endDrag,
      onKeyDown,
    },
  };
}
