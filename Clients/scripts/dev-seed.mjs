// Fills a throw-away WorkChat server with a few users, channels and messages so the app has something to show.
//   node scripts/dev-seed.mjs http://127.0.0.1:18601 admin t00lk1t1
// Everything goes through the public API. Safe to run once on an empty server; running twice adds more messages.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [base = "http://127.0.0.1:18601", adminUser = "admin", adminPass = "t00lk1t1"] = process.argv.slice(2);
const API = `${base}/api/v1`;
const PASS = "t00lk1t1";

async function call(token, method, path, body, headers = {}) {
  const res = await fetch(API + path, {
    method,
    headers: { ...(token ? { authorization: `Bearer ${token}` } : {}), ...(body && !(body instanceof Uint8Array) ? { "content-type": "application/json" } : {}), ...headers },
    body: body === undefined ? undefined : body instanceof Uint8Array ? body : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) throw new Error(`${method} ${path} -> ${res.status} ${text}`);
  return text ? JSON.parse(text) : null;
}
const login = async (username, password) => (await call(null, "POST", "/login", { username, password, device_name: "seed" })).access_token;

const admin = await login(adminUser, adminPass);
const people = [
  ["sarah", "Sarah Chen"],
  ["david", "David Kim"],
  ["marcus", "Marcus Brody"],
  ["priya", "Priya Patel"],
  ["liam", "Liam Chen"],
  ["elena", "Elena Vance"],
];
const users = {};
const existing = await call(admin, "GET", "/users");
for (const [username, display_name] of people) {
  let u = existing.find((x) => x.username === username);
  if (!u) {
    const r = await call(admin, "POST", "/admin/users", { username, display_name, password: PASS, role: username === "elena" ? "admin" : "member" });
    u = r.user;
  }
  users[username] = { id: u.id, token: await login(username, PASS) };
}
const me = (await call(admin, "GET", "/me")).user;

async function channel(name, topic, members) {
  const list = await call(admin, "GET", "/conversations");
  const found = list.find((c) => c.name === name && c.kind === "channel");
  if (found) return found.id;
  return (await call(admin, "POST", "/conversations", { kind: "channel", name, topic, members })).id;
}
const all = [me.id, ...Object.values(users).map((u) => u.id)];
const ann = await channel("company-announcements", "Company-wide news", all);
const eng = await channel("engineering-deployments", "Deploys and incidents", [me.id, users.david.id, users.marcus.id, users.priya.id, users.sarah.id]);
const gen = await channel("general", "Everything else", all);

const tmp = mkdtempSync(join(tmpdir(), "wc-seed-"));
execFileSync("convert", ["-size", "640x360", "gradient:#21375B-#1E5BB8", "-gravity", "center", "-pointsize", "40", "-fill", "white", "-annotate", "0", "Design Spec", join(tmp, "spec.png")]);
execFileSync("convert", ["-size", "400x520", "xc:white", "-gravity", "North", "-pointsize", "28", "-annotate", "+0+40", "Q3 Workplace Report", join(tmp, "report.pdf")]);
execFileSync("ffmpeg", ["-y", "-loglevel", "error", "-f", "lavfi", "-i", "sine=frequency=440:duration=3", "-c:a", "libvorbis", join(tmp, "voice-note.ogg")], { stdio: "ignore" });
async function upload(token, file, type, meta) {
  const bytes = new Uint8Array(readFileSync(join(tmp, file)));
  const r = await call(token, "POST", `/files?name=${encodeURIComponent(file)}`, bytes, { "content-type": type });
  if (meta) await call(token, "PUT", `/files/${r.attachment.id}/meta`, new TextEncoder().encode(JSON.stringify(meta)), { "content-type": "application/json" }).catch(() => {});
  return r.attachment.id;
}

const cid = () => crypto.randomUUID().replaceAll("-", "");
const say = (who, conv, body, extra = {}) => call(who.token, "POST", `/conversations/${conv}/messages`, { client_id: cid(), body, mentions: [], attachment_ids: [], ...extra });
const adm = { token: admin };

const dm = async (who) => (await call(who.token, "POST", "/conversations/dm", { user_id: me.id })).id;
const dSarah = await dm(users.sarah), dDavid = await dm(users.david), dMarcus = await dm(users.marcus), dPriya = await dm(users.priya), dLiam = await dm(users.liam), dElena = await dm(users.elena);

await say(users.sarah, ann, "*Q3 review* scheduled for 2pm today. Please read the _attached report_ first.");
await say(users.sarah, ann, "Town Hall recording and slides now available: https://example.com/townhall", { attachment_ids: [await upload(users.sarah.token, "report.pdf", "application/pdf")] });
await say(users.david, eng, "v3.1.0 deployed to staging successfully");
await say(users.marcus, eng, "Rollout checklist:\n- migrate database\n- warm caches\n- flip the flag\n\nThen run `make smoke` and paste results.");
const root = await say(users.priya, eng, "Anyone seen the flaky test in `sync_cursor`?");
await say(users.david, eng, "Yes, I think it is the clock. Looking now.", { thread_root: root.id });
await say(users.marcus, eng, "> clock skew?\nCould be, ping me.", { thread_root: root.id });
await say(adm, eng, "Thanks both, keep me posted.", { thread_root: root.id });
await say(users.liam, gen, "Lunch on Friday?\n```\nplace = rooftop\n```");

await say(users.sarah, dSarah, "Morning! Can you check the typography guidelines?");
await say(adm, dSarah, "Morning Sarah, yes, just finalized the documentation for the components and brand tokens.");
await say(users.sarah, dSarah, "Awesome. Here is the formal executive audit report for reference:", { attachment_ids: [await upload(users.sarah.token, "report.pdf", "application/pdf")] });
await say(adm, dSarah, "Got it, reviewing now. Also sharing the updated dashboard hero layout:", { attachment_ids: [await upload(admin, "spec.png", "image/png", { width: 640, height: 360 })] });
await say(users.sarah, dSarah, "Looks super crisp! The deep navy *#21375B* gives it a very polished enterprise feel.");
await say(users.david, dDavid, "Thanks, sending the revised deck shortly.");
await say(adm, dDavid, "Great, thank you!");
await say(users.marcus, dMarcus, "", { attachment_ids: [await upload(users.marcus.token, "voice-note.ogg", "audio/ogg", { duration_ms: 42000, peaks: [10, 40, 80, 120, 90, 60, 30, 70, 110, 50] })] });
await say(adm, dPriya, "Sounds good! Let me loop in engineering.");
await say(users.liam, dLiam, "Uploaded the contract draft for legal", { attachment_ids: [await upload(users.liam.token, "report.pdf", "application/pdf")] });
await say(users.elena, dElena, "Can you check the typography guidelines? I left comments on @admin's draft.", { mentions: [me.id] });

// reactions and a pin
const hist = await call(admin, "GET", `/conversations/${dSarah}/messages?limit=10`);
await call(admin, "PUT", `/messages/${hist.messages[hist.messages.length - 1].id}/reactions/${encodeURIComponent("👍")}`);
await call(users.sarah.token, "PUT", `/messages/${hist.messages[hist.messages.length - 1].id}/reactions/${encodeURIComponent("✅")}`);
for (const c of [dElena, ann, eng]) await call(admin, "PUT", `/conversations/${c}/prefs`, { favourite: true });
// the last messages stay unread for the admin: read two chats only
await call(admin, "POST", `/conversations/${dDavid}/read`, { seq: 99 }).catch(() => {});
console.log("seeded", { ann, eng, gen, dSarah });
