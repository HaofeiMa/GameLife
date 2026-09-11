import { useEffect, useState } from "react";
import { PermissionBanner } from "../components/PermissionBanner";
import {
  BUILTIN_NEVER_CAPTURE,
  getSettings,
  hasApiKey,
  saveSettings,
  setApiKey,
  type AppSettings,
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
          placeholder="新增一项"
          onChange={(e) => setDraft(e.target.value)}
        />
        <button type="button" disabled={disabled || !draft.trim()} onClick={add}>
          添加
        </button>
      </div>
    </section>
  );
}

export function Settings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [apiKey, setApiKeyLocal] = useState("");
  const [keyPresent, setKeyPresent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<string | null>(null);

  useEffect(() => {
    getSettings().then(setSettings).catch(console.error);
    hasApiKey().then(setKeyPresent).catch(() => setKeyPresent(false));
  }, []);

  if (!settings) return <p className="muted">加载中…</p>;

  async function handleSave() {
    if (!settings) return;
    setBusy(true);
    setMsg(null);
    try {
      await saveSettings(settings);
      if (apiKey.trim()) {
        await setApiKey(apiKey.trim());
        setKeyPresent(true);
        setApiKeyLocal("");
      }
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
        <h3>OpenAI API Key（钥匙串）</h3>
        <p className="muted">{keyPresent ? "已配置钥匙串条目" : "未配置"}</p>
        <input
          type="password"
          placeholder="新 Key（保存时写入钥匙串）"
          value={apiKey}
          disabled={busy}
          onChange={(e) => setApiKeyLocal(e.target.value)}
        />
      </section>

      <button type="button" disabled={busy} onClick={handleSave}>
        {busy ? "保存中…" : "保存设置"}
      </button>
      {msg && <p className="muted">{msg}</p>}
    </div>
  );
}
