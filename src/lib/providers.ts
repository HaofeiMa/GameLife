import type { VisionProviderSettings } from "./api";

export const KIND_CUSTOM = "custom";
export const KIND_CODEX = "codex";
export const DEFAULT_CODEX_MODEL = "gpt-5.4";
export const PROVIDER_CODEX = "codex";

export function isCodexProvider(p: VisionProviderSettings): boolean {
  const kind = (p.kind ?? "").trim();
  if (kind === KIND_CODEX) return true;
  if (kind === KIND_CUSTOM) return false;
  return p.id === "openai" || p.id === PROVIDER_CODEX;
}

function kindsAlreadySet(providers: VisionProviderSettings[]): boolean {
  return (
    providers.length > 0 &&
    providers.every((p) => (p.kind ?? "").trim() !== "")
  );
}

function asCodex(p: VisionProviderSettings): VisionProviderSettings {
  const model =
    !p.model.trim() || p.model === "gpt-4o-mini"
      ? DEFAULT_CODEX_MODEL
      : p.model;
  return {
    id: PROVIDER_CODEX,
    kind: KIND_CODEX,
    baseUrl: "",
    model,
  };
}

function asCustom(p: VisionProviderSettings): VisionProviderSettings {
  return { ...p, kind: KIND_CUSTOM };
}

function normalizeOne(p: VisionProviderSettings): VisionProviderSettings {
  return isCodexProvider(p) ? asCodex(p) : asCustom(p);
}

function dropEmptyLegacyCustom(p: VisionProviderSettings): boolean {
  if (p.kind !== KIND_CUSTOM || p.id !== "custom") return true;
  return Boolean(p.baseUrl.trim() || p.model.trim());
}

function dedupeCodex(providers: VisionProviderSettings[]): VisionProviderSettings[] {
  let seen = false;
  return providers.filter((p) => {
    if (p.kind !== KIND_CODEX) return true;
    if (seen) return false;
    seen = true;
    return true;
  });
}

function mappedId(id: string): string {
  return id === "openai" ? PROVIDER_CODEX : id;
}

export function migrateVisionProviders(
  providers: VisionProviderSettings[],
  primary: string,
  fallback: string,
): VisionProviderSettings[] {
  if (kindsAlreadySet(providers)) {
    return providers.map((p) => ({ ...p }));
  }
  const normalized = dedupeCodex(
    providers.map(normalizeOne).filter(dropEmptyLegacyCustom),
  );
  const rest = [...normalized];
  const ordered: VisionProviderSettings[] = [];
  for (const wanted of [mappedId(primary), mappedId(fallback)]) {
    if (!wanted || wanted === "none") continue;
    const i = rest.findIndex((p) => p.id === wanted);
    if (i >= 0) ordered.push(rest.splice(i, 1)[0]!);
  }
  ordered.push(...rest);
  return ordered;
}

export function nextCustomId(existingIds: string[]): string {
  const taken = new Set(existingIds);
  for (let n = 2; n < 1000; n++) {
    const id = `custom-${n}`;
    if (!taken.has(id)) return id;
  }
  return `custom-${Date.now()}`;
}

export function moveProvider<T>(items: T[], index: number, delta: number): T[] {
  const dest = index + delta;
  if (dest < 0 || dest >= items.length || index < 0 || index >= items.length) {
    return items;
  }
  const next = [...items];
  const [row] = next.splice(index, 1);
  next.splice(dest, 0, row!);
  return next;
}

export function addCustomPanel(
  providers: VisionProviderSettings[],
): VisionProviderSettings[] {
  return [
    ...providers,
    {
      id: nextCustomId(providers.map((p) => p.id)),
      kind: KIND_CUSTOM,
      baseUrl: "",
      model: "",
    },
  ];
}

export function addCodexPanel(
  providers: VisionProviderSettings[],
): VisionProviderSettings[] {
  if (providers.some(isCodexProvider)) return providers;
  return [
    ...providers,
    {
      id: PROVIDER_CODEX,
      kind: KIND_CODEX,
      baseUrl: "",
      model: DEFAULT_CODEX_MODEL,
    },
  ];
}

export function customPanelTitle(
  providers: VisionProviderSettings[],
  id: string,
): string {
  const named = providers.find((p) => p.id === id)?.name?.trim();
  if (named) return named;
  const customs = providers.filter((p) => p.kind === KIND_CUSTOM);
  const index = customs.findIndex((p) => p.id === id);
  if (index <= 0) return "自定义 API";
  return `自定义 API ${index + 1}`;
}

export type ProviderTestTone = "idle" | "testing" | "ok" | "fail";

export function providerTestDotClass(status: ProviderTestTone): string {
  if (status === "ok") return "bg-success";
  if (status === "fail") return "bg-destructive";
  return "bg-muted-foreground/40";
}

export function providerTestDotLabel(status: ProviderTestTone): string {
  switch (status) {
    case "testing":
      return "测试中";
    case "ok":
      return "测试成功";
    case "fail":
      return "测试失败";
    default:
      return "未测试";
  }
}

export function providerTestButtonLabel(status: ProviderTestTone): string {
  return status === "testing" ? "测试中…" : "测试连接";
}

export function providerTestPendingNote(status: ProviderTestTone): string {
  return status === "testing" ? "正在连接…" : "";
}

export function providerIsReady(
  p: VisionProviderSettings,
  keys: Record<string, boolean>,
  codexLoggedIn: boolean,
): boolean {
  if (isCodexProvider(p)) return codexLoggedIn && Boolean(p.model.trim());
  return Boolean(keys[p.id] && p.baseUrl.trim() && p.model.trim());
}

export function chainHasUsable(
  providers: VisionProviderSettings[],
  keys: Record<string, boolean>,
  codexLoggedIn: boolean,
): boolean {
  return providers.some((p) => providerIsReady(p, keys, codexLoggedIn));
}

export function withVisionProviders<T extends {
  primaryProvider: string;
  fallbackProvider: string;
  visionProviders: VisionProviderSettings[];
}>(settings: T, visionProviders: VisionProviderSettings[]): T {
  return {
    ...settings,
    visionProviders,
    primaryProvider: visionProviders[0]?.id ?? "",
    fallbackProvider: visionProviders[1]?.id ?? "none",
  };
}
