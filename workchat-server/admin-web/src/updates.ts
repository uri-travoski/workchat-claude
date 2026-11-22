// Overview > Updates card: this server's version against the newest GitHub release. The server does the GitHub request
// (GET /admin/updates, cached for an hour), so the admin pages need no relaxed Content-Security-Policy.
import { api } from "./api";
import type { AdminUpdates } from "./api-types";
import { copy, fmtDate, h } from "./dom";

export const UPGRADE_COMMAND = "docker compose pull && docker compose up -d";

function assetLabel(name: string): string | null {
  const l = name.toLowerCase();
  if (l.endsWith(".sha256")) return null;
  if (l.endsWith(".apk")) return "Android (APK)";
  if (l.endsWith(".appimage")) return "Linux (AppImage)";
  if (l.endsWith(".deb")) return "Linux (.deb)";
  if (l.endsWith("-setup.exe")) return "Windows (installer)";
  if (l.endsWith("-portable.exe")) return "Windows (portable)";
  return null;
}

function cmdBox(cmd: string): HTMLElement {
  return h("div", { class: "row", style: "align-items:flex-start;gap:8px" },
    h("pre", { class: "mono cmd", style: "flex:1;min-width:0;margin:0;padding:8px 10px;background:var(--row);border:1px solid var(--border);border-radius:8px" }, cmd),
    h("button", { type: "button", class: "copy-cmd", onclick: () => copy(cmd) }, "Copy"));
}

function fill(box: HTMLElement, u: AdminUpdates, refresh: () => void) {
  const rel = u.latest;
  const status = u.error
    ? h("span", { class: "pill warn", id: "upd-status" }, "Could not check")
    : u.update_available
      ? h("span", { class: "pill warn", id: "upd-status" }, "Update available")
      : h("span", { class: "pill ok", id: "upd-status" }, "Up to date");
  const parts: (Node | string | false | null)[] = [
    h("div", { class: "row", style: "justify-content:space-between;gap:12px" },
      h("h2", { style: "margin:0" }, "Updates"),
      h("div", { class: "row" }, status, h("button", { type: "button", id: "upd-refresh", onclick: refresh }, "Check again"))),
    h("div", { class: "grid", style: "margin-top:12px" },
      h("div", { class: "stat" }, h("b", { id: "upd-current" }, "v" + u.current), h("span", { class: "muted" }, "This server")),
      h("div", { class: "stat" }, h("b", { id: "upd-latest" }, rel ? "v" + rel.version : "-"), h("span", { class: "muted" }, rel ? "Latest release, " + (rel.published ? fmtDate(Date.parse(rel.published)) : "") : "Latest release")),
      h("div", { class: "stat" }, h("b", {}, fmtDate(u.checked_ms)), h("span", { class: "muted" }, "Last checked"))),
  ];
  if (u.error) parts.push(h("p", { class: "muted", id: "upd-error" }, u.error + " You can still update by hand with the commands below."));
  if (rel && u.update_available) {
    parts.push(h("h3", {}, "What's new in v" + rel.version));
    parts.push(h("div", { id: "upd-notes", style: "white-space:pre-wrap;max-height:220px;overflow:auto;padding:10px;border:1px solid var(--border);border-radius:8px;background:var(--row)" }, rel.notes || "No release notes."));
  }
  parts.push(h("h3", {}, u.update_available ? "Update this server" : "Update commands"));
  parts.push(h("p", { class: "muted" }, "On the machine that runs the server, in the folder with docker-compose.yml (the image comes from ghcr.io, nothing is built locally)."));
  parts.push(h("p", { class: "muted" }, "Set the image in .env first, for example ",
    h("code", {}, "WORKCHAT_IMAGE=ghcr.io/uri-travoski/workchat-server:" + (rel?.version ?? u.current)),
    " (or download the new docker-compose.yml from the release page, which pins that release), then run the commands below. With ",
    h("code", {}, "WORKCHAT_IMAGE=ghcr.io/uri-travoski/workchat-server:latest"), " the commands alone always get the newest release. Your data volume is kept."));
  parts.push(cmdBox(UPGRADE_COMMAND));
  const dl = (rel?.assets ?? []).map((a) => ({ a, l: assetLabel(a.name) })).filter((x) => x.l);
  parts.push(h("h3", {}, "Client downloads" + (rel ? " in v" + rel.version : "")));
  parts.push(dl.length
    ? h("ul", { id: "upd-downloads", style: "margin:0;padding-left:20px" }, ...dl.map(({ a, l }) => h("li", {}, h("a", { href: a.url, rel: "noopener noreferrer", target: "_blank" }, l!), h("span", { class: "muted" }, "  " + a.name))))
    : h("p", { class: "muted" }, "No installers were listed for this release."));
  parts.push(h("p", { class: "muted" }, rel ? h("a", { href: rel.html_url, rel: "noopener noreferrer", target: "_blank", id: "upd-release-link" }, "Release page on GitHub") : "", ""));
  box.replaceChildren(...parts.filter((p): p is Node | string => !!p));
}

export function updatesCard(): HTMLElement {
  const box = h("div", { class: "card", id: "updates-card" }, h("h2", { style: "margin-top:0" }, "Updates"), h("p", { class: "muted" }, "Checking for updates..."));
  const load = async (refresh: boolean) => {
    try {
      fill(box, await api("GET", "/admin/updates" + (refresh ? "?refresh=true" : "")) as AdminUpdates, () => { box.querySelector("#upd-refresh")?.setAttribute("disabled", ""); load(true); });
    } catch (e: any) {
      box.replaceChildren(h("h2", { style: "margin-top:0" }, "Updates"), h("p", { class: "muted", id: "upd-error" }, "Could not check for updates: " + (e?.message ?? String(e))), cmdBox(UPGRADE_COMMAND));
    }
  };
  void load(false);
  return box;
}
