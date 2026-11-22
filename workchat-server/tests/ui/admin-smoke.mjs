// Playwright smoke test of the admin panel: first-run setup, overview, create user (login link), storage, sessions,
// backups and conversations pages load without console errors or failed requests. Starts its own server (port 18315).
//
//   cargo build -p workchat-server && (cd admin-web && npm run build)
//   PLAYWRIGHT_DIR=<dir with node_modules/playwright-core> node tests/ui/admin-smoke.mjs
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
const PORT = 18315;
const BASE = `http://127.0.0.1:${PORT}`;
const data = fs.mkdtempSync(path.join(os.tmpdir(), "workchat-ui-smoke-"));
const shots = process.env.SHOT_DIR ?? path.join(data, "shots");
fs.mkdirSync(shots, { recursive: true });
const server = spawn(path.join(root, "target/debug/workchat-server"), [], {
  env: { ...process.env, WORKCHAT_BIND: `127.0.0.1:${PORT}`, WORKCHAT_DATA_DIR: path.join(data, "srv"), WORKCHAT_ADMIN_DIR: path.join(root, "admin-web/dist"), WORKCHAT_BACKUP_WORK_FACTOR: "10", RUST_LOG: "warn" },
  stdio: ["ignore", "inherit", "inherit"],
});
const cleanup = async () => { server.kill("SIGTERM"); await new Promise((r) => setTimeout(r, 200)); fs.rmSync(data, { recursive: true, force: true }); };
process.on("exit", () => { try { server.kill("SIGKILL"); } catch {} });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let failed = false;
try {
  for (let i = 0; i < 100; i++) { try { await fetch(`${BASE}/api/v1/meta`); break; } catch { await sleep(100); } }
  const exe = fs.readdirSync(process.env.HOME + "/.cache/ms-playwright").filter((d) => d.startsWith("chromium-")).sort().pop();
  const b = await chromium.launch({ executablePath: `${process.env.HOME}/.cache/ms-playwright/${exe}/chrome-linux64/chrome`, args: ["--no-sandbox"] });
  const p = await b.newPage({ viewport: { width: 1200, height: 800 } });
  const problems = [];
  p.on("response", (r) => { if (r.status() >= 400) problems.push(`HTTP ${r.status()} ${r.url()}`); });
  p.on("console", (m) => { if (m.type() === "error") problems.push("console error: " + m.text()); });
  p.on("pageerror", (e) => problems.push("pageerror: " + e.message));
  const shot = (n) => p.screenshot({ path: path.join(shots, `${n}.png`) });
  await p.goto(`${BASE}/admin/`);
  await p.waitForSelector("text=Set up");
  await shot("1-setup");
  await p.fill("input[type=password][autocomplete=new-password]", "Passw0rd!pass");
  await p.click("text=Test storage");
  await p.waitForSelector("text=The test needs an account");
  problems.length = 0; // the wizard's pre-setup storage test is refused with 401 by design (it shows that message)
  await p.click("text=Create account and finish setup");
  await p.waitForSelector("text=Overview");
  await shot("2-overview");
  await p.click("a[href='#/users']");
  await p.click("text=Create user");
  const inputs = p.locator("dialog input");
  await inputs.nth(0).fill("dan");
  await inputs.nth(1).fill("Dan Smith");
  await p.click("dialog button:has-text(\"Create\")");
  await p.waitForSelector("text=Send this link");
  await shot("3-link");
  assert.match(await p.locator(".linkbox").innerText(), /^https?:\/\//, "a login link is shown");
  await p.click("dialog >> text=Cancel");
  await shot("4-users");
  await p.click("a[href='#/storage']"); await p.waitForSelector("text=Attachment storage"); await shot("5-storage");
  await p.click("a[href='#/sessions']"); await p.waitForSelector("text=Signed-in devices");
  await p.click("a[href='#/backups']"); await p.waitForSelector("text=Backups");
  await p.click("a[href='#/conversations']"); await p.waitForSelector("text=Channels and group DMs"); await shot("6-convs");
  assert.deepEqual(problems, [], "no failed requests or console errors");
  await b.close();
  console.log("admin-smoke OK");
} catch (e) {
  failed = true;
  console.error("FAILED:", e);
} finally {
  await cleanup();
}
process.exit(failed ? 1 : 0);
