export type PaintTone = "plain" | "time" | "tag";
export type PaintSeg = { text: string; tone: PaintTone };

const ROLE_ALIAS: Record<string, string[]> = {
  mainline: ["主线", "主线任务"],
  side: ["支线", "支线任务"],
  longterm: ["长期", "长期规划", "长期计划"],
  chore: ["杂项"],
};

function aliases(list: { name: string; role: string }): string[] {
  const extra = ROLE_ALIAS[list.role] ?? [];
  return extra.includes(list.name) ? extra : [list.name, ...extra];
}

export function paintSegments(
  text: string,
  spans: { start: number; end: number }[],
  tag?: { start: number; end: number } | null,
): PaintSeg[] {
  const marks: { start: number; end: number; tone: PaintTone }[] = spans
    .filter((s) => s.end > s.start)
    .map((s) => ({ ...s, tone: "time" as const }));
  if (tag && tag.end > tag.start) {
    marks.push({ ...tag, tone: "tag" });
  }
  marks.sort((a, b) => a.start - b.start || b.end - a.end);
  const out: PaintSeg[] = [];
  let i = 0;
  for (const mark of marks) {
    const start = Math.max(mark.start, i);
    const end = Math.min(mark.end, text.length);
    if (start < i || end <= start) continue;
    if (start > i) out.push({ text: text.slice(i, start), tone: "plain" });
    out.push({ text: text.slice(start, end), tone: mark.tone });
    i = end;
  }
  if (i < text.length) out.push({ text: text.slice(i), tone: "plain" });
  return out.filter((s) => s.text.length > 0);
}

export function hashQuery(
  text: string,
  caret: number,
  tag?: { start: number; end: number } | null,
): { start: number; query: string } | null {
  const at = Math.max(0, Math.min(caret, text.length));
  const hash = text.lastIndexOf("#", Math.max(0, at - 1));
  if (hash < 0) return null;
  if (tag && hash >= tag.start && hash < tag.end) return null;
  const after = text.slice(hash + 1, at);
  if (/[\s，,]/.test(after)) return null;
  return { start: hash, query: after };
}

export function filterHashLists<T extends { name: string; role: string }>(
  lists: T[],
  query: string,
): T[] {
  const q = query.trim();
  if (!q) return lists;
  return lists.filter((list) => list.name.includes(q) || aliases(list).some((name) => name === q));
}

export function committedTag(
  text: string,
  lists: { name: string; role: string }[],
): { start: number; end: number; name: string } | null {
  const hash = text.lastIndexOf("#");
  if (hash < 0) return null;
  const rest = text.slice(hash + 1);
  const names = lists.flatMap((list) => aliases(list)).sort((a, b) => b.length - a.length);
  const hit = names.find(
    (name) => rest === name || rest.startsWith(`${name} `) || rest.startsWith(`${name}，`),
  );
  if (!hit) return null;
  return { start: hash, end: hash + 1 + hit.length, name: hit };
}

export function deleteTagIfBackspace(
  text: string,
  caret: number,
  tag: { start: number; end: number },
): { text: string; caret: number } | null {
  if (caret <= tag.start || caret > tag.end) return null;
  return {
    text: text.slice(0, tag.start) + text.slice(tag.end),
    caret: tag.start,
  };
}

export function applyHashPick(
  text: string,
  queryStart: number,
  caret: number,
  name: string,
): { text: string; caret: number } {
  const next = `${text.slice(0, queryStart)}#${name}${text.slice(caret)}`;
  const caretAt = queryStart + 1 + name.length;
  return { text: next, caret: caretAt };
}
