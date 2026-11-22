#!/usr/bin/env python3
"""Fills a WorkChat server with a believable company: 10 people, 10 channels, a group chat, 8 direct messages and 2 bots,
30 messages in each conversation (photos, videos, voice notes, Word/Excel/PowerPoint/PDF files, rich text, reactions, edits,
pins, threads), spread over the last two weeks. Everything goes through the public HTTP API; nothing touches the database.

    ./seed.sh --server http://192.168.8.101:18600 --owner-password '...' --password 'user-password'

Refuses to run when any of the sample users already exists.
"""
import argparse
import json
import random
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path

import people
from script_channels import CHANNELS as CHANNEL_SCRIPTS
from script_dms import DMS as DM_SCRIPTS, GROUP as GROUP_SCRIPTS

HERE = Path(__file__).resolve().parent
DAY = 86_400_000
HOUR = 3_600_000
CLIENT_VERSION = "1.1.0"


class ApiError(Exception):
    def __init__(self, status, code, message, where):
        super().__init__(f"{where}: HTTP {status} {code}: {message}")
        self.status, self.code = status, code


class Server:
    """Minimal JSON/bytes client with a gentle request rate (the server limits each address to about 40 requests per second)."""

    def __init__(self, base, pause=0.03):
        self.base = base.rstrip("/")
        self.pause = pause
        self.count = 0

    def request(self, method, path, body=None, token=None, data=None, content_type=None, query=None, retries=6):
        url = self.base + "/api/v1" + path
        if query:
            url += "?" + urllib.parse.urlencode(query)
        headers = {"X-Client-Version": CLIENT_VERSION}
        if token:
            headers["Authorization"] = "Bearer " + token
        payload = None
        if body is not None:
            payload = json.dumps(body).encode()
            headers["Content-Type"] = "application/json"
        elif data is not None:
            payload = data
            headers["Content-Type"] = content_type or "application/octet-stream"
        elif method in ("PUT", "POST", "PATCH"):
            payload = b""
        for attempt in range(retries):
            time.sleep(self.pause)
            self.count += 1
            req = urllib.request.Request(url, data=payload, headers=headers, method=method)
            try:
                with urllib.request.urlopen(req, timeout=120) as r:
                    raw = r.read()
                    return json.loads(raw) if raw else None
            except urllib.error.HTTPError as e:
                raw = e.read()
                try:
                    err = json.loads(raw)["error"]
                except Exception:
                    err = {"code": "http", "message": raw[:200].decode(errors="replace")}
                if e.code == 429 and attempt < retries - 1:
                    time.sleep(1.5 * (attempt + 1))
                    continue
                raise ApiError(e.code, err.get("code"), err.get("message"), f"{method} {path}")
            except urllib.error.URLError as e:
                if attempt < retries - 1:
                    time.sleep(1.0)
                    continue
                raise SystemExit(f"cannot reach {self.base}: {e.reason}")


class Account:
    """One signed-in person (or the owner). Refreshes the access token when it expires."""

    def __init__(self, server, username, password):
        self.server, self.username, self.password = server, username, password
        r = server.request("POST", "/login", {"username": username, "password": password, "device_name": "seed script", "client_version": CLIENT_VERSION})
        self.access, self.refresh_token = r["access_token"], r["refresh_token"]
        self.me = r["me"]["user"]
        self.id = self.me["id"]

    def call(self, method, path, body=None, **kw):
        try:
            return self.server.request(method, path, body, token=self.access, **kw)
        except ApiError as e:
            if e.status == 401 and e.code == "token_expired":
                t = self.server.request("POST", "/refresh", {"refresh_token": self.refresh_token})
                self.access = t["access_token"]
                return self.server.request(method, path, body, token=self.access, **kw)
            raise

    def upload(self, name, data, content_type, purpose=None):
        q = {"name": name}
        if purpose:
            q["purpose"] = purpose
        return self.call("POST", "/files", data=data, content_type=content_type, query=q)["attachment"]


class Bot:
    def __init__(self, server, token, user_id):
        self.server, self.token, self.id = server, token, user_id

    def call(self, method, path, body=None, **kw):
        return self.server.request(method, path, body, token=self.token, **kw)


def log(*a):
    print(*a, flush=True)


# ------------------------------------------------------------------ timestamps

def plan_times(n, end_ms, rnd, long_gap_p):
    """n increasing timestamps ending at end_ms: bursts of quick replies separated by long gaps, kept to daytime (Perth time)."""
    gaps = []
    for _ in range(n - 1):
        if rnd.random() < long_gap_p:
            gaps.append(rnd.uniform(2.5, 26) * HOUR)
        else:
            gaps.append(rnd.uniform(20, 420) * 1000)
    ts = [0.0]
    for g in gaps:
        ts.append(ts[-1] + g)
    total = ts[-1]
    base = end_ms - total
    out = []
    prev = 0
    for t in ts:
        v = int(base + t)
        hour_utc = (v // HOUR) % 24
        if hour_utc >= 11:  # 19:00-08:00 in Perth: push into the next morning
            v += (24 - hour_utc) * HOUR + rnd.randint(0, 90) * 60_000
        out.append(max(v, prev + 5_000))
        prev = out[-1]
    shift = out[-1] - end_ms
    if shift > 0:  # keep the last message at the requested time
        out = [v - shift for v in out]
    return out


# ------------------------------------------------------------------ seeding

def flatten(entries):
    """Top-level entries with their thread replies expanded: list of dict(who, text, opts, top_index, thread_of)."""
    flat = []
    top = 0
    for e in entries:
        who, text = e[0], e[1]
        opts = dict(e[2]) if len(e) > 2 else {}
        replies = opts.pop("t", [])
        flat.append({"who": who, "text": opts.get("text", text), "opts": opts, "top": top, "reply_of_root": False})
        for r in replies:
            ropts = dict(r[2]) if len(r) > 2 else {}
            flat.append({"who": r[0], "text": ropts.get("text", r[1]), "opts": ropts, "top": top, "reply_of_root": True})
        top += 1
    return flat


def main():
    ap = argparse.ArgumentParser(description="Seed a WorkChat server with sample people, channels, chats and files.")
    ap.add_argument("--server", required=True, help="server address, e.g. http://192.168.8.101:18600")
    ap.add_argument("--owner", default="admin", help="owner or admin user name (default admin)")
    ap.add_argument("--owner-password", required=True)
    ap.add_argument("--password", required=True, help="password given to all 10 sample users (and used to sign them in)")
    ap.add_argument("--assets", default=str(HERE / "generated"), help="folder for the generated sample files (made on first use)")
    ap.add_argument("--regenerate", action="store_true", help="draw the sample files again even if they exist")
    ap.add_argument("--days", type=int, default=14, help="spread messages over about this many recent days (default 14)")
    ap.add_argument("--random-seed", type=int, default=2026)
    ap.add_argument("--pause", type=float, default=0.03, help="seconds between requests (default 0.03)")
    args = ap.parse_args()
    rnd = random.Random(args.random_seed)

    # sample files
    assets = Path(args.assets)
    if args.regenerate or not (assets / "manifest.json").exists():
        import assets as assets_mod
        log(f"generating sample files in {assets} ...")
        assets_mod.build(assets)
    manifest = json.loads((assets / "manifest.json").read_text())
    files = manifest["files"]
    cache = {}

    def file_bytes(name):
        if name not in cache:
            cache[name] = Path(files[name]["path"]).read_bytes()
        return cache[name]

    srv = Server(args.server, args.pause)
    meta = srv.request("GET", "/meta")
    log(f"server {meta['name']} {meta['server_version']} at {args.server}")
    if meta.get("setup_required"):
        sys.exit("the server has no owner yet: open the admin panel or run the setup first")
    owner = Account(srv, args.owner, args.owner_password)
    existing = {u["username"] for u in owner.call("GET", "/admin/users")}
    clash = sorted(existing & set(people.USERNAMES + [b[0] for b in people.BOTS]))
    if clash:
        log("These sample users already exist: " + ", ".join(clash))
        log("Nothing was changed. (Seed a fresh server, or delete those users in the admin panel first.)")
        sys.exit(3)

    # ---- users
    log("creating users ...")
    accounts, ids = {}, {}
    for username, display, title, phone, look in people.PEOPLE:
        r = owner.call("POST", "/admin/users", {"username": username, "display_name": display, "password": args.password, "title": title, "phone": phone, "role": "member", "create_login_link": False})
        ids[username] = r["user"]["id"]
        acc = Account(srv, username, args.password)
        accounts[username] = acc
        if username in manifest["avatars"]:
            att = acc.upload(f"{username}.png", Path(manifest["avatars"][username]).read_bytes(), "image/png", purpose="avatar")
            acc.call("PATCH", "/me", {"avatar": att["id"]})
    user_by_id = {v: k for k, v in ids.items()}

    # ---- channels and the group chat
    log("creating channels ...")
    convs = {}  # key -> dict(id, members)
    for name, topic, _icon, members in people.CHANNELS:
        c = owner.call("POST", "/conversations", {"kind": "channel", "name": name, "topic": topic, "members": [ids[m] for m in members], "visibility": "private", "include_creator": False})
        convs[name] = {"id": c["id"], "members": members}
        av = owner.upload(f"{name}.png", Path(manifest["avatars"]["channel:" + name]).read_bytes(), "image/png", purpose="avatar")
        owner.call("PUT", f"/conversations/{c['id']}/avatar", {"avatar": av["id"]})
    gname, _gicon, gmembers = people.GROUP
    c = owner.call("POST", "/conversations", {"kind": "group_dm", "name": gname, "members": [ids[m] for m in gmembers], "include_creator": False})
    convs["group:" + gname] = {"id": c["id"], "members": gmembers}
    av = owner.upload(f"{gname}.png", Path(manifest["avatars"]["channel:" + gname]).read_bytes(), "image/png", purpose="avatar")
    owner.call("PUT", f"/conversations/{c['id']}/avatar", {"avatar": av["id"]})
    for a, b in people.DMS:
        c = accounts[a].call("POST", "/conversations/dm", {"user_id": ids[b]})
        convs[f"dm:{a}:{b}"] = {"id": c["id"], "members": [a, b]}

    # ---- bots (appear as normal members)
    log("creating bots ...")
    bots = {}
    for username, display, title, desc, _icon, chans in people.BOTS:
        b = owner.call("POST", "/admin/bots", {"username": username, "display_name": display, "description": desc, "conv_ids": [convs[c]["id"] for c in chans]})
        av = owner.upload(f"{username}.png", Path(manifest["avatars"][username]).read_bytes(), "image/png", purpose="avatar")
        owner.call("PATCH", f"/admin/bots/{b['user']['id']}", {"avatar": av["id"], "title": title})
        bots[username] = Bot(srv, b["token"], b["user"]["id"])
        ids[username] = b["user"]["id"]
        user_by_id[b["user"]["id"]] = username
        for c in chans:
            convs[c]["members"] = convs[c]["members"] + [username]

    # ---- conversations: when each one ends (some very recent, some days old)
    scripts = {}
    scripts.update({k: v for k, v in CHANNEL_SCRIPTS.items()})
    scripts.update(DM_SCRIPTS)
    scripts.update(GROUP_SCRIPTS)
    now = int(time.time() * 1000)
    end_hours = {
        "executive-leadership": 0.4, "product-strategy": 1.5, "dm:katherine.wells:elena.vance": 2.2, "dm:katherine.wells:david.kim": 3.5,
        "dm:katherine.wells:marcus.brody": 6, "dm:katherine.wells:priya.patel": 9, "dm:katherine.wells:liam.chen": 27, "company-announcements": 30,
        "engineering-deployments": 1.0, "incident-response": 52, "design-system": 76, "sales-pipeline": 20, "people-culture": 100,
        "legal-compliance": 125, "random-lunch": 4.5, "group:launch-war-room": 5.5, "dm:david.kim:priya.patel": 14,
        "dm:alex.rivera:elena.vance": 33, "dm:sarah.mitchell:marcus.brody": 60,
    }

    log("posting messages ...")
    summary = []
    posted = {}  # conv key -> {top index -> message id}
    stats = {"messages": 0, "thread_replies": 0, "files": {}, "reactions": 0, "edits": 0, "pins": 0, "deleted": 0}
    thread_roots = {}
    for key, entries in scripts.items():
        conv = convs[key]
        flat = flatten(entries)
        n_top = len(entries)
        if n_top != 30:
            log(f"  warning: {key} has {n_top} top-level messages, expected 30")
        times = plan_times(len(flat), int(now - end_hours.get(key, rnd.uniform(3, 100)) * HOUR), rnd, 0.22 + 0.1 * rnd.random())
        # compress when a conversation would reach back further than --days
        first = times[0]
        limit = now - args.days * DAY
        if first < limit:
            span = times[-1] - first
            times = [int(limit + (t - first) * (times[-1] - limit) / span) for t in times]
        top_ids = {}
        root_id = None
        for i, m in enumerate(flat):
            who, opts = m["who"], m["opts"]
            if who not in conv["members"]:
                raise SystemExit(f"{key}: {who} is not a member")
            body = m["text"]
            mentions = sorted({ids[u] for u in re.findall(r"@([a-z][a-z0-9._-]*)", body) if u in ids})
            attachments, att_names = [], []
            fnames = [opts["f"]] if isinstance(opts.get("f"), str) else list(opts.get("f", []))
            if opts.get("v"):
                fnames.append(opts["v"])
            if fnames and who in bots:
                raise SystemExit("bots in this script do not send files")
            for fname in fnames:
                spec = files[fname]
                shown = fname
                if fname.startswith("voice-"):
                    stamp = time.strftime("%Y%m%d-%H%M%S", time.gmtime(times[i] / 1000))
                    shown = f"voice-{stamp}.{fname.rsplit('.', 1)[1]}"
                att = accounts[who].upload(shown, file_bytes(fname), spec["content_type"])
                if spec["meta"]:
                    accounts[who].call("PUT", f"/files/{att['id']}/meta", spec["meta"])
                attachments.append(att["id"])
                kind = spec["content_type"].split("/")[0] if spec["content_type"].split("/")[0] in ("image", "video", "audio") else fname.rsplit(".", 1)[1]
                if fname.startswith("voice-"):
                    kind = "voice"
                stats["files"][kind] = stats["files"].get(kind, 0) + 1
            reply_to = None
            if opts.get("q") and not m["reply_of_root"]:
                reply_to = top_ids.get(m["top"] + opts["q"])
            payload = {"client_id": uuid.uuid4().hex, "body": body, "thread_root": root_id if m["reply_of_root"] else None, "reply_to": reply_to, "mentions": mentions, "attachment_ids": attachments}
            if who in bots:
                msg = bots[who].call("POST", "/bot/messages", {"conv_id": conv["id"], "body": body, "thread_root": payload["thread_root"], "reply_to": reply_to, "mentions": mentions})
            else:
                msg = accounts[who].call("POST", f"/conversations/{conv['id']}/messages", payload)
            mid = msg["id"]
            if not m["reply_of_root"]:
                top_ids[m["top"]] = mid
                root_id = mid
                stats["messages"] += 1
            else:
                stats["thread_replies"] += 1
            owner.call("PUT", f"/admin/messages/{mid}/ts", {"ts": times[i]})
            if opts.get("e"):
                accounts[who].call("PATCH", f"/messages/{mid}", {"body": opts["e"], "mentions": mentions})
                stats["edits"] += 1
            for emoji, users in (opts.get("r") or {}).items():
                for u in users.split(","):
                    if u in conv["members"] and u != who and u in accounts:
                        accounts[u].call("PUT", f"/messages/{mid}/reactions/{urllib.parse.quote(emoji, safe='')}")
                        stats["reactions"] += 1
            if opts.get("pin"):
                owner.call("PUT", f"/conversations/{conv['id']}/pins/{mid}")
                stats["pins"] += 1
            if opts.get("rm"):
                (accounts[who] if who in accounts else owner).call("DELETE", f"/messages/{mid}")
                stats["deleted"] += 1
        posted[key] = top_ids
        summary.append((key, n_top, len(flat) - n_top, len(conv["members"])))
        log(f"  {key}: {n_top} messages, {len(flat) - n_top} thread replies")

    # ---- the main user's view: favourites, a muted channel, some unread
    log("setting favourites, mute and read positions ...")
    main_acc = accounts[people.MAIN_USER]
    for k in people.FAVOURITES:
        key = k if not k.startswith("dm:") else f"dm:{people.MAIN_USER}:{k[3:]}"
        main_acc.call("PUT", f"/conversations/{convs[key]['id']}/prefs", {"favourite": True})
    for k in people.MUTED:
        main_acc.call("PUT", f"/conversations/{convs[k]['id']}/prefs", {"mute_until_ms": -1})
    for key, conv in convs.items():
        for u in conv["members"]:
            if u not in accounts:
                continue
            acc = accounts[u]
            c = acc.call("GET", f"/conversations/{conv['id']}")
            last = c["last_seq"]
            # Everyone has read everything except, for some chats, the last few messages written by others. (Sending a message
            # marks the chat read up to that message, so what stays unread is whatever the others wrote after your last one.)
            if u == people.MAIN_USER:
                continue  # keep what the others wrote after her last message unread
            back = rnd.choice([0, 0, 0, 1, 2, 3])
            seq = last
            if back:
                hist = acc.call("GET", f"/conversations/{conv['id']}/messages", query={"limit": 40})["messages"]
                others = [m for m in hist if m["sender_id"] != acc.id and m["kind"] != "system"]
                if len(others) >= back:
                    seq = others[-back]["seq"] - 1
            if seq > 0:
                acc.call("POST", f"/conversations/{conv['id']}/read", {"seq": seq})
    # threads: most are read; the main user keeps a few unread
    for u, acc in accounts.items():
        threads = acc.call("GET", "/threads")
        keep = 3 if u == people.MAIN_USER else 0
        for t in threads:
            if t["unread"] > 0 and keep > 0:
                keep -= 1
                continue
            if t["unread"] > 0 and rnd.random() < 0.7:
                acc.call("POST", f"/threads/{t['root']['id']}/read")

    # ---- report
    counts = accounts[people.MAIN_USER].call("GET", "/me/counts")
    log("")
    log("Done. Summary")
    log(f"  users {len(people.PEOPLE)} (password: the one you passed), bots {len(bots)}, channels {len(people.CHANNELS)}, group chats 1, direct messages {len(people.DMS)}")
    log(f"  messages {stats['messages']} (+{stats['thread_replies']} thread replies), reactions {stats['reactions']}, edits {stats['edits']}, pins {stats['pins']}, deleted {stats['deleted']}")
    log("  files sent: " + ", ".join(f"{k} {v}" for k, v in sorted(stats["files"].items())))
    log(f"  {people.MAIN_USER} sees: {counts['conversations']} chats ({counts['dms']} DMs, {counts['channels']} channels), {counts['threads']} threads ({counts['threads_unread']} unread), {counts['files']} files, {counts['unread_conversations']} unread chats")
    log(f"  {srv.count} API requests")
    log("  sign in as e.g. " + ", ".join(people.USERNAMES[:3]) + " ...")


if __name__ == "__main__":
    main()
