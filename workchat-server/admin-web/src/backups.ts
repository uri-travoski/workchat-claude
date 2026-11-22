// Backups page: full encrypted backups (settings, schedule, back up now), per-user backups and restore.
import { api, apiBlob } from "./api";
import type { AdminUser, BackupInfo, BackupSettings, RestoreResponse, StorageConfig } from "./api-types";
import { dialog, field, fmtBytes, fmtDate, h, toast } from "./dom";
import { storageForm } from "./storage-form";

export async function backupsPage(el: HTMLElement, reload: () => void, fail: (e: unknown) => void): Promise<void> {
  const [list, settings, users] = await Promise.all([
    api<BackupInfo[]>("GET", "/admin/backups"),
    api<BackupSettings>("GET", "/admin/backups/settings"),
    api<AdminUser[]>("GET", "/admin/users"),
  ]);
  const userName = (id?: string | null) => users.find((u) => u.id === id)?.display_name ?? "";

  // ---- settings card
  const enabled = h("input", { type: "checkbox", checked: settings.enabled, style: "width:auto", "aria-label": "Daily backups" });
  const interval = h("input", { type: "number", min: "1", value: String(settings.interval_hours), "aria-label": "Hours between backups" });
  const retention = h("input", { type: "number", min: "1", value: String(settings.retention), "aria-label": "Backups to keep" });
  const pass = h("input", { type: "password", autocomplete: "new-password", placeholder: settings.passphrase_set ? "set (leave empty to keep)" : "choose a passphrase (8+ characters)", "aria-label": "Backup passphrase" });
  const sameStore = h("input", { type: "checkbox", checked: !settings.target, style: "width:auto", "aria-label": "Use the attachment storage" });
  const sf = storageForm(settings.target ?? undefined);
  const sfBox = h("div", { style: settings.target ? "" : "display:none" }, sf.el);
  sameStore.addEventListener("change", () => { sfBox.style.display = sameStore.checked ? "none" : ""; });
  const status = h("p", { class: "muted" }, settings.last_run_ms ? `Last backup: ${fmtDate(settings.last_run_ms)}. ${settings.last_status}` : "No full backup yet.");

  const save = async () => {
    const body: any = { enabled: enabled.checked, interval_hours: Number(interval.value), retention: Number(retention.value) };
    if (pass.value) body.passphrase = pass.value;
    if (sameStore.checked) body.clear_target = true; else body.target = sf.value() as StorageConfig;
    await api("PUT", "/admin/backups/settings", body);
  };

  el.append(h("div", { class: "card" }, h("h2", {}, "Full backup"),
    h("p", { class: "muted" }, "A full backup is the whole database (accounts, chats, settings), encrypted with your passphrase, written to the place you choose. Attachments stay where they are; a new server that reconnects to the same attachment storage gets everything back. Keep the passphrase safe: without it a backup cannot be opened."),
    status,
    h("label", { class: "radio" }, enabled, " Back up automatically"),
    h("div", { class: "grid" }, field("Hours between backups", interval), field("Backups to keep", retention), field("Passphrase", pass)),
    h("label", { class: "radio" }, sameStore, " Store backups in the attachment storage (separate folder)"),
    sfBox,
    h("div", { class: "row", style: "margin-top:14px" },
      h("button", { class: "primary", id: "save-backup-settings", onclick: async () => { try { await save(); toast("Backup settings saved"); reload(); } catch (e) { fail(e); } } }, "Save settings"),
      h("button", { onclick: async () => { try { await save(); const r = await api("POST", "/admin/backups/test-target", {}); toast((r.ok ? "Backup target OK: " : "Backup target failed: ") + r.message, !r.ok); } catch (e) { fail(e); } } }, "Test target"),
      h("button", { id: "backup-now", onclick: async (ev: Event) => { const b = ev.currentTarget as HTMLButtonElement; b.disabled = true; try { await save(); const i = await api<BackupInfo>("POST", "/admin/backups/full", {}); toast("Backup written: " + i.key); reload(); } catch (e) { fail(e); b.disabled = false; } } }, "Back up now"))));

  // ---- per-user backup
  const who = h("select", { "aria-label": "User to back up" }, ...users.filter((u) => !u.deleted).map((u) => h("option", { value: u.id }, `${u.display_name} (${u.username})`)));
  el.append(h("div", { class: "card" }, h("h2", {}, "Back up one person"),
    h("p", { class: "muted" }, "Their account, memberships, messages and the files they can see. Also written automatically before Clear history, Delete user and date-range deletes."),
    h("div", { class: "row" }, who, h("button", { id: "backup-user", onclick: async () => { try { const b = await api<BackupInfo>("POST", `/admin/users/${who.value}/backup`, {}); toast("Backup written: " + b.key); reload(); } catch (e) { fail(e); } } }, "Back up this person"))));

  // ---- list
  const download = async (b: BackupInfo) => {
    try {
      const blob = await apiBlob(`/admin/backups/${b.id}/download`);
      const a = h("a", { href: URL.createObjectURL(blob), download: b.key.split("/").pop() ?? "backup" });
      document.body.append(a); a.click(); a.remove();
    } catch (e) { fail(e); }
  };
  const rowFor = (b: BackupInfo) => h("tr", { "data-backup": b.id },
    h("td", {}, fmtDate(b.created_ms)), h("td", {}, b.kind), h("td", {}, userName(b.user_id)), h("td", {}, b.reason), h("td", {}, fmtBytes(b.size)),
    h("td", {}, b.encrypted ? h("span", { class: "pill ok" }, "yes") : h("span", { class: "pill warn" }, "no")),
    h("td", {}, h("div", { class: "row" },
      h("button", { class: "restore", onclick: () => restoreDialog(b, users, reload, fail) }, "Restore\u2026"),
      h("button", { onclick: () => download(b) }, "Download"))));
  const table = list.length === 0 ? h("p", {}, "No backups yet.") :
    h("table", { id: "backup-table" },
      h("thead", {}, h("tr", {}, ...["Created", "Kind", "Person", "Reason", "Size", "Encrypted", ""].map((x) => h("th", {}, x)))),
      h("tbody", {}, ...list.map(rowFor)));
  el.append(h("div", { class: "card" }, h("h2", {}, "Backups"), table));
}

function restoreDialog(b: BackupInfo, users: AdminUser[], reload: () => void, fail: (e: unknown) => void) {
  if (b.kind === "messages") {
    dialog("Restore deleted messages", h("p", {}, "Puts the messages from this backup back for everyone who lost them, and moves their files back from the backup folder."), [{
      label: "Restore", primary: true, action: async (close) => { const r = await api<RestoreResponse>("POST", "/admin/restore", { backup_id: b.id, user_ids: [], all: false }); close(); toast(summary(r)); reload(); },
    }]);
    return;
  }
  const picks = users.filter((u) => !u.deleted && (b.kind !== "user" || u.id === b.user_id)).map((u) => ({ u, c: h("input", { type: "checkbox", style: "width:auto", checked: b.kind === "user", "data-user": u.id }) }));
  const all = h("input", { type: "checkbox", style: "width:auto", id: "restore-all" });
  all.addEventListener("change", () => picks.forEach((p) => { p.c.disabled = all.checked; }));
  const merge = h("input", { type: "radio", name: "mode", value: "merge", checked: true, style: "width:auto" });
  const replace = h("input", { type: "radio", name: "mode", value: "replace", style: "width:auto" });
  dialog("Restore from backup", h("div", {},
    h("p", { class: "muted" }, "Choose who to restore. Everyone else is left alone. Deleted accounts come back too."),
    b.kind === "full" ? h("label", { class: "radio" }, all, " All users in this backup") : null,
    h("div", { style: "max-height:220px;overflow:auto" }, ...picks.map(({ u, c }) => h("label", { class: "radio" }, c, `${u.display_name} (${u.username})`))),
    h("h3", {}, "How"),
    h("label", { class: "radio" }, merge, " Merge: add what is missing (messages, files, chats); keep everything else"),
    h("label", { class: "radio" }, replace, " Replace: make their copy match the backup exactly (newer messages are hidden for them; others keep theirs)")), [{
    label: "Restore", primary: true, action: async (close) => {
      const ids = picks.filter((p) => p.c.checked).map((p) => p.u.id);
      if (!all.checked && ids.length === 0) throw new Error("Choose at least one person.");
      const r = await api<RestoreResponse>("POST", "/admin/restore", { backup_id: b.id, user_ids: ids, all: all.checked, mode: replace.checked ? "replace" : "merge" });
      close(); toast(summary(r)); reload();
    },
  }]);
}

const summary = (r: RestoreResponse) => `Restored ${r.users_restored} people: ${r.messages_added} messages added, ${r.messages_unhidden} un-hidden, ${r.messages_hidden} hidden, ${r.attachments_moved_back} files moved back.` + (r.warnings.length ? " Warnings: " + r.warnings.join("; ") : "");

/** First-run alternative: restore a whole server from an encrypted full backup. */
export function restoreSetupForm(onDone: () => void): HTMLElement {
  const sf = storageForm();
  const key = h("input", { placeholder: "empty = newest (backups/full/latest.json)" });
  const pass = h("input", { type: "password", autocomplete: "off", "aria-label": "Backup passphrase" });
  const status = h("p", { class: "muted", role: "status" });
  return h("div", {},
    h("p", { class: "muted" }, "Choose where the backups are stored (the same place you set under Backups > Full backup on the old server), enter the passphrase, and everything comes back: accounts, chats, settings. Reconnecting to the attachment storage is automatic."),
    h("h3", {}, "Where are the backups?"), sf.el,
    field("Backup file (optional)", key), field("Passphrase", pass), status,
    h("div", { class: "row", style: "margin-top:12px" }, h("button", { class: "primary", id: "do-restore", onclick: async (ev: Event) => {
      const b = ev.currentTarget as HTMLButtonElement; b.disabled = true; status.textContent = "Restoring…";
      try {
        const res = await fetch("/api/v1/restore", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ target: sf.value(), key: key.value || null, passphrase: pass.value }) });
        const j = await res.json();
        if (!res.ok) throw new Error(j.error?.message ?? "restore failed");
        status.textContent = `Restored ${j.users} accounts, ${j.conversations} conversations, ${j.messages} messages. ${j.attachments_resolvable} of ${j.attachments} files found in storage.`;
        setTimeout(onDone, 1500);
      } catch (e: any) { status.textContent = e.message; b.disabled = false; }
    } }, "Restore this server")));
}
