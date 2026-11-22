// Playwright test of the automation pages: bots, webhooks, AI settings, APP tag and forms in the
// chat viewer, scheduled messages in the viewer. Starts its own server (port 18312) and local fakes.
//
//   cargo build -p workchat-server && (cd admin-web && npm run build)
//   PLAYWRIGHT_DIR=<dir with node_modules/playwright-core> node tests/ui/admin-automation.mjs
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire((process.env.PLAYWRIGHT_DIR ?? process.cwd()) + "/");
const { chromium } = require("playwright-core");
const PORT = 18312;
const BASE = `http://127.0.0.1:${PORT}`;
const data = fs.mkdtempSync(path.join(os.tmpdir(), "workchat-ui-auto-"));
const shots = process.env.SHOT_DIR ?? path.join(data, "shots");
fs.mkdirSync(shots, { recursive: true });

// Fake receiver for outgoing webhooks and fake AI provider.
const received = [];
const fake = http.createServer((req, res) => {
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    received.push({ url: req.url, headers: req.headers, body });
    res.setHeader("content-type", "application/json");
    if (req.url.startsWith("/v1/messages")) res.end(JSON.stringify({ content: [{ type: "text", text: "ok" }] }));
    else res.end("{}");
  });
});
await new Promise((r) => fake.listen(0, "127.0.0.1", r));
const FAKE = `http://127.0.0.1:${fake.address().port}`;

const server = spawn(path.join(root, "target/debug/workchat-server"), [], {
  env: { ...process.env, WORKCHAT_BIND: `127.0.0.1:${PORT}`, WORKCHAT_DATA_DIR: path.join(data, "srv"), WORKCHAT_ADMIN_DIR: path.join(root, "admin-web/dist"), WORKCHAT_BACKUP_WORK_FACTOR: "10", WORKCHAT_WORKER_TICK_MS: "200", RUST_LOG: "warn" },
  stdio: ["ignore", "inherit", "inherit"],
});
const cleanup = async () => { server.kill("SIGTERM"); fake.close(); await new Promise((r) => setTimeout(r, 200)); fs.rmSync(data, { recursive: true, force: true }); };
process.on("exit", () => { try { server.kill("SIGKILL"); } catch {} });

async function api(method, p, body, token) {
  const res = await fetch(`${BASE}/api/v1${p}`, { method, headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...(token ? { Authorization: `Bearer ${token}` } : {}) }, body: body ? JSON.stringify(body) : undefined });
  const t = await res.text();
  if (!res.ok) throw new Error(`${method} ${p}: ${res.status} ${t}`);
  return t ? JSON.parse(t) : null;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const uid = () => crypto.randomUUID().replaceAll("-", "");

let failed = false;
try {
  for (let i = 0; i < 100; i++) { try { await fetch(`${BASE}/api/v1/meta`); break; } catch { await sleep(100); } }
  const setup = await api("POST", "/setup", { username: "admin", display_name: "Admin", password: "test-admin-pw", email: null, storage: null });
  const adm = setup.access_token;
  const aliceId = (await api("POST", "/admin/users", { username: "alice", display_name: "Alice", password: "password-123", role: "member" }, adm)).user.id;
  const general = (await api("POST", "/conversations", { kind: "channel", name: "general", topic: null, members: [aliceId] }, adm)).id;
  const alice = (await api("POST", "/login", { username: "alice", password: "password-123", device_name: "t", client_version: "1.4.1" })).access_token;

  const exe = fs.readdirSync(process.env.HOME + "/.cache/ms-playwright").filter((d) => d.startsWith("chromium-")).sort().pop();
  const browser = await chromium.launch({ executablePath: `${process.env.HOME}/.cache/ms-playwright/${exe}/chrome-linux64/chrome`, args: ["--no-sandbox"] });
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
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

  // ---- bots page: create, show token, membership
  await page.click("a[href='#/bots']");
  await page.waitForSelector("#new-bot");
  await page.click("#new-bot");
  await page.fill("input[aria-label='Bot username']", "receipt_bot");
  await page.fill("input[aria-label='Bot display name']", "Receipt Logger");
  await page.fill("input[aria-label='Description']", "Logs receipts");
  await page.locator("dialog label", { hasText: "general" }).locator("input").check();
  await page.click("dialog button:has-text('Create')");
  await page.waitForSelector("dialog:has-text('New token')");
  const token = (await page.locator("dialog .linkbox").innerText()).trim();
  assert.match(token, /^bt_/);
  await page.click("dialog button:has-text('Cancel')");
  await page.waitForSelector(".bot[data-bot='receipt_bot']");
  assert.ok(await page.locator(".bot .chip", { hasText: "general" }).count() === 1, "member of general");
  assert.equal(await page.locator(".bot .secret").innerText(), "•".repeat(12), "token hidden by default");
  await page.click(".bot button:has-text('Show')");
  assert.equal(await page.locator(".bot .secret").innerText(), token);
  await shot("auto-1-bots");

  // The bot works with the Telegram-style API, and its message shows up with an APP tag.
  const tg = async (m, body) => (await (await fetch(`${BASE}/bot${token}/${m}`, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) })).json());
  const sent = await tg("sendMessage", { chat_id: general, text: "Receipt logged: $12.50" });
  assert.equal(sent.ok, true, JSON.stringify(sent));
  await tg("sendMessage", { chat_id: general, text: "Approve this?", form: { id: "f1", title: "Expense", fields: [{ type: "text", id: "ref", label: "Reference", required: true }], buttons: [{ id: "ok", label: "Approve", style: "primary" }] } });
  // Webhook set: shows on the bot card.
  await tg("setWebhook", { url: FAKE + "/bot-hook", secret_token: "abc" });
  await page.reload();
  await page.waitForSelector(".bot[data-bot='receipt_bot']");
  await page.waitForSelector(".bot:has-text('secret token set')");
  assert.ok(await page.locator(".bot .mono", { hasText: "/bot-hook" }).count() >= 1, "webhook URL shown");
  // Regenerate the token: the old one stops working.
  await page.click(".bot button:has-text('Regenerate token')");
  await page.waitForSelector("dialog:has-text('New token')");
  const token2 = (await page.locator("dialog .linkbox").innerText()).trim();
  assert.notEqual(token2, token);
  await page.click("dialog button:has-text('Cancel')");
  assert.equal((await fetch(`${BASE}/bot${token}/getMe`)).status, 401);
  assert.equal((await fetch(`${BASE}/bot${token2}/getMe`)).status, 200);

  // ---- chat viewer: APP tag, form card, scheduled messages
  await api("POST", `/conversations/${general}/scheduled`, { body: "scheduled lunch ping", due_ms: Date.now() + 3600e3 }, alice);
  await page.click("a[href='#/chats']");
  await page.waitForSelector(".viewer .conv");
  await page.locator(".conv", { hasText: "general" }).first().click();
  await page.waitForSelector(".bubble:has-text('Receipt logged')");
  assert.ok(await page.locator(".bubble:has-text('Receipt logged') .app-tag").count() === 1, "APP tag on bot message");
  await page.waitForSelector(".bubble .formcard[data-form='f1']");
  assert.ok(await page.locator(".formcard .fakebtn", { hasText: "Approve" }).count() === 1);
  await page.waitForSelector("#scheduled-box:has-text('scheduled lunch ping')");
  await shot("auto-2-viewer");

  // ---- webhooks page
  await page.click("a[href='#/webhooks']");
  await page.waitForSelector("#new-incoming");
  await page.click("#new-incoming");
  await page.fill("input[aria-label='Webhook name']", "CI builds");
  await page.click("dialog button:has-text('Create')");
  await page.waitForSelector("tr[data-incoming='CI builds']");
  await page.click("tr[data-incoming='CI builds'] button:has-text('Show')");
  const hookUrl = (await page.locator("tr[data-incoming='CI builds'] .secret").innerText()).trim();
  assert.match(hookUrl, /\/api\/v1\/hooks\//);
  const posted = await fetch(hookUrl, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ text: "build 42 passed" }) });
  assert.equal(posted.status, 201);
  // outgoing
  await page.click("#new-outgoing");
  await page.fill("input[aria-label='Webhook name']", "Audit");
  await page.fill("input[aria-label='Receiver URL']", FAKE + "/audit");
  await page.fill("input[aria-label='Secret']", "topsecret");
  await page.click("dialog button:has-text('Create')");
  await page.waitForSelector("tr[data-outgoing='Audit']");
  await api("POST", `/conversations/${general}/messages`, { client_id: uid(), body: "audit me", mentions: [], attachment_ids: [] }, alice);
  const t0 = Date.now();
  while (!received.some((r) => r.url === "/audit" && r.body.includes("audit me")) && Date.now() - t0 < 10000) await sleep(100);
  const hit = received.find((r) => r.url === "/audit" && r.body.includes("audit me"));
  assert.ok(hit, "outgoing webhook delivered");
  const expect = "sha256=" + crypto.createHmac("sha256", "topsecret").update(`${hit.headers["x-workchat-timestamp"]}.${hit.body}`).digest("hex");
  assert.equal(hit.headers["x-workchat-signature"], expect, "signature verifies");
  await page.click("tr[data-outgoing='Audit'] button:has-text('Send test')");
  await page.reload();
  await page.waitForSelector("tr[data-outgoing='Audit'] .pill:has-text('delivered')", { timeout: 10000 });
  await shot("auto-3-webhooks");

  // ---- AI page
  await page.click("a[href='#/ai']");
  await page.waitForSelector("#ai-notice");
  assert.match(await page.locator("#ai-notice").innerText(), /sent to the provider/);
  await page.waitForSelector("#ai-empty");
  assert.equal((await api("GET", "/meta")).ai_search_enabled, false, "off by default");
  // add a provider through the dialog
  await page.click("#ai-add");
  await page.fill("#ai-name", "Claude");
  await page.fill("#ai-key", "sk-test-fake-0000000");
  await page.fill("#ai-base", FAKE);
  await page.click("dialog button:has-text('Save')");
  await page.waitForSelector(".ai-provider[data-provider='Claude']");
  assert.equal((await api("GET", "/meta")).ai_search_enabled, true);
  await page.click(".ai-provider[data-provider='Claude'] .ai-test");
  await page.waitForSelector(".ai-provider[data-provider='Claude'] [data-ai-status]:has-text('OK')");
  assert.ok(received.some((r) => r.url === "/v1/messages" && r.headers["x-api-key"] === "sk-test-fake-0000000"));
  assert.match(await page.locator(".ai-provider[data-provider='Claude']").innerText(), /ends in 0000/);
  assert.doesNotMatch(await page.content(), /sk-test-fake-0000000/, "the key is never in the page");
  // a second, OpenAI-compatible provider; recap goes to it
  await page.click("#ai-add");
  await page.fill("#ai-name", "Local");
  await page.selectOption("#ai-kind", "openai");
  assert.equal(await page.locator("#ai-model").inputValue(), "gpt-4o-mini");
  await page.fill("#ai-key", "local-key-123456789");
  await page.fill("#ai-base", FAKE);
  await page.click("dialog button:has-text('Save')");
  await page.waitForSelector(".ai-provider[data-provider='Local']");
  await page.selectOption("#ai-recap-provider", { label: "Local" });
  await page.click("#ai-defaults-save");
  await sleep(500);
  const ov = await api("GET", "/admin/ai", null, adm);
  assert.equal(ov.providers.length, 2);
  assert.equal(ov.recap_provider, ov.providers.find((p) => p.name === "Local").id);
  assert.equal(ov.search_provider, ov.providers.find((p) => p.name === "Claude").id);
  assert.ok(!JSON.stringify(ov).includes("local-key-123456789"));
  // edit keeps the key when the field stays empty; disable and delete
  await page.click(".ai-provider[data-provider='Local'] .ai-edit");
  await page.fill("#ai-model", "local-model-2");
  await page.click("dialog button:has-text('Save')");
  await page.waitForSelector(".ai-provider[data-provider='Local']:has-text('local-model-2')");
  await page.click(".ai-provider[data-provider='Local'] .ai-toggle");
  await page.waitForSelector(".ai-provider[data-provider='Local'] .pill:has-text('disabled')");
  await shot("auto-4-ai");
  await page.click(".ai-provider[data-provider='Local'] .ai-delete");
  await page.click("dialog button:has-text('Delete')");
  await page.waitForFunction(() => !document.querySelector(".ai-provider[data-provider='Local']"));

  // ---- user groups page
  await page.click("a[href='#/groups']");
  await page.waitForSelector("#new-group");
  await page.click("#new-group");
  await page.fill("input[aria-label='Group handle']", "oncall");
  await page.fill("input[aria-label='Group name']", "On-call");
  await page.locator("dialog label", { hasText: "Alice" }).locator("input").check();
  await page.click("dialog button:has-text('Save')");
  await page.waitForSelector("tr[data-group='oncall']:has-text('Alice')");
  assert.equal((await api("GET", "/groups", null, adm)).length, 1);
  // @oncall mentions Alice
  const gm = await api("POST", `/conversations/${general}/messages`, { client_id: uid(), body: "ping @oncall", mentions: [], attachment_ids: [] }, alice);
  assert.ok(JSON.stringify(gm).includes(aliceId), "group mention expands");
  await shot("auto-5-groups");

  // ---- bookmarks page
  await page.click("a[href='#/bookmarks']");
  await page.waitForSelector("#new-bookmark");
  await page.selectOption("#bm-conv", general);
  await page.click("#new-bookmark");
  await page.fill("input[aria-label='Bookmark title']", "Runbook");
  await page.fill("input[aria-label='Bookmark link']", "https://example.com/runbook");
  await page.click("dialog button:has-text('Save')");
  await page.waitForSelector("tr[data-bookmark='Runbook']");
  assert.equal((await api("GET", `/conversations/${general}/bookmarks`, null, alice)).length, 1);
  await shot("auto-6-bookmarks");
  await page.click("tr[data-bookmark='Runbook'] button:has-text('Remove')");
  await page.waitForSelector("#bm-list:has-text('No bookmarks')");

  assert.deepEqual(errors.filter((e) => !/favicon|401/.test(e)), [], "no console errors");
  await browser.close();
  console.log("admin-automation UI test: OK");
} catch (e) {
  failed = true;
  console.error("FAILED:", e);
} finally {
  await cleanup();
}
process.exit(failed ? 1 : 0);
