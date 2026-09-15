#!/usr/bin/env node
/**
 * Render the four rail tabs plus settings sub-pages with VITE_PREVIEW=1
 * and write PNGs for the README.
 */
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "docs/screenshots");
const port = 4179;
const origin = `http://127.0.0.1:${port}`;

function waitFor(url, ms) {
  const deadline = Date.now() + ms;
  return (async () => {
    while (Date.now() < deadline) {
      try {
        const res = await fetch(url);
        if (res.ok) return;
      } catch {
        /* vite still starting */
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    throw new Error(`timed out waiting for ${url}`);
  })();
}

async function loadPlaywright() {
  const pwDir = join(os.tmpdir(), "gamelife-playwright-1.56.1");
  const pkg = join(pwDir, "node_modules", "playwright", "index.js");
  if (!existsSync(pkg)) {
    await mkdir(pwDir, { recursive: true });
    await writeFile(
      join(pwDir, "package.json"),
      JSON.stringify({ name: "gl-pw", private: true }),
    );
    const install = spawn("npm", ["install", "playwright@1.56.1"], {
      cwd: pwDir,
      stdio: "inherit",
    });
    await new Promise((resolve, reject) => {
      install.on("exit", (code) =>
        code === 0 ? resolve() : reject(new Error(`npm install playwright ${code}`)),
      );
    });
  }
  return createRequire(join(pwDir, "package.json"))("playwright");
}

const vite = spawn(
  "npx",
  ["vite", "--port", String(port), "--strictPort", "--host", "127.0.0.1"],
  {
    cwd: root,
    env: { ...process.env, VITE_PREVIEW: "1" },
    stdio: "inherit",
  },
);

try {
  await waitFor(`${origin}/`, 30_000);
  const { chromium } = await loadPlaywright();
  const browser = await chromium.launch({ channel: "chrome" }).catch(() =>
    chromium.launch(),
  );
  const page = await browser.newPage({
    viewport: { width: 1100, height: 720 },
    deviceScaleFactor: 2,
  });
  await page.addInitScript(() => {
    localStorage.setItem("gl-theme", "light");
    localStorage.setItem("gl-sidebar-width", "208");
    document.documentElement.classList.remove("dark");
  });

  await mkdir(outDir, { recursive: true });
  const tabs = [
    ["today", "today.png"],
    ["week", "stats.png"],
    ["shop", "shop.png"],
    ["settings", "settings.png"],
  ];
  async function settle() {
    await page.waitForSelector("h1", { timeout: 10_000 });
    await new Promise((r) => setTimeout(r, 800));
  }

  for (const [tab, file] of tabs) {
    await page.goto(`${origin}/?tab=${tab}`, {
      waitUntil: "networkidle",
    });
    await settle();
    const dest = join(outDir, file);
    await page.screenshot({ path: dest, type: "png" });
    console.log("wrote", dest);
  }

  async function captureSettingsTab(label, file, after) {
    await page.setViewportSize({ width: 1100, height: 720 });
    await page.goto(`${origin}/?tab=settings`, { waitUntil: "networkidle" });
    await settle();
    await page.getByRole("tab", { name: label, exact: true }).click();
    await new Promise((r) => setTimeout(r, 800));
    if (after) await after();
    const height = await page.evaluate(() => {
      const scroller = document.querySelector("main .overflow-y-auto");
      const header = document.querySelector("main header");
      const needed =
        (header?.getBoundingClientRect().height ?? 86) +
        (scroller?.scrollHeight ?? 720) +
        8;
      return Math.min(Math.max(Math.ceil(needed), 720), 2400);
    });
    await page.setViewportSize({ width: 1100, height });
    await new Promise((r) => setTimeout(r, 300));
    const dest = join(outDir, file);
    await page.screenshot({ path: dest, type: "png" });
    console.log("wrote", dest, "h=" + height);
  }

  await captureSettingsTab("TickTick", "settings-ticktick.png", async () => {
    const expand = page.getByLabel("展开分组");
    if ((await expand.count()) > 0) {
      await expand.first().click();
      await new Promise((r) => setTimeout(r, 400));
    }
  });
  await captureSettingsTab("云端", "settings-cloud.png");
  await browser.close();
} finally {
  vite.kill("SIGTERM");
}
