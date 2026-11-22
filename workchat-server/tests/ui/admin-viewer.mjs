// Playwright test of the admin chat viewer and backups pages (AD viewer, SV-20 UI).
// Starts its own workchat-server (port 18311, temp data dir), seeds chats through the REST API, then
// drives the real admin web app in headless Chromium.
//
//   cargo build -p workchat-server && (cd admin-web && npm run build)
//   PLAYWRIGHT_DIR=<dir with node_modules/playwright-core> node tests/ui/admin-viewer.mjs
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
const PORT = 18311;
const BASE = `http://127.0.0.1:${PORT}`;
const data = fs.mkdtempSync(path.join(os.tmpdir(), "workchat-ui-"));
const shots = process.env.SHOT_DIR ?? path.join(data, "shots");
fs.mkdirSync(shots, { recursive: true });

const server = spawn(path.join(root, "target/debug/workchat-server"), [], {
  env: { ...process.env, WORKCHAT_BIND: `127.0.0.1:${PORT}`, WORKCHAT_DATA_DIR: path.join(data, "srv"), WORKCHAT_ADMIN_DIR: path.join(root, "admin-web/dist"), WORKCHAT_BACKUP_WORK_FACTOR: "10", RUST_LOG: "warn" },
  stdio: ["ignore", "inherit", "inherit"],
});
const cleanup = async () => { server.kill("SIGTERM"); await new Promise((r) => setTimeout(r, 200)); fs.rmSync(data, { recursive: true, force: true }); };
process.on("exit", () => { try { server.kill("SIGKILL"); } catch {} });

async function api(method, p, body, token) {
  const res = await fetch(`${BASE}/api/v1${p}`, { method, headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...(token ? { Authorization: `Bearer ${token}` } : {}) }, body: body ? JSON.stringify(body) : undefined });
  const t = await res.text();
  const j = t ? JSON.parse(t) : null;
  if (!res.ok) throw new Error(`${method} ${p}: ${res.status} ${t}`);
  return j;
}
const loginApi = async (u, p) => (await api("POST", "/login", { username: u, password: p, device_name: "ui-test", client_version: "1.4.1" })).access_token;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const uid = () => crypto.randomUUID().replaceAll("-", "");

let failed = false;
try {
  for (let i = 0; i < 100; i++) { try { await fetch(`${BASE}/api/v1/meta`); break; } catch { await sleep(100); } }
  // ---- seed
  const setup = await api("POST", "/setup", { username: "admin", display_name: "Admin", password: "test-admin-pw", email: null, storage: null });
  const adm = setup.access_token;
  const mk = async (u) => (await api("POST", "/admin/users", { username: u, display_name: u[0].toUpperCase() + u.slice(1), password: "password-123", role: "member" }, adm)).user.id;
  const aliceId = await mk("alice"), bobId = await mk("bob");
  const general = (await api("POST", "/conversations", { kind: "channel", name: "general", topic: null, members: [aliceId, bobId] }, adm)).id;
  const alice = await loginApi("alice", "password-123"), bob = await loginApi("bob", "password-123");
  const send = (tok, conv, body, extra = {}) => api("POST", `/conversations/${conv}/messages`, { client_id: uid(), body, mentions: [], attachment_ids: [], ...extra }, tok);
  const png = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==", "base64");
  const up = await fetch(`${BASE}/api/v1/files`, { method: "POST", headers: { Authorization: `Bearer ${alice}`, "Content-Type": "image/png", "X-File-Name": "dot.png" }, body: png });
  const upj = await up.json();
  await send(alice, general, "Morning team, budget apples ready");
  const photo = await send(alice, general, "photo of the whiteboard", { attachment_ids: [upj.attachment.id] });
  const second = await send(bob, general, "thanks Alice, see https://example.com/doc");
  await send(bob, general, "second apples message");
  await api("PUT", `/messages/${second.id}/reactions/${encodeURIComponent("\u{1F44D}")}`, null, alice);
  await send(alice, general, "thread reply here", { thread_root: second.id });
  const dm = (await api("POST", "/conversations/dm", { user_id: bobId }, alice)).id;
  await send(alice, dm, "private chat about pears");
  await send(bob, dm, "pears are great");
  console.log("seeded");

  // ---- browser
  const exe = fs.readdirSync(process.env.HOME + "/.cache/ms-playwright").filter((d) => d.startsWith("chromium-")).sort().pop();
  const browser = await chromium.launch({ executablePath: `${process.env.HOME}/.cache/ms-playwright/${exe}/chrome-linux64/chrome`, args: ["--no-sandbox"] });
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  page.on("dialog", (d) => d.accept());
  const shot = (n) => page.screenshot({ path: path.join(shots, n + ".png") });

  await page.goto(`${BASE}/admin/`);
  await page.fill("input[autocomplete=username]", "admin");
  await page.fill("input[type=password]", "test-admin-pw");
  await page.click("button:has-text('Sign in')");
  await page.waitForSelector("text=Overview");
  await page.click("a[href='#/chats']");
  await page.waitForSelector(".viewer .conv");

  // 1. conversation list for everyone, then as one person
  assert.ok(await page.locator(".conv", { hasText: "general" }).count() >= 1, "general listed");
  await page.selectOption("select[aria-label='View as']", aliceId);
  await page.waitForSelector(".conv:has-text('Bob')"); // alice's DM is titled with the other person
  await page.selectOption("select[aria-label='View as']", "");
  await page.locator(".conv", { hasText: "general" }).first().click();
  await page.waitForSelector(".bubble:has-text('Morning team')");
  await shot("viewer-1-timeline");

  // 2. media inline, links, reactions, thread badge
  await page.waitForFunction(() => { const i = document.querySelector(".bubble img.media"); return i && i.complete && i.naturalWidth > 0; });
  assert.equal(await page.locator(".bubble a[href='https://example.com/doc']").count(), 1, "link is clickable");
  assert.ok(await page.locator(".bubble .chip", { hasText: "1" }).count() >= 1, "reaction chip");
  assert.ok(await page.locator("button.linkish", { hasText: "1 reply" }).count() === 1, "thread badge");
  assert.equal(await page.locator(".bubble:has-text('thread reply here')").count(), 0, "replies are not in the main timeline");
  await page.click("button.linkish");
  await page.waitForSelector(".thread .bubble:has-text('thread reply here')");

  // 3. search with highlight and jump
  await page.fill("input[aria-label='Search messages']", "apples");
  await page.press("input[aria-label='Search messages']", "Enter");
  await page.waitForSelector(".result");
  assert.equal(await page.locator(".result").count(), 2);
  assert.ok(await page.locator(".result mark", { hasText: "apples" }).count() >= 2, "snippets highlight the match");
  await shot("viewer-2-search");
  await page.locator(".result").first().click();
  await page.waitForSelector(".bubble.hl");

  // 4. date filters: today contains everything, a future window nothing
  const today = new Date().toISOString().slice(0, 10);
  const todayLocal = new Date(Date.now() - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);
  const tomorrow = new Date(Date.now() + 36 * 3600e3 - new Date().getTimezoneOffset() * 60000).toISOString().slice(0, 10);
  await page.fill("input[aria-label='From date']", tomorrow);
  await page.click(".tools button:has-text('Filter')");
  await page.waitForSelector("text=No messages in that date range");
  await page.fill("input[aria-label='From date']", todayLocal);
  await page.fill("input[aria-label='To date']", tomorrow);
  await page.click(".tools button:has-text('Filter')");
  await page.waitForSelector(".bubble:has-text('Morning team')");
  await page.click("button:has-text('Clear dates')");

  // 5. bulk select and delete for everyone: backup first, tombstones afterwards
  await page.click(".main-head button:has-text('Select')");
  await page.locator(".msg:has-text('Morning team') input.pick").check();
  await page.locator(".msg:has-text('second apples') input.pick").check();
  assert.match(await page.locator(".pickbar .count").innerText(), /2 selected/);
  await page.click("button.do-delete");
  await page.waitForSelector("dialog");
  await shot("viewer-3-delete");
  await page.click("dialog button:has-text('Back up and delete')");
  await page.waitForSelector(".toast:has-text('Deleted 2')");
  await page.waitForSelector(".bubble.gone");
  assert.equal(await page.locator(".bubble.gone").count(), 2);
  const msgBackups = (await api("GET", "/admin/backups", null, adm)).filter((b) => b.kind === "messages");
  assert.equal(msgBackups.length, 1, "a backup was written before deleting");
  // bob really lost them.
  const bobView = (await api("GET", `/conversations/${general}/messages`, null, bob)).messages;
  assert.equal(bobView.filter((m) => m.deleted).length, 2);

  // 6. delete only for one person in a date range, from the DM
  await page.click("button:has-text('Select')").catch(() => {});
  await page.click(".main-head button:has-text('Done selecting')").catch(() => {});
  await page.selectOption("select[aria-label='Conversation kind']", "dm");
  await page.locator(".conv", { hasText: "Alice & Bob" }).first().click();
  await page.waitForSelector(".bubble:has-text('pears')");
  await page.fill("input[aria-label='From date']", todayLocal);
  await page.fill("input[aria-label='To date']", tomorrow);
  await page.click(".tools button:has-text('Filter')");
  await page.click("button:has-text('Delete this range')");
  await page.waitForSelector("dialog");
  await page.check("input[name=scope][value=user]");
  await page.selectOption("dialog select[aria-label='Person']", aliceId);
  await page.click("dialog button:has-text('Back up and delete')");
  await page.waitForSelector(".toast:has-text('Deleted 2')");
  const aliceDm = (await api("GET", `/conversations/${dm}/messages`, null, alice)).messages;
  const bobDm = (await api("GET", `/conversations/${dm}/messages`, null, bob)).messages;
  assert.equal(aliceDm.length, 0, "alice lost her copy");
  assert.equal(bobDm.length, 2, "bob keeps his");

  // 7. admin chats like a normal user once a member
  await api("POST", `/admin/conversations/${general}/members`, { user_ids: [setup.me.user.id] }, adm);
  await page.click("a[href='#/users']"); await page.click("a[href='#/chats']");
  await page.waitForSelector(".viewer .conv");
  await page.selectOption("select[aria-label='Conversation kind']", "channel");
  await page.locator(".conv", { hasText: "general" }).first().click();
  await page.fill("textarea.main-input", "Hello from the admin viewer");
  await page.press("textarea.main-input", "Enter");
  await page.waitForSelector(".msg.right .bubble:has-text('Hello from the admin viewer')");
  await page.locator(".msg.right .bubble:has-text('Hello from the admin viewer')").hover();
  await page.locator(".msg.right .bubble:has-text('Hello from the admin viewer') button[aria-label='React with thumbs up']").click();
  await page.waitForSelector(".msg.right .chip");
  await page.locator(".msg.right .bubble:has-text('Hello from the admin viewer')").hover();
  await page.locator(".msg.right .bubble:has-text('Hello from the admin viewer') button:has-text('Edit')").click();
  await page.fill("dialog textarea", "Hello (edited) from the admin viewer");
  await page.click("dialog button:has-text('Save')");
  await page.waitForSelector(".bubble:has-text('(edited)')");
  await page.locator(".msg.right .bubble:has-text('(edited)')").hover();
  await page.locator(".msg.right .bubble:has-text('(edited)') button:has-text('Thread')").click();
  await page.fill("textarea.thread-input", "a threaded note");
  await page.press("textarea.thread-input", "Enter");
  await page.waitForSelector(".thread .bubble:has-text('a threaded note')");
  await shot("viewer-4-admin-chat");

  // 8. backups page: settings, back up now, restore deleted messages and a person
  await page.click("a[href='#/backups']");
  await page.waitForSelector("text=Full backup");
  await page.fill("input[aria-label='Backup passphrase']", "ui test passphrase");
  await page.click("#save-backup-settings");
  await page.waitForSelector(".toast:has-text('saved')");
  await page.waitForSelector("#backup-now");
  await page.click("#backup-now");
  await page.waitForSelector("#backup-table tr:has-text('full')");
  assert.ok(await page.locator("#backup-table tr:has-text('full') .pill.ok").count() >= 1, "full backup is encrypted");
  await shot("viewer-5-backups");
  // restore the deleted messages
  await page.locator("#backup-table tr:has-text('messages') button.restore").first().click();
  await page.click("dialog button:has-text('Restore')");
  await page.waitForSelector(".toast:has-text('Restored')");
  const bobAfter = (await api("GET", `/conversations/${general}/messages`, null, bob)).messages;
  assert.equal(bobAfter.filter((m) => m.deleted).length, 0, "restored for everyone");
  // restore alice's copy of the DM from the person backup that the delete-for-one wrote
  const userBackup = await page.locator("#backup-table tr:has-text('user') button.restore").first();
  await userBackup.click();
  await page.waitForSelector("dialog input[data-user]");
  await page.click("dialog button:has-text('Restore')");
  await page.waitForSelector(".toast:has-text('Restored')");
  const aliceDm2 = (await api("GET", `/conversations/${dm}/messages`, null, alice)).messages;
  assert.equal(aliceDm2.length, 2, "alice has her chat back");

  assert.deepEqual(errors.filter((e) => !/favicon/.test(e)), [], "no console errors");
  await browser.close();
  console.log("admin-viewer UI test: OK");
} catch (e) {
  failed = true;
  console.error("FAILED:", e);
} finally {
  await cleanup();
}
process.exit(failed ? 1 : 0);
