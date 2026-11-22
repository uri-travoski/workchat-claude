// Chat viewer (admin): browse every conversation like a normal chat app, with search, date filters,
// bulk delete (always backed up first) and normal chatting in conversations the admin belongs to.
import { api } from "./api";
import type { AdminDeleteMessagesResponse, AdminUser, Attachment, Form, Message, ScheduledMessage, SearchHit, ViewerConversation, ViewerHistory } from "./api-types";
import { dialog, fmtBytes, h, newKey, toast } from "./dom";

const PALETTE = ["#d1495b", "#00798c", "#edae49", "#66a182", "#8d6a9f", "#2e4057", "#c06c84", "#4f772d"];
const colorFor = (id: string) => PALETTE[[...id].reduce((a, c) => a + c.charCodeAt(0), 0) % PALETTE.length];
const initials = (n: string) => n.split(/\s+/).filter(Boolean).slice(0, 2).map((p) => p[0].toUpperCase()).join("") || "?";

function dayLabel(ms: number): string {
  return new Date(ms).toLocaleDateString("en-AU", { weekday: "long", day: "numeric", month: "long", year: "numeric" });
}
function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString("en-AU", { hour: "numeric", minute: "2-digit", hour12: true }).replace(/\s/g, " ");
}
const dayKey = (ms: number) => new Date(ms).toDateString();
/** Local start of the given yyyy-mm-dd day. */
const startOfDay = (v: string) => new Date(v + "T00:00:00").getTime();
const endOfDay = (v: string) => new Date(v + "T23:59:59.999").getTime();

function avatar(name: string, id: string, size = 36): HTMLElement {
  return h("div", { class: "av", style: `width:${size}px;height:${size}px;background:${colorFor(id)};font-size:${Math.round(size * 0.4)}px` }, initials(name));
}

function linkify(text: string): (Node | string)[] {
  const out: (Node | string)[] = [];
  const re = /(https?:\/\/[^\s<]+)/g;
  let last = 0, m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) out.push(text.slice(last, m.index));
    out.push(h("a", { href: m[1], target: "_blank", rel: "noopener noreferrer" }, m[1]));
    last = m.index + m[1].length;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

async function fileUrl(id: string): Promise<string> {
  const r = await api<{ url: string }>("GET", `/files/${id}/url`);
  return r.url;
}

function attachmentView(a: Attachment, selectable: boolean, onRemoved: () => void): HTMLElement {
  if (a.removed) return h("div", { class: "file-chip muted" }, "File removed");
  const box = h("div", { class: "att" });
  const ct = a.content_type || "";
  const chip = (href?: string) => h("a", { class: "file-chip", href: href ?? "#", target: "_blank", rel: "noopener noreferrer", download: a.name }, h("span", { class: "ficon" }, "FILE"), h("span", {}, a.name), h("span", { class: "muted" }, fmtBytes(a.size)));
  if (ct.startsWith("image/") || ct.startsWith("video/") || ct.startsWith("audio/")) {
    fileUrl(a.id).then((url) => {
      const media = ct.startsWith("image/") ? h("img", { src: url, alt: a.name, loading: "lazy", class: "media" })
        : ct.startsWith("video/") ? h("video", { src: url, controls: true, preload: "metadata", class: "media" })
          : h("audio", { src: url, controls: true });
      box.replaceChildren(media, h("div", { class: "muted small" }, a.name + " (" + fmtBytes(a.size) + ")"));
    }).catch(() => box.replaceChildren(chip()));
  } else {
    fileUrl(a.id).then((url) => box.replaceChildren(chip(url))).catch(() => box.replaceChildren(chip()));
    box.append(chip());
  }
  if (selectable) {
    box.append(h("button", { class: "tiny danger", title: "Remove this file for everyone (the message stays)", onclick: async () => {
      if (!confirm(`Remove the file "${a.name}" for everyone? The message stays and shows "File removed".`)) return;
      try { await api("DELETE", `/admin/files/${a.id}`); toast("File removed"); onRemoved(); } catch (e: any) { toast(e.message, true); }
    } }, "Remove file"));
  }
  return box;
}

function formCard(f: Form, responded: number, userName: (id: string) => string, who: string[]): HTMLElement {
  const box = h("div", { class: "formcard", "data-form": f.id });
  if (f.title) box.append(h("b", {}, f.title));
  if (f.description) box.append(h("div", { class: "muted small" }, f.description));
  for (const fld of f.fields ?? []) box.append(h("div", { class: "small" }, `${fld.label || fld.id}${fld.required ? " *" : ""}: `, h("span", { class: "muted" }, fld.type + (fld.options?.length ? " (" + fld.options.map((o) => o.label || o.value).join(", ") + ")" : ""))));
  const btns = [...(f.fields?.length ? [f.submit_label || "Submit"] : []), ...(f.buttons ?? []).map((b) => b.label)];
  if (btns.length) box.append(h("div", { class: "btns" }, ...btns.map((b) => h("span", { class: "fakebtn" }, b))));
  box.append(h("div", { class: "muted tiny" }, responded ? `Answered by ${who.map(userName).join(", ")}` : "No answers yet"));
  return box;
}

export async function chatsPage(el: HTMLElement): Promise<void> {
  const [users, me] = await Promise.all([api<AdminUser[]>("GET", "/admin/users"), api<{ user: { id: string; display_name: string } }>("GET", "/me")]);
  const userName = (id: string) => users.find((u) => u.id === id)?.display_name ?? "Unknown";
  const myId = me.user.id;

  const st = {
    as: "" as string, kind: "" as string, q: "",
    convs: [] as ViewerConversation[],
    cur: null as ViewerConversation | null,
    msgs: [] as Message[], replyCounts: {} as Record<string, number>,
    hasBefore: false, hasAfter: false,
    from: "", to: "",
    selectMode: false, picked: new Set<string>(),
    thread: null as Message | null, threadMsgs: [] as Message[],
    results: null as SearchHit[] | null, searchQ: "",
    highlight: "" as string,
  };

  const root = h("div", { class: "viewer" });
  el.replaceChildren(root);
  document.body.classList.add("viewer-page");

  // ---------------------------------------------------------------- sidebar
  const asSel = h("select", { "aria-label": "View as", onchange: () => { st.as = asSel.value; st.cur = null; loadConvs(); drawMain(); } },
    h("option", { value: "" }, "Everyone (all conversations)"),
    ...users.filter((u) => !u.deleted && !u.is_bot).map((u) => h("option", { value: u.id }, `${u.display_name} (${u.username})`)));
  const kindSel = h("select", { "aria-label": "Conversation kind", onchange: () => { st.kind = kindSel.value; loadConvs(); } },
    h("option", { value: "" }, "Channels and groups"), h("option", { value: "channel" }, "Channels"), h("option", { value: "group_dm" }, "Group DMs"), h("option", { value: "dm" }, "Direct messages"), h("option", { value: "self" }, "Notes to self"));
  const filter = h("input", { type: "search", placeholder: "Filter conversations", "aria-label": "Filter conversations", oninput: () => { st.q = filter.value; loadConvs(); } });
  const list = h("div", { class: "convlist", role: "list" });
  const side = h("aside", { class: "side" }, h("div", { class: "side-head" }, h("label", {}, "Show conversations of"), asSel, kindSel, filter), list);

  async function loadConvs() {
    const p = new URLSearchParams();
    if (st.as) p.set("user_id", st.as);
    else if (st.kind) p.set("kind", st.kind);
    if (st.as && st.kind) { /* kind filter applies client-side when viewing as a user */ }
    if (st.q) p.set("q", st.q);
    let rows = await api<ViewerConversation[]>("GET", "/admin/viewer/conversations?" + p);
    if (st.as && st.kind) rows = rows.filter((c) => c.kind === st.kind);
    st.convs = rows;
    list.replaceChildren(...rows.map((c) => {
      const last = c.last_message;
      const ts = last?.ts ?? c.created_ms;
      return h("button", { class: "conv" + (st.cur?.id === c.id ? " on" : ""), role: "listitem", "data-conv": c.id, onclick: () => openConv(c) },
        avatar(c.title, c.id, 40),
        h("div", { class: "conv-text" },
          h("div", { class: "row between" }, h("b", { class: "ellipsis" }, (c.kind === "channel" ? "# " : "") + c.title), h("span", { class: "muted small" }, new Date(ts).toLocaleDateString("en-AU", { day: "numeric", month: "short" }))),
          h("div", { class: "muted small ellipsis" }, last ? (last.deleted ? "Message deleted" : (last.body || (last.attachments?.length ? "Attachment" : ""))) : "No messages"),
          h("div", { class: "muted tiny" }, `${c.message_count} messages`)));
    }));
    if (rows.length === 0) list.append(h("p", { class: "muted pad" }, "No conversations."));
  }

  // ---------------------------------------------------------------- main
  const main = h("section", { class: "main" });
  const threadPane = h("aside", { class: "thread", style: "display:none" });
  root.append(side, main, threadPane);

  const canWrite = () => !!st.cur && st.cur.members.some((m) => m.user_id === myId) && !st.cur.archived;
  const perspective = () => st.as || myId;

  function rangeQuery(): string {
    const p = new URLSearchParams();
    if (st.from) p.set("from_ms", String(startOfDay(st.from)));
    if (st.to) p.set("to_ms", String(endOfDay(st.to)));
    return p.toString();
  }

  async function openConv(c: ViewerConversation, aroundId?: string) {
    st.cur = c; st.picked.clear(); st.thread = null; threadPane.style.display = "none"; st.results = null; st.highlight = aroundId ?? "";
    for (const b of list.querySelectorAll(".conv")) b.classList.toggle("on", (b as HTMLElement).dataset.conv === c.id);
    const p = new URLSearchParams(rangeQuery());
    p.set("limit", "50"); p.set("top_level", "true");
    if (st.as) p.set("as_user", st.as);
    if (aroundId) p.set("around_id", aroundId);
    const h2 = await api<ViewerHistory>("GET", `/admin/viewer/conversations/${c.id}/messages?` + p);
    st.msgs = h2.messages; st.replyCounts = h2.reply_counts; st.hasBefore = h2.has_more_before; st.hasAfter = h2.has_more_after;
    drawMain(true);
  }

  async function reloadCur(keepScroll = false) {
    if (!st.cur) return;
    const fresh = (await api<ViewerConversation[]>("GET", "/admin/viewer/conversations?" + (st.as ? "user_id=" + st.as : "kind=" + (st.cur.kind)))).find((c) => c.id === st.cur!.id);
    if (fresh) st.cur = fresh;
    const p = new URLSearchParams(rangeQuery());
    p.set("limit", String(Math.max(50, st.msgs.length))); p.set("top_level", "true");
    if (st.as) p.set("as_user", st.as);
    const h2 = await api<ViewerHistory>("GET", `/admin/viewer/conversations/${st.cur.id}/messages?` + p);
    st.msgs = h2.messages; st.replyCounts = h2.reply_counts; st.hasBefore = h2.has_more_before; st.hasAfter = h2.has_more_after;
    drawMain(!keepScroll); loadConvs();
    if (st.thread) openThread(st.thread);
  }

  async function olderPage() {
    if (!st.cur || !st.msgs.length) return;
    const p = new URLSearchParams(rangeQuery());
    p.set("limit", "50"); p.set("top_level", "true"); p.set("before", String(st.msgs[0].seq));
    if (st.as) p.set("as_user", st.as);
    const h2 = await api<ViewerHistory>("GET", `/admin/viewer/conversations/${st.cur.id}/messages?` + p);
    st.msgs = [...h2.messages, ...st.msgs]; st.hasBefore = h2.has_more_before;
    Object.assign(st.replyCounts, h2.reply_counts);
    drawMain(false, true);
  }

  function messageRow(m: Message, inThread = false): HTMLElement {
    const mine = m.sender_id === perspective();
    const name = userName(m.sender_id);
    const bubble = h("div", { class: "bubble" + (mine ? " mine" : "") + (m.deleted ? " gone" : "") + (st.highlight === m.id ? " hl" : "") });
    const isApp = users.find((u) => u.id === m.sender_id)?.is_bot;
    if ((!mine && st.cur && st.cur.kind !== "dm") || isApp) bubble.append(h("div", { class: "who", style: `color:${colorFor(m.sender_id)}` }, name, isApp ? h("span", { class: "app-tag", title: "Sent by a bot or webhook" }, "APP") : null));
    if (m.forwarded) bubble.append(h("div", { class: "muted small" }, "Forwarded from " + userName(m.forwarded.sender_id)));
    if (m.deleted) {
      bubble.append(h("div", { class: "muted" }, "Message deleted"));
    } else {
      if (m.reply_to) {
        const q = st.msgs.find((x) => x.id === m.reply_to);
        bubble.append(h("div", { class: "quote" }, q ? userName(q.sender_id) + ": " + q.body.slice(0, 120) : "Replying to an earlier message"));
      }
      if (m.body) bubble.append(h("div", { class: "text" }, ...linkify(m.body)));
      if (m.form) bubble.append(formCard(m.form, m.responded?.length ?? 0, userName, m.responded ?? []));
      for (const a of m.attachments ?? []) bubble.append(attachmentView(a, st.selectMode, () => reloadCur(true)));
    }
    bubble.append(h("div", { class: "meta muted tiny" }, clock(m.ts) + (m.version > 1 && !m.deleted ? " (edited)" : "")));
    if (m.reactions?.length) bubble.append(h("div", { class: "chips" }, ...m.reactions.map((r) => h("span", { class: "chip" }, r.emoji + " " + r.user_ids.length))));
    const rc = st.replyCounts[m.id];
    if (!inThread && rc) bubble.append(h("button", { class: "linkish", onclick: () => openThread(m) }, `${rc} ${rc === 1 ? "reply" : "replies"}`));
    const actions = h("div", { class: "msg-actions" });
    if (!m.deleted && canWrite()) {
      actions.append(h("button", { class: "tiny", title: "React", "aria-label": "React with thumbs up", onclick: () => react(m) }, "\u{1F44D}"));
      if (!inThread) actions.append(h("button", { class: "tiny", onclick: () => openThread(m) }, "Thread"));
      if (m.sender_id === myId) actions.append(h("button", { class: "tiny", onclick: () => editDialog(m) }, "Edit"));
    }
    if (!m.deleted) actions.append(h("button", { class: "tiny danger", onclick: () => deleteDialog([m.id]) }, "Delete"));
    bubble.append(actions);
    const checkbox = st.selectMode ? h("input", { type: "checkbox", class: "pick", "aria-label": "Select message", "data-msg": m.id, checked: st.picked.has(m.id), onchange: (e: Event) => {
      if ((e.target as HTMLInputElement).checked) st.picked.add(m.id); else st.picked.delete(m.id); updatePickBar();
    } }) : null;
    return h("div", { class: "msg" + (mine ? " right" : ""), "data-msg": m.id },
      checkbox, mine ? null : avatar(name, m.sender_id, 32), bubble, mine ? avatar(name, m.sender_id, 32) : null);
  }

  let pickBar: HTMLElement;
  function updatePickBar() {
    if (!pickBar) return;
    const n = st.picked.size;
    pickBar.style.display = st.selectMode ? "flex" : "none";
    pickBar.querySelector(".count")!.textContent = `${n} selected`;
    (pickBar.querySelector(".do-delete") as HTMLButtonElement).disabled = n === 0;
  }

  function drawMain(scrollBottom = false, keepPos = false) {
    const prevScroll = main.querySelector(".timeline") as HTMLElement | null;
    const oldH = prevScroll?.scrollHeight ?? 0, oldTop = prevScroll?.scrollTop ?? 0;
    root.classList.toggle("conv-open", !!st.cur);
    if (!st.cur) {
      main.replaceChildren(h("div", { class: "empty" }, h("h2", {}, "Chat viewer"), h("p", { class: "muted" }, "Pick a person or channel on the left, then a conversation. You can read everything, search, and delete messages (a backup is always written first).")));
      return;
    }
    const c = st.cur;
    const members = c.members.map((m) => userName(m.user_id)).join(", ");
    const searchBox = h("input", { type: "search", placeholder: "Search messages", "aria-label": "Search messages", value: st.searchQ, onkeydown: (e: KeyboardEvent) => { if (e.key === "Enter") runSearch(searchBox.value, true); } });
    const from = h("input", { type: "date", "aria-label": "From date", value: st.from });
    const to = h("input", { type: "date", "aria-label": "To date", value: st.to });
    const applyRange = () => { st.from = from.value; st.to = to.value; openConv(c); };
    pickBar = h("div", { class: "pickbar", style: "display:none" },
      h("span", { class: "count" }, "0 selected"),
      h("button", { onclick: () => { for (const m of st.msgs) if (!m.deleted) st.picked.add(m.id); drawMain(false, true); } }, "Select all loaded"),
      h("button", { onclick: () => { st.picked.clear(); drawMain(false, true); } }, "Clear"),
      h("button", { class: "danger do-delete", onclick: () => deleteDialog([...st.picked]) }, "Delete selected…"));
    const header = h("div", { class: "main-head" },
      h("button", { class: "back", id: "viewer-back", "aria-label": "Back to the conversation list", onclick: () => { st.cur = null; st.thread = null; threadPane.style.display = "none"; for (const b of list.querySelectorAll(".conv")) b.classList.remove("on"); drawMain(); } }, "← Back"),
      avatar(c.title, c.id, 40),
      h("div", { class: "grow" }, h("b", {}, (c.kind === "channel" ? "# " : "") + c.title), h("div", { class: "muted small ellipsis", title: members }, `${c.kind === "channel" ? "Channel" : c.kind === "dm" ? "Direct message" : c.kind === "group_dm" ? "Group" : "Notes to self"}  ·  ${members}`)),
      h("button", { class: st.selectMode ? "primary" : "", onclick: () => { st.selectMode = !st.selectMode; st.picked.clear(); drawMain(false, true); } }, st.selectMode ? "Done selecting" : "Select"));
    const tools = h("div", { class: "tools" },
      searchBox, h("button", { onclick: () => runSearch(searchBox.value, true) }, "Search"),
      h("label", { class: "inline" }, "From", from), h("label", { class: "inline" }, "To", to),
      h("button", { onclick: applyRange }, "Filter"),
      (st.from || st.to) ? h("button", { onclick: () => { st.from = ""; st.to = ""; openConv(c); } }, "Clear dates") : null,
      (st.from || st.to) ? h("button", { class: "danger", onclick: () => deleteRangeDialog() }, "Delete this range…") : null);
    const body = h("div", { class: "timeline", tabindex: "0", "aria-label": "Messages" });
    if (st.results) {
      body.append(h("div", { class: "results-head row between" }, h("b", {}, `${st.results.length} result${st.results.length === 1 ? "" : "s"} for "${st.searchQ}"`), h("button", { onclick: () => { st.results = null; drawMain(true); } }, "Back to conversation")));
      for (const r of st.results) {
        const conv = st.convs.find((x) => x.id === r.message.conv_id);
        body.append(h("button", { class: "result", "data-result": r.message.id, onclick: async () => { const target = conv ?? st.cur!; await openConv(target, r.message.id); } },
          h("div", { class: "muted small" }, `${conv?.title ?? "Conversation"} · ${userName(r.message.sender_id)} · ${dayLabel(r.message.ts)} ${clock(r.message.ts)}`),
          h("div", {}, ...snippet(r.snippet))));
      }
      if (st.results.length === 0) body.append(h("p", { class: "muted pad" }, "No matches."));
    } else {
      if (st.hasBefore) body.append(h("div", { class: "center" }, h("button", { onclick: olderPage }, "Load older messages")));
      let lastDay = "";
      for (const m of st.msgs) {
        const k = dayKey(m.ts);
        if (k !== lastDay) { lastDay = k; body.append(h("div", { class: "day" }, h("span", {}, dayLabel(m.ts)))); }
        body.append(messageRow(m));
      }
      if (!st.msgs.length) body.append(h("p", { class: "muted pad" }, st.from || st.to ? "No messages in that date range." : "No messages."));
      if (st.hasAfter) body.append(h("div", { class: "center" }, h("button", { onclick: () => { st.from = ""; st.to = ""; openConv(c); } }, "Jump to newest")));
    }
    const schedBox = h("div", { class: "scheduled-box", style: "display:none", id: "scheduled-box" });
    main.replaceChildren(header, tools, pickBar, body, schedBox, composer(false));
    updatePickBar();
    api<ScheduledMessage[]>("GET", "/admin/scheduled?conv_id=" + c.id).then((items) => {
      const shown = items.filter((i) => i.state === "pending" || i.state === "failed");
      if (!shown.length || st.cur?.id !== c.id) return;
      schedBox.style.display = "";
      schedBox.replaceChildren(h("b", {}, "Scheduled messages"), ...shown.map((i) => h("div", { class: "sched", "data-sched": i.id },
        h("span", { class: "muted small" }, (i.state === "failed" ? "Failed: " : "Sends ") + dayLabel(i.due_ms) + " " + clock(i.due_ms) + " as " + userName(i.user_id)), h("span", {}, i.body.slice(0, 160)), i.error ? h("span", { class: "pill bad" }, i.error) : null)));
    }).catch(() => {});
    if (scrollBottom) {
      const hl = st.highlight ? body.querySelector(`[data-msg="${st.highlight}"]`) : null;
      if (hl) (hl as HTMLElement).scrollIntoView({ block: "center" }); else body.scrollTop = body.scrollHeight;
    } else if (keepPos) body.scrollTop = body.scrollHeight - oldH + oldTop;
  }

  function snippet(s: string): (Node | string)[] {
    const out: (Node | string)[] = [];
    for (const part of s.split(/(\[\[.*?\]\])/)) out.push(part.startsWith("[[") ? h("mark", {}, part.slice(2, -2)) : part);
    return out;
  }

  async function runSearch(q: string, inConv: boolean) {
    st.searchQ = q.trim();
    if (!st.searchQ) { st.results = null; drawMain(true); return; }
    const p = new URLSearchParams({ q: st.searchQ, limit: "100" });
    if (st.as) p.set("user_id", st.as);
    if (inConv && st.cur) p.set("conv_id", st.cur.id);
    if (st.from) p.set("from_ms", String(startOfDay(st.from)));
    if (st.to) p.set("to_ms", String(endOfDay(st.to)));
    st.results = await api<SearchHit[]>("GET", "/admin/viewer/search?" + p);
    drawMain(false);
  }

  // ---------------------------------------------------------------- composer and message actions
  function composer(inThreadOf: boolean): HTMLElement {
    if (!st.cur || !canWrite()) return h("div", { class: "composer ro muted" }, st.cur?.archived ? "This conversation is archived." : "You are not a member of this conversation, so you can read it but not write. Add yourself under Conversations > Members to chat.");
    const box = h("textarea", { rows: "1", placeholder: inThreadOf ? "Reply in thread" : "Message " + (st.cur.kind === "channel" ? "#" : "") + st.cur.title, "aria-label": inThreadOf ? "Thread reply" : "Message", class: inThreadOf ? "thread-input" : "main-input" });
    const send = async () => {
      const body = box.value.trim();
      if (!body) return;
      box.disabled = true;
      try {
        await api("POST", `/conversations/${st.cur!.id}/messages`, { client_id: newKey(), body, thread_root: inThreadOf ? st.thread?.id : null, mentions: [], attachment_ids: [] });
        box.value = "";
        await reloadCur(inThreadOf);
        if (!inThreadOf) { const t = main.querySelector(".timeline") as HTMLElement; t.scrollTop = t.scrollHeight; }
      } catch (e: any) { toast(e.message, true); } finally { box.disabled = false; box.focus(); }
    };
    box.addEventListener("keydown", (e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); send(); } });
    return h("div", { class: "composer" }, box, h("button", { class: "primary send", onclick: send }, "Send"));
  }

  async function react(m: Message) {
    const mineAlready = m.reactions?.some((r) => r.emoji === "\u{1F44D}" && r.user_ids.includes(myId));
    try { await api(mineAlready ? "DELETE" : "PUT", `/messages/${m.id}/reactions/${encodeURIComponent("\u{1F44D}")}`); await reloadCur(true); } catch (e: any) { toast(e.message, true); }
  }

  function editDialog(m: Message) {
    const t = h("textarea", { rows: "4" }); t.value = m.body;
    dialog("Edit message", h("div", {}, t), [{ label: "Save", primary: true, action: async (close) => { await api("PATCH", `/messages/${m.id}`, { body: t.value, mentions: [] }); close(); await reloadCur(true); } }]);
  }

  async function openThread(root: Message) {
    st.thread = root;
    const p = new URLSearchParams({ thread_root: root.id, limit: "200" });
    if (st.as) p.set("as_user", st.as);
    const hist = await api<ViewerHistory>("GET", `/admin/viewer/conversations/${root.conv_id}/messages?` + p);
    st.threadMsgs = hist.messages;
    threadPane.style.display = "flex";
    threadPane.replaceChildren(
      h("div", { class: "main-head" }, h("b", { class: "grow" }, "Thread"), h("button", { onclick: () => { st.thread = null; threadPane.style.display = "none"; } }, "Close")),
      h("div", { class: "timeline" }, messageRow(root, true), h("div", { class: "day" }, h("span", {}, `${hist.messages.length} ${hist.messages.length === 1 ? "reply" : "replies"}`)), ...hist.messages.map((m) => messageRow(m, true))),
      composer(true));
  }

  // ---------------------------------------------------------------- deleting
  function memberOptions(): HTMLElement[] {
    return (st.cur?.members ?? []).map((m) => h("option", { value: m.user_id }, userName(m.user_id)));
  }

  function scopeControls() {
    const everyone = h("input", { type: "radio", name: "scope", value: "everyone", checked: true, style: "width:auto" });
    const one = h("input", { type: "radio", name: "scope", value: "user", style: "width:auto" });
    const who = h("select", { "aria-label": "Person" }, ...memberOptions());
    if (st.as) who.value = st.as;
    const el = h("div", {},
      h("label", { class: "radio" }, everyone, " Delete for everyone (a tombstone \"Message deleted\" remains; files move into the backup folder)"),
      h("label", { class: "radio" }, one, " Delete only for one person (everyone else keeps it)"), who);
    return { el, scope: () => (one.checked ? "user" : "everyone"), user: () => who.value };
  }

  function deleteDialog(ids: string[]) {
    const sc = scopeControls();
    dialog(`Delete ${ids.length} message${ids.length === 1 ? "" : "s"}`, h("div", {},
      h("p", {}, "A backup of these messages and their files is written first. If the backup fails nothing is deleted. You can restore from Backups afterwards."), sc.el), [{
      label: "Back up and delete", danger: true, action: async (close) => {
        const r = await api<AdminDeleteMessagesResponse>("POST", "/admin/messages/delete", { message_ids: ids, scope: sc.scope(), user_id: sc.scope() === "user" ? sc.user() : null });
        close(); toast(`Deleted ${r.messages_deleted}. Backup ${r.backup_id}`); st.picked.clear(); await reloadCur(true);
      },
    }]);
  }

  function deleteRangeDialog() {
    if (!st.cur) return;
    const c = st.cur;
    const sc = scopeControls();
    const desc = st.from && st.to ? `between ${st.from} and ${st.to}` : st.to ? `before the end of ${st.to}` : `from ${st.from} onward`;
    dialog("Delete a date range", h("div", {}, h("p", {}, `Deletes every message in ${c.title} ${desc}. A backup is written first.`), sc.el), [{
      label: "Back up and delete", danger: true, action: async (close) => {
        const body: any = { conv_id: c.id, scope: sc.scope(), user_id: sc.scope() === "user" ? sc.user() : null };
        if (st.from) body.from_ms = startOfDay(st.from); else body.from_ms = 0;
        if (st.to) body.to_ms = endOfDay(st.to); else body.to_ms = Date.now() + 86400000;
        if (!st.from && st.to) { delete body.from_ms; delete body.to_ms; body.before_ms = endOfDay(st.to) + 1; }
        const r = await api<AdminDeleteMessagesResponse>("POST", "/admin/messages/delete", body);
        close(); toast(`Deleted ${r.messages_deleted}. Backup ${r.backup_id}`); await reloadCur(true);
      },
    }]);
  }

  await loadConvs();
  drawMain();
}

export function leaveViewer() {
  document.body.classList.remove("viewer-page");
}
