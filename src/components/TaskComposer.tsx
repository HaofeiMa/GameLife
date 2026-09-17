import { useEffect, useMemo, useRef, useState } from "react";
import type { TaskListView } from "../lib/api";
import { isPresetListId } from "../lib/taskBoard";
import {
  applyHashPick,
  committedTag,
  deleteTagIfBackspace,
  filterHashLists,
  hashQuery,
  paintSegments,
} from "../lib/taskComposer";
import { cn } from "../lib/utils";

function tagLabel(list: TaskListView): string {
  if (!isPresetListId(list.id)) return list.name;
  switch (list.role) {
    case "mainline":
      return "主线";
    case "side":
      return "支线";
    case "longterm":
      return "长期";
    case "chore":
      return "杂项";
    default:
      return list.name;
  }
}

export function TaskComposer({
  value,
  lists,
  spans,
  onChange,
  onSubmit,
}: {
  value: string;
  lists: TaskListView[];
  spans: { start: number; end: number }[];
  onChange: (value: string) => void;
  onSubmit: () => void;
}) {
  const areaRef = useRef<HTMLTextAreaElement | null>(null);
  const [caret, setCaret] = useState(0);
  const [menuIndex, setMenuIndex] = useState(0);
  const [composing, setComposing] = useState(false);

  const tag = useMemo(() => committedTag(value, lists), [value, lists]);
  const query = hashQuery(value, caret, tag);
  const menuOpen = Boolean(query) && !composing;
  const filtered = useMemo(
    () => (query ? filterHashLists(lists, query.query) : []),
    [lists, query],
  );

  useEffect(() => {
    setMenuIndex(0);
  }, [query?.start, query?.query]);

  useEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${el.scrollHeight}px`;
  }, [value]);

  function syncCaret(el: HTMLTextAreaElement) {
    setCaret(el.selectionStart ?? 0);
  }

  function setBoth(next: string, nextCaret: number) {
    onChange(next);
    setCaret(nextCaret);
    requestAnimationFrame(() => {
      const el = areaRef.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(nextCaret, nextCaret);
    });
  }

  function pick(name: string) {
    if (!query) return;
    const next = applyHashPick(value, query.start, caret, name);
    setBoth(next.text, next.caret);
  }

  const segs = paintSegments(value, spans, tag);

  return (
    <div className="relative">
      <div className="relative min-h-[34px] rounded-[10px] border border-pip bg-loot px-3 py-[6px] text-[14.5px] leading-[22px] text-btn-ink">
        <div
          className="pointer-events-none absolute inset-0 overflow-hidden whitespace-pre-wrap break-words px-3 py-[6px] text-[14.5px] leading-[22px]"
          aria-hidden
        >
          {value ? (
            segs.map((seg, i) => (
              <span
                key={`${i}-${seg.tone}`}
                className={cn(
                  seg.tone === "time" && "text-energy",
                  seg.tone === "tag" &&
                    "rounded-[6px] bg-energy/15 px-0.5 text-energy",
                  seg.tone === "plain" && "text-btn-ink",
                )}
              >
                {seg.text}
              </span>
            ))
          ) : (
            <span className="text-dim2">明天上午十点到十二点，写方法节 #主线</span>
          )}
        </div>
        <textarea
          ref={areaRef}
          value={value}
          rows={1}
          aria-label="添加任务"
          spellCheck={false}
          className="relative block min-h-[22px] w-full resize-none bg-transparent text-[14.5px] leading-[22px] text-transparent caret-foreground outline-none"
          onChange={(e) => {
            onChange(e.target.value);
            syncCaret(e.target);
          }}
          onSelect={(e) => syncCaret(e.currentTarget)}
          onClick={(e) => syncCaret(e.currentTarget)}
          onKeyUp={(e) => syncCaret(e.currentTarget)}
          onCompositionStart={() => setComposing(true)}
          onCompositionEnd={(e) => {
            setComposing(false);
            syncCaret(e.currentTarget);
          }}
          onKeyDown={(e) => {
            if (e.nativeEvent.isComposing) return;
            if (menuOpen && filtered.length > 0) {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setMenuIndex((i) => (i + 1) % filtered.length);
                return;
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                setMenuIndex((i) => (i - 1 + filtered.length) % filtered.length);
                return;
              }
              if (e.key === "Enter") {
                e.preventDefault();
                const hit = filtered[menuIndex] ?? filtered[0];
                if (hit) pick(tagLabel(hit));
                return;
              }
              if (e.key === "Escape") {
                e.preventDefault();
                setMenuIndex(-1);
                return;
              }
            }
            if (e.key === "Enter") {
              e.preventDefault();
              onSubmit();
              return;
            }
            if (e.key === "Backspace" && tag) {
              const at = e.currentTarget.selectionStart ?? caret;
              const next = deleteTagIfBackspace(value, at, tag);
              if (next) {
                e.preventDefault();
                setBoth(next.text, next.caret);
              }
            }
          }}
        />
      </div>
      {menuOpen && filtered.length > 0 && menuIndex >= 0 && (
        <ul
          className="absolute z-30 mt-1 max-h-48 w-full overflow-auto rounded-[10px] border border-pip bg-card py-1 shadow-lg"
          role="listbox"
        >
          {filtered.map((list, i) => (
            <li key={list.id}>
              <button
                type="button"
                role="option"
                aria-selected={i === menuIndex}
                className={cn(
                  "flex w-full px-3 py-1.5 text-left text-[14px]",
                  i === menuIndex ? "bg-accent text-foreground" : "text-btn-ink",
                )}
                onMouseDown={(e) => {
                  e.preventDefault();
                  pick(tagLabel(list));
                }}
              >
                #{tagLabel(list)}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
