// Playwright test of the admin panel on a phone (390x844): no horizontal scroll on any page, touch-sized controls,
// hamburger navigation, single-pane chat viewer with a back button, and the key actions (create user with login link
// and QR code, disable/enable, clear history, conversations, backups, storage, bots, AI).
// The QR code on screen is decoded with ZXing (Java) when the decoder jars can be fetched into target/qr-tools.
//
//   cargo build -p workchat-server && (cd admin-web && npm run build)
//   PLAYWRIGHT_DIR=<dir with node_modules/playwright-core> [SHOT_DIR=docs/screenshots] node tests/ui/admin-mobile.mjs
import { spawn, spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire((process.env.PLAYWRIGHT_DIR ?? process.cwd()) + "/");
const { chromium } = require("playwright-core");
const PORT = 18313;
const BASE = `http://127.0.0.1:${PORT}`;
const W = 390;
const data = fs.mkdtempSync(path.join(os.tmpdir(), "workchat-ui-mobile-"));
const shots = process.env.SHOT_DIR ?? path.join(data, "shots");
fs.mkdirSync(shots, { recursive: true });

const server = spawn(path.join(root, "target/debug/workchat-server"), [], {
  env: { ...process.env, WORKCHAT_BIND: `127.0.0.1:${PORT}`, WORKCHAT_DATA_DIR: path.join(data, "srv"), WORKCHAT_ADMIN_DIR: path.join(root, "admin-web/dist"), WORKCHAT_BACKUP_WORK_FACTOR: "10", WORKCHAT_PUBLIC_URL: `http://127.0.0.1:${PORT}`, RUST_LOG: "warn" },
  stdio: ["ignore", "inherit", "inherit"],
});
const cleanup = async () => { server.kill("SIGTERM"); await new Promise((r) => setTimeout(r, 200)); fs.rmSync(data, { recursive: true, force: true }); };
process.on("exit", () => { try { server.kill("SIGKILL"); } catch {} });

async function api(method, p, body, token) {
  const res = await fetch(`${BASE}/api/v1${p}`, { method, headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...(token ? { Authorization: `Bearer ${token}` } : {}) }, body: body ? JSON.stringify(body) : undefined });
  const t = await res.text();
  if (!res.ok) throw new Error(`${method} ${p}: ${res.status} ${t}`);
  return t ? JSON.parse(t) : null;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const uid = () => crypto.randomUUID().replaceAll("-", "");

// ZXing decoder for the QR check (jars from Maven Central, cached in target/qr-tools).
async function decoder() {
  const dir = path.join(root, "target/qr-tools");
  fs.mkdirSync(dir, { recursive: true });
  const jars = { "core.jar": "https://repo1.maven.org/maven2/com/google/zxing/core/3.5.3/core-3.5.3.jar", "javase.jar": "https://repo1.maven.org/maven2/com/google/zxing/javase/3.5.3/javase-3.5.3.jar" };
  try {
    for (const [f, url] of Object.entries(jars)) if (!fs.existsSync(path.join(dir, f))) fs.writeFileSync(path.join(dir, f), Buffer.from(await (await fetch(url)).arrayBuffer()));
  } catch (e) { console.warn("QR decoder jars unavailable, skipping the decode check:", e.message); return null; }
  return (png) => {
    const r = spawnSync("java", ["-cp", `${dir}/core.jar:${dir}/javase.jar`, path.join(root, "tests/ui/qr/DecodeQr.java"), png], { encoding: "utf8" });
    if (r.status !== 0) throw new Error("QR decode failed: " + r.stderr);
    return r.stdout;
  };
}

let failed = false;
try {
  for (let i = 0; i < 100; i++) { try { await fetch(`${BASE}/api/v1/meta`); break; } catch { await sleep(100); } }
  const setup = await api("POST", "/setup", { username: "admin", display_name: "Admin", password: "test-admin-pw", email: null, storage: null });
  const adm = setup.access_token;
  const mk = async (u) => (await api("POST", "/admin/users", { username: u, display_name: u[0].toUpperCase() + u.slice(1), password: "password-123", role: "member" }, adm)).user.id;
  const aliceId = await mk("alice"), bobId = await mk("bob");
  const general = (await api("POST", "/conversations", { kind: "channel", name: "general", topic: "Everything", members: [aliceId, bobId] }, adm)).id;
  const tok = async (u) => (await api("POST", "/login", { username: u, password: "password-123", device_name: "t", client_version: "1.4.1" })).access_token;
  const alice = await tok("alice"), bob = await tok("bob");
  const send = (t, conv, body, extra = {}) => api("POST", `/conversations/${conv}/messages`, { client_id: uid(), body, mentions: [], attachment_ids: [], ...extra }, t);
  await send(alice, general, "Morning team, budget apples ready");
  const second = await send(bob, general, "thanks Alice, see https://example.com/a/very/long/link/that/should/wrap/instead/of/scrolling/the/page/sideways");
  await send(alice, general, "thread reply here", { thread_root: second.id });
  await api("POST", "/admin/users/" + bobId + "/backup", {}, adm);

  const exe = fs.readdirSync(process.env.HOME + "/.cache/ms-playwright").filter((d) => d.startsWith("chromium-")).sort().pop();
  const browser = await chromium.launch({ executablePath: `${process.env.HOME}/.cache/ms-playwright/${exe}/chrome-linux64/chrome`, args: ["--no-sandbox"] });
  const ctx = await browser.newContext({ viewport: { width: W, height: 844 }, deviceScaleFactor: 2, isMobile: true, hasTouch: true });
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  const shot = (n) => page.screenshot({ path: path.join(shots, "mobile-" + n + ".png") });

  /** No horizontal page scroll and no control smaller than a fingertip. */
  async function layoutOk(label, scope = "body") {
    const r = await page.evaluate(({ W, scope }) => {
      const de = document.documentElement;
      const small = [];
      for (const el of document.querySelectorAll(`${scope} button, ${scope} select, ${scope} input:not([type=checkbox]):not([type=radio]):not([type=hidden]), nav.tabs a`)) {
        const b = el.getBoundingClientRect();
        const cs = getComputedStyle(el);
        if (b.width === 0 || b.height === 0 || cs.visibility === "hidden" || cs.display === "none") continue;
        if (el.closest("[hidden]")) continue;
        if (b.height < 43.5) small.push(`${el.tagName.toLowerCase()} "${(el.textContent || el.getAttribute("aria-label") || el.type || "").trim().slice(0, 24)}" ${Math.round(b.height)}px`);
      }
      const wide = [];
      for (const el of document.querySelectorAll("body *")) { const b = el.getBoundingClientRect(); if (b.right > W + 1 && b.width > 0 && getComputedStyle(el).position !== "fixed") { wide.push(el.tagName.toLowerCase() + "." + el.className); if (wide.length > 5) break; } }
      return { sw: de.scrollWidth, bw: document.body.scrollWidth, small, wide };
    }, { W, scope });
    assert.ok(r.sw <= W && r.bw <= W, `${label}: horizontal overflow (page ${r.sw}, body ${r.bw}); wide: ${r.wide.join(", ")}`);
    assert.deepEqual(r.small, [], `${label}: touch targets under 44px`);
  }
  const openTab = async (name) => {
    await page.click("#menu-btn");
    await page.waitForSelector("nav.tabs.open");
    await layoutOk("menu open");
    await page.click(`nav.tabs a[href='#/${name}']`);
    await page.waitForSelector("nav.tabs:not(.open)", { state: "attached" });
    await page.waitForLoadState("networkidle");
  };

  // ---- sign in on the phone
  await page.goto(`${BASE}/admin/`);
  await page.fill("input[autocomplete=username]", "admin");
  await page.fill("input[type=password]", "test-admin-pw");
  await layoutOk("login");
  await shot("0-login");
  await page.click("button:has-text('Sign in')");
  await page.waitForSelector("main h2:has-text('Overview')");
  assert.equal(await page.locator("nav.tabs").isVisible(), false, "navigation is collapsed behind the menu button");
  await layoutOk("overview");
  await shot("1-overview");
  await page.click("#menu-btn");
  await shot("2-menu");
  await page.click("#menu-btn");

  // ---- users: table is a list of cards; create user with login link and QR
  await openTab("users");
  await page.waitForSelector("td[data-label='Name']");
  await layoutOk("users");
  await shot("3-users");
  await page.click("button:has-text('Create user')");
  await page.waitForSelector("dialog");
  await layoutOk("create user dialog", "dialog");
  await page.fill("dialog input >> nth=0", "carol");
  await page.fill("dialog input >> nth=1", "Carol");
  await page.click("dialog button:has-text('Create')");
  await page.waitForSelector("#login-qr svg.qr");
  const link = (await page.locator("#login-link").innerText()).trim();
  assert.match(link, new RegExp(`^${BASE.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/l/[A-Za-z0-9_-]{16,}$`), "the login link is a clickable web URL on the server");
  assert.equal(await page.locator("#login-link").getAttribute("href"), link, "the link is clickable");
  await layoutOk("login link dialog", "dialog");
  await shot("4-login-link-qr");
  const dec = await decoder();
  if (dec) {
    const qrPng = path.join(data, "qr.png");
    await page.locator("#login-qr svg.qr").screenshot({ path: qrPng });
    assert.equal(dec(qrPng), link, "the QR code decodes to exactly the one-time login link");
  }
  // The landing page behind the link (opened as a phone would): shows the app button, does not use the token up.
  const lp = await page.context().newPage();
  await lp.setViewportSize({ width: W, height: 844 });
  await lp.goto(link, { waitUntil: "domcontentloaded" });
  await lp.waitForSelector("#open");
  assert.equal((await lp.locator("#open").innerText()).trim(), "Open in WorkChat");
  assert.match(await lp.locator("#open").getAttribute("data-app"), /^workchat:\/\/login\?server=.*&token=/);
  assert.match(await lp.locator("#open").getAttribute("data-intent"), /^intent:\/\/login\?server=.*;scheme=workchat;package=app\.workchat;S\.browser_fallback_url=.*;end$/);
  assert.ok(await lp.locator("#copy").isVisible() && await lp.locator("#dl").isVisible(), "copy button and download section are shown");
  const lw = await lp.evaluate(() => [document.documentElement.scrollWidth, window.innerWidth]);
  assert.ok(lw[0] <= lw[1], `landing page has no horizontal scroll (${lw})`);
  const bh = await lp.locator("#open").boundingBox();
  assert.ok(bh.height >= 44 && bh.width >= 300, "the open button is large");
  await lp.screenshot({ path: path.join(shots, "mobile-4b-landing-page.png") });
  assert.equal((await fetch(link)).status, 200, "viewing the landing page did not consume the one-time token");
  await lp.close();
  await page.click("dialog button:has-text('Cancel')");
  await page.waitForSelector("tr:has-text('Carol')");

  // disable / enable
  const carol = page.locator("tr", { hasText: "carol" });
  await carol.locator("button:has-text('Disable')").click();
  await page.waitForSelector("tr:has-text('carol') .pill:has-text('disabled')");
  await carol.locator("button:has-text('Enable')").click();
  await page.waitForSelector("tr:has-text('carol') button:has-text('Disable')");
  // a fresh link for an existing user shows the QR too
  await carol.locator("button:has-text('Login link')").click();
  await page.waitForSelector("#login-qr svg.qr");
  await page.click("dialog button:has-text('Cancel')");
  // clear history (typed confirmation)
  await page.locator("tr", { hasText: "bob" }).locator("button:has-text('Clear history')").click();
  await page.waitForSelector("dialog");
  await layoutOk("confirm dialog", "dialog");
  await page.fill("dialog input", "bob");
  await page.click("dialog button:has-text('Clear history')");
  await page.waitForSelector(".toast:has-text('Cleared')");
  await page.waitForSelector("dialog", { state: "detached" });

  // ---- conversations
  await openTab("conversations");
  await page.waitForSelector("td[data-label='Name']:has-text('general')");
  await layoutOk("conversations");
  await shot("5-conversations");
  await page.click("button:has-text('Create')");
  await layoutOk("create conversation dialog", "dialog");
  await page.fill("dialog input >> nth=0", "mobile-room");
  await page.click("dialog button:has-text('Create')");
  await page.waitForSelector("td:has-text('mobile-room')");
  await page.locator("tr", { hasText: "mobile-room" }).locator("button:has-text('Archive')").click();
  await page.waitForSelector("tr:has-text('mobile-room') .pill:has-text('archived')");

  // ---- backups (the clear-history backup is listed)
  await openTab("backups");
  await page.waitForSelector("#backup-table");
  await layoutOk("backups");
  await shot("6-backups");
  await page.locator("#backup-table button.restore").first().click();
  await layoutOk("restore dialog", "dialog");
  await page.click("dialog button:has-text('Cancel')");

  // ---- storage
  await openTab("storage");
  await page.waitForSelector("button:has-text('Test')");
  await layoutOk("storage");
  await page.click("button:has-text('Test')");
  await page.waitForSelector(".pill:has-text('OK')");
  await shot("7-storage");

  // ---- bots
  await openTab("bots");
  await page.waitForSelector("#new-bot");
  await layoutOk("bots");
  await page.click("#new-bot");
  await layoutOk("create bot dialog", "dialog");
  await page.click("dialog button:has-text('Cancel')");
  await shot("8-bots");

  // ---- AI
  await openTab("ai");
  await page.waitForSelector("#ai-add");
  await layoutOk("ai");
  await page.click("#ai-add");
  await page.fill("#ai-name", "Claude");
  await page.fill("#ai-key", "sk-test-fake-0000000");
  await layoutOk("ai dialog", "dialog");
  await page.click("dialog button:has-text('Save')");
  await page.waitForSelector(".ai-provider[data-provider='Claude']");
  await layoutOk("ai with a provider");
  await shot("9-ai");

  // ---- webhooks, groups, bookmarks, sessions: layout only
  for (const t of ["webhooks", "groups", "bookmarks", "sessions"]) {
    await openTab(t);
    await page.waitForSelector("main .card");
    await layoutOk(t);
  }
  await shot("10-sessions");

  // ---- chat viewer: list -> conversation -> back; thread opens full screen
  await openTab("chats");
  await page.waitForSelector(".viewer .conv");
  assert.equal(await page.locator(".viewer .main").isVisible(), false, "list first, single pane");
  await layoutOk("viewer list");
  await shot("11-viewer-list");
  await page.locator(".conv", { hasText: "general" }).first().click();
  await page.waitForSelector(".bubble:has-text('Morning team')");
  assert.equal(await page.locator(".viewer .side").isVisible(), false, "the list is hidden while reading");
  await layoutOk("viewer conversation");
  await shot("12-viewer-chat");
  await page.click("button.linkish");
  await page.waitForSelector(".thread .bubble:has-text('thread reply here')");
  assert.equal((await page.locator(".thread").boundingBox()).width, W, "the thread fills the screen");
  await layoutOk("viewer thread");
  await page.click(".thread button:has-text('Close')");
  await page.click("#viewer-back");
  await page.waitForSelector(".viewer .conv");
  assert.equal(await page.locator(".viewer .main").isVisible(), false);

  assert.deepEqual(errors.filter((e) => !/favicon|401/.test(e)), [], "no console errors");
  await browser.close();
  console.log("admin-mobile UI test: OK");
} catch (e) {
  failed = true;
  console.error("FAILED:", e);
} finally {
  await cleanup();
}
process.exit(failed ? 1 : 0);
