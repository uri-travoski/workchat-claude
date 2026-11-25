// Reactive app state shared by every component. The Rust core owns the data; this mirrors what the UI needs
// and keeps it current from core events.
import { core, errorMessage, onCoreEvent, type AudioState, type LockStatus, type OrgSettings, type UpdateInfo, type ChatRow, type ConnState, type Counts, type CoreEvent, type FileHit, type Label, type LocalAvatar, type MessageView, type Me, type Staged, type ThreadSummary, type UploadInfo, type User } from "./core";

export type Tab = "chats" | "contacts" | "settings";
/** Settings pages. On a phone "" is the list; on desktop "profile" is shown first. */
export type SettingsPage = "" | "profile" | "account" | "saved" | "starred" | "labels" | "notifications" | "privacy" | "storage" | "appearance" | "prefs" | "server" | "devices" | "about";

/** The PIN keypad: what it is for, and what happens after a correct PIN. */
export interface PadState {
  mode: "set" | "unlock" | "change";
  title?: string;
  then?: () => void | Promise<void>;
}
export type Filter = "all" | "dm" | "channel" | "threads" | "files" | "saved";
export type ChatSort = "recent" | "az" | "za";

/** One media item in the full-screen viewer. */
export interface ViewerItem {
  id: string;
  name: string;
  content_type: string;
  size: number;
  poster?: string;
  conv_id?: string;
  message_id?: string;
  ts?: number;
  sender_id?: string;
}

/** The one sheet / modal open at a time. */
export type Sheet =
  | { kind: "attach"; conv: string; thread: string | null }
  | { kind: "preview"; conv: string; thread: string | null; files: Staged[]; caption: string }
  | { kind: "forward"; msg: MessageView }
  | { kind: "shareto"; files: { path: string; name: string }[]; text: string }
  | { kind: "newchat"; step?: "menu" | "dm" | "group" | "channel" }
  | { kind: "profile"; userId: string }
  | { kind: "crop"; stagedId: string; target: { kind: "me" } | { kind: "conv"; id: string } | { kind: "local"; what: "user" | "conv"; id: string } | { kind: "newchat" } }
  | { kind: "pins"; conv: string }
  | { kind: "info"; msg: MessageView }
  | { kind: "msgmenu"; msg: MessageView; x: number; y: number; inThread: boolean; onedit?: (m: MessageView) => void }
  | { kind: "emoji"; msg: MessageView }
  | { kind: "labels"; msg: MessageView }
  | { kind: "chatmenu"; conv: string }
  | { kind: "lockedchats" }
  | { kind: "editprofile" }
  | { kind: "sharecard" }
  | { kind: "chatactions"; conv: string; x: number; y: number }
  | { kind: "channelinfo"; conv: string };

export interface ConvState {
  items: MessageView[];
  /** Showing a search hit in context: the list is not the latest page. */
  detached?: boolean;
  hasMore: boolean;
  loading: boolean;
  loaded: boolean;
  error: string;
}

export const app = $state({
  phase: "loading" as "loading" | "out" | "in",
  me: null as Me | null,
  server: "",
  conn: "offline" as ConnState,
  refreshing: false,
  /** Multi-select of messages in one chat (null when not selecting). */
  select: null as null | { conv: string; ids: string[] },
  connDetail: "",
  users: {} as Record<string, User>,
  rows: {} as Record<string, ChatRow>,
  threads: {} as Record<string, ThreadSummary>,
  tab: "chats" as Tab,
  filter: "all" as Filter,
  /** The chat list box: filters only the names of the chats showing in the list (separate from the top-bar search). */
  listq: "",
  searching: false,
  selected: null as string | null,
  thread: null as { conv: string; root: string } | null,
  files: [] as FileHit[],
  filesLoaded: false,
  filesError: "",
  typing: {} as Record<string, Record<string, number>>,
  toast: null as { text: string; kind: "info" | "error" } | null,
  signedOutNote: "",
  counts: null as Counts | null,
  org: { channel_creation: "everyone", channel_pins: "everyone" } as OrgSettings,
  saved: [] as string[],
  /** The person's own labels, and the label ids of each saved message. */
  labels: [] as Label[],
  savedLabels: {} as Record<string, string[]>,
  /** The Saved list shows only messages with this label (a label id). */
  labelFilter: null as string | null,
  pins: {} as Record<string, string[]>,
  localAvatars: {} as Record<string, string>,
  maxUpload: 100 * 1024 * 1024,
  uploads: {} as Record<string, UploadInfo>,
  audio: { id: "", state: "stopped" as AudioState | "stopped", pos: 0, dur: 0, speed: 1, error: "" },
  voice: { level: 0, elapsed: 0, levels: [] as number[] },
  viewer: null as { items: ViewerItem[]; index: number } | null,
  /** A file no application could open: the fallback dialog (Open with / Save as / Show in folder). */
  openFail: null as { id: string; name: string; reason: string } | null,
  sheet: null as Sheet | null,
  /** Global search (top bar) text and whether the results panel is open. */
  gsearch: "",
  focus: null as { conv: string; id: string } | null,
  /** The new channel / group being filled in (kept while the picture cropper is open). */
  chatDraft: { name: "", topic: "", members: [] as string[], avatarId: "" as string },
  lock: { pin_set: false, open: false, wait_secs: 0, attempts_left: 5, timeout_secs: 300, on_background: true } as LockStatus,
  pad: null as PadState | null,
  /** A login link opened from outside the app (deep link), waiting for the login screen. */
  pendingLink: "",
  info: { platform: "linux", arch: "x86_64", version: "1.2.0", portable: false, appimage: false, data_dir: "" },
  spage: "" as SettingsPage,
  /** Update checking: the latest version found, the last answer and the download progress. */
  upd: { found: null as string | null, checking: false, info: null as UpdateInfo | null, error: "", state: "" as "" | "downloading" | "ready" | "error" | "needs_permission" | "installing", got: 0, total: 0, message: "" },
  phone: typeof window !== "undefined" ? window.innerWidth < 760 : false,
  /** Phone: the slide-in menu (Files, People, Settings) is open. */
  drawer: false,
});

export const msgs = $state<Record<string, ConvState>>({});
export const threadMsgs = $state<Record<string, MessageView[]>>({});

// ---------------------------------------------------------------- preferences (this device only)

export interface Prefs {
  theme: "system" | "light" | "dark";
  accent: "navy" | "blue" | "slate" | "deep";
  bubbleRadius: number;
  textSize: number;
  /** Message text weight (400 regular, 500 medium, 600 semibold, 700 bold). */
  textWeight: number;
  sendWithReturn: boolean;
  /** Download photos in chats automatically (off: tap to download). */
  autoPhotos: boolean;
  /** Order of the unpinned chats: newest activity first, or by name. */
  chatSort: ChatSort;
}
const PREFS_KEY = "workchat.prefs";
const defaultPrefs: Prefs = { theme: "system", accent: "navy", bubbleRadius: 12, textSize: 16, textWeight: 400, sendWithReturn: false, autoPhotos: true, chatSort: "recent" };

function loadPrefs(): Prefs {
  try {
    return { ...defaultPrefs, ...JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}") };
  } catch {
    return { ...defaultPrefs };
  }
}
export const prefs = $state<Prefs>(loadPrefs());

export function applyPrefs() {
  const r = document.documentElement;
  if (prefs.theme === "system") r.removeAttribute("data-theme");
  else r.setAttribute("data-theme", prefs.theme);
  if (prefs.accent === "navy") r.removeAttribute("data-accent");
  else r.setAttribute("data-accent", prefs.accent);
  r.style.setProperty("--radius-bubble", `${prefs.bubbleRadius}px`);
  r.style.setProperty("--msg-size", `${prefs.textSize}px`);
  r.style.setProperty("--msg-weight", String(prefs.textWeight));
  // The phone's status bar follows the theme (a no-op elsewhere).
  setTimeout(() => barsHook?.(), 60);
}
let barsHook: (() => void) | undefined;
export function onThemeApplied(f: () => void) {
  barsHook = f;
}
export function savePrefs() {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
  } catch {
    /* private mode: the choice just lasts for this run */
  }
  applyPrefs();
}

// ---------------------------------------------------------------- toasts

let toastTimer: ReturnType<typeof setTimeout> | undefined;
export function toast(text: string, kind: "info" | "error" = "info") {
  app.toast = { text, kind };
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (app.toast = null), kind === "error" ? 5000 : 2600);
}
export const fail = (e: unknown) => toast(errorMessage(e), "error");

// ---------------------------------------------------------------- derived helpers

export const userName = (id: string) => app.users[id]?.display_name ?? "Someone";

const bySeq = (a: MessageView, b: MessageView) => {
  const pa = a.pending ? 1 : 0;
  const pb = b.pending ? 1 : 0;
  if (pa !== pb) return pa - pb;
  return pa ? a.ts - b.ts : a.seq - b.seq;
};

const emptyConv: ConvState = { items: [], hasMore: false, loading: false, loaded: false, error: "" };

/** Read-only view (safe inside $derived): an empty placeholder until the conversation is opened. */
export function convOf(id: string): ConvState {
  return msgs[id] ?? emptyConv;
}

/** Creates the state for a conversation on first use. Only call from actions, never from a derived value. */
export function ensureConv(id: string): ConvState {
  // Return the proxied entry: mutating the raw object that `??=` evaluates to would not be tracked.
  msgs[id] ??= { items: [], hasMore: false, loading: false, loaded: false, error: "" };
  return msgs[id];
}

function upsertInto(list: MessageView[], m: MessageView, replaces: string | null) {
  if (replaces) {
    const i = list.findIndex((x) => x.id === replaces);
    if (i >= 0) list.splice(i, 1);
  }
  const i = list.findIndex((x) => x.id === m.id);
  if (i >= 0) list[i] = m;
  else {
    list.push(m);
    list.sort(bySeq);
  }
}

/** Replaces the labels. A filter on a label that no longer exists is dropped. */
export function setLabels(labels: Label[]) {
  app.labels = [...labels].sort((a, b) => a.position - b.position || a.name.localeCompare(b.name));
  if (app.labelFilter && !labels.some((l) => l.id === app.labelFilter)) app.labelFilter = null;
}

export function setLocalAvatars(items: LocalAvatar[]) {
  const m: Record<string, string> = {};
  for (const a of items) m[`${a.kind}:${a.target}`] = `la_${a.kind}_${a.target}_${a.version}`;
  app.localAvatars = m;
}

/** The picture id to show for a person: the one set on this device, else their own. */
export const userAvatar = (u: User | undefined | null): string | null => (u ? (app.localAvatars[`user:${u.id}`] ?? u.avatar ?? null) : null);

export function sortedRows(): ChatRow[] {
  return Object.values(app.rows);
}

/** Total unread conversations' messages for the bottom-nav badge. */
export const unreadTotal = () => Object.values(app.rows).filter((r) => !r.muted).reduce((n, r) => n + (r.unread > 0 ? r.unread : r.marked_unread ? 1 : 0), 0);

// ---------------------------------------------------------------- core events

let unlisten: (() => void) | undefined;

function onEvent(ev: CoreEvent) {
  switch (ev.type) {
    case "conn":
      app.conn = ev.state;
      app.connDetail = ev.detail ?? "";
      break;
    case "chat":
      app.rows[ev.row.id] = ev.row;
      break;
    case "chat_remove":
      delete app.rows[ev.id];
      delete msgs[ev.id];
      if (app.selected === ev.id) app.selected = null;
      break;
    case "message": {
      const m = ev.message;
      if (m.thread_root) {
        const list = threadMsgs[m.thread_root];
        if (list) upsertInto(list, m, ev.replaces);
        // The reply count of a thread shows on its root; the core sends a thread event for that.
        // A reply that was also sent to the channel is one message that shows in both places.
        if (!m.also_send_to_channel) break;
      }
      const c = msgs[m.conv_id];
      if (c?.loaded && !c.detached) {
        upsertInto(c.items, m, ev.replaces);
        if (app.selected === m.conv_id && !m.pending && m.sender_id !== app.me?.user.id) scheduleRead(m.conv_id);
      }
      break;
    }
    case "message_remove": {
      const c = msgs[ev.conv_id];
      if (c) c.items = c.items.filter((x) => x.id !== ev.id);
      for (const k of Object.keys(threadMsgs)) threadMsgs[k] = threadMsgs[k].filter((x) => x.id !== ev.id);
      break;
    }
    case "user":
      app.users[ev.user.id] = ev.user;
      break;
    case "thread":
      app.threads[ev.thread.root.id] = ev.thread;
      break;
    case "thread_remove":
      delete app.threads[ev.root_id];
      break;
    case "typing": {
      if (ev.user_id === app.me?.user.id) break;
      app.typing[ev.conv_id] ??= {};
      app.typing[ev.conv_id][ev.user_id] = Date.now() + 4500;
      setTimeout(pruneTyping, 4700);
      break;
    }
    case "presence": {
      const u = app.users[ev.user_id];
      if (u) {
        u.presence = ev.presence;
        if (ev.last_active_ms) u.last_active_ms = ev.last_active_ms;
      }
      break;
    }
    case "me":
      app.me = ev.me;
      break;
    case "reset":
      void refreshLock();
      void loadSnapshot();
      break;
    case "counts":
      app.counts = ev.counts;
      break;
    case "saved":
      app.saved = ev.ids;
      app.savedLabels = ev.labels;
      break;
    case "labels":
      setLabels(ev.labels);
      break;
    case "pins":
      app.pins[ev.conv_id] = ev.ids;
      break;
    case "org_settings":
      app.org = ev.settings;
      break;
    case "upload":
      app.uploads[ev.local_id] = ev.info;
      break;
    case "local_avatars":
      setLocalAvatars(ev.items);
      break;
    case "voice":
      app.voice.level = ev.level;
      app.voice.elapsed = ev.elapsed_ms;
      app.voice.levels.push(ev.level);
      if (app.voice.levels.length > 120) app.voice.levels.shift();
      break;
    case "audio":
      if (ev.state === "stopped" && ev.id !== app.audio.id) break;
      app.audio.id = ev.id;
      app.audio.state = ev.state;
      app.audio.pos = ev.pos_ms;
      if (ev.dur_ms) app.audio.dur = ev.dur_ms;
      app.audio.speed = ev.speed;
      app.audio.error = ev.error ?? "";
      if (ev.error) toast(ev.error, "error");
      break;
    case "update":
      app.upd.state = ev.state;
      app.upd.got = ev.got;
      app.upd.total = ev.total;
      app.upd.message = ev.message;
      break;
    case "signed_out":
      resetState();
      app.phase = "out";
      app.signedOutNote = ev.code === "signed_out" ? "" : ev.message;
      break;
  }
}

function pruneTyping() {
  const now = Date.now();
  for (const [c, m] of Object.entries(app.typing)) {
    for (const [u, t] of Object.entries(m)) if (t < now) delete m[u];
    if (!Object.keys(m).length) delete app.typing[c];
  }
}

let readTimer: ReturnType<typeof setTimeout> | undefined;
function scheduleRead(conv: string) {
  clearTimeout(readTimer);
  readTimer = setTimeout(() => {
    if (app.selected === conv && document.visibilityState === "visible") void core.markRead(conv).catch(() => {});
  }, 350);
}

function resetState() {
  app.me = null;
  app.users = {};
  app.rows = {};
  app.threads = {};
  app.selected = null;
  app.thread = null;
  app.files = [];
  app.filesLoaded = false;
  app.counts = null;
  app.saved = [];
  app.labels = [];
  app.savedLabels = {};
  app.labelFilter = null;
  app.pins = {};
  app.localAvatars = {};
  app.uploads = {};
  app.viewer = null;
  app.openFail = null;
  app.sheet = null;
  app.gsearch = "";
  app.listq = "";
  app.filter = "all";
  app.drawer = false;
  app.tab = "chats";
  app.pad = null;
  app.spage = "";
  app.upd = { found: null, checking: false, info: null, error: "", state: "", got: 0, total: 0, message: "" };
  app.lock = { pin_set: false, open: false, wait_secs: 0, attempts_left: 5, timeout_secs: 300, on_background: true };
  for (const k of Object.keys(msgs)) delete msgs[k];
  for (const k of Object.keys(threadMsgs)) delete threadMsgs[k];
}

export async function loadSnapshot() {
  try {
    const s = await core.snapshot();
    app.me = s.me;
    app.server = s.server;
    app.conn = s.connection;
    app.users = Object.fromEntries(s.users.map((u) => [u.id, u]));
    app.rows = Object.fromEntries(s.rows.map((r) => [r.id, r]));
    app.threads = Object.fromEntries(s.threads.map((t) => [t.root.id, t]));
    app.counts = s.counts;
    app.org = s.org_settings;
    app.saved = s.saved;
    app.savedLabels = s.saved_labels;
    setLabels(s.labels);
    app.pins = s.pins;
    app.maxUpload = s.max_upload_bytes;
    setLocalAvatars(s.local_avatars);
    void core.getCounts().then((c) => (app.counts = c)).catch(() => {});
    app.phase = "in";
    // A new bootstrap replaced the cache: reload the open conversation.
    for (const k of Object.keys(msgs)) delete msgs[k];
    // A chat that locked or vanished while it was open must not stay open.
    if (app.selected && !app.rows[app.selected]) closeConversation();
    if (app.selected && app.rows[app.selected]) void openConversation(app.selected);
    void loadFiles();
  } catch (e) {
    // Not signed in, or the first sync has not finished: the Reset event calls this again.
    if (app.phase === "loading") app.phase = "in";
    app.connDetail = errorMessage(e);
  }
}

export async function start() {
  applyPrefs();
  window.addEventListener("resize", () => (app.phone = window.innerWidth < 760));
  unlisten = await onCoreEvent(onEvent);
  const info = await core.appInfo();
  app.info = { platform: info.platform, arch: info.arch, version: info.version, portable: info.portable, appimage: info.appimage, data_dir: info.data_dir };
  app.server = info.server ?? "";
  app.conn = info.connection;
  if (!info.signed_in) {
    app.phase = "out";
    return;
  }
  app.phase = "in";
  await refreshLock();
  await loadSnapshot();
}

export function stop() {
  unlisten?.();
}

/** Pull to refresh (chat list and inside a chat): reconnect to the server, then reload what is on screen. */
const REFRESH_LIMIT_MS = 30_000;

/** Pull to refresh: reconnect to the server and reload. The spinner runs until the app is connected, or gives up after 30 seconds. */
export async function refreshNow() {
  if (app.phase !== "in" || app.refreshing) return;
  app.refreshing = true;
  const deadline = Date.now() + REFRESH_LIMIT_MS;
  try {
    await core.reconnect();
    await loadSnapshot();
    while (app.conn !== "online" && Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, 500));
    }
    if (app.conn !== "online") toast("Could not connect to the server. Check the network and try again.", "error");
  } catch (e) {
    fail(e);
  } finally {
    app.refreshing = false;
  }
}

export async function refreshLock() {
  try {
    app.lock = await core.lockStatus();
  } catch {
    /* not signed in */
  }
}

export async function afterLogin() {
  app.signedOutNote = "";
  app.phase = "in";
  await refreshLock();
  await loadSnapshot();
}

// ---------------------------------------------------------------- conversations

export async function openConversation(id: string) {
  app.selected = id;
  app.thread = null;
  const c = ensureConv(id);
  if (c.detached) {
    c.detached = false;
    c.loaded = false;
  }
  if (c.loaded) {
    void core.markRead(id).catch(() => {});
    return;
  }
  c.loading = true;
  c.error = "";
  try {
    const p = await core.openConversation(id);
    c.items = p.messages;
    c.hasMore = p.has_more_before;
    c.loaded = true;
    void core.markRead(id).catch(() => {});
  } catch (e) {
    c.error = errorMessage(e);
  } finally {
    c.loading = false;
  }
}

export function closeConversation() {
  app.selected = null;
  app.thread = null;
  app.select = null;
}

/** Starts selecting messages in a chat with one message picked. */
export function startSelect(conv: string, id: string) {
  app.select = { conv, ids: [id] };
}

/** Adds or removes a message from the selection; leaving the last one ends selecting. */
export function toggleSelect(conv: string, id: string) {
  const cur = app.select;
  if (!cur || cur.conv !== conv) return startSelect(conv, id);
  const ids = cur.ids.includes(id) ? cur.ids.filter((x) => x !== id) : [...cur.ids, id];
  app.select = ids.length ? { conv, ids } : null;
}

export async function loadOlder(id: string): Promise<boolean> {
  const c = ensureConv(id);
  if (c.loading || !c.hasMore) return false;
  const first = c.items.find((m) => !m.pending);
  if (!first) return false;
  c.loading = true;
  try {
    const p = await core.loadOlder(id, first.seq);
    const have = new Set(c.items.map((m) => m.id));
    c.items = [...p.messages.filter((m) => !have.has(m.id)), ...c.items];
    c.hasMore = p.has_more_before;
    return p.messages.length > 0;
  } catch (e) {
    fail(e);
    return false;
  } finally {
    c.loading = false;
  }
}

export async function openThread(conv: string, root: string) {
  app.thread = { conv, root };
  threadMsgs[root] ??= [];
  try {
    const t = await core.getThread(conv, root);
    app.threads[root] ??= t.summary;
    threadMsgs[root] = t.replies as MessageView[];
  } catch (e) {
    fail(e);
  }
}

export async function loadFiles() {
  try {
    app.files = await core.listFiles({ limit: 200 });
    app.filesLoaded = true;
    app.filesError = "";
  } catch (e) {
    app.filesError = errorMessage(e);
  }
}

export async function startDm(userId: string) {
  try {
    const id = await core.createDm(userId);
    app.tab = "chats";
    await openConversation(id);
  } catch (e) {
    fail(e);
  }
}


// ---------------------------------------------------------------- sheets, viewer, jumping

export const closeSheet = () => (app.sheet = null);

/** Opens a message in its conversation, loading the messages around it when it is not in the loaded page. */
export async function jumpTo(conv: string, msgId: string, seq?: number) {
  app.tab = "chats";
  app.sheet = null;
  app.viewer = null;
  app.gsearch = "";
  await openConversation(conv);
  const c = ensureConv(conv);
  if (!c.items.some((m) => m.id === msgId)) {
    try {
      let s = seq;
      if (s === undefined) s = (await core.fetchMessage(msgId)).seq;
      const around = await core.messagesAround(conv, s);
      c.items = around.sort((a, b) => a.seq - b.seq);
      c.hasMore = true;
      c.detached = true;
    } catch (e) {
      fail(e);
      return;
    }
  }
  app.focus = { conv, id: msgId };
}

export async function backToLatest(conv: string) {
  const c = ensureConv(conv);
  c.detached = false;
  c.loaded = false;
  await openConversation(conv);
}
