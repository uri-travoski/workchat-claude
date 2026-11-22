// Typed bridge to the Rust core (src-tauri). Server types come from the generated api-types.ts;
// the view models below mirror src-tauri/src/view.rs and engine.rs.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Attachment, ChannelBrowseItem, ChannelBrowsePage, ConversationSummary, Counts, FileHit, Me, Member, Message, OrgSettings, Pin, Presence, ReactionSummary, SearchResponse, ThreadSummary, ThreadView, User } from "./api-types";

export type { Attachment, ChannelBrowseItem, ChannelBrowsePage, ConversationSummary, Counts, FileHit, Me, Member, Message, OrgSettings, Pin, Presence, SearchResponse, ThreadSummary, ThreadView, User };

/** A file picked, dropped, pasted or recorded, waiting in the core's staging folder. */
export interface Staged {
  id: string;
  name: string;
  content_type: string;
  size: number;
  kind: "image" | "video" | "audio" | "file";
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  pages: number | null;
  poster_b64: string | null;
  peaks: number[];
}

export interface UploadInfo {
  index: number;
  count: number;
  sent: number;
  total: number;
  state: "waiting" | "uploading" | "sending" | "failed";
}

export interface LocalAvatar {
  kind: "user" | "conv";
  target: string;
  version: number;
}

export type ConvKind = "dm" | "self" | "group_dm" | "channel";

export interface Preview {
  kind: "none" | "text" | "voice" | "audio" | "image" | "video" | "file" | "deleted" | "system";
  text: string;
  sender?: string;
  chip?: string;
  duration_ms?: number;
}

export interface ChatRow {
  id: string;
  kind: ConvKind;
  title: string;
  topic: string | null;
  avatar: string | null;
  initials: string;
  peer_id: string | null;
  peer_role: string | null;
  is_bot: boolean;
  presence: string | null;
  member_count: number;
  preview: Preview;
  last_ts: number;
  unread: number;
  mentions: number;
  marked_unread: boolean;
  favourite: boolean;
  muted: boolean;
  position: number | null;
  last_own: boolean;
  last_delivered: boolean;
  has_pending: boolean;
  archived: boolean;
  last_seq: number;
  readers: Record<string, number>;
  avatar_source: "local" | "private" | "shared" | null;
  can_pin: boolean;
  /** An admin or the chat's manager: rename, topic, picture, members, visibility, archive. */
  can_manage: boolean;
  visibility: "public" | "private";
  locked: boolean;
}

/** A server message plus the local send state. Unsent messages have id `local:<client_id>`. */
// The core always sends the list fields; the generated type marks them optional because of serde defaults.
export type MessageView = Omit<Message, "attachments" | "reactions" | "mentions"> & {
  attachments: Attachment[];
  reactions: ReactionSummary[];
  mentions: string[];
  pending?: "sending" | "failed";
  error?: string;
  upload?: UploadInfo;
};

export interface Page {
  messages: MessageView[];
  has_more_before: boolean;
}

export interface Snapshot {
  me: Me;
  server: string;
  users: User[];
  rows: ChatRow[];
  threads: ThreadSummary[];
  connection: ConnState;
  counts: Counts | null;
  org_settings: OrgSettings;
  saved: string[];
  local_avatars: LocalAvatar[];
  max_upload_bytes: number;
  pins: Record<string, string[]>;
}

export type ConnState = "offline" | "connecting" | "online";

export interface AppInfo {
  version: string;
  signed_in: boolean;
  server: string | null;
  username: string | null;
  connection: ConnState;
  data_dir: string;
  platform: string;
  arch: string;
  portable: boolean;
  appimage: boolean;
}

export interface ServerCheck {
  url: string;
  name: string;
  server_version: string;
  max_upload_bytes: number;
}

export type CoreEvent =
  | { type: "conn"; state: ConnState; detail: string | null }
  | { type: "chat"; row: ChatRow }
  | { type: "chat_remove"; id: string }
  | { type: "message"; message: MessageView; replaces: string | null }
  | { type: "message_remove"; conv_id: string; id: string }
  | { type: "user"; user: User }
  | { type: "thread"; thread: ThreadSummary }
  | { type: "thread_remove"; root_id: string }
  | { type: "typing"; conv_id: string; user_id: string }
  | { type: "presence"; user_id: string; presence: Presence; last_active_ms: number | null }
  | { type: "me"; me: Me }
  | { type: "reset" }
  | { type: "signed_out"; code: string; message: string }
  | { type: "counts"; counts: Counts }
  | { type: "voice"; level: number; elapsed_ms: number }
  | { type: "audio"; id: string; state: AudioState; pos_ms: number; dur_ms: number; speed: number; error: string | null }
  | { type: "saved"; ids: string[] }
  | { type: "pins"; conv_id: string; ids: string[] }
  | { type: "org_settings"; settings: OrgSettings }
  | { type: "channel_directory"; conv_id: string; channel: ChannelBrowseItem | null }
  | { type: "upload"; local_id: string; info: UploadInfo }
  | { type: "local_avatars"; items: LocalAvatar[] }
  | { type: "update"; state: "downloading" | "ready" | "error" | "needs_permission" | "installing"; got: number; total: number; message: string };

export interface LockStatus {
  pin_set: boolean;
  open: boolean;
  wait_secs: number;
  attempts_left: number;
  timeout_secs: number;
  on_background: boolean;
}
export interface UnlockResult {
  ok: boolean;
  wait_secs: number;
  attempts_left: number;
}
export interface NotifySettings {
  enabled: boolean;
  dms: boolean;
  channels: boolean;
  threads: boolean;
  mentions_only: boolean;
  mention_alert: boolean;
  sound: "default" | "gentle" | "none";
  channel_sound: boolean;
  vibration: boolean;
  show_preview: boolean;
  dnd: boolean;
  dnd_from: string;
  dnd_to: string;
  close_to_tray: boolean;
  keep_alive: boolean;
}
export interface PushInfo {
  kind: string;
  target_host: string;
  registered_ms: number;
  error: string;
}
export interface NotifyStatus {
  platform: string;
  permission: "granted" | "denied" | "prompt";
  method: "os" | "unifiedpush" | "keepalive" | "fcm" | "apns" | "none";
  distributors: string[];
  distributor: string;
  fcm: boolean;
  keep_alive_running: boolean;
  tray: boolean;
  push: PushInfo;
  install_ok: boolean;
}
export interface StorageInfo {
  database: number;
  media: number;
  other: number;
  total: number;
  files: number;
}
export interface UpdateAsset {
  name: string;
  url: string;
  size: number;
}
export interface UpdateInfo {
  current: string;
  latest: string;
  newer: boolean;
  notes: string;
  page: string;
  published: string;
  asset: UpdateAsset | null;
  action: "install" | "installer" | "portable" | "appimage" | "package" | "page";
  checksum_url: string | null;
  platform: string;
}
export interface UpdateState {
  current: string;
  found: string | null;
  due: boolean;
  api: string;
  custom_api: boolean;
}
export interface SessionItem {
  id: string;
  user_id: string;
  username: string;
  device_name: string;
  client_version: string;
  ip: string;
  created_ms: number;
  last_seen_ms: number;
  current: boolean;
  revoked: boolean;
}

export type AudioState = "loading" | "playing" | "paused" | "ended" | "stopped" | "error";

/** Errors from the core arrive as `{ code, message }`. */
export interface CoreError {
  code: string;
  message: string;
}

export function errorMessage(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) return String((e as CoreError).message);
  return typeof e === "string" ? e : "Something went wrong.";
}
export function errorCode(e: unknown): string {
  return e && typeof e === "object" && "code" in e ? String((e as CoreError).code) : "";
}

export const core = {
  appInfo: () => invoke<AppInfo>("app_info"),
  checkServer: (server: string) => invoke<ServerCheck>("check_server", { server }),
  loginPassword: (server: string, username: string, password: string) => invoke<Me>("login_password", { server, username, password }),
  loginLink: (link: string, server?: string) => invoke<Me>("login_link", { link, server: server || null }),
  signOut: () => invoke<void>("sign_out"),
  snapshot: () => invoke<Snapshot>("get_snapshot"),
  openConversation: (convId: string, limit = 50) => invoke<Page>("open_conversation", { convId, limit }),
  loadOlder: (convId: string, beforeSeq: number, limit = 50) => invoke<Page>("load_older", { convId, beforeSeq, limit }),
  sendMessage: (convId: string, body: string, threadRoot?: string | null, replyTo?: string | null, alsoSendToChannel = false) =>
    invoke<MessageView>("send_message", { convId, body, threadRoot: threadRoot ?? null, replyTo: replyTo ?? null, alsoSendToChannel }),
  retryMessage: (localId: string) => invoke<void>("retry_message", { localId }),
  discardMessage: (localId: string) => invoke<void>("discard_message", { localId }),
  editMessage: (id: string, body: string) => invoke<MessageView>("edit_message", { id, body }),
  deleteMessage: (id: string) => invoke<MessageView>("delete_message", { id }),
  react: (id: string, emoji: string, add: boolean) => invoke<MessageView>("react", { id, emoji, add }),
  markRead: (convId: string) => invoke<void>("mark_read", { convId }),
  setFavourite: (convId: string, favourite: boolean) => invoke<void>("set_favourite", { convId, favourite }),
  orderConversations: (convIds: string[]) => invoke<void>("order_conversations", { convIds }),
  createDm: (userId: string) => invoke<string>("create_dm", { userId }),
  listFiles: (opts: { kind?: string; q?: string; offset?: number; limit?: number } = {}) =>
    invoke<FileHit[]>("list_files", { kind: opts.kind ?? null, q: opts.q ?? null, offset: opts.offset ?? null, limit: opts.limit ?? 100 }),
  getThread: (convId: string, root: string) => invoke<ThreadView>("get_thread", { convId, root }),
  chatRow: (convId: string) => invoke<ChatRow | null>("chat_row", { convId }),
  typing: (convId: string) => invoke<void>("typing", { convId }),
  reconnect: () => invoke<void>("reconnect"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),

  // attachments
  pickFiles: (kind: "media" | "any") => invoke<Staged[]>("pick_files", { kind }),
  stagePaths: (paths: string[]) => invoke<Staged[]>("stage_paths", { paths }),
  unstage: (id: string) => invoke<void>("unstage", { id }),
  setStagedMeta: (id: string, meta: { width?: number; height?: number; duration_ms?: number; poster_b64?: string }) => invoke<void>("set_staged_meta", { id, meta }),
  sendFiles: (convId: string, ids: string[], body: string, threadRoot?: string | null, replyTo?: string | null, alsoSendToChannel = false) =>
    invoke<MessageView>("send_files", { convId, ids, body, threadRoot: threadRoot ?? null, replyTo: replyTo ?? null, alsoSendToChannel }),
  saveAttachment: (id: string, name: string) => invoke<string>("save_attachment", { id, name }),
  openAttachment: (id: string, name: string) => invoke<void>("open_attachment", { id, name }),
  /** Sends bytes (a pasted image, a camera photo) to the core in slices and returns the staged file. */
  async stageBlob(blob: Blob, name: string): Promise<Staged> {
    const id = await invoke<string>("stage_begin", { name });
    const SLICE = 2 * 1024 * 1024;
    try {
      for (let off = 0; off < blob.size; off += SLICE) {
        const buf = await blob.slice(off, off + SLICE).arrayBuffer();
        await invoke<void>("stage_chunk", buf, { headers: { "x-stage-id": id } });
      }
      return await invoke<Staged>("stage_end", { id });
    } catch (e) {
      void invoke("unstage", { id }).catch(() => {});
      throw e;
    }
  },

  // voice and playback
  voiceStart: () => invoke<void>("voice_start"),
  voiceFinish: () => invoke<Staged>("voice_finish"),
  voiceCancel: () => invoke<void>("voice_cancel"),
  audioPlay: (id: string, speed = 1, fromMs = 0) => invoke<void>("audio_play", { id, speed, fromMs }),
  audioCtl: (action: "pause" | "resume" | "seek" | "speed" | "stop", value?: number) => invoke<void>("audio_ctl", { action, value: value ?? null }),

  // threads, counts, search, people
  getCounts: () => invoke<Counts>("get_counts"),
  listThreads: (opts: { unread?: boolean; before?: number; limit?: number } = {}) => invoke<ThreadSummary[]>("list_threads", { unread: opts.unread ?? false, before: opts.before ?? null, limit: opts.limit ?? 50 }),
  followThread: (root: string, on: boolean) => invoke<void>("follow_thread", { root, on }),
  readThread: (root: string) => invoke<void>("read_thread", { root }),
  search: (q: string, convId?: string | null, limit = 30) => invoke<SearchResponse>("search", { q, convId: convId ?? null, limit }),
  listUsers: (q?: string) => invoke<User[]>("list_users", { q: q ?? null }),
  updateProfile: (p: { display_name?: string; title?: string; phone?: string; read_receipts?: boolean; hide_last_active?: boolean }) =>
    invoke<Me>("update_profile", { displayName: p.display_name ?? null, title: p.title ?? null, phone: p.phone ?? null, readReceipts: p.read_receipts ?? null, hideLastActive: p.hide_last_active ?? null }),

  // message actions
  forwardMessage: (id: string, convIds: string[], comment?: string) => invoke<void>("forward_message", { id, convIds, comment: comment ?? null }),
  pinMessage: (convId: string, id: string, on: boolean) => invoke<void>("pin_message", { convId, id, on }),
  getPins: (convId: string) => invoke<MessageView[]>("get_pins", { convId }),
  saveMessage: (id: string, on: boolean) => invoke<void>("save_message", { id, on }),
  fetchMessage: (id: string) => invoke<MessageView>("fetch_message", { id }),
  messagesAround: (convId: string, seq: number) => invoke<MessageView[]>("messages_around", { convId, seq }),

  // avatars and new chats
  setAvatar: (kind: "me" | "conv", target: string | null, stagedId: string) => invoke<void>("set_avatar", { kind, target, stagedId }),
  removeAvatar: (kind: "me" | "conv", target: string | null) => invoke<void>("remove_avatar", { kind, target }),
  setLocalAvatar: (kind: "user" | "conv", target: string, stagedId: string) => invoke<void>("set_local_avatar", { kind, target, stagedId }),
  removeLocalAvatar: (kind: "user" | "conv", target: string) => invoke<void>("remove_local_avatar", { kind, target }),
  createConversation: (kind: "channel" | "group_dm", name: string, members: string[], topic?: string, avatarStaged?: string | null, visibility?: "public" | "private") =>
    invoke<string>("create_conversation", { kind, name, topic: topic ?? null, members, visibility: visibility ?? null, avatarStaged: avatarStaged ?? null }),
  getMembers: (convId: string) => invoke<Member[]>("get_members", { convId }),
  removeMember: (convId: string, userId: string) => invoke<void>("remove_member", { convId, userId }),
  browseChannels: (q = "", limit = 50, offset = 0) => invoke<ChannelBrowsePage>("browse_channels", { q, limit, offset }),
  joinChannel: (convId: string) => invoke<string>("join_channel", { convId }),
  leaveConversation: (convId: string) => invoke<void>("leave_conversation", { convId }),
  hideMessage: (id: string) => invoke<void>("hide_message", { id }),
  getOrgSettings: () => invoke<OrgSettings>("get_org_settings"),
  addMembers: (convId: string, userIds: string[]) => invoke<void>("add_members", { convId, userIds }),
  updateConversation: (convId: string, patch: { name?: string; topic?: string; archived?: boolean; visibility?: "public" | "private" }) =>
    invoke<void>("update_conversation", { convId, name: patch.name ?? null, topic: patch.topic ?? null, archived: patch.archived ?? null, visibility: patch.visibility ?? null }),

  // lock
  lockStatus: () => invoke<LockStatus>("lock_status"),
  lockSetPin: (pin: string, confirm: string) => invoke<void>("lock_set_pin", { pin, confirm }),
  lockUnlock: (pin: string) => invoke<UnlockResult>("lock_unlock", { pin }),
  lockRelock: () => invoke<void>("lock_relock"),
  lockChangePin: (old: string, next: string, confirm: string) => invoke<UnlockResult>("lock_change_pin", { old, new: next, confirm }),
  lockSetTimeout: (secs: number, onBackground: boolean) => invoke<void>("lock_set_timeout", { secs, onBackground }),
  lockTick: () => invoke<boolean>("lock_tick"),
  lockTouch: () => invoke<void>("lock_touch"),
  lockBackground: () => invoke<void>("lock_background"),
  lockChat: (convId: string, on: boolean) => invoke<void>("lock_chat", { convId, on }),
  lockForgot: () => invoke<void>("lock_forgot"),
  lockedIds: () => invoke<string[]>("locked_ids"),

  // notifications and push
  getNotifySettings: () => invoke<NotifySettings>("get_notify_settings"),
  setNotifySettings: (settings: NotifySettings) => invoke<void>("set_notify_settings", { settings }),
  uiState: (focused: boolean, conv: string | null, tzOffsetMin: number) => invoke<void>("ui_state", { focused, conv, tzOffsetMin }),
  notifyStatus: () => invoke<NotifyStatus>("notify_status"),
  notifyRequestPermission: () => invoke<string>("notify_request_permission"),
  notifyTest: () => invoke<void>("notify_test"),
  pushRegister: (distributor?: string | null) => invoke<Record<string, unknown>>("push_register", { distributor: distributor ?? null }),
  pushEndpoint: (kind: string, target: string) => invoke<PushInfo>("push_endpoint", { kind, target }),
  pushUnregister: () => invoke<void>("push_unregister"),
  setSystemBars: (color: string, dark: boolean) => invoke<void>("set_system_bars", { color, dark }),
  requestPermission: (name: "microphone" | "camera" | "notifications") => invoke<boolean>("request_permission", { name }),

  // storage
  storageInfo: () => invoke<StorageInfo>("storage_info"),
  clearCache: () => invoke<StorageInfo>("clear_cache"),
  compactDb: () => invoke<StorageInfo>("compact_db"),
  mediaCached: (ids: string[]) => invoke<string[]>("media_cached", { ids }),

  // updates
  updateState: () => invoke<UpdateState>("update_state"),
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  setUpdateApi: (url: string) => invoke<void>("set_update_api", { url }),
  cancelUpdate: () => invoke<void>("cancel_update"),
  startUpdate: (info: UpdateInfo) => invoke<void>("start_update", { info }),

  // devices
  listSessions: () => invoke<SessionItem[]>("list_sessions"),
  revokeSession: (id: string) => invoke<void>("revoke_session", { id }),
  revokeOtherSessions: () => invoke<void>("revoke_other_sessions"),

  // debug builds only (automated UI tests)
  testScript: () => invoke<string | null>("test_script"),
  testMark: (name: string) => invoke<void>("test_mark", { name }),
  testRun: (script: string) => invoke<void>("test_run", { script }),
};

export function onCoreEvent(cb: (e: CoreEvent) => void): Promise<UnlistenFn> {
  return listen<CoreEvent>("wc", (ev) => cb(ev.payload));
}
