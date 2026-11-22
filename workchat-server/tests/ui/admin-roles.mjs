// Playwright test of role management and the Privacy options dropdown in the admin panel, on desktop (1200x800)
// and on a phone (390x844). The owner sees a role menu per other user; admins see plain text; the own row has no menu.
//
//   cargo build -p workchat-server && (cd admin-web && npm run build)
//   PLAYWRIGHT_DIR=<dir with node_modules/playwright-core> node tests/ui/admin-roles.mjs
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
const PORT = 18314;
const BASE = `http://127.0.0.1:${PORT}`;
const data = fs.mkdtempSync(path.join(os.tmpdir(), "workchat-ui-roles-"));
const server = spawn(path.join(root, "target/debug/workchat-server"), [], {
  env: { ...process.env, WORKCHAT_BIND: `127.0.0.1:${PORT}`, WORKCHAT_DATA_DIR: path.join(data, "srv"), WORKCHAT_ADMIN_DIR: path.join(root, "admin-web/dist"), WORKCHAT_BACKUP_WORK_FACTOR: "10", RUST_LOG: "warn" },
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
try {
  for (let i = 0; i < 100; i++) { try { await fetch(`${BASE}/api/v1/meta`); break; } catch { await sleep(100); } }
  const setup = await api("POST", "/setup", { username: "admin", display_name: "Admin", password: "test-admin-pw", email: null, storage: null });
  const adm = setup.access_token;
  const mk = async (u, role) => (await api("POST", "/admin/users", { username: u, display_name: u[0].toUpperCase() + u.slice(1), password: "password-123", role, create_login_link: false }, adm)).user.id;
  const ids = { alice: await mk("alice", "admin"), bob: await mk("bob", "member"), carol: await mk("carol", "member") };
  const roleOf = async (n) => (await api("GET", "/admin/users", null, adm)).find((u) => u.username === n).role;
  const privOf = async (n) => (await api("GET", "/admin/users", null, adm)).find((u) => u.username === n).privacy_controls_enabled;

  const exe = fs.readdirSync(process.env.HOME + "/.cache/ms-playwright").filter((d) => d.startsWith("chromium-")).sort().pop();
  const browser = await chromium.launch({ executablePath: `${process.env.HOME}/.cache/ms-playwright/${exe}/chrome-linux64/chrome`, args: ["--no-sandbox"] });

  async function signIn(ctx, user, pw) {
    const page = await ctx.newPage();
    page.errors = [];
    page.on("pageerror", (e) => page.errors.push(e.message));
    page.on("console", (m) => { if (m.type() === "error" && !/favicon|40[139]|forbidden|validation/i.test(m.text())) page.errors.push(m.text()); });
    await page.goto(`${BASE}/admin/`);
    await page.fill("input[autocomplete=username]", user);
    await page.fill("input[type=password]", pw);
    await page.click("button:has-text('Sign in')");
    await page.waitForSelector("header.top");
    if (await page.locator("#menu-btn").isVisible()) await page.click("#menu-btn");
    await page.click("a[href='#/users']");
    await page.waitForSelector("tbody tr");
    return page;
  }
  const overflow = (page, W) => page.evaluate((W) => document.documentElement.scrollWidth <= W && document.body.scrollWidth <= W, W);

  for (const vp of [{ name: "desktop", width: 1200, height: 800, mobile: false }, { name: "mobile", width: 390, height: 844, mobile: true }]) {
    const ctx = await browser.newContext({ viewport: { width: vp.width, height: vp.height }, deviceScaleFactor: vp.mobile ? 2 : 1, isMobile: vp.mobile, hasTouch: vp.mobile });
    const page = await signIn(ctx, "admin", "test-admin-pw");
    const sel = (u) => page.locator(`select.role-select[data-user=${u}]`);
    const row = (u) => page.locator("tbody tr", { has: page.locator(`select.privacy-select[data-user=${u}]`) });

    // Owner: a menu for every other user, none on the own row.
    assert.equal(await page.locator("select.role-select").count(), 3, vp.name + ": menus for alice, bob, carol only");
    assert.equal(await page.locator("tbody tr", { hasText: "Administrator" }).locator("select.role-select").count(), 0, "no menu on own row");
    assert.equal(await sel("alice").inputValue(), "admin");
    if (vp.mobile) {
      assert.ok(await overflow(page, 390), "no horizontal scroll");
      for (const s of ["select.role-select", "select.privacy-select"]) assert.ok((await page.locator(s).first().boundingBox()).height >= 43.5, s + " is touch sized");
    }

    // Promote bob to admin: confirm dialog, Cancel keeps the old role, confirm applies it.
    await sel("bob").selectOption("admin");
    await page.waitForSelector("#role-dialog");
    assert.match(await page.locator("#role-dialog h2").innerText(), /Change role to Admin/);
    await page.click("#role-dialog >> text=Cancel");
    await page.waitForSelector("#role-dialog", { state: "detached" });
    await page.waitForFunction(() => document.querySelector("select.role-select[data-user=bob]")?.value === "member");
    assert.equal(await roleOf("bob"), "member");
    await sel("bob").selectOption("admin");
    await page.click("#role-dialog button:has-text('Make admin')");
    await page.waitForSelector("#role-dialog", { state: "detached" });
    await page.waitForFunction(() => document.querySelector("select.role-select[data-user=bob]")?.value === "admin");
    assert.equal(await roleOf("bob"), "admin");

    // Demote back to member (confirm), then promote carol to owner and take it back via the API for the next round.
    await sel("bob").selectOption("member");
    await page.waitForSelector("#role-dialog");
    assert.match(await page.locator("#role-dialog").innerText(), /lose admin rights/);
    await page.click("#role-dialog button:has-text('Make member')");
    await page.waitForSelector("#role-dialog", { state: "detached" });
    await page.waitForFunction(() => document.querySelector("select.role-select[data-user=bob]")?.value === "member");
    await sel("carol").selectOption("owner");
    await page.waitForSelector("#role-dialog");
    assert.match(await page.locator("#role-dialog").innerText(), /owner/i);
    await page.click("#role-dialog button:has-text('Make owner')");
    await page.waitForSelector("#role-dialog", { state: "detached" });
    await page.waitForFunction(() => document.querySelector("select.role-select[data-user=carol]")?.value === "owner");
    assert.equal(await roleOf("carol"), "owner");
    // Carol goes back to member for the next round.
    await api("PUT", `/admin/users/${ids.carol}/role`, { role: "member" }, adm);

    // Privacy options dropdown: Allow / Disallow, saved at once.
    const ps = page.locator("select.privacy-select[data-user=bob]");
    assert.deepEqual(await ps.locator("option").allInnerTexts(), ["Allow", "Disallow"]);
    assert.equal(await ps.inputValue(), "disallow");
    await ps.selectOption("allow");
    await page.waitForSelector("text=Saved");
    assert.equal(await privOf("bob"), true);
    await ps.selectOption("disallow");
    await sleep(300);
    assert.equal(await privOf("bob"), false);
    assert.ok((await page.content()).includes("Privacy options") && !(await page.content()).includes("Privacy screen:"), "label renamed");
    if (vp.mobile) assert.ok(await overflow(page, 390));
    await page.screenshot({ path: path.join(data, `roles-${vp.name}.png`) });
    assert.deepEqual(page.errors, [], vp.name + ": no console errors");
    await ctx.close();
    // Reset for the next viewport.
    await api("PUT", `/admin/users/${ids.bob}/role`, { role: "member" }, adm);
  }

  // A rule violation shows the server's message in the dialog and changes nothing (the account vanishes while the dialog is open).
  {
    const dave = await mk("dave", "member");
    const octx = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
    const op = await signIn(octx, "admin", "test-admin-pw");
    await op.locator("select.role-select[data-user=dave]").selectOption("admin");
    await op.waitForSelector("#role-dialog");
    await api("DELETE", `/admin/users/${dave}`, null, adm);
    await op.click("#role-dialog button:has-text('Make admin')");
    await op.waitForSelector(".toast.err");
    assert.match(await op.locator(".toast.err").innerText(), /deleted/);
    assert.ok(await op.locator("#role-dialog").isVisible(), "the dialog stays open on an error");
    await octx.close();
  }

  // An admin (not the owner) sees roles as text, with no menu at all.
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
  const page = await signIn(ctx, "alice", "password-123");
  assert.equal(await page.locator("select.role-select").count(), 0, "admins get no role menus");
  assert.ok((await page.locator("tbody").innerText()).match(/Owner/), "roles are shown as text");
  // Even a hand-made request is refused, with the server's message.
  const alice = (await api("POST", "/login", { username: "alice", password: "password-123", device_name: "t", client_version: "1.4.1" })).access_token;
  await assert.rejects(api("PUT", `/admin/users/${ids.bob}/role`, { role: "admin" }, alice), /forbidden/);
  await browser.close();
  console.log("admin-roles UI test: OK");
} catch (e) {
  failed = true;
  console.error("FAILED:", e);
} finally {
  await cleanup();
}
process.exit(failed ? 1 : 0);
