import { useCallback, useRef, useState } from "react";
import {
  clampListWidth,
  LIST_MAX,
  LIST_MIN,
  readStoredListWidth,
  writeStoredListWidth,
} from "../lib/taskSplit";

/**
 * Width of the left list pane on 任务. Geometry, stored in localStorage.
 */
export function useTaskSplit() {
  const [listWidth, setListWidth] = useState(() =>
    readStoredListWidth(window.localStorage),
  );
  const [dragging, setDragging] = useState(false);
  const draggingRef = useRef(false);
  const widthRef = useRef(listWidth);
  widthRef.current = listWidth;
  const splitRef = useRef<HTMLDivElement>(null);

  const persistWidth = useCallback((width: number) => {
    writeStoredListWidth(window.localStorage, width);
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
    setListWidth(clampListWidth(e.clientX - box.left));
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
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        next = clampListWidth(widthRef.current - step);
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        next = clampListWidth(widthRef.current + step);
      } else if (e.key === "Home") {
        e.preventDefault();
        next = LIST_MIN;
      } else if (e.key === "End") {
        e.preventDefault();
        next = LIST_MAX;
      }
      if (next == null) return;
      setListWidth(next);
      persistWidth(next);
    },
    [persistWidth],
  );

  return {
    listWidth,
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
