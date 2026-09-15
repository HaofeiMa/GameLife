import { useCallback, useRef, useState } from "react";
import {
  clampActualWidth,
  readStoredActualWidth,
  writeStoredActualWidth,
} from "../lib/timelinePlan";

/**
 * Width of the actual-slot column on 今日. Dragging only resizes the
 * plan-gutter layout; the ratio lives in localStorage like the sidebar.
 */
export function useTimelineSplit() {
  const [actualWidth, setActualWidth] = useState(() =>
    readStoredActualWidth(window.localStorage),
  );
  const [dragging, setDragging] = useState(false);
  const draggingRef = useRef(false);
  const widthRef = useRef(actualWidth);
  widthRef.current = actualWidth;
  const splitRef = useRef<HTMLDivElement>(null);

  const persistWidth = useCallback((width: number) => {
    writeStoredActualWidth(window.localStorage, width);
  }, []);

  const onPointerDown = useCallback((e: React.PointerEvent<HTMLElement>) => {
    e.preventDefault();
    draggingRef.current = true;
    setDragging(true);
    e.currentTarget.setPointerCapture(e.pointerId);
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent<HTMLElement>) => {
    if (!draggingRef.current) return;
    const box = splitRef.current?.getBoundingClientRect();
    if (!box) return;
    setActualWidth(clampActualWidth(box.right - e.clientX));
  }, []);

  const endDrag = useCallback(
    (e: React.PointerEvent<HTMLElement>) => {
      if (!draggingRef.current) return;
      draggingRef.current = false;
      setDragging(false);
      try {
        e.currentTarget.releasePointerCapture(e.pointerId);
      } catch {
        /* already released */
      }
      persistWidth(widthRef.current);
    },
    [persistWidth],
  );

  const onKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLElement>) => {
      const step = e.shiftKey ? 24 : 8;
      let next: number | null = null;
      if (e.key === "ArrowRight") {
        e.preventDefault();
        next = clampActualWidth(widthRef.current - step);
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        next = clampActualWidth(widthRef.current + step);
      } else if (e.key === "Home") {
        e.preventDefault();
        next = clampActualWidth(0);
      } else if (e.key === "End") {
        e.preventDefault();
        next = clampActualWidth(9_999);
      }
      if (next == null) return;
      setActualWidth(next);
      persistWidth(next);
    },
    [persistWidth],
  );

  return {
    actualWidth,
    dragging,
    splitRef,
    handleProps: {
      onPointerDown,
      onPointerMove,
      onPointerUp: endDrag,
      onPointerCancel: endDrag,
      onKeyDown,
    },
  };
}
