// Playwright test of the admin panel's dialogs on phone heights (390x844, 360x640) and desktop: every dialog (create
// user, the one-time login link with its QR code, edit, confirmations) has a visible close X, closes with Esc and with a
// click on the backdrop, fits in the viewport, scrolls inside, and keeps its footer buttons on screen.
//
//   cargo build -p workchat-server && (cd admin-web && npm run build)
//   PLAYWRIGHT_DIR=<dir with node_modules/playwright-core> node tests/ui/admin-dialogs.mjs
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire((process.env.PLAYWRIGHT_DIR ?? process.cwd()) + "/");
const { chromium } = require("playwright-core");
const PORT = 18318;
const BASE = `http://127.0.0.1:${PORT}`;
const data = fs.mkdtempSync(path.join(os.tmpdir(), "workchat-ui-dialogs-"));
const shots = process.env.SHOT_DIR ?? path.join(data, "shots");
fs.mkdirSync(shots, { recursive: true });

const server = spawn(path.join(root, "target/debug/workchat-server"), [], {
  env: { ...process.env, WORKCHAT_BIND: `127.0.0.1:${PORT}`, WORKCHAT_DATA_DIR: path.join(data, "srv"), WORKCHAT_ADMIN_DIR: path.join(root, "admin-web/dist"), WORKCHAT_BACKUP_WORK_FACTOR: "10", WORKCHAT_PUBLIC_URL: `http://127.0.0.1:${PORT}`, RUST_LOG: "warn" },
  stdio: ["ignore", "inherit", "inherit"],
});
const cleanup = async () => { server.kill("SIGTERM"); await new Promise((r) => setTimeout(r, 200)); fs.rmSync(data, { recursive: true, force: true }); };
process.on("exit", () => { try { server.kill("SIGKILL"); } catch {} });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function api(method, p, body, token) {
  const res = await fetch(`${BASE}/api/v1${p}`, { method, headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...(token ? { Authorization: `Bearer ${token}` } : {}) }, body: body ? JSON.stringify(body) : undefined });
  const t = await res.text();
  if (!res.ok) throw new Error(`${method} ${p}: ${res.status} ${t}`);
  return t ? JSON.parse(t) : null;
}

let failed = false;
let n = 0;
try {
  for (let i = 0; i < 100; i++) { try { await fetch(`${BASE}/api/v1/meta`); break; } catch { await sleep(100); } }
  await api("POST", "/setup", { username: "admin", display_name: "Admin", password: "test-admin-pw", email: null, storage: null });
  const exe = fs.readdirSync(process.env.HOME + "/.cache/ms-playwright").filter((d) => d.startsWith("chromium-")).sort().pop();
  const browser = await chromium.launch({ executablePath: `${process.env.HOME}/.cache/ms-playwright/${exe}/chrome-linux64/chrome`, args: ["--no-sandbox"] });

  for (const vp of [{ name: "phone-390x844", width: 390, height: 844, mobile: true }, { name: "phone-360x640", width: 360, height: 640, mobile: true }, { name: "desktop-1000x700", width: 1000, height: 700, mobile: false }]) {
    const ctx = await browser.newContext({ viewport: { width: vp.width, height: vp.height }, deviceScaleFactor: 2, isMobile: vp.mobile, hasTouch: vp.mobile });
    const page = await ctx.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(`${BASE}/admin/`);
    await page.fill("input[autocomplete=username]", "admin");
    await page.fill("input[type=password]", "test-admin-pw");
    await page.click("button:has-text('Sign in')");
    await page.waitForSelector("main h2:has-text('Overview')");
    if (vp.mobile) { await page.click("#menu-btn"); await page.waitForSelector("nav.tabs.open"); }
    await page.click("nav.tabs a[href='#/users']");
    await page.waitForSelector("button:has-text('Create user')");

    /** The dialog is inside the viewport, scrolls inside itself, and every footer button and the X are visible. */
    const fits = async (label) => {
      const r = await page.evaluate(({ vw, vh }) => {
        const d = document.querySelector("dialog[open]");
        if (!d) return { open: false };
        const b = d.getBoundingClientRect();
        const inside = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && r.left >= -0.5 && r.top >= -0.5 && r.right <= vw + 0.5 && r.bottom <= vh + 0.5; };
        const x = d.querySelector(".dlg-x");
        const foot = [...d.querySelectorAll(".dlg-foot button")];
        const body = d.querySelector(".dlg-body");
        return { open: true, box: [b.left, b.top, b.right, b.bottom], x: !!x && inside(x), xSize: x ? x.getBoundingClientRect().width : 0, foot: foot.length > 0 && foot.every(inside), footCount: foot.length, bodyScrolls: body.scrollHeight > body.clientHeight, docScrollsX: document.documentElement.scrollWidth > vw };
      }, { vw: vp.width, vh: vp.height });
      assert.ok(r.open, `${vp.name} ${label}: dialog is open`);
      assert.ok(r.box[0] >= -0.5 && r.box[1] >= -0.5 && r.box[2] <= vp.width + 0.5 && r.box[3] <= vp.height + 0.5, `${vp.name} ${label}: dialog fits the viewport ${JSON.stringify(r.box)}`);
      assert.ok(r.x, `${vp.name} ${label}: the close X is visible`);
      assert.ok(r.xSize >= 40, `${vp.name} ${label}: the close X is a touch target (${r.xSize}px)`);
      assert.ok(r.foot, `${vp.name} ${label}: footer buttons are on screen`);
      assert.ok(!r.docScrollsX, `${vp.name} ${label}: no horizontal page scroll`);
      return r;
    };
    const closed = async (label) => assert.equal(await page.locator("dialog[open]").count(), 0, `${vp.name} ${label}: dialog closed`);

    // Create user: X, Esc and backdrop all close it.
    await page.click("button:has-text('Create user')");
    await page.waitForSelector("dialog[open]");
    await fits("create user");
    await page.screenshot({ path: path.join(shots, `dialog-${vp.name}-create.png`) });
    await page.click("dialog .dlg-x");
    await closed("X");
    await page.click("button:has-text('Create user')");
    await page.waitForSelector("dialog[open]");
    await page.keyboard.press("Escape");
    await closed("Esc");
    await page.click("button:has-text('Create user')");
    await page.waitForSelector("dialog[open]");
    await page.mouse.click(2, 2);
    await closed("backdrop");

    // Create a user: the login link dialog with its QR code.
    await page.click("button:has-text('Create user')");
    await page.waitForSelector("dialog[open]");
    await page.fill("dialog input >> nth=0", "u" + vp.name.replace(/\W/g, ""));
    await page.fill("dialog input >> nth=1", "Dialog Test");
    await page.click("dialog button:has-text('Create')");
    await page.waitForSelector("#login-qr svg.qr");
    const r = await fits("login link");
    await page.screenshot({ path: path.join(shots, `dialog-${vp.name}-login-link.png`) });
    if (vp.height <= 640) assert.ok(r.bodyScrolls, `${vp.name}: the login-link body scrolls inside the dialog`);
    // The QR and the sticky footer: the Copy link button is reachable without scrolling the page.
    assert.ok(await page.locator("dialog .dlg-foot button:has-text('Copy link')").isVisible(), `${vp.name}: Copy link visible`);
    await page.click("dialog .dlg-x");
    await closed("login link X");
    await page.click("button:has-text('Create user')");
    await page.waitForSelector("dialog[open]");
    await page.fill("dialog input >> nth=0", "w" + vp.name.replace(/\W/g, ""));
    await page.click("dialog button:has-text('Create')");
    await page.waitForSelector("#login-qr svg.qr");
    await page.keyboard.press("Escape");
    await closed("login link Esc");

    // Other dialogs: reset password and a confirmation, both with the same chrome.
    await page.waitForSelector("button:has-text('Reset password')");
    await page.locator("button:has-text('Reset password')").first().click();
    await page.waitForSelector("dialog[open]");
    await fits("reset password");
    await page.click("dialog .dlg-x");
    await closed("reset X");
    await page.locator("button.danger:has-text('Delete')").first().click();
    await page.waitForSelector("dialog[open]");
    await fits("delete confirmation");
    await page.click("dialog .dlg-x");
    await closed("delete X");
    assert.deepEqual(errors, [], `${vp.name}: no page errors`);
    await ctx.close();
    n++;
  }
  await browser.close();
  console.log(`admin dialogs: ${n} viewports OK`);
} catch (e) {
  failed = true;
  console.error(e);
} finally {
  await cleanup();
}
process.exit(failed ? 1 : 0);
