import { ChevronDown, ChevronRight, ChevronUp, Plus, X } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import appIcon from "../../src-tauri/icons/128x128@2x.png";
import { PageHeader } from "../components/PageHeader";
import { PermissionPanel } from "../components/PermissionBanner";
import { PlatformNotice } from "../components/PlatformNotice";
import { IS_MACOS } from "../lib/platform";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { Card } from "../components/ui/card";
import { Dialog } from "../components/ui/dialog";
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
  syncListDevices,
  syncNow,
  syncRestore,
  syncSetCredentials,
  syncStatus,
  syncTestConnection,
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
  type SyncDevice,
  type SyncSettings,
  type SyncStatus,
  type TickTickTree,
  type TickTickStatus,
  type VisionProviderSettings,
} from "../lib/api";
import {
  defaultSyncSettings,
  deviceName,
  formatBytes,
  formatCloudStatus,
  formatLastSeen,
  normalizeScope,
  normalizeSettleGraceHours,
  scopeLabel,
} from "../lib/cloudSync";
import { GUIDE_PLACEHOLDERS, savedCategoryGuides } from "../lib/guides";
import {
  callbackPasteKind,
  oauthErrorMessage,
  oauthWaitingHint,
  policySignature,
  SECRET_MASK,
  secretToPersist,
  showSecretMask,
  ticktickSecretReady,
} from "../lib/secretField";
import {
  addCodexPanel,
  addCustomPanel,
  chainHasUsable,
  customPanelTitle,
  isCodexProvider,
  migrateVisionProviders,
  moveProvider,
  withVisionProviders,
} from "../lib/providers";
import { notifySettingsChanged } from "../lib/settingsEvents";
import {
  columnRoleKey,
  formatTicktickLastSync,
  groupTicktickTodayTasks,
  nextRoleMap,
  TICKTICK_ROLE_COLUMNS,
  TICKTICK_TASK_ROLES,
  ticktickRoleLabel,
  ticktickSyncButtonLabel,
  ticktickSyncErrorMessage,
  ticktickTaskTimeLabel,
} from "../lib/ticktickBoard";
import { cn } from "../lib/utils";

type SettingsTab =
  | "basic"
  | "api"
  | "lists"
  | "ticktick"
  | "cloud"
  | "permissions"
  | "about";

/** One draggable thing in the TickTick tree. */
type AssignTarget =
  | { kind: "project"; projectId: string; label: string }
  | { kind: "column"; projectId: string; columnId: string; label: string };

const TABS: { value: SettingsTab; label: string }[] = [
  { value: "basic", label: "基础" },
  { value: "api", label: "API" },
  { value: "lists", label: "名单" },
  { value: "ticktick", label: "TickTick" },
  { value: "cloud", label: "云端" },
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
    <div className="flex flex-col gap-[6px]">
      <Label>{label}</Label>
      {children}
      {hint && <p className="text-[11px] leading-relaxed text-muted-foreground">{hint}</p>}
    </div>
  );
}

/** The mockup's .frow — an inset card per setting, one per line. */
function Row({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center gap-[14px] rounded-[14px] bg-loot px-[13px] py-2.5">
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-semibold">{title}</p>
        {description && (
          <p className="mt-0.5 text-[11px] leading-[1.5] text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {children}
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
    <Row title={title} description={description}>
      <Switch
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
        aria-label={title}
      />
    </Row>
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
  codexLoggedIn,
  canMoveUp,
  canMoveDown,
  onMove,
  onRemove,
  onPatch,
  onKeyChange,
  onTest,
  onRefreshCodex,
}: {
  title: string;
  spec: VisionProviderSettings;
  keyPresent: boolean;
  keyValue: string;
  busy: boolean;
  codexLoggedIn: boolean;
  canMoveUp: boolean;
  canMoveDown: boolean;
  onMove: (delta: number) => void;
  onRemove: () => void;
  onPatch: (patch: Partial<VisionProviderSettings>) => void;
  onKeyChange: (value: string) => void;
  onTest: () => void;
  onRefreshCodex: () => void;
}) {
  const codex = isCodexProvider(spec);
  return (
    <Section
      title={title}
      caption={
        <span className={codex ? (codexLoggedIn ? "text-success" : undefined) : keyPresent ? "text-success" : undefined}>
          {codex ? (codexLoggedIn ? "已授权" : "未登录") : keyPresent ? "已保存" : "未配置"}
        </span>
      }
    >
      <div className="flex justify-end gap-1">
        <Button variant="ghost" size="icon-sm" disabled={busy || !canMoveUp} onClick={() => onMove(-1)} aria-label="上移">
          <ChevronUp className="size-4" />
        </Button>
        <Button variant="ghost" size="icon-sm" disabled={busy || !canMoveDown} onClick={() => onMove(1)} aria-label="下移">
          <ChevronDown className="size-4" />
        </Button>
        <Button variant="ghost" size="icon-sm" disabled={busy} onClick={onRemove} aria-label="删除">
          <X className="size-4" />
        </Button>
      </div>
      {codex ? (
        <>
          <p className="text-[11px] leading-relaxed text-muted-foreground">
            使用本机 <code className="font-mono">codex login</code>{" "}
            的会话，不必填写 API Key。模型需能看图，判定会传截图。
          </p>
          <Field label="模型">
            <Input
              value={spec.model}
              disabled={busy}
              placeholder="gpt-5.4"
              onChange={(e) => onPatch({ model: e.target.value })}
            />
          </Field>
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" size="sm" disabled={busy} onClick={onRefreshCodex}>
              刷新登录状态
            </Button>
            <Button variant="outline" size="sm" disabled={busy || !codexLoggedIn} onClick={onTest}>
              测试连接
            </Button>
          </div>
        </>
      ) : (
        <>
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
        </>
      )}
    </Section>
  );
}

/* -------------------------------- page ------------------------------ */

export function Settings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [keyStatus, setKeyStatus] = useState<ProviderKeyStatus>({
    keys: {},
    codexLoggedIn: false,
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
    todayTasks: [],
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
  const [cloudStatus, setCloudStatus] = useState<SyncStatus | null>(null);
  const [cloudPassword, setCloudPassword] = useState("");
  const [cloudBusy, setCloudBusy] = useState(false);
  const [cloudNote, setCloudNote] = useState<string | null>(null);
  const [restoreDevice, setRestoreDevice] = useState<SyncDevice | null>(null);
  const formLocked = saving;
  /** Older config.json may predate the cloud settings; never render undefined. */
  const sync = settings?.sync ?? defaultSyncSettings();
  /** The policy fingerprint as the backend last saw it. */
  const lastPolicySig = useRef<string | null>(null);

  useEffect(() => {
    getSettings()
      .then((s) => {
        const visionProviders = migrateVisionProviders(
          s.visionProviders,
          s.primaryProvider,
          s.fallbackProvider,
        );
        setSettings(withVisionProviders(s, visionProviders));
        lastPolicySig.current = policySignature(s);
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

  // Cached status only — opening 云端 must not contact the remote.
  useEffect(() => {
    if (tab !== "cloud") return;
    let cancelled = false;
    syncStatus()
      .then((s) => {
        if (!cancelled) setCloudStatus(s);
      })
      .catch((e) => {
        if (!cancelled) setCloudNote(String(e));
      });
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
        <div className="flex-1 overflow-y-auto px-[22px]">
          <div className="mx-auto flex w-[728px] max-w-full flex-col gap-3 pb-4">
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
        <div className="flex-1 overflow-y-auto px-[22px]">
          <div className="mx-auto flex w-[728px] max-w-full flex-col gap-3 pb-4">
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-32 animate-shimmer rounded-xl bg-muted" />
            ))}
          </div>
        </div>
      </>
    );
  }

  function patchProvider(id: string, patch: Partial<VisionProviderSettings>) {
    if (!settings) return;
    const exists = settings.visionProviders.some((p) => p.id === id);
    const visionProviders = exists
      ? settings.visionProviders.map((p) => (p.id === id ? { ...p, ...patch } : p))
      : [...settings.visionProviders, { id, baseUrl: "", model: "", kind: "custom", ...patch }];
    setSettings({ ...settings, visionProviders });
  }

  async function persistSettings(next: AppSettings, updatePolicy: boolean) {
    const trimmed = {
      ...next,
      categoryGuides: savedCategoryGuides(next.categoryGuides),
      ticktickProjectRoles: next.ticktickProjectRoles ?? {},
      ticktickColumnRoles: next.ticktickColumnRoles ?? {},
      sync: {
        ...next.sync,
        settleGraceHours: normalizeSettleGraceHours(next.sync.settleGraceHours),
      },
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
      for (const p of settings.visionProviders) {
        if (isCodexProvider(p)) continue;
        const typed = secretToPersist(keys[p.id] ?? "");
        if (!typed) continue;
        await setProviderApiKey(p.id, typed);
      }
      const tt = secretToPersist(ticktickSecret);
      if (tt) {
        await ticktickSetClientSecret(tt);
        setTicktickSecret("");
      }
      const trimmed = await persistSettings(
        settings,
        policySignature(settings) !== lastPolicySig.current,
      );
      lastPolicySig.current = policySignature(trimmed);
      notifySettingsChanged();
      const confirmed = await providerKeyStatus();
      setKeyStatus(confirmed);
      setKeys({});
      const nextTt = await ticktickStatus();
      setTtStatus(nextTt);
      if (!chainHasUsable(trimmed.visionProviders, confirmed.keys, confirmed.codexLoggedIn)) {
        setMsg("设置已写入，但还没有可用的 API。请填写自定义 Key，或先在终端运行 codex login。");
      } else {
        setMsg("已保存");
      }
    } catch (e) {
      setMsg(String(e));
    } finally {
      setSaving(false);
    }
  }

  /** Cloud settings save on change — 保存设置 is the policy form's button, and a
   * half-configured remote should not depend on it. */
  async function persistSync(next: SyncSettings) {
    if (!settings) return;
    await persistBasic({
      ...settings,
      sync: {
        ...next,
        settleGraceHours: normalizeSettleGraceHours(next.settleGraceHours),
      },
    });
  }

  async function handleCloudSync() {
    setCloudBusy(true);
    setCloudNote(null);
    try {
      setCloudStatus(await syncNow());
      setCloudNote("已上传");
    } catch (e) {
      setCloudNote(String(e));
    } finally {
      setCloudBusy(false);
    }
  }

  async function handleCloudTest() {
    setCloudBusy(true);
    setCloudNote(null);
    try {
      setCloudNote(await syncTestConnection());
    } catch (e) {
      setCloudNote(String(e));
    } finally {
      setCloudBusy(false);
    }
  }

  async function handleCloudPassword() {
    const typed = secretToPersist(cloudPassword);
    if (!typed) return;
    setCloudBusy(true);
    setCloudNote(null);
    try {
      await syncSetCredentials(typed);
      setCloudPassword("");
      setCloudNote("凭据已保存");
    } catch (e) {
      setCloudNote(String(e));
    } finally {
      setCloudBusy(false);
    }
  }

  async function handleCloudDevices() {
    setCloudBusy(true);
    setCloudNote(null);
    try {
      const devices = await syncListDevices();
      setCloudStatus((prev) => (prev ? { ...prev, devices } : prev));
    } catch (e) {
      setCloudNote(String(e));
    } finally {
      setCloudBusy(false);
    }
  }

  async function handleCloudRestore() {
    if (!restoreDevice) return;
    setCloudBusy(true);
    setCloudNote(null);
    try {
      const path = await syncRestore(restoreDevice.deviceId);
      setCloudNote(`已写成 ${path}。退出应用后把它改名为 gamelife.db 再打开。`);
      setRestoreDevice(null);
    } catch (e) {
      setCloudNote(String(e));
    } finally {
      setCloudBusy(false);
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
        todayTasks: [],
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
          : `同步完成（${result.count} 条任务）`,
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
  const todayGroups = groupTicktickTodayTasks(ttStatus.todayTasks ?? []);

  return (
    <>
      {header}
      <div className="flex-1 overflow-y-auto px-[22px]">
        <div className="mx-auto flex w-[728px] max-w-full flex-col gap-3 pb-4">
          {msg && (
            <p className="rounded-lg bg-muted px-3 py-2 text-xs text-muted-foreground">
              {msg}
            </p>
          )}

          {tab === "basic" && (
            <div className="flex flex-col gap-3">
              <Section title="外观">
                <Row
                  title="主题"
                  description="跟随系统时，会跟着 macOS / Windows 的深色模式切换。"
                >
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
                      { value: "light", label: "浅色" },
                      { value: "dark", label: "深色" },
                    ]}
                  />
                </Row>
              </Section>

              <Section title="启动与导航">
                <ToggleRow
                  title="登录时启动"
                  description="开机后自动在托盘常驻，不弹窗。"
                  checked={settings.loginAtStartup}
                  disabled={formLocked}
                  onChange={(v) =>
                    void persistBasic({ ...settings, loginAtStartup: v })
                  }
                />
                <ToggleRow
                  title="静默启动"
                  description="启动时不打开窗口，只在菜单栏出现图标。"
                  checked={settings.silentStart !== false}
                  disabled={formLocked}
                  onChange={(v) => void persistBasic({ ...settings, silentStart: v })}
                />
                <ToggleRow
                  title="导航显示文字"
                  description="关掉后侧栏只剩图标，窗口可以更窄。"
                  checked={settings.showRailLabels}
                  disabled={formLocked}
                  onChange={(v) =>
                    void persistBasic({ ...settings, showRailLabels: v })
                  }
                />
              </Section>

              <Section
                title="保留与清理"
                caption="原始样本会被定期删除；日汇总表会一直留着，所以月度统计不会因为清理而消失。"
              >
                <Field label="截图保留">
                  <Select
                    value={settings.screenshotRetention}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({ ...settings, screenshotRetention: e.target.value })
                    }
                  >
                    <option value="none">不保留截图</option>
                    <option value="24h">24 小时</option>
                    <option value="3d">3 天</option>
                    <option value="14d">14 天</option>
                  </Select>
                </Field>
                <Field label="样本保留天数">
                  <Input
                    type="number"
                    min={3}
                    max={14}
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
            <div className="flex flex-col gap-3">
              <Section
                title="判定与视觉"
                caption="按从上到下的顺序尝试。仅超时、网络错误或 HTTP 5xx 才试下一张。自定义 Key 只保存在本机 secrets.json（权限 600）。"
              >
                {!chainHasUsable(
                  settings.visionProviders,
                  keyStatus.keys,
                  keyStatus.codexLoggedIn,
                ) && (
                  <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
                    还没有可用的 API。请填写自定义 Key 后点「保存设置」，或在终端运行 codex login 后刷新。
                  </p>
                )}
              </Section>

              {settings.visionProviders.map((spec, index) => (
                <ProviderEditor
                  key={spec.id}
                  title={
                    isCodexProvider(spec)
                      ? "Codex"
                      : customPanelTitle(settings.visionProviders, spec.id)
                  }
                  spec={spec}
                  keyPresent={Boolean(keyStatus.keys[spec.id])}
                  keyValue={keys[spec.id] ?? ""}
                  busy={formLocked || testing}
                  codexLoggedIn={keyStatus.codexLoggedIn}
                  canMoveUp={index > 0}
                  canMoveDown={index < settings.visionProviders.length - 1}
                  onMove={(delta) =>
                    setSettings(
                      withVisionProviders(
                        settings,
                        moveProvider(settings.visionProviders, index, delta),
                      ),
                    )
                  }
                  onRemove={() =>
                    setSettings(
                      withVisionProviders(
                        settings,
                        settings.visionProviders.filter((p) => p.id !== spec.id),
                      ),
                    )
                  }
                  onPatch={(patch) => patchProvider(spec.id, patch)}
                  onKeyChange={(v) => setKeys({ ...keys, [spec.id]: v })}
                  onTest={() => void handleTestProvider(spec.id)}
                  onRefreshCodex={() => {
                    void providerKeyStatus().then(setKeyStatus);
                  }}
                />
              ))}

              <div className="flex flex-wrap gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  disabled={formLocked}
                  onClick={() =>
                    setSettings(
                      withVisionProviders(
                        settings,
                        addCustomPanel(settings.visionProviders),
                      ),
                    )
                  }
                >
                  <Plus className="size-3.5" />
                  添加自定义 API
                </Button>
                {!settings.visionProviders.some(isCodexProvider) && (
                  <Button
                    variant="outline"
                    size="sm"
                    disabled={formLocked}
                    onClick={() =>
                      setSettings(
                        withVisionProviders(
                          settings,
                          addCodexPanel(settings.visionProviders),
                        ),
                      )
                    }
                  >
                    添加 Codex
                  </Button>
                )}
              </div>
            </div>
          )}

          {tab === "lists" && (
            <div className="flex flex-col gap-3">
              <p className="rounded-lg bg-muted px-3 py-2 text-xs text-muted-foreground">
                Chrome / Safari / Arc 的当前标签 URL 需要「自动化」权限；拒绝则 URL 为空，不影响采样与截图。
              </p>
              <ListEditor
                label="支线应用"
                items={settings.sideProjectRules}
                disabled={formLocked}
                onChange={(sideProjectRules) =>
                  setSettings({ ...settings, sideProjectRules })
                }
              />
              <ListEditor
                label="杂项应用 / 网站"
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
            <div className="flex flex-col gap-3">
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
                  <div>
                    <p className="text-xs font-medium">当天任务</p>
                    <p className="mt-0.5 text-[11px] text-muted-foreground">
                      按映射角色列出当天未完成的任务名。全天或只有日期的也会出现在这里，但不进入判定。
                    </p>
                  </div>
                  <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
                    {TICKTICK_TASK_ROLES.map((role) => {
                      const list = todayGroups[role];
                      return (
                        <div
                          key={role}
                          className="rounded-lg border px-3 py-2"
                        >
                          <p className="text-[11px] font-medium">
                            {ticktickRoleLabel(role)}
                            <span className="ml-1 tabular-nums text-muted-foreground">
                              {list.length}
                            </span>
                          </p>
                          {list.length === 0 ? (
                            <p className="mt-1 text-[11px] text-muted-foreground">
                              无
                            </p>
                          ) : (
                            <ul className="mt-1 space-y-1">
                              {list.map((task) => (
                                <li key={task.id} className="min-w-0">
                                  <p className="truncate text-[12px]">{task.title}</p>
                                  <p className="text-[10px] tabular-nums text-muted-foreground">
                                    {ticktickTaskTimeLabel(task)}
                                  </p>
                                </li>
                              ))}
                            </ul>
                          )}
                        </div>
                      );
                    })}
                  </div>
                  {truncated && (
                    <p className="text-[11px] text-warning">
                      当天有时段任务超过 20，请在 TickTick 勾完或改期。
                    </p>
                  )}
                </Section>
              )}
            </div>
          )}

          {tab === "cloud" && (
            <div className="flex flex-col gap-3">
              <Section
                title="云端备份"
                caption="把本地库的一致性快照上传到你自己的 WebDAV 或 S3 兼容存储。远端只是副本，采样、判定与结算永远读本地库。"
              >
                <ToggleRow
                  title="开启云端备份"
                  description="关闭时不会发出任何请求。"
                  checked={sync.enabled}
                  disabled={formLocked}
                  onChange={(v) => void persistSync({ ...sync, enabled: v })}
                />
                <Field label="存储类型">
                  <Select
                    value={sync.target}
                    disabled={formLocked}
                    onChange={(e) =>
                      void persistSync({ ...sync, target: e.target.value })
                    }
                  >
                    <option value="webdav">WebDAV（坚果云 / Nextcloud）</option>
                    <option value="s3">S3 兼容（Cloudflare R2 / B2 / MinIO）</option>
                  </Select>
                </Field>
                <Field
                  label={sync.target === "s3" ? "Endpoint" : "WebDAV 地址"}
                  hint={
                    sync.target === "s3"
                      ? undefined
                      : "填到目录为止，例如 https://dav.jianguoyun.com/dav/"
                  }
                >
                  <Input
                    value={sync.url}
                    disabled={formLocked}
                    placeholder={
                      sync.target === "s3"
                        ? "https://<accountid>.r2.cloudflarestorage.com"
                        : "https://dav.jianguoyun.com/dav/"
                    }
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        sync: { ...sync, url: e.target.value },
                      })
                    }
                  />
                </Field>
                {sync.target === "s3" && (
                  <>
                    <Field label="Bucket">
                      <Input
                        value={sync.bucket}
                        disabled={formLocked}
                        onChange={(e) =>
                          setSettings({
                            ...settings,
                            sync: { ...sync, bucket: e.target.value },
                          })
                        }
                      />
                    </Field>
                    <Field label="Region" hint="Cloudflare R2 填 auto。">
                      <Input
                        value={sync.region}
                        disabled={formLocked}
                        onChange={(e) =>
                          setSettings({
                            ...settings,
                            sync: { ...sync, region: e.target.value },
                          })
                        }
                      />
                    </Field>
                  </>
                )}
                <Field label={sync.target === "s3" ? "Access Key ID" : "账号"}>
                  <Input
                    value={sync.username}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        sync: { ...sync, username: e.target.value },
                      })
                    }
                  />
                </Field>
                <Field
                  label={sync.target === "s3" ? "Secret Access Key" : "密码 / 应用密码"}
                  hint="只写进本机 secrets.json（权限 0600），不会回读，也不会随快照上传。"
                >
                  <div className="flex gap-2">
                    <Input
                      type="password"
                      value={cloudPassword}
                      disabled={formLocked || cloudBusy}
                      placeholder={SECRET_MASK}
                      onChange={(e) => setCloudPassword(e.target.value)}
                    />
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={
                        formLocked || cloudBusy || !secretToPersist(cloudPassword)
                      }
                      onClick={() => void handleCloudPassword()}
                    >
                      保存凭据
                    </Button>
                  </div>
                </Field>
              </Section>

              <Section
                title="同步内容与频率"
                caption="默认档位只上传判定结果、金币与能量流水、每日汇总和商店数据。"
              >
                <Field label="同步范围">
                  <Select
                    value={normalizeScope(sync.scope)}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        sync: { ...sync, scope: e.target.value },
                      })
                    }
                  >
                    <option value="aggregate">仅判定与汇总（不含窗口标题）</option>
                    <option value="samples">含原始采样（含窗口标题与路径）</option>
                  </Select>
                </Field>
                {normalizeScope(sync.scope) === "samples" && (
                  <p className="rounded-[10px] bg-warning/10 px-3 py-2 text-[11px] leading-relaxed text-warning">
                    这一档会把窗口标题、URL 和文档路径一起上传。受保护窗口的脱敏只发生在 AI
                    层，数据库里仍是原文，请只在完全自控的存储上使用。
                  </p>
                )}
                <Field
                  label="远端目录"
                  hint={`快照写到 ${sync.remotePath || "gamelife"}/<设备号>/ 下。`}
                >
                  <Input
                    value={sync.remotePath}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        sync: { ...sync, remotePath: e.target.value },
                      })
                    }
                  />
                </Field>
                <Field label="同步间隔（分钟）">
                  <Input
                    type="number"
                    min={5}
                    value={sync.intervalMinutes}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        sync: {
                          ...sync,
                          intervalMinutes: Number(e.target.value),
                        },
                      })
                    }
                  />
                </Field>
                <Field
                  label="保留快照份数"
                  hint="0 表示只留最新一份，不再保留每日快照。"
                >
                  <Input
                    type="number"
                    min={0}
                    value={sync.keepSnapshots}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        sync: {
                          ...sync,
                          keepSnapshots: Number(e.target.value),
                        },
                      })
                    }
                  />
                </Field>
                <Field
                  label="结算宽限期（小时）"
                  hint="两台以上设备时，一天结束后再等这么久才结算迟到的机器。默认 36。"
                >
                  <Input
                    type="number"
                    min={1}
                    value={
                      Number.isFinite(sync.settleGraceHours) ? sync.settleGraceHours : 36
                    }
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        sync: {
                          ...sync,
                          settleGraceHours: Number(e.target.value),
                        },
                      })
                    }
                    onBlur={() =>
                      void persistSync({
                        ...sync,
                        settleGraceHours: normalizeSettleGraceHours(
                          sync.settleGraceHours,
                        ),
                      })
                    }
                  />
                </Field>
                <Field label="设备名称" hint="留空时自动使用「系统 · 设备号前六位」。">
                  <Input
                    value={sync.deviceLabel}
                    disabled={formLocked}
                    onChange={(e) =>
                      setSettings({
                        ...settings,
                        sync: { ...sync, deviceLabel: e.target.value },
                      })
                    }
                  />
                </Field>
              </Section>

              <Section
                title="状态"
                caption={
                  cloudNote ??
                  (cloudStatus
                    ? `${formatCloudStatus(cloudStatus)} · 范围：${scopeLabel(cloudStatus.scope)}`
                    : "读取中…")
                }
              >
                <Row
                  title="本机设备号"
                  description={cloudStatus?.deviceId ?? "—"}
                >
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={cloudBusy || formLocked}
                    onClick={() => void handleCloudSync()}
                  >
                    {cloudBusy ? "处理中…" : "立即同步"}
                  </Button>
                </Row>
                <Row
                  title="上次快照"
                  description={cloudStatus ? formatBytes(cloudStatus.snapshotBytes) : "—"}
                >
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={cloudBusy || formLocked}
                    onClick={() => void handleCloudTest()}
                  >
                    测试连接
                  </Button>
                </Row>
                <Row
                  title="已登记设备"
                  description="点「刷新列表」从远端读取。恢复不会覆盖正在用的库。"
                >
                  <Button
                    size="sm"
                    variant="outline"
                    disabled={cloudBusy || formLocked}
                    onClick={() => void handleCloudDevices()}
                  >
                    刷新列表
                  </Button>
                </Row>
                {(cloudStatus?.devices ?? []).length === 0 ? (
                  <p className="text-[11px] text-muted-foreground">还没有读取到设备。</p>
                ) : (
                  <ul className="space-y-2">
                    {cloudStatus!.devices.map((device) => (
                      <li
                        key={device.deviceId}
                        className="flex items-start justify-between gap-3 rounded-lg border px-3 py-2"
                      >
                        <div className="min-w-0">
                          <p className="text-sm font-medium">{deviceName(device)}</p>
                          <p className="text-[11px] text-muted-foreground">
                            {device.platform || "未知平台"} · 最近{" "}
                            {formatLastSeen(device.lastSeen)}
                          </p>
                        </div>
                        <Button
                          size="sm"
                          variant="outline"
                          disabled={cloudBusy || formLocked}
                          onClick={() => setRestoreDevice(device)}
                        >
                          恢复
                        </Button>
                      </li>
                    ))}
                  </ul>
                )}
              </Section>
            </div>
          )}

          {tab === "permissions" && (
            <div className="flex flex-col gap-3">
              {/* The macOS permission report would read "已允许" on every other
                  platform, because the stubs report the grants as true. */}
              {IS_MACOS ? <PermissionPanel /> : <PlatformNotice />}
            </div>
          )}

          {tab === "about" && (
            <div className="flex flex-col gap-3">
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
      <Dialog
        open={restoreDevice != null}
        onClose={() => {
          if (!cloudBusy) setRestoreDevice(null);
        }}
        title="从这台设备恢复？"
        description={
          restoreDevice
            ? `会把「${deviceName(restoreDevice)}」的最新快照写成 gamelife.restored.db，不会覆盖正在使用的库。退出应用后自行改名替换。`
            : undefined
        }
        className="max-w-sm"
        footer={
          <>
            <Button
              variant="outline"
              onClick={() => setRestoreDevice(null)}
              disabled={cloudBusy}
            >
              取消
            </Button>
            <Button onClick={() => void handleCloudRestore()} disabled={cloudBusy}>
              {cloudBusy ? "下载中…" : "下载快照"}
            </Button>
          </>
        }
      />
    </>
  );
}

