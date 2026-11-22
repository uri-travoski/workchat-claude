// Admin pages for user groups (M-18, `@handle` mentions) and channel bookmarks (C-07).
import { api } from "./api";
import type { AdminConversation, AdminUser, Bookmark, UserGroup } from "./api-types";
import { dialog, field, h, toast } from "./dom";

type Fail = (e: unknown) => void;

export async function groupsPage(el: HTMLElement, reload: () => void, fail: Fail): Promise<void> {
  const [groups, users] = await Promise.all([api<UserGroup[]>("GET", "/groups"), api<AdminUser[]>("GET", "/admin/users")]);
  const live = users.filter((u) => !u.deleted);
  const name = (id: string) => users.find((u) => u.id === id)?.display_name ?? id.slice(0, 6);
  const run = (f: () => Promise<any>, ok?: string) => async () => { try { await f(); if (ok) toast(ok); reload(); } catch (e) { fail(e); } };
  const edit = (g: UserGroup | null) => {
    const handle = h("input", { value: g?.handle ?? "", placeholder: "oncall", "aria-label": "Group handle" });
    const gname = h("input", { value: g?.name ?? "", placeholder: "On-call team", "aria-label": "Group name" });
    const boxes = live.map((u) => ({ u, i: h("input", { type: "checkbox", style: "width:auto", checked: !!g?.member_ids.includes(u.id) }) }));
    dialog(g ? "Edit group" : "Create group", h("div", {}, field("Handle (people type @handle)", handle), field("Name", gname), h("h3", {}, "Members"),
      h("div", { style: "max-height:240px;overflow:auto" }, ...boxes.map(({ u, i }) => h("label", { style: "display:flex;gap:8px;align-items:center;color:var(--text)" }, i, u.display_name + " (" + u.username + ")")))), [{
      label: "Save", primary: true, action: async (close) => {
        const body = { handle: handle.value.trim(), name: gname.value.trim() || handle.value.trim(), member_ids: boxes.filter((b) => b.i.checked).map((b) => b.u.id) };
        if (g) await api("PATCH", `/admin/groups/${g.id}`, body); else await api("POST", "/admin/groups", body);
        close(); reload();
      },
    }]);
  };
  el.append(h("div", { class: "card", id: "groups" },
    h("div", { class: "row" }, h("h2", { style: "margin:0;flex:1" }, "User groups"), h("button", { class: "primary", id: "new-group", onclick: () => edit(null) }, "Create group")),
    h("p", { class: "muted" }, "Typing @handle in a message mentions every member of the group."),
    groups.length === 0 ? h("p", { class: "muted" }, "No groups yet.") : h("table", {}, h("thead", {}, h("tr", {}, ...["Handle", "Name", "Members", ""].map((x) => h("th", {}, x)))),
      h("tbody", {}, ...groups.map((g) => h("tr", { "data-group": g.handle },
        h("td", {}, h("b", {}, "@" + g.handle)), h("td", {}, g.name), h("td", {}, g.member_ids.map(name).join(", ") || "nobody"),
        h("td", { class: "row" }, h("button", { onclick: () => edit(g) }, "Edit"),
          h("button", { class: "danger", onclick: () => { if (confirm(`Delete the group @${g.handle}?`)) run(() => api("DELETE", `/admin/groups/${g.id}`), "Group deleted")(); } }, "Delete"))))))));
}

export async function bookmarksPage(el: HTMLElement, reload: () => void, fail: Fail): Promise<void> {
  const convs = (await api<AdminConversation[]>("GET", "/admin/conversations")).filter((c) => !c.archived);
  const sel = h("select", { id: "bm-conv", "aria-label": "Conversation" }, ...convs.map((c) => h("option", { value: c.id }, (c.kind === "channel" ? "# " : "") + (c.name ?? "(unnamed)"))));
  const list = h("div", { id: "bm-list" });
  const load = async () => {
    list.replaceChildren();
    if (!sel.value) { list.append(h("p", { class: "muted" }, "No channels yet.")); return; }
    try {
      const items = await api<Bookmark[]>("GET", `/conversations/${sel.value}/bookmarks`);
      list.append(items.length === 0 ? h("p", { class: "muted" }, "No bookmarks in this conversation.") : h("table", {}, h("thead", {}, h("tr", {}, ...["Title", "Link", ""].map((x) => h("th", {}, x)))),
        h("tbody", {}, ...items.map((b) => h("tr", { "data-bookmark": b.title }, h("td", {}, h("b", {}, b.title)), h("td", { class: "mono small" }, b.url ?? (b.attachment_id ? "file" : "")),
          h("td", { class: "row" },
            h("button", { onclick: () => form(b) }, "Edit"),
            h("button", { class: "danger", onclick: async () => { try { await api("DELETE", `/bookmarks/${b.id}`); toast("Removed"); load(); } catch (e) { fail(e); } } }, "Remove")))))));
    } catch (e) { fail(e); }
  };
  const form = (b: Bookmark | null) => {
    const title = h("input", { value: b?.title ?? "", placeholder: "Runbook", "aria-label": "Bookmark title" });
    const url = h("input", { value: b?.url ?? "", placeholder: "https://...", "aria-label": "Bookmark link" });
    dialog(b ? "Edit bookmark" : "Add bookmark", h("div", {}, field("Title", title), field("Link", url)), [{
      label: "Save", primary: true, action: async (close) => {
        const body = { title: title.value.trim(), url: url.value.trim() || null };
        if (b) await api("PATCH", `/bookmarks/${b.id}`, body); else await api("POST", `/conversations/${sel.value}/bookmarks`, body);
        close(); load();
      },
    }]);
  };
  sel.addEventListener("change", load);
  el.append(h("div", { class: "card", id: "bookmarks" },
    h("div", { class: "row" }, h("h2", { style: "margin:0;flex:1" }, "Bookmarks"), sel, h("button", { class: "primary", id: "new-bookmark", onclick: () => form(null) }, "Add bookmark")),
    h("p", { class: "muted" }, "Links shown in a bar above the conversation (up to 50 per conversation)."), list));
  await load();
  void reload;
}
