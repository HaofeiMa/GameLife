import { ChevronRight, X } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import appIcon from "../../src-tauri/icons/128x128@2x.png";
import { PageHeader } from "../components/PageHeader";
import { PermissionPanel } from "../components/PermissionBanner";
import { PlatformNotice } from "../components/PlatformNotice";
import { IS_MACOS } from "../lib/platform";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { Input } from "../components/ui/input";
import { Label } from "../components/ui/label";
import { Segmented } from "../components/ui/segmented";
import { Select } from "../components/ui/select";
import { Switch } from "../components/ui/switch";
import { Textarea } from "../components/ui/textarea";
import {
  BUILTIN_NEVER_CAPTURE,
  getSettings,
  providerKeyStatus,
  saveSettings,
  setProviderApiKey,
  testVisionProvider,
  ticktickBeginOauth,
  ticktickDisconnect,
  ticktickFinishOauth,
  ticktickSetClientSecret,
  ticktickStatus,
  ticktickSync,
  ticktickTree,
  type AppSettings,
  type ProviderKeyStatus,
  type TickTickTree,
  type TickTickStatus,
  type VisionProviderSettings,
} from "../lib/api";
import { GUIDE_PLACEHOLDERS, savedCategoryGuides } from "../lib/guides";
import {
  callbackPasteKind,
  oauthErrorMessage,
  oauthWaitingHint,
  primaryProviderHasKey,
  saveTouchesPolicy,
  SECRET_MASK,
  secretToPersist,
  showSecretMask,
  ticktickSecretReady,
} from "../lib/secretField";
import { notifySettingsChanged } from "../lib/settingsEvents";
import {
  columnRoleKey,
  formatTicktickLastSync,
  nextRoleMap,
  TICKTICK_ROLE_COLUMNS,
  ticktickRoleLabel,
  ticktickSyncButtonLabel,
  ticktickSyncErrorMessage,
} from "../lib/ticktickBoard";
import { cn } from "../lib/utils";

type SettingsTab = "basic" | "api" | "lists" | "ticktick" | "permissions" | "about";

/** One draggable thing in the TickTick tree. */
type AssignTarget =
  | { kind: "project"; projectId: string; label: string }
  | { kind: "column"; projectId: string; columnId: string; label: string };

const TABS: { value: SettingsTab; label: string }[] = [
  { value: "basic", label: "基础" },
  { value: "api", label: "API" },
  { value: "lists", label: "名单" },
  { value: "ticktick", label: "TickTick" },
  { value: "permissions", label: "权限" },
  { value: "about", label: "关于" },
];

/* --------------------------- layout atoms --------------------------- */

function Section({
  title,
  caption,
  children,
  className,
}: {
  title: string;
  caption?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <Card className={cn("overflow-hidden", className)}>
      <div className="px-[18px] pt-[11px] pb-2">
        <h2 className="text-[13.5px] font-semibold tracking-[-0.005em]">
          {title}
        </h2>
        {caption && (
          <div className="mt-[3px] text-[11px] leading-[1.5] text-muted-foreground">
            {caption}
          </div>
        )}
      </div>
      <div className="flex flex-col gap-2 px-[18px] pb-3.5">{children}</div>
    </Card>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="space-y-3">
      <Label>{label}</Label>
      {children}
      {hint && <p className="text-[11px] leading-relaxed text-muted-foreground">{hint}</p>}
    </div>
  );
}

function ToggleRow({
  title,
  description,
  checked,
  disabled,
  onChange,
}: {
  title: string;
  description?: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    /* The mockup's .frow — an inset card per setting, one per line. */
    <div className="flex items-center gap-[14px] rounded-[14px] bg-loot px-[13px] py-2.5">
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-semibold">{title}</p>
        {description && (
          <p className="mt-0.5 text-[11px] leading-[1.5] text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      <Switch
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
        aria-label={title}
      />
    </div>
  );
}

/* ---------------------------- field atoms --------------------------- */

function ListEditor({
  label,
  items,
  disabled,
  onChange,
}: {
  label: string;
  items: string[];
  disabled: boolean;
  onChange: (items: string[]) => void;
}) {
  const [draft, setDraft] = useState("");

  function add() {
    const v = draft.trim();
    if (!v) return;
    if (items.some((x) => x.toLowerCase() === v.toLowerCase())) {
      setDraft("");
      return;
    }
    onChange([...items, v]);
    setDraft("");
  }

  return (
    <Section title={label} className="gap-0">
      <div className="flex flex-wrap gap-1.5">
        {items.length === 0 && (
          <span className="text-xs text-muted-foreground">还没有条目</span>
        )}
        {items.map((item) => (
          <span
            key={item}
            className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs"
          >
            {item}
            <button
              type="button"
              disabled={disabled}
              aria-label={`删除 ${item}`}
              className="text-muted-foreground transition-colors hover:text-destructive disabled:opacity-50"
              onClick={() => onChange(items.filter((x) => x !== item))}
            >
              <X className="size-3" />
            </button>
          </span>
        ))}
      </div>
      <div className="flex gap-2">
        <Input
          value={draft}
          disabled={disabled}
          placeholder="输入名称后回车或点添加"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              add();
            }
          }}
        />
        <Button size="sm" disabled={disabled} onClick={add} className="shrink-0">
          添加
        </Button>
      </div>
    </Section>
  );
}

function SecretField({
  label,
  present,
  draft,
  busy,
  placeholder,
  onDraftChange,
}: {
  label: string;
  present: boolean;
  draft: string;
  busy: boolean;
  placeholder: string;
  onDraftChange: (value: string) => void;
}) {
  const [replacing, setReplacing] = useState(false);
  const masked = showSecretMask(present, draft) && !replacing;

  useEffect(() => {
    if (present && !draft) setReplacing(false);
  }, [present, draft]);

  return (
    <Field label={label}>
      {masked ? (
        <div className="flex items-center gap-2">
          <div
            className="flex h-9 flex-1 select-none items-center rounded-md border bg-muted px-3 font-mono text-sm text-muted-foreground"
            aria-label="已保存"
            onCopy={(e) => e.preventDefault()}
            onCut={(e) => e.preventDefault()}
            onContextMenu={(e) => e.preventDefault()}
          >
            {SECRET_MASK}
          </div>
          <Button
            variant="outline"
            size="sm"
            disabled={busy}
            onClick={() => {
              setReplacing(true);
              onDraftChange("");
            }}
          >
            更换
          </Button>
        </div>
      ) : (
        <Input
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={draft}
          disabled={busy}
          placeholder={present ? "输入新密钥以替换" : placeholder}
          onChange={(e) => onDraftChange(e.target.value)}
        />
      )}
    </Field>
  );
}

function ProviderEditor({
  title,
  spec,
  keyPresent,
  keyValue,
  busy,
  onPatch,
  onKeyChange,
  onTest,
}: {
  title: string;
  spec: VisionProviderSettings;
  keyPresent: boolean;
  keyValue: string;
  busy: boolean;
  onPatch: (patch: Partial<VisionProviderSettings>) => void;
  onKeyChange: (value: string) => void;
  onTest: () => void;
}) {
  return (
    <Section
      title={title}
      caption={
        <span className={keyPresent ? "text-success" : undefined}>
          {keyPresent ? "已保存" : "未配置"}
        </span>
      }
    >
      <Field label="Base URL">
        <Input
          value={spec.baseUrl}
          disabled={busy}
          placeholder="https://…"
          onChange={(e) => onPatch({ baseUrl: e.target.value })}
        />
      </Field>
      <Field label="模型">
        <Input
          value={spec.model}
          disabled={busy}
          placeholder="模型 ID"
          onChange={(e) => onPatch({ model: e.target.value })}
        />
      </Field>
      <SecretField
        label="API Key"
        present={keyPresent}
        draft={keyValue}
        busy={busy}
        placeholder="新 Key（保存时写入本机）"
        onDraftChange={onKeyChange}
      />
      {keyPresent && (
        <Button variant="outline" size="sm" disabled={busy} onClick={onTest}>
          测试连接
        </Button>
      )}
    </Section>
  );
}

/* -------------------------------- page ------------------------------ */

export function Settings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [keys, setKeys] = useState<Record<string, string>>({
    "opencode-go": "",
    openai: "",
    custom: "",
  });
  const [keyStatus, setKeyStatus] = useState<ProviderKeyStatus>({
    opencodeGo: false,
    openai: false,
    custom: false,
  });
  const [saving, setSaving] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [testing, setTesting] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [tab, setTab] = useState<SettingsTab>("basic");
  const [ticktickSecret, setTicktickSecret] = useState("");
  const [ttStatus, setTtStatus] = useState<TickTickStatus>({
    connected: false,
    lastSync: null,
    lastError: null,
    secretPresent: false,
  });
  const [callbackDraft, setCallbackDraft] = useState("");
  const [authorizeUrl, setAuthorizeUrl] = useState("");
  const [tree, setTree] = useState<TickTickTree | null>(null);
  const [treeLoading, setTreeLoading] = useState(false);
  const [treeError, setTreeError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [picked, setPicked] = useState<AssignTarget | null>(null);
  const [dragPayload, setDragPayload] = useState<AssignTarget | null>(null);
  const [dragOverRole, setDragOverRole] = useState<string | null>(null);
  const [truncated, setTruncated] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [syncNote, setSyncNote] = useState<string | null>(null);
  const formLocked = saving;

  useEffect(() => {
    getSettings()
      .then((s) => {
        setSettings(s);
        setLoadError(null);
      })
      .catch((e) => setLoadError(String(e)));
    providerKeyStatus().then(setKeyStatus).catch(() => undefined);
  }, []);

  // Opening the tab reads local status plus the *cached* tree; the network
  // round trip only happens on an explicit sync.
  useEffect(() => {
    if (tab !== "ticktick") return;
    let cancelled = false;
    ticktickStatus()
      .then((status) => {
        if (cancelled) return;
        setTtStatus(status);
        if (!status.connected) return;
        setTreeLoading(true);
        setTreeError(null);
        void ticktickTree(false)
          .then((t) => {
            if (!cancelled) setTree(t);
          })
          .catch((e) => {
            if (!cancelled) setTreeError(String(e));
          })
          .finally(() => {
            if (!cancelled) setTreeLoading(false);
          });
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [tab]);

  const header = (
    <PageHeader
      title={<h1 className="text-[19px] font-bold tracking-[-0.02em]">设置</h1>}
      center={
        <Segmented
          size="sm"
          aria-label="设置分区"
          value={tab}
          onChange={setTab}
          options={TABS}
        />
      }
      actions={
        tab !== "permissions" && tab !== "about" ? (
          <Button
            size="sm"
            disabled={formLocked || connecting || !settings}
            onClick={() => void handleSave()}
          >
            {saving ? "保存中…" : "保存设置"}
          </Button>
        ) : undefined
      }
    />
  );

  if (loadError) {
    return (
      <>
        {header}
        <div className="flex-1 overflow-y-auto">
          <div className="mx-auto flex w-[728px] max-w-full flex-col gap-3 px-4 pt-3 pb-4">
            <Card className="flex flex-col items-center gap-3 p-8 text-center">
              <p className="text-sm text-destructive">{loadError}</p>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  setLoadError(null);
                  getSettings()
                    .then(setSettings)
                    .catch((e) => setLoadError(String(e)));
                }}
              >
                重试
              </Button>
            </Card>
          </div>
        </div>
      </>
    );
  }

  if (!settings) {
    return (
      <>
        {header}
        <div className="flex-1 overflow-y-auto">
          <div className="mx-auto flex w-[728px] max-w-full flex-col gap-3 px-4 pt-3 pb-4">
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-32 animate-shimmer rounded-xl bg-muted" />
            ))}
          </div>
        </div>
      </>
    );
  }

  function provider(id: string): VisionProviderSettings {
    return (
      settings!.visionProviders.find((p) => p.id === id) ?? {
        id,
        baseUrl: "",
        model: "",
      }
    );
  }

  function patchProvider(id: string, patch: Partial<VisionProviderSettings>) {
    if (!settings) return;
    const exists = settings.visionProviders.some((p) => p.id === id);
    const visionProviders = exists
      ? settings.visionProviders.map((p) => (p.id === id ? { ...p, ...patch } : p))
      : [...settings.visionProviders, { id, baseUrl: "", model: "", ...patch }];
    setSettings({ ...settings, visionProviders });
  }

  async function persistSettings(next: AppSettings, updatePolicy: boolean) {
    const trimmed = {
      ...next,
      categoryGuides: savedCategoryGuides(next.categoryGuides),
      ticktickProjectRoles: next.ticktickProjectRoles ?? {},
      ticktickColumnRoles: next.ticktickColumnRoles ?? {},
    };
    await saveSettings(trimmed, updatePolicy);
    setSettings(trimmed);
    return trimmed;
  }

  async function persistBasic(next: AppSettings) {
    setSettings(next);
    setSaving(true);
    setMsg(null);
    try {
      await persistSettings(next, false);
      notifySettingsChanged();
    } catch (e) {
      setMsg(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handleSave() {
    if (!settings) return;
    setSaving(true);
    setMsg(null);
    try {
      for (const id of ["opencode-go", "openai", "custom"] as const) {
        const typed = secretToPersist(keys[id] ?? "");
        if (!typed) continue;
        await setProviderApiKey(id, typed);
      }
      const tt = secretToPersist(ticktickSecret);
      if (tt) {
        await ticktickSetClientSecret(tt);
        setTicktickSecret("");
      }
      await persistSettings(settings, saveTouchesPolicy(tab));
      notifySettingsChanged();
      const confirmed = await providerKeyStatus();
      setKeyStatus(confirmed);
      setKeys({ "opencode-go": "", openai: "", custom: "" });
      const nextTt = await ticktickStatus();
      setTtStatus(nextTt);
      if (!primaryProviderHasKey(settings.primaryProvider, confirmed)) {
        setMsg("设置已写入，但主用 API Key 尚未保存，请重新填写后保存");
      } else {
        setMsg("已保存");
      }
    } catch (e) {
      setMsg(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handleTestProvider(id: string) {
    setTesting(true);
    setMsg(null);
    try {
      const result = await testVisionProvider(id);
      setMsg(result.ok ? `测试成功：${result.preview}` : result.preview);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setTesting(false);
    }
  }

  async function handleConnect() {
    if (!settings) return;
    const ready = ticktickSecretReady(ticktickSecret, ttStatus.secretPresent);
    if (!ready.ok) {
      setMsg(ready.error);
      return;
    }
    if (!settings.ticktickClientId.trim()) {
      setMsg("请填写 Client ID");
      return;
    }
    setConnecting(true);
    setMsg(null);
    try {
      const result = await ticktickBeginOauth(settings.ticktickClientId, ready.toWrite);
      setAuthorizeUrl(result.authorizeUrl);
      const statusAfter = await ticktickStatus();
      setTtStatus(statusAfter);
      if (ready.toWrite) setTicktickSecret("");
      if (!result.listenOk) {
        setMsg(
          `${oauthWaitingHint()} 本机未能自动收回调。授权后请把浏览器地址栏整段粘贴回来。`,
        );
        return;
      }
      setMsg(
        result.opened
          ? oauthWaitingHint()
          : "未能自动打开浏览器，请复制下方授权链接到浏览器打开。",
      );
      const deadline = Date.now() + 180_000;
      while (Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 1000));
        const status = await ticktickStatus();
        setTtStatus(status);
        if (status.connected) {
          setTree(await ticktickTree(true));
          setAuthorizeUrl("");
          setMsg("已连接 TickTick");
          return;
        }
      }
      setMsg("等待授权超时。可复制下方授权链接到浏览器，或把跳转后的地址粘贴回来");
    } catch (e) {
      setMsg(oauthErrorMessage(String(e)));
    } finally {
      setConnecting(false);
    }
  }

  async function handleDisconnect() {
    setSaving(true);
    setMsg(null);
    try {
      await ticktickDisconnect();
      setTtStatus({
        connected: false,
        lastSync: null,
        lastError: null,
        secretPresent: ttStatus.secretPresent,
      });
      setTree(null);
      setTruncated(false);
      setPicked(null);
      setExpanded({});
      setAuthorizeUrl("");
      setMsg("已断开 TickTick");
    } catch (e) {
      setMsg(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handlePasteCallback() {
    const url = callbackDraft.trim();
    const kind = callbackPasteKind(url);
    if (kind === "empty") return;
    if (kind !== "code") {
      setMsg(oauthErrorMessage("oauth redirect setting"));
      return;
    }
    const ready = ticktickSecretReady(ticktickSecret, ttStatus.secretPresent);
    if (!ready.ok) {
      setMsg(ready.error);
      return;
    }
    setConnecting(true);
    setMsg(null);
    try {
      await ticktickFinishOauth(url, ready.toWrite);
      setCallbackDraft("");
      if (ready.toWrite) setTicktickSecret("");
      const status = await ticktickStatus();
      setTtStatus(status);
      if (status.connected) {
        setTree(await ticktickTree(true));
        setAuthorizeUrl("");
        setMsg("已连接 TickTick");
      } else if (status.lastError) {
        setMsg(oauthErrorMessage(status.lastError));
      }
    } catch (e) {
      setMsg(oauthErrorMessage(String(e)));
    } finally {
      setConnecting(false);
    }
  }

  /**
   * Files a project or one of its columns under a role. Roles are not part
   * of the policy snapshot, so this does not need a new policy version.
   */
  async function assignRole(target: AssignTarget, role: string) {
    if (!settings) return;
    const next = { ...settings };
    if (target.kind === "project") {
      next.ticktickProjectRoles = nextRoleMap(
        settings.ticktickProjectRoles ?? {},
        target.projectId,
        role,
      );
    } else {
      next.ticktickColumnRoles = nextRoleMap(
        settings.ticktickColumnRoles ?? {},
        columnRoleKey(target.projectId, target.columnId),
        role,
      );
    }
    setSettings(next);
    setPicked(null);
    setSaving(true);
    setMsg(null);
    try {
      await persistSettings(next, false);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setSaving(false);
    }
  }

  async function handleSync() {
    setSyncing(true);
    setSyncNote(null);
    try {
      const result = await ticktickSync();
      setTruncated(result.truncated);
      setTtStatus(await ticktickStatus());
      // sync_projects also refreshes the cached tree.
      setTree(await ticktickTree(false));
      setSyncNote(
        result.truncated
          ? "同步完成（当天时段任务超过 20）"
          : `同步完成（${result.count} 条时段任务）`,
      );
    } catch (e) {
      setSyncNote(ticktickSyncErrorMessage(String(e)));
    } finally {
      setSyncing(false);
    }
  }





  async function copyRedirectUri() {
    try {
      await navigator.clipboard.writeText("http://127.0.0.1:18789/callback");
      setMsg("已复制 Redirect URI（仅填开发者中心，不要用浏览器打开）");
    } catch {
      setMsg("请手动复制 http://127.0.0.1:18789/callback");
    }
  }

  async function copyAuthorizeUrl() {
    if (!authorizeUrl) return;
    try {
      await navigator.clipboard.writeText(authorizeUrl);
      setMsg("已复制授权链接，请粘贴到浏览器打开");
    } catch {
      setMsg("请手动选中下方授权链接并复制");
    }
  }

  const userNeverCapture = settings.neverCaptureApps.filter(
    (n) => !BUILTIN_NEVER_CAPTURE.some((b) => b.toLowerCase() === n.toLowerCase()),
  );
  // How many things sit in each role bucket, for the drop targets.
  const roleCounts: Record<string, number> = {};
  for (const role of settings?.ticktickProjectRoles
    ? Object.values(settings.ticktickProjectRoles)
    : []) {
    roleCounts[role] = (roleCounts[role] ?? 0) + 1;
  }
  for (const role of settings?.ticktickColumnRoles
    ? Object.values(settings.ticktickColumnRoles)
    : []) {
    roleCounts[role] = (roleCounts[role] ?? 0) + 1;
  }

  return (
    <>
      {header}
      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto flex w-[728px] max-w-full flex-col gap-3 px-4 pt-3 pb-4">
          {msg && (
            <p className="rounded-lg bg-muted px-3 py-2 text-xs text-muted-foreground">
              {msg}
            </p>
          )}

          {tab === "basic" && (
            <div className="space-y-4">
              <Section title="外观">
                <div className="flex items-center gap-6">
                  <Label className="w-12 shrink-0">主题</Label>
                  <Segmented
                    aria-label="主题"
                    size="sm"
                    value={
                      settings.theme === "light" || settings.theme === "dark"
                        ? settings.theme
                        : "system"
                    }
                    onChange={(theme) => void persistBasic({ ...settings, theme })}
                    options={[
                      { value: "system", label: "跟随系统" },
                      { value: "light", label: "亮色" },
                      { value: "dark", label: "暗色" },
                    ]}
                  />
                </div>
              </Section>

              <Section title="启动与导航">
                <ToggleRow
                  title="登录时启动"
                  description="默认开启。"
                  checked={settings.loginAtStartup}
                  disabled={formLocked}
                  onChange={(v) =>
                    void persistBasic({ ...settings, loginAtStartup: v })
                  }
                />
                <ToggleRow
                  title="静默启动"
                  description="启动时不显示窗口，只挂菜单栏。"
                  checked={settings.silentStart !== false}
                  disabled={formLocked}
                  onChange={(v) => void persistBasic({ ...settings, silentStart: v })}
                />
                <ToggleRow
                  title="导航显示文字"
                  description="关闭后侧栏只留图标。"
                  checked={settings.showRailLabels}
                  disabled={formLocked}
                  onChange={(v) =>
                    void persistBasic({ ...settings, showRailLabels: v })
                  }
                />
              </Section>

              <Section title="保留与清理">
                <Field label="截图保留" hint="过期截图由采样器按此策略自动清理。">
                  <Select
                    value={settings.screenshotRetention}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({ ...settings, screenshotRetention: e.target.value })
                    }
                  >
                    <option value="none">不保留</option>
                    <option value="24h">24 小时</option>
                    <option value="3d">3 天</option>
                    <option value="14d">14 天</option>
                  </Select>
                </Field>
                <Field
                  label="样本保留天数"
                  hint="超过保留天数的样本行在启动时清理（默认 7 天）。采样间隔固定 15 秒。"
                >
                  <Input
                    type="number"
                    min={3}
                    max={14}
                    className="w-32"
                    value={settings.sampleKeepDays}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({ ...settings, sampleKeepDays: Number(e.target.value) })
                    }
                  />
                </Field>
              </Section>
            </div>
          )}

          {tab === "api" && (
            <div className="space-y-4">
              <Section
                title="判定与视觉"
                caption="Key 只保存在本机 secrets.json（权限 600），不进 config.json。主用失败仅在超时、网络错误或 HTTP 5xx 时改走 fallback。"
              >
                {!primaryProviderHasKey(settings.primaryProvider, keyStatus) && (
                  <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
                    主用提供商还没有 API Key。只填 URL / 模型不够，请在下方密码框粘贴 Key 后点「保存设置」。
                  </p>
                )}
                <Field label="主用">
                  <Select
                    value={settings.primaryProvider}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({ ...settings, primaryProvider: e.target.value })
                    }
                  >
                    <option value="opencode-go">OpenCode Go</option>
                    <option value="openai">OpenAI</option>
                    <option value="custom">自定义</option>
                  </Select>
                </Field>
                <Field label="回退">
                  <Select
                    value={settings.fallbackProvider}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({ ...settings, fallbackProvider: e.target.value })
                    }
                  >
                    <option value="none">无</option>
                    <option value="opencode-go">OpenCode Go</option>
                    <option value="openai">OpenAI</option>
                    <option value="custom">自定义</option>
                  </Select>
                </Field>
              </Section>

              <ProviderEditor
                title="OpenCode Go"
                spec={provider("opencode-go")}
                keyPresent={keyStatus.opencodeGo}
                keyValue={keys["opencode-go"] ?? ""}
                busy={formLocked || testing}
                onPatch={(patch) => patchProvider("opencode-go", patch)}
                onKeyChange={(v) => setKeys({ ...keys, "opencode-go": v })}
                onTest={() => void handleTestProvider("opencode-go")}
              />
              <ProviderEditor
                title="OpenAI / Codex 兼容"
                spec={provider("openai")}
                keyPresent={keyStatus.openai}
                keyValue={keys.openai ?? ""}
                busy={formLocked || testing}
                onPatch={(patch) => patchProvider("openai", patch)}
                onKeyChange={(v) => setKeys({ ...keys, openai: v })}
                onTest={() => void handleTestProvider("openai")}
              />
              <ProviderEditor
                title="自定义"
                spec={provider("custom")}
                keyPresent={keyStatus.custom}
                keyValue={keys.custom ?? ""}
                busy={formLocked || testing}
                onPatch={(patch) => patchProvider("custom", patch)}
                onKeyChange={(v) => setKeys({ ...keys, custom: v })}
                onTest={() => void handleTestProvider("custom")}
              />
            </div>
          )}

          {tab === "lists" && (
            <div className="space-y-4">
              <p className="rounded-lg bg-muted px-3 py-2 text-xs text-muted-foreground">
                Chrome / Safari / Arc 的当前标签 URL 需要「自动化」权限；拒绝则 URL 为空，不影响采样与截图。
              </p>
              <ListEditor
                label="主线应用"
                items={settings.trustedApps}
                disabled={formLocked}
                onChange={(trustedApps) => setSettings({ ...settings, trustedApps })}
              />
              <ListEditor
                label="支线应用"
                items={settings.sideProjectRules}
                disabled={formLocked}
                onChange={(sideProjectRules) =>
                  setSettings({ ...settings, sideProjectRules })
                }
              />
              <ListEditor
                label="杂项应用"
                items={settings.adminApps}
                disabled={formLocked}
                onChange={(adminApps) => setSettings({ ...settings, adminApps })}
              />
              <ListEditor
                label="娱乐应用 / 网站"
                items={settings.distractionRules}
                disabled={formLocked}
                onChange={(distractionRules) =>
                  setSettings({ ...settings, distractionRules })
                }
              />
              <ListEditor
                label="阅读"
                items={settings.readingApps}
                disabled={formLocked}
                onChange={(readingApps) => setSettings({ ...settings, readingApps })}
              />

              <Section
                title="永不截屏（内置只读）"
                caption="内置项不可删除。GameLife 自身也不发币。"
              >
                <div className="flex flex-wrap gap-1.5">
                  {BUILTIN_NEVER_CAPTURE.map((n) => (
                    <span
                      key={n}
                      className="inline-flex items-center gap-1 rounded-md bg-muted px-2 py-1 text-xs text-muted-foreground"
                    >
                      {n}
                    </span>
                  ))}
                </div>
              </Section>

              <ListEditor
                label="额外永不截屏"
                items={userNeverCapture}
                disabled={formLocked}
                onChange={(extra) =>
                  setSettings({
                    ...settings,
                    neverCaptureApps: [...BUILTIN_NEVER_CAPTURE, ...extra],
                  })
                }
              />

              <Section title="类别说明" caption="灰字为样稿，空着保存不会写进判定规则。">
                {(
                  [
                    ["mainline", "主线"],
                    ["side", "支线"],
                    ["admin", "杂项"],
                    ["entertainment", "娱乐"],
                  ] as const
                ).map(([key, label]) => (
                  <Field key={key} label={label}>
                    <Textarea
                      maxLength={500}
                      disabled={formLocked}
                      placeholder={GUIDE_PLACEHOLDERS[key]}
                      value={settings.categoryGuides[key]}
                      onChange={(e) =>
                        setSettings({
                          ...settings,
                          categoryGuides: {
                            ...settings.categoryGuides,
                            [key]: e.target.value,
                          },
                        })
                      }
                    />
                  </Field>
                ))}
              </Section>
            </div>
          )}

          {tab === "ticktick" && (
            <div className="space-y-4">
              <Section
                title="连接"
                caption={
                  <>
                    到 TickTick 开发者中心建应用，Redirect URI 必须填{" "}
                    <code className="rounded bg-muted px-1 py-0.5 font-mono text-[11px]">
                      http://127.0.0.1:18789/callback
                    </code>
                    。不要用 localhost:3000。Client Secret 只保存在本机，不进 config.json。
                  </>
                }
              >
                <Button
                  variant="outline"
                  size="sm"
                  disabled={formLocked}
                  onClick={() => void copyRedirectUri()}
                >
                  复制 Redirect URI（仅开发者中心）
                </Button>

                {!ttStatus.connected && (
                  <p className="text-xs text-muted-foreground">尚未连接 TickTick。</p>
                )}
                {ttStatus.lastError && (
                  <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
                    {oauthErrorMessage(ttStatus.lastError)}
                  </p>
                )}

                <Field label="Client ID">
                  <Input
                    value={settings.ticktickClientId}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({ ...settings, ticktickClientId: e.target.value })
                    }
                  />
                </Field>
                <SecretField
                  label="Client Secret"
                  present={ttStatus.secretPresent}
                  draft={ticktickSecret}
                  busy={formLocked}
                  placeholder="保存或连接时写入本机"
                  onDraftChange={setTicktickSecret}
                />

                <div className="flex gap-2">
                  <Button
                    size="sm"
                    disabled={formLocked || connecting}
                    onClick={() => void handleConnect()}
                  >
                    {connecting ? "等待授权…" : "连接"}
                  </Button>
                  {ttStatus.connected && (
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={formLocked || connecting}
                      onClick={() => void handleDisconnect()}
                    >
                      断开
                    </Button>
                  )}
                </div>

                {authorizeUrl && !ttStatus.connected && (
                  <Field label="授权链接" hint={oauthWaitingHint()}>
                    <Textarea
                      readOnly
                      rows={4}
                      className="font-mono text-[11px]"
                      value={authorizeUrl}
                      onFocus={(e) => e.currentTarget.select()}
                    />
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => void copyAuthorizeUrl()}
                    >
                      复制授权链接
                    </Button>
                  </Field>
                )}

                {!ttStatus.connected && (
                  <>
                    <Field label="回调地址">
                      <Input
                        value={callbackDraft}
                        disabled={saving}
                        placeholder="授权后浏览器地址栏整段，须含 code="
                        onChange={(e) => setCallbackDraft(e.target.value)}
                      />
                    </Field>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={saving || connecting || !callbackDraft.trim()}
                      onClick={() => void handlePasteCallback()}
                    >
                      粘贴回调完成连接
                    </Button>
                  </>
                )}
              </Section>

              {ttStatus.connected && (
                <Section
                  title="任务类别"
                  caption="把整个清单，或清单里的某个分组，拖到下面任一栏；也可以先点一行选中，再点那一栏。归到「忽略」的清单不参与判定。"
                >
                  <div className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-5">
                    {TICKTICK_ROLE_COLUMNS.map(([role, label]) => (
                      <button
                        key={role}
                        type="button"
                        disabled={formLocked || !picked}
                        onClick={() => picked && void assignRole(picked, role)}
                        onDragOver={(e) => {
                          if (!dragPayload) return;
                          e.preventDefault();
                          e.dataTransfer.dropEffect = "move";
                        }}
                        onDragEnter={() => dragPayload && setDragOverRole(role)}
                        onDragLeave={() =>
                          setDragOverRole((r) => (r === role ? null : r))
                        }
                        onDrop={(e) => {
                          e.preventDefault();
                          if (dragPayload) void assignRole(dragPayload, role);
                          setDragPayload(null);
                          setDragOverRole(null);
                        }}
                        className={cn(
                          "rounded-lg border border-dashed px-3 py-3 text-xs font-medium transition-colors",
                          dragOverRole === role
                            ? "border-primary bg-primary/15 text-primary"
                            : picked
                              ? "border-primary/50 text-primary hover:bg-primary/10"
                              : "text-muted-foreground",
                        )}
                      >
                        <span className="block text-center">{label}</span>
                        <span className="mt-1 block text-center text-[10px] tabular-nums opacity-70">
                          {roleCounts[role] ?? 0} 项
                        </span>
                      </button>
                    ))}
                  </div>

                  {(picked || dragPayload) && (
                    <p className="text-[11px] text-muted-foreground">
                      正在移动「{(picked ?? dragPayload)?.label}」——
                      拖到上面任一栏，或点那一栏。
                    </p>
                  )}

                  {treeLoading && (
                    <p className="text-xs text-muted-foreground">正在载入清单…</p>
                  )}
                  {treeError && (
                    <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
                      拉取清单失败：{treeError}
                    </p>
                  )}
                  {!treeLoading && !treeError && (tree?.projects.length ?? 0) === 0 && (
                    <p className="text-xs text-muted-foreground">
                      还没有清单结构。点下面的「同步任务」拉取全部清单与分组。
                    </p>
                  )}

                  <ul className="space-y-1">
                    {tree?.projects.map((project) => {
                      const projectRole =
                        settings.ticktickProjectRoles[project.id] ?? "ignore";
                      const open = expanded[project.id] === true;
                      const target: AssignTarget = {
                        kind: "project",
                        projectId: project.id,
                        label: project.name || project.id,
                      };
                      const selected =
                        picked?.kind === "project" && picked.projectId === project.id;
                      return (
                        <li key={project.id} className="rounded-lg border">
                          <div
                            draggable={!formLocked}
                            onDragStart={() => setDragPayload(target)}
                            onDragEnd={() => {
                              setDragPayload(null);
                              setDragOverRole(null);
                            }}
                            className={cn(
                              "flex cursor-grab items-center gap-2 px-3 py-2 text-sm transition-colors active:cursor-grabbing",
                              selected && "bg-primary/10",
                            )}
                          >
                            <button
                              type="button"
                              disabled={project.columns.length === 0}
                              aria-expanded={open}
                              aria-label={open ? "折叠分组" : "展开分组"}
                              onClick={() =>
                                setExpanded((prev) => ({
                                  ...prev,
                                  [project.id]: !open,
                                }))
                              }
                              className="rounded p-0.5 text-muted-foreground transition-colors hover:bg-accent disabled:opacity-30"
                            >
                              <ChevronRight
                                className={cn(
                                  "size-3.5 transition-transform",
                                  open && "rotate-90",
                                )}
                                aria-hidden
                              />
                            </button>
                            <button
                              type="button"
                              disabled={formLocked}
                              onClick={() => setPicked(target)}
                              className="min-w-0 flex-1 truncate text-left"
                            >
                              {project.name || project.id}
                            </button>
                            {project.columns.length > 0 && (
                              <span className="shrink-0 text-[10px] text-muted-foreground">
                                {project.columns.length} 个分组
                              </span>
                            )}
                            <Badge
                              tone={projectRole === "ignore" ? "neutral" : "primary"}
                            >
                              {ticktickRoleLabel(projectRole)}
                            </Badge>
                          </div>

                          {open && project.columns.length > 0 && (
                            <ul className="space-y-0.5 border-t px-3 py-2">
                              {project.columns.map((column) => {
                                const columnRole =
                                  settings.ticktickColumnRoles[
                                    columnRoleKey(project.id, column.id)
                                  ] ?? "ignore";
                                const columnTarget: AssignTarget = {
                                  kind: "column",
                                  projectId: project.id,
                                  columnId: column.id,
                                  label: column.name || column.id,
                                };
                                const columnSelected =
                                  picked?.kind === "column" &&
                                  picked.columnId === column.id &&
                                  picked.projectId === project.id;
                                return (
                                  <li
                                    key={column.id}
                                    draggable={!formLocked}
                                    onDragStart={() => setDragPayload(columnTarget)}
                                    onDragEnd={() => {
                                      setDragPayload(null);
                                      setDragOverRole(null);
                                    }}
                                    className={cn(
                                      "flex cursor-grab items-center gap-2 rounded-md px-2 py-1.5 text-xs transition-colors active:cursor-grabbing",
                                      columnSelected
                                        ? "bg-primary/10"
                                        : "hover:bg-accent",
                                    )}
                                  >
                                    <span
                                      className="size-1.5 shrink-0 rounded-full bg-muted-foreground/40"
                                      aria-hidden
                                    />
                                    <button
                                      type="button"
                                      disabled={formLocked}
                                      onClick={() => setPicked(columnTarget)}
                                      className="min-w-0 flex-1 truncate text-left"
                                    >
                                      {column.name || column.id}
                                    </button>
                                    <Badge
                                      tone={
                                        columnRole === "ignore" ? "neutral" : "primary"
                                      }
                                    >
                                      {ticktickRoleLabel(columnRole)}
                                    </Badge>
                                  </li>
                                );
                              })}
                            </ul>
                          )}
                        </li>
                      );
                    })}
                  </ul>

                  <div className="flex flex-wrap items-center gap-3">
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={formLocked || syncing}
                      onClick={() => void handleSync()}
                    >
                      {ticktickSyncButtonLabel(syncing)}
                    </Button>
                    {syncNote && (
                      <p className="text-[11px] text-muted-foreground">{syncNote}</p>
                    )}
                  </div>
                  <p className="text-[11px] text-muted-foreground">
                    {formatTicktickLastSync(ttStatus.lastSync, Math.floor(Date.now() / 1000))}
                    。同步会拉取全部清单与分组，并自动最多每 30 分钟一次。
                  </p>
                  {truncated && (
                    <p className="text-[11px] text-warning">
                      当天有时段任务超过 20，请在 TickTick 勾完或改期。
                    </p>
                  )}
                </Section>
              )}
            </div>
          )}

          {tab === "permissions" && (
            <div className="space-y-4">
              {/* The macOS permission report would read "已允许" on every other
                  platform, because the stubs report the grants as true. */}
              {IS_MACOS ? <PermissionPanel /> : <PlatformNotice />}
            </div>
          )}

          {tab === "about" && (
            <div className="space-y-4">
              <Section title="关于">
                <div className="flex items-center gap-4">
                  <img src={appIcon} alt="" className="size-12 shrink-0" />
                  <div className="space-y-0.5">
                    <p className="text-base font-semibold tracking-tight">GameLife</p>
                    <p className="font-mono text-xs text-muted-foreground">
                      版本 {__APP_VERSION__}
                    </p>
                  </div>
                </div>
                <p className="text-xs leading-relaxed text-muted-foreground">
                  每 15 秒采样一次前台窗口，按 15 分钟槽判定主线 / 支线 / 杂项 / 娱乐，
                  再给有效主线时间发硬币与能量。排期留在 TickTick，本机只负责观测、判定、发币与统计。
                </p>
              </Section>

              <Section title="判定顺序" caption="改动设置后，只影响还没开始的槽。">
                <ol className="space-y-2 text-xs leading-relaxed text-muted-foreground">
                  <li className="flex gap-2">
                    <span className="font-medium text-foreground">1</span>
                    硬规则，按顺序：锁屏或暂停 →离开；娱乐名单 →娱乐；杂项名单 →杂项；
                    支线名单 →支线；阅读应用 →阅读桥接；主线应用 →主线候选；都不命中 →待定。
                  </li>
                  <li className="flex gap-2">
                    <span className="font-medium text-foreground">2</span>
                    元数据够确定就直接结算，不调 AI：落地活跃主线满 13 分钟自动记主线，
                    支线 / 杂项 / 娱乐合计满 5 分钟且压过主线就自动归到其中之一。
                  </li>
                  <li className="flex gap-2">
                    <span className="font-medium text-foreground">3</span>
                    剩下的是灰区，交给文本 AI：当天有带时段的 TickTick 任务就匹配任务，
                    没有就按「名单」里那四段类别说明归类。
                  </li>
                  <li className="flex gap-2">
                    <span className="font-medium text-foreground">4</span>
                    文本 AI 没给出高置信结论，才用那一槽的截图走视觉判断。
                    视觉失败或返回非法结果 → 记待复核，绝不猜成已确认。
                  </li>
                </ol>
              </Section>

              <Section title="本机数据">
                <div className="space-y-2 text-xs">
                  {[
                    ["数据目录", "~/Library/Application Support/GameLife/"],
                    ["数据库", "gamelife.db"],
                    ["设置", "config.json"],
                    ["密钥", "secrets.json（权限 600，不进 config.json）"],
                    ["截图", "screenshots/（按保留策略自动清理）"],
                    ["应用标识", "ma.haofei.gamelife"],
                  ].map(([label, value]) => (
                    <div key={label} className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                      <span className="w-20 shrink-0 text-muted-foreground">{label}</span>
                      <code className="break-all rounded bg-muted px-1.5 py-0.5 font-mono text-[11px]">
                        {value}
                      </code>
                    </div>
                  ))}
                </div>
                <p className="text-[11px] leading-relaxed text-muted-foreground">
                  采样间隔固定 15 秒。样本保留天数与截图保留都在「基础」里调。
                </p>
              </Section>
            </div>
          )}
        </div>
      </div>
    </>
  );
}

