// Playwright test of the admin panel's version display and Updates card, against a fake GitHub release server (the real server
// asks it, via WORKCHAT_UPDATE_URL): update available (notes, upgrade commands, app downloads), up to date, and GitHub unreachable /
// rate limited. Also checks the server version in the header and footer, and that the browser never talks to GitHub itself.
//
//   cargo build -p workchat-server && (cd admin-web && npm run build)
//   PLAYWRIGHT_DIR=<dir with node_modules/playwright-core> node tests/ui/admin-updates.mjs
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import assert from "node:assert/strict";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire((process.env.PLAYWRIGHT_DIR ?? process.cwd()) + "/");
const { chromium } = require("playwright-core");
const PORT = 18321, FAKE = 18322;
const BASE = `http://127.0.0.1:${PORT}`;
const VERSION = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8").match(/^version = "(.+)"$/m)[1];
const data = fs.mkdtempSync(path.join(os.tmpdir(), "workchat-ui-updates-"));
const shots = process.env.SHOT_DIR ?? path.join(data, "shots");
fs.mkdirSync(shots, { recursive: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- a fake GitHub: scenario is switched per test; it records what the server asked
let scenario = "new";
const seen = [];
const fake = http.createServer((req, res) => {
  seen.push({ url: req.url, ua: req.headers["user-agent"] });
  const send = (code, obj, headers = {}) => { res.writeHead(code, { "content-type": "application/json", ...headers }); res.end(JSON.stringify(obj)); };
  if (scenario === "limit") return send(403, { message: "API rate limit exceeded" }, { "x-ratelimit-remaining": "0" });
  if (scenario === "none") return send(404, { message: "Not Found" });
  const v = scenario === "same" ? VERSION : "9.9.9";
  const names = [`workchat-${v}-android.apk`, `WorkChat-${v}-x86_64.AppImage`, `workchat_${v}-1_amd64.deb`, `workchat-${v}-x64-setup.exe`, `workchat-${v}-x64-portable.exe`];
  send(200, {
    tag_name: `v${v}`, name: `WorkChat Server ${v}`, draft: false, prerelease: false, html_url: `https://github.com/uri-travoski/workchat-claude/releases/tag/v${v}`,
    published_at: "2026-11-20T01:02:03Z", body: "## What's new\n* Update checks in every app\n* An Updates card in the admin panel\n",
    assets: names.flatMap((n) => [{ name: n, size: 1000, browser_download_url: `https://github.com/uri-travoski/workchat-claude/releases/download/v${v}/${n}` }, { name: n + ".sha256", size: 90, browser_download_url: `https://github.com/uri-travoski/workchat-claude/releases/download/v${v}/${n}.sha256` }]),
  });
});
await new Promise((r) => fake.listen(FAKE, "127.0.0.1", r));

let server;
async function startServer(tag) {
  server = spawn(path.join(root, "target/debug/workchat-server"), [], {
    env: { ...process.env, WORKCHAT_BIND: `127.0.0.1:${PORT}`, WORKCHAT_DATA_DIR: path.join(data, "srv-" + tag), WORKCHAT_ADMIN_DIR: path.join(root, "admin-web/dist"), WORKCHAT_BACKUP_WORK_FACTOR: "10", WORKCHAT_PUBLIC_URL: BASE, WORKCHAT_UPDATE_URL: `http://127.0.0.1:${FAKE}/releases/latest`, RUST_LOG: "warn" },
    stdio: ["ignore", "inherit", "inherit"],
  });
  for (let i = 0; i < 100; i++) { try { await fetch(`${BASE}/api/v1/meta`); break; } catch { await sleep(100); } }
}
async function stopServer() { server.kill("SIGTERM"); await sleep(300); }
process.on("exit", () => { try { server.kill("SIGKILL"); } catch {} });
async function api(method, p, body, token) {
  const res = await fetch(`${BASE}/api/v1${p}`, { method, headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...(token ? { Authorization: `Bearer ${token}` } : {}) }, body: body ? JSON.stringify(body) : undefined });
  const t = await res.text();
  if (!res.ok) throw new Error(`${method} ${p}: ${res.status} ${t}`);
  return t ? JSON.parse(t) : null;
}

let failed = false;
try {
  const exe = fs.readdirSync(process.env.HOME + "/.cache/ms-playwright").filter((d) => d.startsWith("chromium-")).sort().pop();
  const browser = await chromium.launch({ executablePath: `${process.env.HOME}/.cache/ms-playwright/${exe}/chrome-linux64/chrome`, args: ["--no-sandbox"] });

  async function overview(tag, vp = { width: 1100, height: 900 }) {
    scenario = tag;
    seen.length = 0;
    await startServer(tag);
    await api("POST", "/setup", { username: "admin", display_name: "Admin", password: "test-admin-pw", email: null, storage: null });
    const ctx = await browser.newContext({ viewport: vp, deviceScaleFactor: 1 });
    const page = await ctx.newPage();
    const external = [];
    page.on("request", (r) => { if (!r.url().startsWith(BASE)) external.push(r.url()); });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(`${BASE}/admin/`);
    await page.fill("input[autocomplete=username]", "admin");
    await page.fill("input[type=password]", "test-admin-pw");
    await page.click("button:has-text('Sign in')");
    await page.waitForSelector("main h2:has-text('Overview')");
    await page.waitForSelector("#updates-card #upd-status");
    return { page, ctx, external, errors };
  }

  // 1. A newer release exists.
  {
    const { page, ctx, external, errors } = await overview("new");
    // Version in the header and the footer.
    assert.match(await page.textContent("header.top"), new RegExp(`v${VERSION.replaceAll(".", "\\.")}`));
    assert.match(await page.textContent("#foot"), new RegExp(`server v${VERSION.replaceAll(".", "\\.")}`));
    assert.equal((await page.textContent("#upd-status")).trim(), "Update available");
    assert.equal((await page.textContent("#upd-current")).trim(), `v${VERSION}`);
    assert.equal((await page.textContent("#upd-latest")).trim(), "v9.9.9");
    assert.match(await page.textContent("#upd-notes"), /Updates card in the admin panel/);
    const cmds = await page.$$eval("#updates-card pre.cmd", (n) => n.map((x) => x.textContent));
    assert.deepEqual(cmds, ["docker compose pull && docker compose up -d"]);
    assert.match(await page.textContent("#updates-card"), /WORKCHAT_IMAGE=ghcr\.io\/uri-travoski\/workchat-server:9\.9\.9/);
    const links = await page.$$eval("#upd-downloads a", (n) => n.map((x) => [x.textContent, x.href]));
    assert.deepEqual(links.map((l) => l[0]), ["Android (APK)", "Linux (AppImage)", "Linux (.deb)", "Windows (installer)", "Windows (portable)"]);
    assert.ok(links.every((l) => l[1].startsWith("https://github.com/uri-travoski/workchat-claude/releases/download/v9.9.9/")));
    assert.equal(await page.getAttribute("#upd-release-link", "href"), "https://github.com/uri-travoski/workchat-claude/releases/tag/v9.9.9");
    // The server (not the browser) asked, and said who it is.
    assert.equal(seen.length, 1);
    assert.equal(seen[0].ua, `WorkChat-Server/${VERSION}`);
    assert.deepEqual(external, [], "the admin page must not call anything but its own server");
    // Cached for an hour: a reload does not ask again.
    await page.reload();
    await page.waitForSelector("#updates-card #upd-status");
    assert.equal(seen.length, 1, "a second view must come from the cache");
    // Copy button.
    await ctx.grantPermissions(["clipboard-read", "clipboard-write"], { origin: BASE }).catch(() => {});
    await page.click("#updates-card button.copy-cmd");
    await page.screenshot({ path: path.join(shots, "admin-updates-available.png"), fullPage: true });
    assert.deepEqual(errors, []);
    await ctx.close();
    await stopServer();
  }

  // 2. Up to date.
  {
    const { page, ctx, errors } = await overview("same");
    assert.equal((await page.textContent("#upd-status")).trim(), "Up to date");
    assert.equal((await page.$("#upd-notes")), null, "no release notes when nothing is new");
    assert.ok(await page.$("#updates-card pre.cmd"), "the update commands stay available");
    await page.screenshot({ path: path.join(shots, "admin-updates-current.png"), fullPage: true });
    assert.deepEqual(errors, []);
    await ctx.close();
    await stopServer();
  }

  // 3. GitHub rate limits the server; then (on a phone) nothing is published.
  {
    const { page, ctx, errors } = await overview("limit");
    assert.equal((await page.textContent("#upd-status")).trim(), "Could not check");
    assert.match(await page.textContent("#upd-error"), /limiting update checks/);
    assert.ok(await page.$("#updates-card pre.cmd"));
    await ctx.close();
    await stopServer();
    const m = await overview("none", { width: 390, height: 844 });
    assert.match(await m.page.textContent("#upd-error"), /No release has been published yet/);
    assert.ok(await m.page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1), "no horizontal scroll on a phone");
    await m.page.screenshot({ path: path.join(shots, "admin-updates-phone.png"), fullPage: true });
    assert.deepEqual(errors.concat(m.errors), []);
    await m.ctx.close();
    await stopServer();
  }

  // 4. Only admins may ask.
  {
    scenario = "new";
    await startServer("auth");
    await api("POST", "/setup", { username: "admin", display_name: "Admin", password: "test-admin-pw", email: null, storage: null });
    const res = await fetch(`${BASE}/api/v1/admin/updates`);
    assert.equal(res.status, 401);
    await stopServer();
  }
  await browser.close();
  console.log("admin-updates OK");
} catch (e) {
  failed = true;
  console.error(e);
} finally {
  fake.close();
  try { server.kill("SIGTERM"); } catch {}
  await sleep(200);
  if (!failed && !process.env.SHOT_DIR) fs.rmSync(data, { recursive: true, force: true });
  process.exit(failed ? 1 : 0);
}
