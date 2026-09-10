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

  return (
    <div className="page">
      <h2>设置</h2>
      <PermissionBanner />
      <p className="muted">采样间隔固定 15s，此处不提供调节。</p>

      <section>
        <h3>Never Capture（内置只读）</h3>
        <ul>
          {BUILTIN_NEVER_CAPTURE.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
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
