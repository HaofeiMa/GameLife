export const SECRET_MASK = "••••••••";

export function secretToPersist(draft: string): string | null {
  const trimmed = draft.trim();
  if (!trimmed || trimmed === SECRET_MASK) return null;
  return trimmed;
}

export function showSecretMask(present: boolean, draft: string): boolean {
  return present && !draft.trim();
}

export function ticktickSecretReady(
  draft: string,
  secretPresent: boolean,
): { ok: true; toWrite: string | null } | { ok: false; error: string } {
  const toWrite = secretToPersist(draft);
  if (toWrite) return { ok: true, toWrite };
  if (secretPresent) return { ok: true, toWrite: null };
  return { ok: false, error: "请填写 Client Secret 后再连接" };
}

export function primaryProviderHasKey(
  primary: string,
  status: { opencodeGo: boolean; openai: boolean; custom: boolean },
): boolean {
  if (primary === "opencode-go") return status.opencodeGo;
  if (primary === "openai") return status.openai;
  if (primary === "custom") return status.custom;
  return false;
}

export function oauthErrorMessage(raw: string): string {
  const lower = raw.toLowerCase();
  if (
    lower.includes("missing ticktick client secret") ||
    lower.includes("client secret")
  ) {
    return "缺少 TickTick Client Secret，请重新填写后连接";
  }
  if (lower.includes("missing ticktick client id")) {
    return "请填写 Client ID";
  }
  if (lower.includes("oauth listen")) {
    return "无法监听本机 18789 端口。请把 TickTick 开发者中心的 Redirect URI 改成 http://127.0.0.1:18789/callback，授权后把浏览器地址栏整段粘贴回来。";
  }
  if (lower.includes("redirect setting")) {
    return "这不是授权完成的地址。请把 Redirect URI 设为 http://127.0.0.1:18789/callback，点连接后把带 code= 的整段地址粘贴回来。";
  }
  return raw;
}

export function saveTouchesPolicy(tab: string): boolean {
  return tab === "lists";
}

export function callbackPasteKind(
  url: string,
): "code" | "setting" | "empty" | "invalid" {
  const trimmed = url.trim();
  if (!trimmed) return "empty";
  if (/[?&]code=/.test(trimmed)) return "code";
  const path = trimmed.split(/[?#]/)[0]?.replace(/\/$/, "") ?? "";
  if (path.endsWith("/callback")) return "setting";
  return "invalid";
}

export function isTicktickAuthorizeUrl(url: string): boolean {
  return (
    url.startsWith("https://ticktick.com/oauth/authorize?") ||
    url.startsWith("https://dida365.com/oauth/authorize?")
  );
}

export function oauthWaitingHint(): string {
  return "请打开下方授权链接（ticktick.com/oauth/authorize）。不要打开 127.0.0.1:18789 的 Redirect URI，那只会显示未完成授权。";
}
