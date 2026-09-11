import { useEffect, useState } from "react";
import { PermissionBanner } from "../components/PermissionBanner";
import {
  BUILTIN_NEVER_CAPTURE,
  getSettings,
  providerKeyStatus,
  saveSettings,
  setProviderApiKey,
  type AppSettings,
  type ProviderKeyStatus,
  type VisionProviderSettings,
} from "../lib/api";

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

  useEffect(() => {
    getSettings()
      .then((s) => {
        setSettings(s);
        setLoadError(null);
      })
      .catch((e) => setLoadError(String(e)));
    providerKeyStatus().then(setKeyStatus).catch(() => undefined);
  }, []);

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

  async function handleSave() {
    if (!settings) return;
    setBusy(true);
    setMsg(null);
    try {
      await saveSettings(settings);
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

  const userNeverCapture = settings.neverCaptureApps.filter(
    (n) => !BUILTIN_NEVER_CAPTURE.some((b) => b.toLowerCase() === n.toLowerCase()),
  );

  return (
    <div className="page">
      <h2>设置</h2>
      <PermissionBanner />
      <p className="muted">采样间隔固定 15s，此处不提供调节。</p>
      <p className="muted">
        Chrome / Safari / Arc 的当前标签 URL 需要「自动化」权限；拒绝则 URL
        为空，不影响采样与截图。不要把这项做成缺了就无法观测。
      </p>

      <ListEditor
        label="Trusted 应用"
        items={settings.trustedApps}
        disabled={busy}
        onChange={(trustedApps) => setSettings({ ...settings, trustedApps })}
      />

      <ListEditor
        label="Distraction 规则"
        items={settings.distractionRules}
        disabled={busy}
        onChange={(distractionRules) =>
          setSettings({ ...settings, distractionRules })
        }
      />

      <ListEditor
        label="Reading 应用"
        items={settings.readingApps}
        disabled={busy}
        onChange={(readingApps) => setSettings({ ...settings, readingApps })}
      />

      <section>
        <h3>Never Capture（内置只读）</h3>
        <ul>
          {BUILTIN_NEVER_CAPTURE.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
        <p className="muted">GameLife 侧项目规则内置，不可删除。</p>
      </section>

      <ListEditor
        label="额外 Never Capture"
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
        <p className="muted">超过保留天数的样本行由采样器启动时自动清理（默认 7 天）。</p>
      </section>

      <section>
        <label>
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
        <h3>视觉模型</h3>
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
          Fallback
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

      <button type="button" disabled={busy} onClick={handleSave}>
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
