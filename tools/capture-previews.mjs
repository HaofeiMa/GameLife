#!/usr/bin/env node
/**
 * Render rail tabs and settings sub-pages with VITE_PREVIEW=1
 * and write PNGs for the README.
 */
import { spawn } from "node:child_process";
import { mkdir, unlink, writeFile } from "node:fs/promises";
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
    localStorage.setItem("gl-accent", "default");
    localStorage.setItem("gl-sidebar-width", "208");
    localStorage.setItem("gl-task-cal-days", "3");
    document.documentElement.classList.remove("dark");
  });

  await mkdir(outDir, { recursive: true });

  async function settle(ms = 800) {
    await page.waitForSelector("h1", { timeout: 10_000 });
    await new Promise((r) => setTimeout(r, ms));
  }

  async function shot(file) {
    const dest = join(outDir, file);
    await page.screenshot({ path: dest, type: "png" });
    console.log("wrote", dest);
  }

  async function pngOf() {
    return page.screenshot({ type: "png" });
  }

  async function gotoTab(tab) {
    await page.setViewportSize({ width: 1100, height: 720 });
    await page.goto(`${origin}/?tab=${tab}`, { waitUntil: "networkidle" });
    await settle(tab === "tasks" ? 1200 : 800);
  }

  async function composeStrip(thumbs, dest, figureWidth, columns = thumbs.length) {
    const gap = 14;
    const padX = 16;
    const padTop = 18;
    const cols = Math.min(columns, thumbs.length);
    const width =
      padX * 2 + cols * figureWidth + (cols - 1) * gap;
    const board = await browser.newPage({
      viewport: { width, height: 420 },
      deviceScaleFactor: 2,
    });
    const figures = thumbs
      .map(
        (t) => `<figure>
          <img src="data:image/png;base64,${t.png.toString("base64")}" alt="${t.label}">
          <figcaption>${t.label}</figcaption>
        </figure>`,
      )
      .join("");
    await board.setContent(`<!doctype html>
<html><head><meta charset="utf-8">
<style>
  html, body { margin: 0; background: #e7e5e4; }
  .row {
    display: grid;
    grid-template-columns: repeat(${cols}, ${figureWidth}px);
    gap: ${gap}px;
    padding: ${padTop}px ${padX}px 14px;
    justify-content: start;
  }
  figure { margin: 0; width: ${figureWidth}px; }
  img {
    width: ${figureWidth}px;
    height: auto;
    display: block;
    border-radius: 10px;
    box-shadow: 0 8px 24px -12px rgba(60, 40, 20, 0.45);
  }
  figcaption {
    margin-top: 8px;
    text-align: center;
    font: 600 13px/1.3 -apple-system, BlinkMacSystemFont, "PingFang SC", sans-serif;
    color: #44403c;
  }
</style></head>
<body><div class="row">${figures}</div></body></html>`);
    await new Promise((r) => setTimeout(r, 400));
    const stripH = await board.evaluate(() => document.body.scrollHeight);
    await board.setViewportSize({ width, height: Math.ceil(stripH) });
    await board.screenshot({ path: dest, type: "png" });
    console.log("wrote", dest);
    await board.close();
  }

  for (const [tab, file] of [
    ["today", "today.png"],
    ["tasks", "tasks.png"],
    ["shop", "shop.png"],
  ]) {
    await gotoTab(tab);
    await shot(file);
  }

  await gotoTab("week");
  const statsThumbs = [{ label: "周", png: await pngOf() }];
  for (const label of ["月", "习惯"]) {
    await page.getByRole("tab", { name: label, exact: true }).click();
    await new Promise((r) => setTimeout(r, 800));
    statsThumbs.push({ label, png: await pngOf() });
  }
  await composeStrip(statsThumbs, join(outDir, "stats.png"), 420);

  async function captureSettingsTab(label) {
    await page.setViewportSize({ width: 1100, height: 720 });
    await page.goto(`${origin}/?tab=settings`, { waitUntil: "networkidle" });
    await settle();
    await page.getByRole("tab", { name: label, exact: true }).click();
    await new Promise((r) => setTimeout(r, 800));
    return pngOf();
  }

  const settingsThumbs = [];
  for (const label of ["基础", "名单", "权限", "关于"]) {
    settingsThumbs.push({ label, png: await captureSettingsTab(label) });
  }
  await composeStrip(settingsThumbs, join(outDir, "settings.png"), 360);

  await page.setViewportSize({ width: 1100, height: 720 });
  await page.goto(`${origin}/?tab=settings`, { waitUntil: "networkidle" });
  await settle();
  await page.getByRole("tab", { name: "API", exact: true }).click();
  await new Promise((r) => setTimeout(r, 800));
  await shot("settings-api.png");
  await page.getByRole("tab", { name: "云端", exact: true }).click();
  await new Promise((r) => setTimeout(r, 800));
  await shot("settings-cloud.png");

  const appearance = [
    { swatch: "默认", accent: "default", dark: false, label: "默认 · 浅色" },
    { swatch: "默认", accent: "default", dark: true, label: "默认 · 深色" },
    { swatch: "暖黄", accent: "nuanhuang", dark: false, label: "暖黄" },
    { swatch: "晴蓝", accent: "qinglan", dark: false, label: "晴蓝" },
    { swatch: "松石", accent: "songshi", dark: false, label: "松石" },
    { swatch: "桃天", accent: "taotian", dark: false, label: "桃天" },
    { swatch: "暮山紫", accent: "mushanzi", dark: false, label: "暮山紫" },
    { swatch: "藏蓝", accent: "zanglan", dark: false, label: "藏蓝" },
  ];

  async function waitTheme(accent, dark) {
    await page.waitForFunction(
      ({ accent: nextAccent, dark: nextDark }) => {
        const root = document.documentElement;
        const accentOk =
          (root.getAttribute("data-accent") || "default") === nextAccent;
        const darkOk = root.classList.contains("dark") === nextDark;
        return accentOk && darkOk;
      },
      { accent, dark },
      { timeout: 8_000 },
    );
  }

  const thumbs = [];
  for (const theme of appearance) {
    await page.setViewportSize({ width: 1100, height: 720 });
    await page.goto(`${origin}/?tab=settings`, { waitUntil: "networkidle" });
    await settle();
    await page.getByRole("tab", { name: "基础", exact: true }).click();
    await page.getByRole("button", { name: theme.swatch, exact: true }).click();
    await new Promise((r) => setTimeout(r, 500));
    await page
      .getByRole("tab", { name: theme.dark ? "深色" : "浅色", exact: true })
      .click();
    await waitTheme(theme.accent, theme.dark);
    await page.getByRole("button", { name: "今日", exact: true }).click();
    await settle();
    await waitTheme(theme.accent, theme.dark);
    thumbs.push({
      label: theme.label,
      png: await page.screenshot({ type: "png" }),
    });
  }
  await composeStrip(thumbs, join(outDir, "appearance.png"), 420, 4);

  for (const stale of [
    "stats-month.png",
    "stats-rhythm.png",
    "stats-app.png",
    "settings-lists.png",
    "settings-permissions.png",
    "settings-about.png",
  ]) {
    const path = join(outDir, stale);
    try {
      await unlink(path);
      console.log("removed", path);
    } catch {
      /* already gone */
    }
  }

  await browser.close();
} finally {
  vite.kill("SIGTERM");
}
