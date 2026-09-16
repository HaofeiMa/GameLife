export type NotesInline =
  | { type: "text"; value: string }
  | { type: "strong"; value: string }
  | { type: "link"; text: string; href: string };

export type NotesBlock =
  | { type: "p"; children: NotesInline[] }
  | { type: "ul"; items: NotesInline[][] }
  | { type: "ol"; items: NotesInline[][] };

export function notesByteLength(src: string): number {
  return new TextEncoder().encode(src).length;
}

export function parseTaskNotes(src: string): NotesBlock[] {
  if (src.trim() === "") return [];
  const chunks = src.split(/\n\n+/);
  const blocks: NotesBlock[] = [];
  for (const chunk of chunks) {
    const lines = chunk.split("\n").filter((line) => line.length > 0);
    if (lines.length === 0) continue;
    if (lines.every((line) => line.startsWith("- "))) {
      blocks.push({
        type: "ul",
        items: lines.map((line) => parseInline(line.slice(2))),
      });
      continue;
    }
    if (lines.every((line) => /^\d+\.\s/.test(line))) {
      blocks.push({
        type: "ol",
        items: lines.map((line) => parseInline(line.replace(/^\d+\.\s/, ""))),
      });
      continue;
    }
    for (const line of lines) {
      blocks.push({ type: "p", children: parseInline(line) });
    }
  }
  return blocks;
}

function parseInline(src: string): NotesInline[] {
  const out: NotesInline[] = [];
  let i = 0;
  while (i < src.length) {
    const rest = src.slice(i);
    const link = rest.match(/^\[([^\]]+)\]\(([^)]+)\)/);
    if (link) {
      const href = link[2] ?? "";
      if (href.startsWith("http://") || href.startsWith("https://")) {
        out.push({ type: "link", text: link[1] ?? "", href });
      } else {
        pushText(out, link[0]);
      }
      i += link[0].length;
      continue;
    }
    const strong = rest.match(/^\*\*([^*]+)\*\*/);
    if (strong) {
      out.push({ type: "strong", value: strong[1] ?? "" });
      i += strong[0].length;
      continue;
    }
    const next = rest.slice(1).search(/\[|\*\*/);
    const take = next < 0 ? rest.length : 1 + next;
    if (take === 0) break;
    pushText(out, rest.slice(0, take === 0 ? 1 : take));
    i += take === 0 ? 1 : take;
  }
  return out;
}

function pushText(out: NotesInline[], value: string) {
  if (!value) return;
  const last = out[out.length - 1];
  if (last?.type === "text") last.value += value;
  else out.push({ type: "text", value });
}
