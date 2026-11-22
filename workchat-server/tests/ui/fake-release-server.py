#!/usr/bin/env python3
"""A fake GitHub release server for testing the apps' update check (tests only; own port, never a real one).

  fake-release-server.py --port 18390 [--version 9.9.9] [--apk app.apk] [--rate 400000]

Point an app at it with WORKCHAT_UPDATE_URL=http://127.0.0.1:18390/<scenario>/releases/latest (the Android emulator
reaches the host as 10.0.2.2). The first path segment is the scenario:

  new      a release <version> with all installers and .sha256 files
  slow     the same, but downloads are throttled to --rate bytes/s (so a progress bar can be seen)
  same     the release equals CURRENT (--current, default 1.4.1): "You're up to date"
  limit    HTTP 403 with GitHub's rate-limit headers
  nosha    like new, without the .sha256 assets
  bad      like new, but the installer bytes do not match the checksum
  pre      only a pre-release
  none     404 (no release published)
  oldname  like new, with the single universal -android.apk of releases before 1.4

The .apk asset is --apk when given (so an emulator can really install it); the other installers are small random files.
"""
import argparse, hashlib, json, os, random, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

A = argparse.ArgumentParser()
A.add_argument("--port", type=int, default=18390)
A.add_argument("--version", default="9.9.9")
A.add_argument("--current", default="1.4.1")
A.add_argument("--apk", default="")
A.add_argument("--rate", type=int, default=300000)
A.add_argument("--size", type=int, default=3_000_000)
ARGS = A.parse_args()

def payload(name):
    # --apk is served for the x86_64 (emulator) and the universal name; the arm64 APK is other bytes, so installing the wrong one fails.
    if name.endswith(".apk") and ARGS.apk and "arm64" not in name:
        return open(ARGS.apk, "rb").read()
    r = random.Random(name)
    return bytes(r.getrandbits(8) for _ in range(ARGS.size // 8)) * 8

def names(v, sc=""):
    # 1.4 ships one APK per ABI; the "oldname" scenario has the single universal APK of releases before 1.4.
    apks = [f"workchat-{v}-android.apk"] if sc == "oldname" else [f"workchat-{v}-android-arm64.apk", f"workchat-{v}-android-x86_64.apk"]
    return apks + [f"workchat_{v}-1_amd64.deb", f"WorkChat-{v}-x86_64.AppImage",
            f"workchat-{v}-x64-setup.exe", f"workchat-{v}-x64-portable.exe"]

NOTES = ("## What's new in {v}\n\n* **Update checks** in every app, with release notes\n* Check for updates button under Settings > About\n"
         "* The admin panel shows the server version and the upgrade commands\n\n### Fixed\n* Several small things\n")

class H(BaseHTTPRequestHandler):
    def log_message(self, fmt, *a):
        sys.stderr.write("fake-release: " + fmt % a + "\n")

    def send(self, code, body=b"", ctype="application/json", extra=None):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def do_OPTIONS(self):
        self.send(204, b"", extra={"Access-Control-Allow-Headers": "*"})

    def do_GET(self):
        parts = self.path.split("?")[0].strip("/").split("/")
        sc = parts[0]
        host = self.headers.get("Host", f"127.0.0.1:{ARGS.port}")
        v = ARGS.current if sc == "same" else ARGS.version
        if parts[1:] == ["releases", "latest"]:
            if sc == "limit":
                return self.send(403, b'{"message":"API rate limit exceeded"}', extra={"x-ratelimit-remaining": "0", "x-ratelimit-reset": str(int(time.time()) + 900)})
            if sc == "none":
                return self.send(404, b'{"message":"Not Found"}')
            assets = []
            for n in names(v, sc):
                data = payload(n)
                assets.append({"name": n, "size": len(data), "browser_download_url": f"http://{host}/{sc}/dl/{n}"})
                if sc != "nosha":
                    assets.append({"name": n + ".sha256", "size": 90, "browser_download_url": f"http://{host}/{sc}/dl/{n}.sha256"})
            rel = {"tag_name": f"v{v}", "name": f"WorkChat Server {v}", "draft": False, "prerelease": sc == "pre",
                   "html_url": f"http://{host}/{sc}/release", "published_at": "2026-11-20T00:00:00Z",
                   "body": NOTES.format(v=v), "assets": assets}
            return self.send(200, json.dumps(rel).encode())
        if len(parts) == 3 and parts[1] == "dl":
            n = parts[2]
            real = n[:-7] if n.endswith(".sha256") else n
            data = payload(real)
            if n.endswith(".sha256"):
                return self.send(200, f"{hashlib.sha256(data).hexdigest()}  {real}\n".encode(), "text/plain")
            if sc == "bad":
                data = data[:-1] + bytes([data[-1] ^ 1])
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            step = 16384
            for i in range(0, len(data), step):
                self.wfile.write(data[i:i + step])
                if sc == "slow":
                    time.sleep(step / ARGS.rate)
            return
        if parts[-1] == "release":
            return self.send(200, b"<html><body>Release page</body></html>", "text/html")
        self.send(404, b'{"message":"Not Found"}')

ThreadingHTTPServer(("0.0.0.0", ARGS.port), H).serve_forever()
