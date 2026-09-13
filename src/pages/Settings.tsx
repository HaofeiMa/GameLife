import { useEffect, useState } from "react";
import { PermissionBanner } from "../components/PermissionBanner";
import {
  BUILTIN_NEVER_CAPTURE,
  getSettings,
  providerKeyStatus,
  saveSettings,
  setProviderApiKey,
  ticktickBeginOauth,
  ticktickDisconnect,
  ticktickListProjects,
  ticktickSetClientSecret,
  ticktickStatus,
  ticktickSync,
  type AppSettings,
  type ProviderKeyStatus,
  type TickTickProject,
  type TickTickStatus,
  type VisionProviderSettings,
} from "../lib/api";
import { GUIDE_PLACEHOLDERS, savedCategoryGuides } from "../lib/guides";

const TICKTICK_ROLES = [
  ["ignore", "忽略"],
  ["mainline", "主线"],
  ["side", "支线"],
  ["longterm", "长期"],
  ["chore", "杂项"],
] as const;

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
    <section>
      <h3>{label}</h3>
      <ul>
        {items.map((item) => (
          <li key={item}>
            {item}
            <button
              type="button"
              disabled={disabled}
              onClick={() => onChange(items.filter((x) => x !== item))}
            >
              删除
            </button>
          </li>
        ))}
      </ul>
      <div className="slot-actions">
        <input
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
        <button
          type="button"
          disabled={disabled}
          onClick={add}
        >
          添加
        </button>
      </div>
    </section>
  );
}

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
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [tab, setTab] = useState<"basic" | "api" | "lists" | "ticktick">("basic");
  const [ticktickSecret, setTicktickSecret] = useState("");
  const [ttStatus, setTtStatus] = useState<TickTickStatus>({
    connected: false,
    lastSync: null,
  });
  const [projects, setProjects] = useState<TickTickProject[]>([]);
  const [truncated, setTruncated] = useState(false);

  useEffect(() => {
    getSettings()
      .then((s) => {
        setSettings(s);
        setLoadError(null);
      })
      .catch((e) => setLoadError(String(e)));
    providerKeyStatus().then(setKeyStatus).catch(() => undefined);
  }, []);

  useEffect(() => {
    if (tab !== "ticktick") return;
    let cancelled = false;
    ticktickStatus()
      .then(async (status) => {
        if (cancelled) return;
        setTtStatus(status);
        if (!status.connected) {
          setProjects([]);
          return;
        }
        const list = await ticktickListProjects();
        if (!cancelled) setProjects(list);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [tab]);

  if (loadError) {
    return (
      <div className="page">
        <p className="error">{loadError}</p>
      </div>
    );
  }
  if (!settings) return <p className="muted">加载中…</p>;

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
      ? settings.visionProviders.map((p) =>
          p.id === id ? { ...p, ...patch } : p,
        )
      : [...settings.visionProviders, { id, baseUrl: "", model: "", ...patch }];
    setSettings({ ...settings, visionProviders });
  }

  async function persistSecret() {
    const secret = ticktickSecret.trim();
    if (!secret) return;
    await ticktickSetClientSecret(secret);
    setTicktickSecret("");
  }

  async function persistSettings(next: AppSettings) {
    const trimmed = {
      ...next,
      categoryGuides: savedCategoryGuides(next.categoryGuides),
    };
    await saveSettings(trimmed);
    setSettings(trimmed);
    await persistSecret();
    return trimmed;
  }

  async function handleSave() {
    if (!settings) return;
    setBusy(true);
    setMsg(null);
    try {
      await persistSettings(settings);
      const nextStatus = { ...keyStatus };
      for (const id of ["opencode-go", "openai", "custom"] as const) {
        const typed = keys[id]?.trim();
        if (!typed) continue;
        await setProviderApiKey(id, typed);
        if (id === "opencode-go") nextStatus.opencodeGo = true;
        if (id === "openai") nextStatus.openai = true;
        if (id === "custom") nextStatus.custom = true;
      }
      setKeyStatus(nextStatus);
      setKeys({ "opencode-go": "", openai: "", custom: "" });
      setMsg("已保存");
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleConnect() {
    if (!settings) return;
    setBusy(true);
    setMsg(null);
    try {
      await persistSettings(settings);
      const { authorizeUrl } = await ticktickBeginOauth();
      window.open(authorizeUrl, "_blank", "noopener,noreferrer");
      const deadline = Date.now() + 180_000;
      while (Date.now() < deadline) {
        await new Promise((r) => setTimeout(r, 1000));
        const status = await ticktickStatus();
        setTtStatus(status);
        if (status.connected) {
          const list = await ticktickListProjects();
          setProjects(list);
          setMsg("已连接 TickTick");
          return;
        }
      }
      setMsg("等待授权超时");
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleDisconnect() {
    setBusy(true);
    setMsg(null);
    try {
      await ticktickDisconnect();
      setTtStatus({ connected: false, lastSync: null });
      setProjects([]);
      setTruncated(false);
      setMsg("已断开 TickTick");
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleSync() {
    setBusy(true);
    setMsg(null);
    try {
      const result = await ticktickSync();
      setTruncated(result.truncated);
      const status = await ticktickStatus();
      setTtStatus(status);
      setMsg(result.truncated ? "同步完成" : `同步完成（${result.count}）`);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleProjectRole(id: string, role: string) {
    if (!settings) return;
    const ticktickProjectRoles = { ...settings.ticktickProjectRoles };
    if (role === "ignore") {
      delete ticktickProjectRoles[id];
    } else {
      ticktickProjectRoles[id] = role;
    }
    const next = { ...settings, ticktickProjectRoles };
    setSettings(next);
    setProjects((rows) => rows.map((p) => (p.id === id ? { ...p, role } : p)));
    setBusy(true);
    setMsg(null);
    try {
      await persistSettings(next);
    } catch (e) {
      setMsg(String(e));
    } finally {
      setBusy(false);
    }
  }

  const userNeverCapture = settings.neverCaptureApps.filter(
    (n) => !BUILTIN_NEVER_CAPTURE.some((b) => b.toLowerCase() === n.toLowerCase()),
  );

  return (
    <div className="page settings-page">
      <h2>设置</h2>
      <PermissionBanner />
      <div className="settings-tabs" role="tablist">
        {(
          [
            ["basic", "基础"],
            ["api", "API"],
            ["lists", "名单"],
            ["ticktick", "TickTick"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={tab === id ? "active" : ""}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>

      {tab === "basic" && (
        <>
          <section>
            <label className="quest-hero-toggle">
              <input
                type="checkbox"
                checked={settings.loginAtStartup}
                disabled={busy}
                onChange={(e) =>
                  setSettings({ ...settings, loginAtStartup: e.target.checked })
                }
              />
              登录时启动（默认开启）
            </label>
          </section>
          <section>
            <label className="quest-hero-toggle">
              <input
                type="checkbox"
                checked={settings.showRailLabels}
                disabled={busy}
                onChange={(e) =>
                  setSettings({ ...settings, showRailLabels: e.target.checked })
                }
              />
              导航显示文字
            </label>
          </section>
          <section>
            <label>
              截图保留
              <select
                value={settings.screenshotRetention}
                disabled={busy}
                onChange={(e) =>
                  setSettings({ ...settings, screenshotRetention: e.target.value })
                }
              >
                <option value="none">不保留</option>
                <option value="24h">24 小时</option>
                <option value="3d">3 天</option>
                <option value="14d">14 天</option>
              </select>
            </label>
            <p className="muted">过期截图由采样器按此策略自动清理。</p>
          </section>
          <section>
            <label>
              样本保留天数
              <input
                type="number"
                min={3}
                max={14}
                value={settings.sampleKeepDays}
                disabled={busy}
                onChange={(e) =>
                  setSettings({ ...settings, sampleKeepDays: Number(e.target.value) })
                }
              />
            </label>
            <p className="muted">超过保留天数的样本行在启动时清理（默认 7 天）。采样间隔固定 15 秒。</p>
          </section>
        </>
      )}

      {tab === "api" && (
        <>
          <section>
            <h3>判定与视觉</h3>
            <p className="muted">
              Key 只进钥匙串。主用失败仅在超时、网络错误或 HTTP 5xx 时改走 fallback。
            </p>
            {!(
              (settings.primaryProvider === "opencode-go" && keyStatus.opencodeGo) ||
              (settings.primaryProvider === "openai" && keyStatus.openai) ||
              (settings.primaryProvider === "custom" && keyStatus.custom)
            ) && (
              <p className="error">
                主用提供商还没有 API Key。只填 URL / 模型不够，请在下方密码框粘贴 Key 后点「保存设置」。
              </p>
            )}
            <label>
              主用
              <select
                value={settings.primaryProvider}
                disabled={busy}
                onChange={(e) =>
                  setSettings({ ...settings, primaryProvider: e.target.value })
                }
              >
                <option value="opencode-go">OpenCode Go</option>
                <option value="openai">OpenAI</option>
                <option value="custom">自定义</option>
              </select>
            </label>
            <label>
              回退
              <select
                value={settings.fallbackProvider}
                disabled={busy}
                onChange={(e) =>
                  setSettings({ ...settings, fallbackProvider: e.target.value })
                }
              >
                <option value="none">无</option>
                <option value="opencode-go">OpenCode Go</option>
                <option value="openai">OpenAI</option>
                <option value="custom">自定义</option>
              </select>
            </label>
          </section>
          <ProviderEditor
            title="OpenCode Go"
            spec={provider("opencode-go")}
            keyPresent={keyStatus.opencodeGo}
            keyValue={keys["opencode-go"] ?? ""}
            busy={busy}
            onPatch={(patch) => patchProvider("opencode-go", patch)}
            onKeyChange={(v) => setKeys({ ...keys, "opencode-go": v })}
          />
          <ProviderEditor
            title="OpenAI / Codex 兼容"
            spec={provider("openai")}
            keyPresent={keyStatus.openai}
            keyValue={keys.openai ?? ""}
            busy={busy}
            onPatch={(patch) => patchProvider("openai", patch)}
            onKeyChange={(v) => setKeys({ ...keys, openai: v })}
          />
          <ProviderEditor
            title="自定义"
            spec={provider("custom")}
            keyPresent={keyStatus.custom}
            keyValue={keys.custom ?? ""}
            busy={busy}
            onPatch={(patch) => patchProvider("custom", patch)}
            onKeyChange={(v) => setKeys({ ...keys, custom: v })}
          />
        </>
      )}

      {tab === "lists" && (
        <>
          <p className="muted">
            Chrome / Safari / Arc 的当前标签 URL 需要「自动化」权限；拒绝则 URL
            为空，不影响采样与截图。
          </p>
          <ListEditor
            label="主线应用"
            items={settings.trustedApps}
            disabled={busy}
            onChange={(trustedApps) => setSettings({ ...settings, trustedApps })}
          />
          <ListEditor
            label="支线应用"
            items={settings.sideProjectRules}
            disabled={busy}
            onChange={(sideProjectRules) =>
              setSettings({ ...settings, sideProjectRules })
            }
          />
          <ListEditor
            label="杂项应用"
            items={settings.adminApps}
            disabled={busy}
            onChange={(adminApps) => setSettings({ ...settings, adminApps })}
          />
          <ListEditor
            label="娱乐应用 / 网站"
            items={settings.distractionRules}
            disabled={busy}
            onChange={(distractionRules) =>
              setSettings({ ...settings, distractionRules })
            }
          />
          <ListEditor
            label="阅读"
            items={settings.readingApps}
            disabled={busy}
            onChange={(readingApps) => setSettings({ ...settings, readingApps })}
          />
          <section>
            <h3>永不截屏（内置只读）</h3>
            <ul>
              {BUILTIN_NEVER_CAPTURE.map((n) => (
                <li key={n}>{n}</li>
              ))}
            </ul>
            <p className="muted">内置项不可删除。GameLife 自身也不发币。</p>
          </section>
          <ListEditor
            label="额外永不截屏"
            items={userNeverCapture}
            disabled={busy}
            onChange={(extra) =>
              setSettings({
                ...settings,
                neverCaptureApps: [...BUILTIN_NEVER_CAPTURE, ...extra],
              })
            }
          />
          <section>
            <h3>类别说明</h3>
            <p className="muted">灰字为样稿，空着保存不会写进判定规则。</p>
            {(
              [
                ["mainline", "主线"],
                ["side", "支线"],
                ["admin", "杂项"],
                ["entertainment", "娱乐"],
              ] as const
            ).map(([key, label]) => (
              <label key={key}>
                {label}
                <textarea
                  className="guide-textarea"
                  maxLength={500}
                  disabled={busy}
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
              </label>
            ))}
          </section>
        </>
      )}

      {tab === "ticktick" && (
        <>
          <section>
            <h3>连接</h3>
            <p className="muted">
              到 TickTick 开发者中心建应用，Redirect URI 填
              {" "}
              <code>http://127.0.0.1:18789/callback</code>
              。Client Secret 只进钥匙串。
            </p>
            {!ttStatus.connected && (
              <p className="error">尚未连接 TickTick。</p>
            )}
            <label>
              Client ID
              <input
                value={settings.ticktickClientId}
                disabled={busy}
                onChange={(e) =>
                  setSettings({ ...settings, ticktickClientId: e.target.value })
                }
              />
            </label>
            <label>
              Client Secret
              <input
                type="password"
                value={ticktickSecret}
                disabled={busy}
                placeholder="保存或连接时写入钥匙串"
                onChange={(e) => setTicktickSecret(e.target.value)}
              />
            </label>
            <div className="slot-actions">
              <button
                type="button"
                disabled={busy}
                onClick={() => void handleConnect()}
              >
                连接
              </button>
              {ttStatus.connected && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void handleDisconnect()}
                >
                  断开
                </button>
              )}
            </div>
          </section>
          {ttStatus.connected && (
            <section>
              <h3>清单角色</h3>
              {projects.length === 0 ? (
                <p className="muted">没有清单。</p>
              ) : (
                <ul>
                  {projects.map((project) => (
                    <li key={project.id} className="slot-actions">
                      <span>{project.name}</span>
                      <select
                        value={project.role || "ignore"}
                        disabled={busy}
                        onChange={(e) =>
                          void handleProjectRole(project.id, e.target.value)
                        }
                      >
                        {TICKTICK_ROLES.map(([value, label]) => (
                          <option key={value} value={value}>
                            {label}
                          </option>
                        ))}
                      </select>
                    </li>
                  ))}
                </ul>
              )}
              <div className="slot-actions">
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void handleSync()}
                >
                  同步任务
                </button>
              </div>
              {truncated && (
                <p className="muted">
                  当天有时段任务超过 20，请在 TickTick 勾完或改期。
                </p>
              )}
            </section>
          )}
        </>
      )}

      <button type="button" disabled={busy} onClick={() => void handleSave()}>
        {busy ? "保存中…" : "保存设置"}
      </button>
      {msg && <p className="muted">{msg}</p>}
    </div>
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
}: {
  title: string;
  spec: VisionProviderSettings;
  keyPresent: boolean;
  keyValue: string;
  busy: boolean;
  onPatch: (patch: Partial<VisionProviderSettings>) => void;
  onKeyChange: (value: string) => void;
}) {
  return (
    <section>
      <h3>{title}</h3>
      <p className="muted">{keyPresent ? "已配置钥匙串条目" : "未配置"}</p>
      <label>
        Base URL
        <input
          value={spec.baseUrl}
          disabled={busy}
          placeholder="https://…"
          onChange={(e) => onPatch({ baseUrl: e.target.value })}
        />
      </label>
      <label>
        模型
        <input
          value={spec.model}
          disabled={busy}
          placeholder="模型 ID"
          onChange={(e) => onPatch({ model: e.target.value })}
        />
      </label>
      <label>
        API Key
        <input
          type="password"
          value={keyValue}
          disabled={busy}
          placeholder="新 Key（保存时写入钥匙串）"
          onChange={(e) => onKeyChange(e.target.value)}
        />
      </label>
    </section>
  );
}
